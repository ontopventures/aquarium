//! Aquarium device-command host: grants, journal, git worktree, agent cwd.
//!
//! The controller (`ctl`) never runs git or spawns agents. The host (`serve`)
//! is the sole process owner. A local NIP-01 multiplexer is a transport
//! fixture, not isolated Buzz-relay acceptance.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod adapter;
mod coord;
mod git_checkout;
mod journal;
mod mux;
mod path_guard;
mod service;
mod session;
mod wire;

pub use adapter::{
    capabilities_from_inspect_evidence, op_result_from_receipt, AdapterSource, DeviceCapabilities,
    DeviceOpResult,
};
pub use coord::{
    coord_filter, handle_coord, parse_coord_event, publish_coord, CoordBind, CoordJournal,
    CoordMessage,
};
pub use git_checkout::{create_worktree, list_worktrees, CheckoutEvidence};
pub use journal::{Journal, JournalEntry, RequestState};
pub use mux::{bind_local, run_mux, run_mux_listener};
pub use path_guard::resolve_under_root;
pub use service::{handle_request, DeviceService, HandleOutcome};
pub use session::{cancel_session, run_agent_fixture, spawn_fixture_agent, SessionEvidence};
pub use wire::{
    decrypt_receipt, decrypt_request, fingerprint_request, generate_request_id,
    publish_advertisement, publish_receipt, publish_request, DeviceReceipt, DeviceRequest,
    ReceiptStatus,
};

use buzz_core::device::DeviceGrant;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Host-side errors. Never converted into a silent local-run success.
#[derive(Debug, Error)]
pub enum DeviceError {
    /// Protocol or authorization failure.
    #[error(transparent)]
    Protocol(#[from] buzz_core::device::DeviceProtocolError),
    /// Filesystem or path fence failure.
    #[error("{0}")]
    Path(String),
    /// Git invocation failed or timed out.
    #[error("git: {0}")]
    Git(String),
    /// Agent process failed.
    #[error("agent: {0}")]
    Agent(String),
    /// Durable journal I/O.
    #[error("journal: {0}")]
    Journal(String),
    /// Wire/encryption/transport.
    #[error("transport: {0}")]
    Transport(String),
    /// Grant file missing or invalid.
    #[error("grant: {0}")]
    Grant(String),
}

/// On-disk grant document (execution authority).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantFile {
    /// Stable device id.
    pub device_id: String,
    /// Host service pubkey hex.
    pub device_pubkey_hex: String,
    /// Owner pubkey hex.
    pub owner_pubkey_hex: String,
    /// Additional actor pubkeys hex.
    #[serde(default)]
    pub actor_pubkeys: Vec<String>,
    /// Absolute allowed roots.
    pub allowed_roots: Vec<String>,
    /// Generation for revoke-by-replace.
    pub generation: u64,
    /// Optional expiry unix ms.
    #[serde(default)]
    pub expires_at_ms: Option<u64>,
    /// Explicit revocation.
    #[serde(default)]
    pub revoked: bool,
}

impl GrantFile {
    /// Convert to the core grant type.
    pub fn to_grant(&self) -> DeviceGrant {
        DeviceGrant {
            device_id: self.device_id.clone(),
            device_pubkey_hex: self.device_pubkey_hex.clone(),
            owner_pubkey_hex: self.owner_pubkey_hex.clone(),
            actor_pubkeys: self.actor_pubkeys.clone(),
            allowed_roots: self.allowed_roots.clone(),
            generation: self.generation,
            expires_at_ms: self.expires_at_ms,
            revoked: self.revoked,
        }
    }

    /// Load from JSON.
    pub fn load(path: &Path) -> Result<Self, DeviceError> {
        let bytes = std::fs::read(path).map_err(|e| DeviceError::Grant(e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| DeviceError::Grant(e.to_string()))
    }

    /// Write JSON.
    pub fn save(&self, path: &Path) -> Result<(), DeviceError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| DeviceError::Grant(e.to_string()))?;
        }
        let body =
            serde_json::to_vec_pretty(self).map_err(|e| DeviceError::Grant(e.to_string()))?;
        std::fs::write(path, body).map_err(|e| DeviceError::Grant(e.to_string()))
    }
}

/// Isolated prototype state directory (journal + pids). Not Buzz/Orca app data.
pub fn ensure_state_dir(path: &Path) -> Result<PathBuf, DeviceError> {
    std::fs::create_dir_all(path).map_err(|e| DeviceError::Path(e.to_string()))?;
    Ok(path.to_path_buf())
}
