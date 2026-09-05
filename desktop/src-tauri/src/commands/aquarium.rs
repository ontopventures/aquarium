//! Aquarium desktop adapter: device request path.
//!
//! Linear commands live in [`super::aquarium_linear`]. No API keys in logs.
//! No general secret getter for the renderer.

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::State;

use crate::app_state::AppState;

#[derive(Debug, Deserialize)]
pub struct DeviceCheckoutArgs {
    /// Tank identity.
    pub tank_id: String,
    /// Selected host.
    pub device_id: String,
    /// Device pubkey hex.
    pub device_pubkey: String,
    /// Canonical repository identity.
    pub repository_id: String,
    /// Branch.
    pub branch: String,
    /// Checkout relpath.
    pub relpath: String,
    /// Caller-stable request id.
    pub request_id: String,
    /// Grant generation.
    pub grant_generation: u64,
}

#[derive(Debug, Deserialize)]
pub struct DeviceStartArgs {
    /// Tank identity.
    pub tank_id: String,
    /// Selected host.
    pub device_id: String,
    /// Device pubkey hex.
    pub device_pubkey: String,
    /// Host checkout path.
    pub checkout_path: String,
    /// Creature instance.
    pub instance_id: String,
    /// Caller-stable request id.
    pub request_id: String,
    /// Grant generation.
    pub grant_generation: u64,
}

#[derive(Debug, Deserialize)]
pub struct DeviceCancelArgs {
    /// Selected host.
    pub device_id: String,
    /// Device pubkey hex.
    pub device_pubkey: String,
    /// Session id from start.
    pub session_id: String,
    /// Caller-stable request id.
    pub request_id: String,
    /// Grant generation.
    pub grant_generation: u64,
}

fn device_keys(state: &State<'_, AppState>) -> Result<nostr::Keys, String> {
    state
        .keys
        .lock()
        .map_err(|e| e.to_string())
        .map(|guard| guard.clone())
}

fn relay_url() -> String {
    crate::relay::relay_ws_url()
}

fn checkout_device_request(
    input: &DeviceCheckoutArgs,
) -> Result<buzz_device_pkg::DeviceRequest, String> {
    buzz_device_pkg::device_request_from_checkout(
        &buzz_device_pkg::CreateCheckoutInput {
            tank_id: input.tank_id.clone(),
            device_id: input.device_id.clone(),
            repository_id: input.repository_id.clone(),
            branch: input.branch.clone(),
            relpath: input.relpath.clone(),
            request_id: input.request_id.clone(),
        },
        input.grant_generation,
    )
    .map_err(|e| e.to_string())
}

fn start_device_request(input: &DeviceStartArgs) -> Result<buzz_device_pkg::DeviceRequest, String> {
    buzz_device_pkg::device_request_from_start(
        &buzz_device_pkg::StartSessionInput {
            tank_id: input.tank_id.clone(),
            device_id: input.device_id.clone(),
            checkout_path: input.checkout_path.clone(),
            instance_id: input.instance_id.clone(),
            request_id: input.request_id.clone(),
        },
        input.grant_generation,
    )
    .map_err(|e| e.to_string())
}

fn cancel_device_request(
    input: &DeviceCancelArgs,
) -> Result<buzz_device_pkg::DeviceRequest, String> {
    buzz_device_pkg::device_request_from_cancel(
        &buzz_device_pkg::CancelSessionInput {
            device_id: input.device_id.clone(),
            session_id: input.session_id.clone(),
            request_id: input.request_id.clone(),
        },
        input.grant_generation,
    )
    .map_err(|e| e.to_string())
}

/// Inspect host capabilities over the signed device request path.
#[tauri::command]
pub async fn aquarium_device_inspect_capabilities(
    state: State<'_, AppState>,
    device_id: String,
    device_pubkey: String,
    grant_generation: u64,
) -> Result<Value, String> {
    let keys = device_keys(&state)?;
    let request = buzz_device_pkg::DeviceRequest {
        v: buzz_core_pkg::device::DEVICE_PROTOCOL_VERSION,
        request_id: buzz_device_pkg::generate_request_id(),
        op: "inspect_capabilities".into(),
        grant_generation,
        device_id,
        params: json!({}),
    };
    let receipt =
        buzz_device_pkg::submit_device_request(&keys, &device_pubkey, &relay_url(), request)
            .await
            .map_err(|e| e.to_string())?;
    let caps = buzz_device_pkg::capabilities_from_inspect_evidence(&receipt.evidence)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(caps).map_err(|e| e.to_string())
}

/// Create a tank checkout. Caller must supply a stable `request_id`.
#[tauri::command]
pub async fn aquarium_device_create_checkout(
    state: State<'_, AppState>,
    input: DeviceCheckoutArgs,
) -> Result<Value, String> {
    let keys = device_keys(&state)?;
    let request = checkout_device_request(&input)?;
    let receipt =
        buzz_device_pkg::submit_device_request(&keys, &input.device_pubkey, &relay_url(), request)
            .await
            .map_err(|e| e.to_string())?;
    serde_json::to_value(buzz_device_pkg::op_result_from_receipt(&receipt))
        .map_err(|e| e.to_string())
}

/// Inspect a prior request by caller-stable id.
#[tauri::command]
pub async fn aquarium_device_inspect_request(
    state: State<'_, AppState>,
    device_id: String,
    device_pubkey: String,
    request_id: String,
    grant_generation: u64,
) -> Result<Value, String> {
    let keys = device_keys(&state)?;
    let id = buzz_device_pkg::require_caller_request_id(&request_id).map_err(|e| e.to_string())?;
    let request = buzz_device_pkg::DeviceRequest {
        v: buzz_core_pkg::device::DEVICE_PROTOCOL_VERSION,
        request_id: id.clone(),
        op: "inspect_request".into(),
        grant_generation,
        device_id,
        params: json!({ "request_id": id }),
    };
    let receipt =
        buzz_device_pkg::submit_device_request(&keys, &device_pubkey, &relay_url(), request)
            .await
            .map_err(|e| e.to_string())?;
    serde_json::to_value(buzz_device_pkg::op_result_from_receipt(&receipt))
        .map_err(|e| e.to_string())
}

/// Start a session. Caller must supply a stable `request_id`.
#[tauri::command]
pub async fn aquarium_device_start_session(
    state: State<'_, AppState>,
    input: DeviceStartArgs,
) -> Result<Value, String> {
    let keys = device_keys(&state)?;
    let request = start_device_request(&input)?;
    let receipt =
        buzz_device_pkg::submit_device_request(&keys, &input.device_pubkey, &relay_url(), request)
            .await
            .map_err(|e| e.to_string())?;
    serde_json::to_value(buzz_device_pkg::op_result_from_receipt(&receipt))
        .map_err(|e| e.to_string())
}

/// Cancel a session. Caller must supply a stable `request_id`.
#[tauri::command]
pub async fn aquarium_device_cancel_session(
    state: State<'_, AppState>,
    input: DeviceCancelArgs,
) -> Result<Value, String> {
    let keys = device_keys(&state)?;
    let request = cancel_device_request(&input)?;
    let receipt =
        buzz_device_pkg::submit_device_request(&keys, &input.device_pubkey, &relay_url(), request)
            .await
            .map_err(|e| e.to_string())?;
    serde_json::to_value(buzz_device_pkg::op_result_from_receipt(&receipt))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALLER_REQUEST_ID: &str = "1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn checkout_args(request_id: &str, repository_id: &str) -> DeviceCheckoutArgs {
        DeviceCheckoutArgs {
            tank_id: "tank-1".into(),
            device_id: "dev-1".into(),
            device_pubkey: "deadbeef".into(),
            repository_id: repository_id.into(),
            branch: "main".into(),
            relpath: "tanks/t1".into(),
            request_id: request_id.into(),
            grant_generation: 1,
        }
    }

    #[test]
    fn no_aquarium_linear_secret_get() {
        let linear = include_str!("aquarium_linear.rs");
        let production = linear
            .split("#[cfg(test)]")
            .next()
            .expect("production source before tests");
        assert!(
            !production.contains("fn aquarium_linear_secret_get"),
            "renderer must not have a Linear secret getter"
        );
        let commands = include_str!("aquarium.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("device production source");
        assert!(!commands.contains("fn aquarium_linear_secret_get"));
        let handler = include_str!("../lib.rs");
        assert!(
            !handler.contains("aquarium_linear_secret_get"),
            "generate_handler must not register a Linear secret getter"
        );
        assert!(handler.contains("aquarium_linear_connect"));
        assert!(handler.contains("aquarium_linear_connection"));
    }

    #[test]
    fn mutating_device_requests_require_caller_request_id() {
        let empty = checkout_device_request(&checkout_args("", "repo-1")).unwrap_err();
        assert!(empty.contains("request_id"), "{empty}");

        let invalid =
            checkout_device_request(&checkout_args("not-a-request-id", "repo-1")).unwrap_err();
        assert!(invalid.contains("request_id"), "{invalid}");

        let start = start_device_request(&DeviceStartArgs {
            tank_id: "tank-1".into(),
            device_id: "dev-1".into(),
            device_pubkey: "deadbeef".into(),
            checkout_path: "/tmp/checkout".into(),
            instance_id: "inst-1".into(),
            request_id: String::new(),
            grant_generation: 1,
        })
        .unwrap_err();
        assert!(start.contains("request_id"), "{start}");

        let cancel = cancel_device_request(&DeviceCancelArgs {
            device_id: "dev-1".into(),
            device_pubkey: "deadbeef".into(),
            session_id: "sess-1".into(),
            request_id: String::new(),
            grant_generation: 1,
        })
        .unwrap_err();
        assert!(cancel.contains("request_id"), "{cancel}");

        let parsed: Result<DeviceCheckoutArgs, _> = serde_json::from_value(json!({
            "tank_id": "tank-1",
            "device_id": "dev-1",
            "device_pubkey": "deadbeef",
            "repository_id": "repo-1",
            "branch": "main",
            "relpath": "tanks/t1",
            "grant_generation": 1
        }));
        assert!(
            parsed.is_err(),
            "createCheckout args must require request_id"
        );

        let ok = checkout_device_request(&checkout_args(CALLER_REQUEST_ID, "repo-1")).unwrap();
        assert_eq!(ok.request_id, CALLER_REQUEST_ID);
        let retry = checkout_device_request(&checkout_args(CALLER_REQUEST_ID, "repo-1")).unwrap();
        assert_eq!(retry.request_id, ok.request_id);
    }

    #[test]
    fn create_checkout_requires_repository_id() {
        let missing = checkout_device_request(&checkout_args(CALLER_REQUEST_ID, "")).unwrap_err();
        assert!(
            missing.contains("repository_id") && missing.contains("relpath"),
            "empty repository_id must not infer from relpath: {missing}"
        );

        let parsed: Result<DeviceCheckoutArgs, _> = serde_json::from_value(json!({
            "tank_id": "tank-1",
            "device_id": "dev-1",
            "device_pubkey": "deadbeef",
            "branch": "main",
            "relpath": "tanks/t1",
            "request_id": CALLER_REQUEST_ID,
            "grant_generation": 1
        }));
        assert!(
            parsed.is_err(),
            "createCheckout args must require repository_id"
        );

        let ok = checkout_device_request(&checkout_args(CALLER_REQUEST_ID, "repo-1")).unwrap();
        assert_eq!(ok.params["repository_id"], "repo-1");
        assert_eq!(ok.params["repo_relpath"], "repo-1");
        assert_eq!(ok.params["relpath"], "tanks/t1");
        assert_ne!(ok.params["repository_id"], ok.params["relpath"]);
    }
}
