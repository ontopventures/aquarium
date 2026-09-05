//! Desktop-safe typed device adapter. Never falls back to local git or spawn.
//!
//! Shapes match `work/ui-demo/adapter-contract.md`. The UI worktree owns
//! desktop binding; this module is the backend mapping from `buzz-device ctl`
//! receipts. Mock results are not produced here.

use crate::{DeviceError, DeviceReceipt, ReceiptStatus};
use serde::{Deserialize, Serialize};

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
    /// Human-readable message. Never implies local success.
    pub message: String,
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
        message: receipt
            .error
            .clone()
            .unwrap_or_else(|| receipt.status.status_label()),
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
}
