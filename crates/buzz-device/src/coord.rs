//! Same-tank coordination over Buzz kind:9 stream messages.
//!
//! Not a scheduler. Durable assignment journal + peer checks. Transport may
//! be the isolated Buzz-relay or the mux fixture.

use crate::path_guard::assert_existing_inside;
use crate::DeviceError;
use buzz_core::coord::{
    assignment_fingerprint, authorize_coord_peer, check_assignment_id, CoordError, CoordPeers,
    CoordPhase, CoordStatus, COORD_MAX_EVENTS,
};
use buzz_core::kind::KIND_STREAM_MESSAGE;
use nostr::{Alphabet, Event, EventBuilder, Filter, Keys, PublicKey, SingleLetterTag, Tag};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One durable assignment row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordEntry {
    /// Assignment id.
    pub assignment_id: String,
    /// Fingerprint of tank/conversation/worker/task.
    pub fingerprint: String,
    /// Runtime status.
    pub status: String,
    /// Events processed.
    pub events_seen: u32,
    /// Last coord phase.
    pub last_phase: String,
}

/// Filesystem journal under `state_dir/coord/`.
pub struct CoordJournal {
    dir: PathBuf,
}

impl CoordJournal {
    /// Open or create the journal directory.
    pub fn open(state_dir: &Path) -> Result<Self, DeviceError> {
        let dir = state_dir.join("coord");
        std::fs::create_dir_all(&dir).map_err(|e| DeviceError::Journal(e.to_string()))?;
        Ok(Self { dir })
    }

    fn path_for(&self, assignment_id: &str) -> PathBuf {
        self.dir.join(format!("{assignment_id}.json"))
    }

    /// Load a row.
    pub fn get(&self, assignment_id: &str) -> Result<Option<CoordEntry>, DeviceError> {
        let path = self.path_for(assignment_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(path).map_err(|e| DeviceError::Journal(e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| DeviceError::Journal(e.to_string()))
    }

    /// Atomic persist.
    pub fn put(&self, entry: &CoordEntry) -> Result<(), DeviceError> {
        let path = self.path_for(&entry.assignment_id);
        let tmp = path.with_extension("json.tmp");
        let body =
            serde_json::to_vec_pretty(entry).map_err(|e| DeviceError::Journal(e.to_string()))?;
        std::fs::write(&tmp, body).map_err(|e| DeviceError::Journal(e.to_string()))?;
        std::fs::rename(tmp, path).map_err(|e| DeviceError::Journal(e.to_string()))?;
        Ok(())
    }
}

/// Process-bound addressing. Fail closed when the event does not match.
#[derive(Debug, Clone)]
pub struct CoordBind {
    /// This process pubkey; a `p` tag must equal it, or the sender must be us
    /// (local apply of an outbound continue/result).
    pub self_pubkey_hex: String,
    /// Expected `h` / `--conversation-id`.
    pub conversation_id: String,
    /// Expected `tank` / `--tank-id`.
    pub tank_id: String,
}

/// Parsed coordination event.
#[derive(Debug, Clone)]
pub struct CoordMessage {
    /// Sender pubkey hex.
    pub sender: String,
    /// Phase.
    pub phase: CoordPhase,
    /// Assignment id.
    pub assignment_id: String,
    /// Tank id.
    pub tank_id: String,
    /// Conversation / channel id.
    pub conversation_id: String,
    /// `#p` mention tags (hex).
    pub p_tags: Vec<String>,
    /// JSON body.
    pub body: serde_json::Value,
}

/// Parse kind:9 tags used by this demo. Unknown events return None.
pub fn parse_coord_event(event: &Event) -> Option<CoordMessage> {
    if u32::from(event.kind.as_u16()) != KIND_STREAM_MESSAGE {
        return None;
    }
    let mut phase = None;
    let mut assignment_id = None;
    let mut tank_id = None;
    let mut conversation_id = None;
    let mut p_tags = Vec::new();
    for tag in event.tags.iter() {
        let items = tag.as_slice();
        match items.first().map(String::as_str) {
            Some("coord") => phase = items.get(1).and_then(|v| CoordPhase::parse(v)),
            Some("assignment") => assignment_id = items.get(1).cloned(),
            Some("tank") => tank_id = items.get(1).cloned(),
            Some("h") => conversation_id = items.get(1).cloned(),
            Some("p") => {
                if let Some(value) = items.get(1) {
                    p_tags.push(value.clone());
                }
            }
            _ => {}
        }
    }
    Some(CoordMessage {
        sender: event.pubkey.to_hex(),
        phase: phase?,
        assignment_id: assignment_id?,
        tank_id: tank_id?,
        conversation_id: conversation_id?,
        p_tags,
        body: serde_json::from_str(&event.content).unwrap_or(serde_json::Value::Null),
    })
}

/// Sign a kind:9 coordination message (Buzz stream primitive + assignment tags).
pub fn publish_coord(
    keys: &Keys,
    peer: &PublicKey,
    conversation_id: &str,
    tank_id: &str,
    assignment_id: &str,
    phase: CoordPhase,
    body: &serde_json::Value,
) -> Result<Event, DeviceError> {
    let content = serde_json::to_string(body).map_err(|e| DeviceError::Transport(e.to_string()))?;
    EventBuilder::new(nostr::Kind::Custom(KIND_STREAM_MESSAGE as u16), content)
        .tags([
            Tag::parse(["h", conversation_id])
                .map_err(|e| DeviceError::Transport(e.to_string()))?,
            Tag::public_key(*peer),
            Tag::parse(["assignment", assignment_id])
                .map_err(|e| DeviceError::Transport(e.to_string()))?,
            Tag::parse(["coord", phase.as_str()])
                .map_err(|e| DeviceError::Transport(e.to_string()))?,
            Tag::parse(["tank", tank_id]).map_err(|e| DeviceError::Transport(e.to_string()))?,
        ])
        .sign_with_keys(keys)
        .map_err(|e| DeviceError::Transport(e.to_string()))
}

/// Apply one inbound event. Returns whether work was mutated.
pub fn handle_coord(
    journal: &CoordJournal,
    peers: &CoordPeers,
    bind: &CoordBind,
    message: &CoordMessage,
    cwd: Option<&Path>,
    allowed_root: Option<&Path>,
) -> Result<(CoordStatus, bool), DeviceError> {
    check_assignment_id(&message.assignment_id).map_err(DeviceError::from_coord)?;
    let addressed_to_self = message
        .p_tags
        .iter()
        .any(|value| eq_hex(value, &bind.self_pubkey_hex));
    let from_self = eq_hex(&message.sender, &bind.self_pubkey_hex);
    if !addressed_to_self && !from_self {
        return Ok((CoordStatus::Rejected, false));
    }
    if message.conversation_id != bind.conversation_id || message.tank_id != bind.tank_id {
        return Ok((CoordStatus::Rejected, false));
    }
    let hops = match hops_from_body(&message.body, message.phase) {
        Some(hops) => hops,
        None => return Ok((CoordStatus::Rejected, false)),
    };
    let existing = journal.get(&message.assignment_id)?;
    let events_seen = existing.as_ref().map(|e| e.events_seen).unwrap_or(0);
    if let Err(err) = authorize_coord_peer(peers, &message.sender, message.phase, hops, events_seen)
    {
        return match err {
            CoordError::UnauthorizedPeer => Ok((CoordStatus::Rejected, false)),
            other => Err(DeviceError::from_coord(other)),
        };
    }
    if let Some(row) = existing.as_ref() {
        if row.last_phase == message.phase.as_str()
            && matches!(message.phase, CoordPhase::Result | CoordPhase::Continue)
        {
            return Ok((CoordStatus::Duplicate, false));
        }
    }
    let task = message
        .body
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let fingerprint = assignment_fingerprint(
        &message.tank_id,
        &message.conversation_id,
        &peers.worker_pubkey_hex,
        &task,
    )
    .map_err(DeviceError::from_coord)?;
    if let Some(row) = existing.as_ref() {
        if row.fingerprint != fingerprint {
            return Ok((CoordStatus::Rejected, false));
        }
    }
    let mut mutated = false;
    let status = match message.phase {
        CoordPhase::Delegate | CoordPhase::Ack => CoordStatus::Succeeded,
        CoordPhase::Result => {
            let Some(ok) = message
                .body
                .get("status")
                .and_then(|v| v.as_str())
                .and_then(CoordStatus::parse)
            else {
                return Ok((CoordStatus::Rejected, false));
            };
            if ok == CoordStatus::Succeeded {
                if let (Some(cwd), Some(root)) = (cwd, allowed_root) {
                    write_marker(root, cwd, &message.assignment_id, "COORD", &task)?;
                    mutated = true;
                }
            }
            ok
        }
        CoordPhase::Continue => {
            if let (Some(cwd), Some(root)) = (cwd, allowed_root) {
                write_marker(root, cwd, &message.assignment_id, "CONTINUE", "continue")?;
                mutated = true;
            }
            CoordStatus::Succeeded
        }
    };
    journal.put(&CoordEntry {
        assignment_id: message.assignment_id.clone(),
        fingerprint,
        status: status.as_str().to_string(),
        events_seen: events_seen.saturating_add(1).min(COORD_MAX_EVENTS),
        last_phase: message.phase.as_str().to_string(),
    })?;
    Ok((status, mutated))
}

fn hops_from_body(body: &serde_json::Value, phase: CoordPhase) -> Option<u32> {
    match body.get("hops") {
        Some(value) if value.is_number() => Some(value.as_u64().unwrap_or(0) as u32),
        Some(_) | None if phase == CoordPhase::Delegate => None,
        _ => Some(0),
    }
}

fn eq_hex(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn write_marker(
    root: &Path,
    cwd: &Path,
    assignment_id: &str,
    prefix: &str,
    body: &str,
) -> Result<(), DeviceError> {
    let dir = assert_existing_inside(root, cwd)?;
    let path = dir.join(format!("{prefix}-{assignment_id}.txt"));
    if path.exists() {
        return Ok(());
    }
    std::fs::write(&path, format!("{body}\n")).map_err(|e| DeviceError::Path(e.to_string()))?;
    Ok(())
}

impl DeviceError {
    fn from_coord(err: CoordError) -> Self {
        DeviceError::Transport(err.to_string())
    }
}

/// Subscription filter for inbound coordination events addressed to `me`.
/// Kind 9 + `#p` + `#h`, matching ACP mentions-mode subscribe.
pub fn coord_filter(me: PublicKey, conversation_id: &str) -> Filter {
    let h_tag = SingleLetterTag::lowercase(Alphabet::H);
    Filter::new()
        .kind(nostr::Kind::Custom(KIND_STREAM_MESSAGE as u16))
        .pubkey(me)
        .custom_tags(h_tag, [conversation_id])
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::Keys;

    fn hex(keys: &Keys) -> String {
        keys.public_key().to_hex()
    }

    fn worker_bind(worker: &Keys) -> CoordBind {
        CoordBind {
            self_pubkey_hex: hex(worker),
            conversation_id: "conv".into(),
            tank_id: "tank".into(),
        }
    }

    fn assignment_id() -> &'static str {
        "1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }

    #[test]
    fn unrelated_sender_does_not_mutate() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = CoordJournal::open(tmp.path()).unwrap();
        let leader = Keys::generate();
        let worker = Keys::generate();
        let stranger = Keys::generate();
        let peers = CoordPeers {
            leader_pubkey_hex: hex(&leader),
            worker_pubkey_hex: hex(&worker),
        };
        let msg = CoordMessage {
            sender: hex(&stranger),
            phase: CoordPhase::Delegate,
            assignment_id: assignment_id().into(),
            tank_id: "tank".into(),
            conversation_id: "conv".into(),
            p_tags: vec![hex(&worker)],
            body: serde_json::json!({"task": "x", "hops": 0}),
        };
        let (status, mutated) = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &msg,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap();
        assert_eq!(status, CoordStatus::Rejected);
        assert!(!mutated);
        assert!(journal.get(&msg.assignment_id).unwrap().is_none());
    }

    #[test]
    fn duplicate_result_does_not_rewrite_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = CoordJournal::open(tmp.path()).unwrap();
        let leader = Keys::generate();
        let worker = Keys::generate();
        let peers = CoordPeers {
            leader_pubkey_hex: hex(&leader),
            worker_pubkey_hex: hex(&worker),
        };
        let id = assignment_id();
        let msg = CoordMessage {
            sender: hex(&worker),
            phase: CoordPhase::Result,
            assignment_id: id.into(),
            tank_id: "tank".into(),
            conversation_id: "conv".into(),
            p_tags: vec![hex(&leader)],
            body: serde_json::json!({"task": "hello", "hops": 0, "status": "succeeded"}),
        };
        let (first, mutated) = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &msg,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap();
        assert_eq!(first, CoordStatus::Succeeded);
        assert!(mutated);
        let marker = tmp.path().join(format!("COORD-{id}.txt"));
        let original = std::fs::read(&marker).unwrap();
        std::fs::write(&marker, b"tamper\n").unwrap();
        let (second, mutated_again) = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &msg,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap();
        assert_eq!(second, CoordStatus::Duplicate);
        assert!(!mutated_again);
        assert_eq!(std::fs::read(&marker).unwrap(), b"tamper\n");
        let _ = original;
    }

    #[test]
    fn unknown_result_status_does_not_write_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = CoordJournal::open(tmp.path()).unwrap();
        let leader = Keys::generate();
        let worker = Keys::generate();
        let peers = CoordPeers {
            leader_pubkey_hex: hex(&leader),
            worker_pubkey_hex: hex(&worker),
        };
        let msg = CoordMessage {
            sender: hex(&worker),
            phase: CoordPhase::Result,
            assignment_id: assignment_id().into(),
            tank_id: "tank".into(),
            conversation_id: "conv".into(),
            p_tags: vec![hex(&leader)],
            body: serde_json::json!({"task": "hello", "hops": 0, "status": "yolo-ok"}),
        };
        let (status, mutated) = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &msg,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap();
        assert_eq!(status, CoordStatus::Rejected);
        assert!(!mutated);
        assert!(journal.get(&msg.assignment_id).unwrap().is_none());
        assert!(!tmp
            .path()
            .join(format!("COORD-{}.txt", assignment_id()))
            .exists());
    }

    #[test]
    fn missing_p_tag_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = CoordJournal::open(tmp.path()).unwrap();
        let leader = Keys::generate();
        let worker = Keys::generate();
        let peers = CoordPeers {
            leader_pubkey_hex: hex(&leader),
            worker_pubkey_hex: hex(&worker),
        };
        let msg = CoordMessage {
            sender: hex(&leader),
            phase: CoordPhase::Delegate,
            assignment_id: assignment_id().into(),
            tank_id: "tank".into(),
            conversation_id: "conv".into(),
            p_tags: vec![],
            body: serde_json::json!({"task": "x", "hops": 0}),
        };
        let (status, mutated) = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &msg,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap();
        assert_eq!(status, CoordStatus::Rejected);
        assert!(!mutated);
        assert!(journal.get(&msg.assignment_id).unwrap().is_none());
    }

    #[test]
    fn wrong_conversation_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = CoordJournal::open(tmp.path()).unwrap();
        let leader = Keys::generate();
        let worker = Keys::generate();
        let peers = CoordPeers {
            leader_pubkey_hex: hex(&leader),
            worker_pubkey_hex: hex(&worker),
        };
        let msg = CoordMessage {
            sender: hex(&leader),
            phase: CoordPhase::Delegate,
            assignment_id: assignment_id().into(),
            tank_id: "tank".into(),
            conversation_id: "other-channel".into(),
            p_tags: vec![hex(&worker)],
            body: serde_json::json!({"task": "x", "hops": 0}),
        };
        let (status, mutated) = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &msg,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap();
        assert_eq!(status, CoordStatus::Rejected);
        assert!(!mutated);
        assert!(journal.get(&msg.assignment_id).unwrap().is_none());
    }

    #[test]
    fn fingerprint_mismatch_is_rejected_without_cwd_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = CoordJournal::open(tmp.path()).unwrap();
        let leader = Keys::generate();
        let worker = Keys::generate();
        let peers = CoordPeers {
            leader_pubkey_hex: hex(&leader),
            worker_pubkey_hex: hex(&worker),
        };
        let first = CoordMessage {
            sender: hex(&worker),
            phase: CoordPhase::Result,
            assignment_id: assignment_id().into(),
            tank_id: "tank".into(),
            conversation_id: "conv".into(),
            p_tags: vec![hex(&leader)],
            body: serde_json::json!({"task": "hello", "hops": 0, "status": "succeeded"}),
        };
        let (status, mutated) = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &first,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap();
        assert_eq!(status, CoordStatus::Succeeded);
        assert!(mutated);
        let marker = tmp.path().join(format!("COORD-{}.txt", assignment_id()));
        let original = std::fs::read(&marker).unwrap();

        let second = CoordMessage {
            sender: hex(&leader),
            phase: CoordPhase::Delegate,
            assignment_id: assignment_id().into(),
            tank_id: "tank".into(),
            conversation_id: "conv".into(),
            p_tags: vec![hex(&worker)],
            body: serde_json::json!({"task": "other", "hops": 0}),
        };
        let (status, mutated) = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &second,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap();
        assert_eq!(status, CoordStatus::Rejected);
        assert!(!mutated);
        assert_eq!(std::fs::read(&marker).unwrap(), original);
    }

    #[test]
    fn hop_limit_on_inbound_delegate_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = CoordJournal::open(tmp.path()).unwrap();
        let leader = Keys::generate();
        let worker = Keys::generate();
        let peers = CoordPeers {
            leader_pubkey_hex: hex(&leader),
            worker_pubkey_hex: hex(&worker),
        };
        let msg = CoordMessage {
            sender: hex(&leader),
            phase: CoordPhase::Delegate,
            assignment_id: assignment_id().into(),
            tank_id: "tank".into(),
            conversation_id: "conv".into(),
            p_tags: vec![hex(&worker)],
            body: serde_json::json!({"task": "x", "hops": 1}),
        };
        let err = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &msg,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap_err();
        assert!(err.to_string().contains("hop limit"));
        assert!(journal.get(&msg.assignment_id).unwrap().is_none());
    }

    #[test]
    fn string_hops_on_delegate_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = CoordJournal::open(tmp.path()).unwrap();
        let leader = Keys::generate();
        let worker = Keys::generate();
        let peers = CoordPeers {
            leader_pubkey_hex: hex(&leader),
            worker_pubkey_hex: hex(&worker),
        };
        let msg = CoordMessage {
            sender: hex(&leader),
            phase: CoordPhase::Delegate,
            assignment_id: assignment_id().into(),
            tank_id: "tank".into(),
            conversation_id: "conv".into(),
            p_tags: vec![hex(&worker)],
            body: serde_json::json!({"task": "x", "hops": "1"}),
        };
        let (status, mutated) = handle_coord(
            &journal,
            &peers,
            &worker_bind(&worker),
            &msg,
            Some(tmp.path()),
            Some(tmp.path()),
        )
        .unwrap();
        assert_eq!(status, CoordStatus::Rejected);
        assert!(!mutated);
        assert!(journal.get(&msg.assignment_id).unwrap().is_none());
    }

    #[test]
    fn parse_reads_p_tag() {
        let leader = Keys::generate();
        let worker = Keys::generate();
        let event = publish_coord(
            &leader,
            &worker.public_key(),
            "conv",
            "tank",
            assignment_id(),
            CoordPhase::Delegate,
            &serde_json::json!({"task": "x", "hops": 0}),
        )
        .unwrap();
        let parsed = parse_coord_event(&event).unwrap();
        assert_eq!(parsed.p_tags, vec![hex(&worker)]);
        assert_eq!(parsed.conversation_id, "conv");
        assert_eq!(parsed.tank_id, "tank");
    }
}
