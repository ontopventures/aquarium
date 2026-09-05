//! Durable per-request journal. Source of truth for retry and crash recovery.

use crate::DeviceError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Journaled request lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestState {
    /// Accepted, not yet executing.
    Accepted,
    /// Mutation in progress.
    Executing,
    /// Terminal success.
    Succeeded,
    /// Terminal failure with no mutation or failed mutation.
    Failed,
    /// Authorization or validation reject (no mutation).
    Rejected,
    /// Same request id, different fingerprint.
    Conflict,
    /// Crash left an outcome that must not be blindly retried.
    Uncertain,
}

/// One durable request row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Request id.
    pub request_id: String,
    /// Parameter fingerprint.
    pub fingerprint: String,
    /// Lifecycle state.
    pub state: RequestState,
    /// Wire op name.
    pub op: String,
    /// Actor pubkey hex.
    pub actor_pubkey_hex: String,
    /// Grant generation used.
    pub grant_generation: u64,
    /// Outcome payload (receipt body).
    #[serde(default)]
    pub outcome: serde_json::Value,
    /// Error message when failed/rejected.
    #[serde(default)]
    pub error: Option<String>,
    /// Original params, used to reconcile after a crash.
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Filesystem-backed journal (`state_dir/journal/{id}.json`).
pub struct Journal {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl Journal {
    /// Open or create a journal directory.
    pub fn open(state_dir: &Path) -> Result<Self, DeviceError> {
        let dir = state_dir.join("journal");
        std::fs::create_dir_all(&dir).map_err(|e| DeviceError::Journal(e.to_string()))?;
        Ok(Self {
            dir,
            lock: Mutex::new(()),
        })
    }

    fn path_for(&self, request_id: &str) -> PathBuf {
        self.dir.join(format!("{request_id}.json"))
    }

    /// Load one row.
    pub fn get(&self, request_id: &str) -> Result<Option<JournalEntry>, DeviceError> {
        let path = self.path_for(request_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(|e| DeviceError::Journal(e.to_string()))?;
        let entry =
            serde_json::from_slice(&bytes).map_err(|e| DeviceError::Journal(e.to_string()))?;
        Ok(Some(entry))
    }

    /// Atomically persist a row.
    pub fn put(&self, entry: &JournalEntry) -> Result<(), DeviceError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|e| DeviceError::Journal(e.to_string()))?;
        let path = self.path_for(&entry.request_id);
        let tmp = path.with_extension("json.tmp");
        let body =
            serde_json::to_vec_pretty(entry).map_err(|e| DeviceError::Journal(e.to_string()))?;
        std::fs::write(&tmp, body).map_err(|e| DeviceError::Journal(e.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|e| DeviceError::Journal(e.to_string()))?;
        Ok(())
    }

    /// Load every row (startup reconcile).
    pub fn load_all(&self) -> Result<HashMap<String, JournalEntry>, DeviceError> {
        let mut out = HashMap::new();
        let entries =
            std::fs::read_dir(&self.dir).map_err(|e| DeviceError::Journal(e.to_string()))?;
        for entry in entries {
            let entry = entry.map_err(|e| DeviceError::Journal(e.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|e| DeviceError::Journal(e.to_string()))?;
            let row: JournalEntry =
                serde_json::from_slice(&bytes).map_err(|e| DeviceError::Journal(e.to_string()))?;
            out.insert(row.request_id.clone(), row);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = Journal::open(tmp.path()).unwrap();
        let entry = JournalEntry {
            request_id: "1700000000000-0123456789abcdef0123456789abcdef".into(),
            fingerprint: "abc".into(),
            state: RequestState::Succeeded,
            op: "create_checkout".into(),
            actor_pubkey_hex: "aa".into(),
            grant_generation: 1,
            outcome: serde_json::json!({"ok": true}),
            error: None,
            params: serde_json::json!({}),
        };
        journal.put(&entry).unwrap();
        let loaded = journal.get(&entry.request_id).unwrap().unwrap();
        assert_eq!(loaded.state, RequestState::Succeeded);
        assert_eq!(loaded.fingerprint, "abc");
    }
}
