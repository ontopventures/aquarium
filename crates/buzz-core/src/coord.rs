//! Same-tank leader/worker assignment protocol. Zero I/O.
//!
//! Assignment identity is distinct from chat text and Git/worktree identity.
//! Transport uses Buzz stream messages (kind:9); this module only validates
//! peers, hops, event budgets and fingerprints.

use crate::device::{parse_request_timestamp, DeviceProtocolError};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Wire version for coordination JSON bodies.
pub const COORD_PROTOCOL_VERSION: u32 = 1;
/// One delegation hop only (leader → worker). Nested worker→worker is out of demo.
pub const COORD_MAX_HOPS: u32 = 1;
/// Hard cap on signed events processed per assignment (loop bound).
pub const COORD_MAX_EVENTS: u32 = 8;

/// Coordination protocol errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoordError {
    /// Assignment id is not the device request-id grammar.
    #[error("invalid assignment id")]
    InvalidAssignmentId,
    /// Sender is not the leader or worker for this assignment.
    #[error("sender is not an authorized peer for this assignment")]
    UnauthorizedPeer,
    /// Worker tried to delegate (hop budget).
    #[error("delegation hop limit exceeded")]
    HopLimit,
    /// Too many events for one assignment.
    #[error("assignment event budget exceeded")]
    EventBudget,
    /// Phase is not allowed from this sender.
    #[error("coordination phase is not allowed from this sender")]
    WrongPhase,
    /// JSON could not be canonicalized.
    #[error("coordination parameters are not canonical JSON")]
    InvalidParams,
}

/// Coordination phase carried in the `coord` tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordPhase {
    /// Leader assigns work.
    Delegate,
    /// Worker received the assignment.
    Ack,
    /// Worker finished or failed.
    Result,
    /// Leader automatically continues after a result.
    Continue,
}

impl CoordPhase {
    /// Parse a `coord` tag value.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "delegate" => Some(Self::Delegate),
            "ack" => Some(Self::Ack),
            "result" => Some(Self::Result),
            "continue" => Some(Self::Continue),
            _ => None,
        }
    }

    /// Tag value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Delegate => "delegate",
            Self::Ack => "ack",
            Self::Result => "result",
            Self::Continue => "continue",
        }
    }
}

/// Runtime-enforced assignment outcome. Model-authored text is not authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordStatus {
    /// Worker completed the bounded task.
    Succeeded,
    /// Worker failed.
    Failed,
    /// Rejected before work.
    Rejected,
    /// Same assignment id already completed.
    Duplicate,
}

impl CoordStatus {
    /// Parse status from JSON.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "rejected" => Some(Self::Rejected),
            "duplicate" => Some(Self::Duplicate),
            _ => None,
        }
    }

    /// Wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Duplicate => "duplicate",
        }
    }
}

/// Peers bound to one assignment.
#[derive(Debug, Clone)]
pub struct CoordPeers {
    /// Leader pubkey hex.
    pub leader_pubkey_hex: String,
    /// Worker pubkey hex.
    pub worker_pubkey_hex: String,
}

/// Validate assignment id grammar (same as device request ids).
pub fn check_assignment_id(assignment_id: &str) -> Result<(), CoordError> {
    parse_request_timestamp(assignment_id).map_err(|_| CoordError::InvalidAssignmentId)?;
    Ok(())
}

/// Authorize a sender for a phase. Unrelated pubkeys fail closed.
pub fn authorize_coord_peer(
    peers: &CoordPeers,
    sender_pubkey_hex: &str,
    phase: CoordPhase,
    hops: u32,
    events_seen: u32,
) -> Result<(), CoordError> {
    if events_seen >= COORD_MAX_EVENTS {
        return Err(CoordError::EventBudget);
    }
    if hops >= COORD_MAX_HOPS && phase == CoordPhase::Delegate {
        return Err(CoordError::HopLimit);
    }
    let sender = sender_pubkey_hex.trim();
    let leader = peers.leader_pubkey_hex.trim();
    let worker = peers.worker_pubkey_hex.trim();
    let leader_ok = eq_hex(sender, leader);
    let worker_ok = eq_hex(sender, worker);
    if !leader_ok && !worker_ok {
        return Err(CoordError::UnauthorizedPeer);
    }
    let allowed = match phase {
        CoordPhase::Delegate | CoordPhase::Continue => leader_ok,
        CoordPhase::Ack | CoordPhase::Result => worker_ok,
    };
    if allowed {
        Ok(())
    } else {
        Err(CoordError::WrongPhase)
    }
}

/// Fingerprint of `{tank_id, conversation_id, worker, task}` (no secrets).
pub fn assignment_fingerprint(
    tank_id: &str,
    conversation_id: &str,
    worker_pubkey_hex: &str,
    task: &str,
) -> Result<String, CoordError> {
    let mut body = serde_json::Map::new();
    body.insert(
        "conversation_id".into(),
        serde_json::Value::String(conversation_id.to_string()),
    );
    body.insert(
        "tank_id".into(),
        serde_json::Value::String(tank_id.to_string()),
    );
    body.insert("task".into(), serde_json::Value::String(task.to_string()));
    body.insert(
        "worker_pubkey_hex".into(),
        serde_json::Value::String(worker_pubkey_hex.trim().to_ascii_lowercase()),
    );
    let encoded = serde_json::to_string(&serde_json::Value::Object(body))
        .map_err(|_| CoordError::InvalidParams)?;
    Ok(hex::encode(Sha256::digest(encoded.as_bytes())))
}

fn eq_hex(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

impl From<DeviceProtocolError> for CoordError {
    fn from(_: DeviceProtocolError) -> Self {
        Self::InvalidAssignmentId
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peers() -> CoordPeers {
        CoordPeers {
            leader_pubkey_hex: "aa".repeat(32),
            worker_pubkey_hex: "bb".repeat(32),
        }
    }

    #[test]
    fn unrelated_sender_is_rejected() {
        let err = authorize_coord_peer(&peers(), &"cc".repeat(32), CoordPhase::Delegate, 0, 0);
        assert_eq!(err, Err(CoordError::UnauthorizedPeer));
    }

    #[test]
    fn worker_cannot_delegate() {
        let err = authorize_coord_peer(&peers(), &"bb".repeat(32), CoordPhase::Delegate, 0, 0);
        assert_eq!(err, Err(CoordError::WrongPhase));
    }

    #[test]
    fn hop_limit_blocks_nested_delegate() {
        let err = authorize_coord_peer(&peers(), &"aa".repeat(32), CoordPhase::Delegate, 1, 0);
        assert_eq!(err, Err(CoordError::HopLimit));
    }

    #[test]
    fn event_budget_is_hard_capped() {
        let err = authorize_coord_peer(
            &peers(),
            &"bb".repeat(32),
            CoordPhase::Ack,
            0,
            COORD_MAX_EVENTS,
        );
        assert_eq!(err, Err(CoordError::EventBudget));
    }

    #[test]
    fn fingerprints_are_stable() {
        let a = assignment_fingerprint("t", "c", "BB", "do").unwrap();
        let b = assignment_fingerprint("t", "c", "bb", "do").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, assignment_fingerprint("t", "c", "bb", "other").unwrap());
    }
}
