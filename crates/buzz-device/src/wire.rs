//! Signed NIP-44 device request/receipt events. Not NIP-AO frames.

use crate::DeviceError;
use buzz_core::device::{parameter_fingerprint, DeviceOp};
use buzz_core::kind::{KIND_DEVICE_ADVERTISEMENT, KIND_DEVICE_RECEIPT, KIND_DEVICE_REQUEST};
use buzz_core::observer::{decrypt_observer_payload, encrypt_observer_payload};
use nostr::{Event, EventBuilder, Keys, PublicKey, Tag};
use rand::RngExt;
use serde::{Deserialize, Serialize};

/// Controller → device request body (plaintext before NIP-44).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRequest {
    /// Protocol version.
    pub v: u32,
    /// Request id.
    pub request_id: String,
    /// Operation name.
    pub op: String,
    /// Grant generation the actor believes is current.
    pub grant_generation: u64,
    /// Device id.
    pub device_id: String,
    /// Operation params.
    pub params: serde_json::Value,
}

/// Device → controller receipt body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceReceipt {
    /// Protocol version.
    pub v: u32,
    /// Request id.
    pub request_id: String,
    /// Fingerprint of executed (or rejected) params.
    pub fingerprint: String,
    /// Outcome status.
    pub status: ReceiptStatus,
    /// Host-specific evidence.
    #[serde(default)]
    pub evidence: serde_json::Value,
    /// Error text.
    #[serde(default)]
    pub error: Option<String>,
}

/// Receipt status matching the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    /// Success.
    Succeeded,
    /// Failed after or instead of mutation.
    Failed,
    /// Rejected before mutation.
    Rejected,
    /// Same id, different params.
    Conflict,
    /// Crash/reconcile could not prove a safe retry.
    Uncertain,
}

impl ReceiptStatus {
    /// Lowercase status name for CLI errors.
    pub fn status_label(self) -> String {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Conflict => "conflict",
            Self::Uncertain => "uncertain",
        }
        .to_string()
    }
}

/// Mint a request id with current time.
pub fn generate_request_id() -> String {
    let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    format!("{now:013}-{hex}", hex = hex::encode(bytes))
}

/// Publish kind 30180 readiness (plaintext JSON; not a job queue).
pub fn publish_advertisement(
    keys: &Keys,
    device_id: &str,
    grant_generation: u64,
) -> Result<Event, DeviceError> {
    let body = serde_json::json!({
        "device_id": device_id,
        "online": true,
        "protocol_version": buzz_core::device::DEVICE_PROTOCOL_VERSION,
        "harnesses": ["fixture-agent"],
        "setup_readiness": "ready",
        "grant_generation": grant_generation,
    });
    EventBuilder::new(
        nostr::Kind::Custom(KIND_DEVICE_ADVERTISEMENT as u16),
        serde_json::to_string(&body).map_err(|e| DeviceError::Transport(e.to_string()))?,
    )
    .tags([Tag::parse(["d", device_id]).map_err(|e| DeviceError::Transport(e.to_string()))?])
    .sign_with_keys(keys)
    .map_err(|e| DeviceError::Transport(e.to_string()))
}

/// Encrypt and sign a kind 43200 request addressed to the device.
pub fn publish_request(
    keys: &Keys,
    device_pubkey: &PublicKey,
    device_id: &str,
    request: &DeviceRequest,
) -> Result<Event, DeviceError> {
    let ciphertext = encrypt_observer_payload(keys, device_pubkey, request)
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    EventBuilder::new(nostr::Kind::Custom(KIND_DEVICE_REQUEST as u16), ciphertext)
        .tags([
            Tag::public_key(*device_pubkey),
            Tag::parse(["d", request.request_id.as_str()])
                .map_err(|e| DeviceError::Transport(e.to_string()))?,
            Tag::parse(["device", device_id]).map_err(|e| DeviceError::Transport(e.to_string()))?,
            Tag::parse(["op", request.op.as_str()])
                .map_err(|e| DeviceError::Transport(e.to_string()))?,
        ])
        .sign_with_keys(keys)
        .map_err(|e| DeviceError::Transport(e.to_string()))
}

/// Encrypt and sign a kind 43201 receipt addressed to the actor.
pub fn publish_receipt(
    keys: &Keys,
    actor_pubkey: &PublicKey,
    device_id: &str,
    receipt: &DeviceReceipt,
) -> Result<Event, DeviceError> {
    let ciphertext = encrypt_observer_payload(keys, actor_pubkey, receipt)
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    EventBuilder::new(nostr::Kind::Custom(KIND_DEVICE_RECEIPT as u16), ciphertext)
        .tags([
            Tag::public_key(*actor_pubkey),
            Tag::parse(["d", receipt.request_id.as_str()])
                .map_err(|e| DeviceError::Transport(e.to_string()))?,
            Tag::parse(["device", device_id]).map_err(|e| DeviceError::Transport(e.to_string()))?,
        ])
        .sign_with_keys(keys)
        .map_err(|e| DeviceError::Transport(e.to_string()))
}

/// Decrypt a request event for this device.
pub fn decrypt_request(keys: &Keys, event: &Event) -> Result<DeviceRequest, DeviceError> {
    decrypt_observer_payload(keys, event).map_err(|e| DeviceError::Transport(e.to_string()))
}

/// Decrypt a receipt event for this actor.
pub fn decrypt_receipt(keys: &Keys, event: &Event) -> Result<DeviceReceipt, DeviceError> {
    decrypt_observer_payload(keys, event).map_err(|e| DeviceError::Transport(e.to_string()))
}

/// Fingerprint a request using core canonical JSON.
pub fn fingerprint_request(request: &DeviceRequest) -> Result<String, DeviceError> {
    let op = DeviceOp::parse(&request.op)
        .ok_or_else(|| DeviceError::Transport(format!("unknown op {}", request.op)))?;
    Ok(parameter_fingerprint(
        op,
        &request.device_id,
        request.grant_generation,
        &request.params,
    )?)
}
