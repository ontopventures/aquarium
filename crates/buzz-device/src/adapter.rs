//! Desktop-safe typed device adapter. Never falls back to local git or spawn.
//!
//! Shapes match `work/ui-demo/adapter-contract.md`. The UI worktree owns
//! desktop binding. This module is the request path: caller-stable
//! `request_id`, required `repository_id` on checkout, and mapping from
//! receipts. Mock results are not produced here. Adapters must not mint a
//! request id; retries reuse the caller's id.

use crate::{
    decrypt_receipt, publish_request, DeviceError, DeviceReceipt, DeviceRequest, ReceiptStatus,
};
use buzz_core::device::{parse_request_timestamp, DEVICE_PROTOCOL_VERSION};
use buzz_core::kind::KIND_DEVICE_RECEIPT;
use buzz_ws_client::{NostrWsConnection, RelayMessage};
use nostr::{Filter, Keys, PublicKey};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{Duration, Instant};

/// Provenance tag. Real adapter results are always [`AdapterSource::Device`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterSource {
    /// UI mock only. This crate must not emit it.
    Mock,
    /// Result of `buzz-device` ops.
    Device,
    /// Linear client. Not this crate.
    Linear,
}

/// Host capabilities returned by `inspectCapabilities`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceCapabilities {
    /// Provenance. Always `device` from this crate.
    pub source: AdapterSource,
    /// Selected execution host.
    pub device_id: String,
    /// Device pubkey for NIP-44 requests.
    pub device_pubkey: String,
    /// Host advertised itself as online.
    pub online: bool,
    /// Device protocol version string.
    pub protocol_version: String,
    /// Advertised harness names.
    pub harnesses: Vec<String>,
    /// Setup readiness.
    pub setup_readiness: String,
    /// Grant generation from capabilities.
    pub grant_generation: u64,
}

/// Outcome of a mutating or inspect op.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceOpResult {
    /// Provenance. Always `device` from this crate.
    pub source: AdapterSource,
    /// Adapter status (receipt mapping).
    pub status: String,
    /// Device request id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Creature/harness session id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Host path after `create_checkout`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    /// Checkout branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Checkout HEAD.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// Canonical bound repository. Never inferred from `relpath`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    /// Human-readable message. Never implies local success.
    pub message: String,
}

/// Require a caller-stable device request id. Empty values fail closed so
/// retries cannot silently mint a second mutation.
pub fn require_caller_request_id(request_id: &str) -> Result<String, DeviceError> {
    let id = request_id.trim();
    if id.is_empty() {
        return Err(DeviceError::Transport(
            "request_id is required; adapter must not mint ids; retries reuse the caller id".into(),
        ));
    }
    parse_request_timestamp(id).map_err(|_| {
        DeviceError::Transport(
            "request_id must match {13-digit-ms}-{32 hex}; retries reuse the caller id".into(),
        )
    })?;
    Ok(id.to_string())
}

/// Require canonical repository identity. Do not infer from checkout relpath.
pub fn require_repository_id(repository_id: &str) -> Result<String, DeviceError> {
    let id = repository_id.trim();
    if id.is_empty() {
        return Err(DeviceError::Transport(
            "repository_id is required; do not infer the repository from relpath".into(),
        ));
    }
    Ok(id.to_string())
}

/// `createCheckout` input. `request_id` is caller-stable.
#[derive(Debug, Clone)]
pub struct CreateCheckoutInput {
    /// Tank identity.
    pub tank_id: String,
    /// Selected execution host.
    pub device_id: String,
    /// Canonical bound repository.
    pub repository_id: String,
    /// Branch name.
    pub branch: String,
    /// Checkout relpath under the allowed root.
    pub relpath: String,
    /// Caller-stable request id.
    pub request_id: String,
}

/// `startSession` input. `request_id` is caller-stable.
#[derive(Debug, Clone)]
pub struct StartSessionInput {
    /// Tank identity.
    pub tank_id: String,
    /// Selected execution host.
    pub device_id: String,
    /// Host checkout path.
    pub checkout_path: String,
    /// Creature instance id.
    pub instance_id: String,
    /// Caller-stable request id.
    pub request_id: String,
}

/// `cancelSession` input. `request_id` is caller-stable.
#[derive(Debug, Clone)]
pub struct CancelSessionInput {
    /// Selected execution host.
    pub device_id: String,
    /// Session id from start.
    pub session_id: String,
    /// Caller-stable request id.
    pub request_id: String,
}

/// Build `create_checkout` params. `repository_id` is the repo identity
/// passed to the host; it is never taken from `relpath`.
pub fn create_checkout_params(
    input: &CreateCheckoutInput,
) -> Result<serde_json::Value, DeviceError> {
    let repository_id = require_repository_id(&input.repository_id)?;
    if input.tank_id.trim().is_empty() || input.device_id.trim().is_empty() {
        return Err(DeviceError::Transport(
            "tank_id and device_id are required; refusing local fallback".into(),
        ));
    }
    Ok(json!({
        "tank_id": input.tank_id,
        "repository_id": repository_id,
        "branch": input.branch,
        "relpath": input.relpath,
        "repo_relpath": repository_id,
        "start_rev": "HEAD",
    }))
}

/// Build a signed request using the **caller** request id.
pub fn device_request_from_checkout(
    input: &CreateCheckoutInput,
    grant_generation: u64,
) -> Result<DeviceRequest, DeviceError> {
    let request_id = require_caller_request_id(&input.request_id)?;
    Ok(DeviceRequest {
        v: DEVICE_PROTOCOL_VERSION,
        request_id,
        op: "create_checkout".into(),
        grant_generation,
        device_id: input.device_id.clone(),
        params: create_checkout_params(input)?,
    })
}

/// Build a signed start-session request using the caller request id.
pub fn device_request_from_start(
    input: &StartSessionInput,
    grant_generation: u64,
) -> Result<DeviceRequest, DeviceError> {
    let request_id = require_caller_request_id(&input.request_id)?;
    if input.device_id.trim().is_empty() || input.checkout_path.trim().is_empty() {
        return Err(DeviceError::Transport(
            "device_id and checkout_path are required; refusing local fallback".into(),
        ));
    }
    Ok(DeviceRequest {
        v: DEVICE_PROTOCOL_VERSION,
        request_id,
        op: "start_session".into(),
        grant_generation,
        device_id: input.device_id.clone(),
        params: json!({
            "checkout_path": input.checkout_path,
            "session_id": input.instance_id,
            "tank_id": input.tank_id,
        }),
    })
}

/// Build a signed cancel-session request using the caller request id.
pub fn device_request_from_cancel(
    input: &CancelSessionInput,
    grant_generation: u64,
) -> Result<DeviceRequest, DeviceError> {
    let request_id = require_caller_request_id(&input.request_id)?;
    if input.device_id.trim().is_empty() || input.session_id.trim().is_empty() {
        return Err(DeviceError::Transport(
            "device_id and session_id are required; refusing local fallback".into(),
        ));
    }
    Ok(DeviceRequest {
        v: DEVICE_PROTOCOL_VERSION,
        request_id,
        op: "cancel_session".into(),
        grant_generation,
        device_id: input.device_id.clone(),
        params: json!({ "session_id": input.session_id }),
    })
}

/// Map inspect-capabilities evidence onto the desktop DTO.
///
/// Refuses to mint a result without a device id and pubkey. Never sets
/// `source` to mock and never invents a local host.
pub fn capabilities_from_inspect_evidence(
    evidence: &serde_json::Value,
) -> Result<DeviceCapabilities, DeviceError> {
    let device_id = evidence
        .get("device_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let device_pubkey = evidence
        .get("device_pubkey_hex")
        .or_else(|| evidence.get("device_pubkey"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if device_id.is_empty() || device_pubkey.is_empty() {
        return Err(DeviceError::Transport(
            "inspect_capabilities missing device_id or device_pubkey; refusing local fallback"
                .into(),
        ));
    }
    let harnesses = evidence
        .get("harnesses")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(DeviceCapabilities {
        source: AdapterSource::Device,
        device_id,
        device_pubkey,
        online: evidence
            .get("online")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        protocol_version: evidence
            .get("protocol_version")
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())
            .or_else(|| {
                evidence
                    .get("protocol_version")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default(),
        harnesses,
        setup_readiness: evidence
            .get("setup_readiness")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        grant_generation: evidence
            .get("grant_generation")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    })
}

/// Map a signed receipt onto the desktop DTO. Offline/missing device stays
/// `failed`/`uncertain` with a message; this never rewrites to local success.
pub fn op_result_from_receipt(receipt: &DeviceReceipt) -> DeviceOpResult {
    let evidence = &receipt.evidence;
    DeviceOpResult {
        source: AdapterSource::Device,
        status: match receipt.status {
            ReceiptStatus::Succeeded => "succeeded",
            ReceiptStatus::Failed => "failed",
            ReceiptStatus::Rejected => "rejected",
            ReceiptStatus::Conflict => "conflict",
            ReceiptStatus::Uncertain => "uncertain",
        }
        .to_string(),
        request_id: Some(receipt.request_id.clone()),
        session_id: evidence
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        worktree_path: evidence
            .get("worktree_path")
            .or_else(|| evidence.get("path"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        branch: evidence
            .get("branch")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        head: evidence
            .get("head")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        repository_id: evidence
            .get("repository_id")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        message: receipt
            .error
            .clone()
            .unwrap_or_else(|| receipt.status.status_label()),
    }
}

/// Publish one device request and wait for the matching receipt.
///
/// Requires a selected `device_pubkey`. Never runs git or agents locally.
pub async fn submit_device_request(
    keys: &Keys,
    device_pubkey_hex: &str,
    relay: &str,
    request: DeviceRequest,
) -> Result<DeviceReceipt, DeviceError> {
    if device_pubkey_hex.trim().is_empty() {
        return Err(DeviceError::Transport(
            "device_pubkey is required; refusing to run locally".into(),
        ));
    }
    let device_pk = PublicKey::from_hex(device_pubkey_hex.trim())
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    let mut conn = NostrWsConnection::connect_authenticated(relay, keys, None)
        .await
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    let filter = Filter::new()
        .kind(nostr::Kind::Custom(KIND_DEVICE_RECEIPT as u16))
        .pubkey(keys.public_key());
    conn.send_raw(&json!(["REQ", "device-out", filter]))
        .await
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    let event = publish_request(keys, &device_pk, &request.device_id, &request)?;
    conn.send_event(event)
        .await
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(DeviceError::Transport(
                "timed out waiting for device receipt; not running locally".into(),
            ));
        }
        match conn
            .next_event(remaining)
            .await
            .map_err(|e| DeviceError::Transport(e.to_string()))?
        {
            RelayMessage::Event { event, .. } => {
                if u32::from(event.kind.as_u16()) != KIND_DEVICE_RECEIPT {
                    continue;
                }
                if event.pubkey != device_pk {
                    continue;
                }
                if let Ok(receipt) = decrypt_receipt(keys, &event) {
                    if receipt.request_id == request.request_id {
                        return Ok(receipt);
                    }
                }
            }
            RelayMessage::Eose { .. } => {}
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_mapping_is_device_source_and_refuses_empty() {
        let err = capabilities_from_inspect_evidence(&serde_json::json!({})).unwrap_err();
        assert!(err.to_string().contains("refusing local fallback"));

        let caps = capabilities_from_inspect_evidence(&serde_json::json!({
            "device_id": "dev-1",
            "device_pubkey_hex": "aa".repeat(32),
            "online": true,
            "protocol_version": 1,
            "harnesses": ["fixture-agent"],
            "setup_readiness": "ready",
            "grant_generation": 3,
        }))
        .unwrap();
        assert_eq!(caps.source, AdapterSource::Device);
        assert_ne!(caps.source, AdapterSource::Mock);
        assert_eq!(caps.device_id, "dev-1");
        assert_eq!(caps.grant_generation, 3);
        assert!(caps.online);
    }

    #[test]
    fn receipt_mapping_never_marks_mock_or_local_success_on_failure() {
        let receipt = DeviceReceipt {
            v: 1,
            request_id: "1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            fingerprint: "fp".into(),
            status: ReceiptStatus::Failed,
            evidence: serde_json::Value::Null,
            error: Some("device offline".into()),
        };
        let result = op_result_from_receipt(&receipt);
        assert_eq!(result.source, AdapterSource::Device);
        assert_eq!(result.status, "failed");
        assert_eq!(result.message, "device offline");
        assert_eq!(
            result.request_id.as_deref(),
            Some("1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    #[test]
    fn mutating_ops_refuse_to_mint_request_id() {
        let err = require_caller_request_id("").unwrap_err();
        assert!(err.to_string().contains("must not mint"));
        let err = require_caller_request_id("not-an-id").unwrap_err();
        assert!(err.to_string().contains("13-digit-ms"));
        let id =
            require_caller_request_id("1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let again =
            require_caller_request_id("1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        assert_eq!(id, again);
    }

    #[test]
    fn checkout_requires_repository_id_not_relpath() {
        let err = require_repository_id("").unwrap_err();
        assert!(err.to_string().contains("relpath"));
        let input = CreateCheckoutInput {
            tank_id: "tank-1".into(),
            device_id: "dev-1".into(),
            repository_id: String::new(),
            branch: "main".into(),
            relpath: "tanks/t1".into(),
            request_id: "1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        };
        let err = create_checkout_params(&input).unwrap_err();
        assert!(err.to_string().contains("repository_id"));
        let mut ok = input.clone();
        ok.repository_id = "repo".into();
        let params = create_checkout_params(&ok).unwrap();
        assert_eq!(params["repository_id"], "repo");
        assert_eq!(params["repo_relpath"], "repo");
        assert_eq!(params["relpath"], "tanks/t1");
        let request = device_request_from_checkout(&ok, 1).unwrap();
        assert_eq!(
            request.request_id,
            "1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        let retry = device_request_from_checkout(&ok, 1).unwrap();
        assert_eq!(retry.request_id, request.request_id);
    }

    #[test]
    fn start_and_cancel_reuse_caller_request_id() {
        let start = StartSessionInput {
            tank_id: "tank-1".into(),
            device_id: "dev-1".into(),
            checkout_path: "/tmp/tanks/t1".into(),
            instance_id: "inst-1".into(),
            request_id: "1788581600001-cccccccccccccccccccccccccccccccc".into(),
        };
        let a = device_request_from_start(&start, 1).unwrap();
        let b = device_request_from_start(&start, 1).unwrap();
        assert_eq!(a.request_id, b.request_id);
        let cancel = CancelSessionInput {
            device_id: "dev-1".into(),
            session_id: "sess-1".into(),
            request_id: "1788581600001-cccccccccccccccccccccccccccccccc".into(),
        };
        let c = device_request_from_cancel(&cancel, 1).unwrap();
        assert_eq!(c.request_id, a.request_id);
        assert_eq!(c.params["session_id"], "sess-1");
    }
}
