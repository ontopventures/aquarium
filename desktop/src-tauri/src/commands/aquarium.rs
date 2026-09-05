//! Aquarium desktop adapter: device request path + Linear OS secret slot.
//!
//! No API keys in logs. No general secret getter for the renderer.
//! Linear HTTP to Linear.app waits for authorized credentials; isolated
//! `lin_fixture_` keys exercise connect/search without a live workspace.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::app_state::AppState;
use crate::secret_store::SecretStore;

/// Keychain blob slot. Never the identity nsec key.
pub const LINEAR_SECRET_SLOT: &str = "aquarium-linear-api-key";
const FIXTURE_PREFIX: &str = "lin_fixture_";
const IDENTITY_SLOT: &str = "identity";

/// Linear connection DTO. Never includes the API key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinearConnection {
    /// Provenance.
    pub source: String,
    /// Whether a usable Linear client is connected.
    pub connected: bool,
    /// Workspace label when connected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_name: Option<String>,
    /// Human-readable status. Must not contain the key.
    pub message: String,
}

/// Linear issue DTO.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinearIssue {
    /// Provenance.
    pub source: String,
    /// Linear issue id.
    pub id: String,
    /// Display identifier (e.g. AQU-1).
    pub identifier: String,
    /// Title.
    pub title: String,
    /// Workflow state.
    pub status: String,
    /// Optional project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    /// Optional URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Optional bound tank.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tank_id: Option<String>,
}

/// In-memory or OS-backed secret slot. Tests use a map; production uses keyring.
pub trait LinearSecretSlot {
    /// Persist the key. Errors must not echo the value.
    fn store(&self, value: &str) -> Result<(), String>;
    /// Load the key for native Linear operations only. Never returned to JS.
    fn load(&self) -> Result<Option<String>, String>;
    /// Forget the key.
    fn delete(&self) -> Result<(), String>;
}

struct KeyringLinearSlot;

impl LinearSecretSlot for KeyringLinearSlot {
    fn store(&self, value: &str) -> Result<(), String> {
        if LINEAR_SECRET_SLOT == IDENTITY_SLOT {
            return Err("refusing to store Linear key in the identity nsec slot".into());
        }
        SecretStore::shared(crate::app_state::keyring_service())
            .store(LINEAR_SECRET_SLOT, value)
            .map_err(|_| "linear secret store unavailable".to_string())
    }

    fn load(&self) -> Result<Option<String>, String> {
        SecretStore::shared(crate::app_state::keyring_service())
            .load(LINEAR_SECRET_SLOT)
            .map_err(|_| "linear secret store unavailable".to_string())
    }

    fn delete(&self) -> Result<(), String> {
        SecretStore::shared(crate::app_state::keyring_service())
            .delete(LINEAR_SECRET_SLOT)
            .map_err(|_| "linear secret store unavailable".to_string())
    }
}

fn is_fixture_key(key: &str) -> bool {
    key.starts_with(FIXTURE_PREFIX)
}

fn connection_from_key(key: Option<&str>) -> LinearConnection {
    match key {
        None => LinearConnection {
            source: "linear".into(),
            connected: false,
            workspace_name: None,
            message: "not connected".into(),
        },
        Some(key) if is_fixture_key(key) => LinearConnection {
            source: "linear".into(),
            connected: true,
            workspace_name: Some("Aquarium fixture".into()),
            message: "fixture workspace".into(),
        },
        Some(_) => LinearConnection {
            source: "linear".into(),
            connected: false,
            workspace_name: None,
            message: "waiting for authorized Linear credentials".into(),
        },
    }
}

fn fixture_issues() -> Vec<LinearIssue> {
    vec![LinearIssue {
        source: "linear".into(),
        id: "fixture-issue-1".into(),
        identifier: "AQU-1".into(),
        title: "Fixture tank".into(),
        status: "Todo".into(),
        project_name: Some("Aquarium".into()),
        url: None,
        tank_id: None,
    }]
}

fn json_has_secret(value: &Value, secret: &str) -> bool {
    let encoded = value.to_string();
    !secret.is_empty() && encoded.contains(secret)
}

/// Store a Linear personal key in the OS keyring. The key is never returned.
#[tauri::command]
pub fn aquarium_linear_connect(api_key: String) -> Result<LinearConnection, String> {
    let key = api_key.trim().to_string();
    if key.is_empty() {
        return Err("linear api key is required".into());
    }
    KeyringLinearSlot.store(&key)?;
    let conn = connection_from_key(Some(&key));
    if json_has_secret(&serde_json::to_value(&conn).unwrap_or(Value::Null), &key) {
        return Err("linear connect refused to emit a credential".into());
    }
    Ok(conn)
}

/// Remove the Linear key from the OS keyring.
#[tauri::command]
pub fn aquarium_linear_disconnect() -> Result<LinearConnection, String> {
    KeyringLinearSlot.delete()?;
    Ok(connection_from_key(None))
}

/// Connection status without reading the key into the renderer.
#[tauri::command]
pub fn aquarium_linear_connection() -> Result<LinearConnection, String> {
    let key = KeyringLinearSlot.load()?;
    let conn = connection_from_key(key.as_deref());
    if let Some(secret) = key.as_deref() {
        if json_has_secret(&serde_json::to_value(&conn).unwrap_or(Value::Null), secret) {
            return Err("linear connection refused to emit a credential".into());
        }
    }
    Ok(conn)
}

/// Search issues using the stored key natively. Fixture keys return isolated data.
#[tauri::command]
pub fn aquarium_linear_search_issues(query: String) -> Result<Vec<LinearIssue>, String> {
    let key = KeyringLinearSlot
        .load()?
        .ok_or_else(|| "linear is not connected".to_string())?;
    if is_fixture_key(&key) {
        let q = query.to_ascii_lowercase();
        return Ok(fixture_issues()
            .into_iter()
            .filter(|issue| {
                q.is_empty()
                    || issue.title.to_ascii_lowercase().contains(&q)
                    || issue.identifier.to_ascii_lowercase().contains(&q)
            })
            .collect());
    }
    Err("waiting for authorized Linear credentials".into())
}

/// Fetch one issue using the stored key natively.
#[tauri::command]
pub fn aquarium_linear_get_issue(id: String) -> Result<Option<LinearIssue>, String> {
    let key = KeyringLinearSlot
        .load()?
        .ok_or_else(|| "linear is not connected".to_string())?;
    if is_fixture_key(&key) {
        return Ok(fixture_issues()
            .into_iter()
            .find(|issue| issue.id == id || issue.identifier == id));
    }
    Err("waiting for authorized Linear credentials".into())
}

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
    let request = buzz_device_pkg::device_request_from_checkout(
        &buzz_device_pkg::CreateCheckoutInput {
            tank_id: input.tank_id,
            device_id: input.device_id,
            repository_id: input.repository_id,
            branch: input.branch,
            relpath: input.relpath,
            request_id: input.request_id,
        },
        input.grant_generation,
    )
    .map_err(|e| e.to_string())?;
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
    let request = buzz_device_pkg::device_request_from_start(
        &buzz_device_pkg::StartSessionInput {
            tank_id: input.tank_id,
            device_id: input.device_id,
            checkout_path: input.checkout_path,
            instance_id: input.instance_id,
            request_id: input.request_id,
        },
        input.grant_generation,
    )
    .map_err(|e| e.to_string())?;
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
    let request = buzz_device_pkg::device_request_from_cancel(
        &buzz_device_pkg::CancelSessionInput {
            device_id: input.device_id,
            session_id: input.session_id,
            request_id: input.request_id,
        },
        input.grant_generation,
    )
    .map_err(|e| e.to_string())?;
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
    use std::sync::Mutex;

    struct MapSlot(Mutex<Option<String>>);

    impl LinearSecretSlot for MapSlot {
        fn store(&self, value: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(value.to_string());
            Ok(())
        }
        fn load(&self) -> Result<Option<String>, String> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn delete(&self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn linear_slot_is_not_identity_nsec() {
        assert_ne!(LINEAR_SECRET_SLOT, IDENTITY_SLOT);
        assert!(!LINEAR_SECRET_SLOT.contains("nsec"));
    }

    #[test]
    fn fixture_connect_does_not_echo_key() {
        let key = "lin_fixture_demo_secret";
        let conn = connection_from_key(Some(key));
        let json = serde_json::to_value(&conn).unwrap();
        assert!(!json_has_secret(&json, key));
        assert!(conn.connected);
        assert_eq!(conn.source, "linear");
    }

    #[test]
    fn unauthorized_real_key_does_not_connect_or_echo() {
        let key = "lin_api_not_authorized_yet";
        let conn = connection_from_key(Some(key));
        let json = serde_json::to_value(&conn).unwrap();
        assert!(!json_has_secret(&json, key));
        assert!(!conn.connected);
        assert!(conn.message.contains("authorized"));
    }

    #[test]
    fn fixture_search_is_isolated() {
        let issues = fixture_issues();
        assert_eq!(issues[0].source, "linear");
        assert!(issues[0].identifier.starts_with("AQU-"));
    }

    #[test]
    fn map_slot_round_trip_delete() {
        let slot = MapSlot(Mutex::new(None));
        slot.store("lin_fixture_x").unwrap();
        assert_eq!(slot.load().unwrap().as_deref(), Some("lin_fixture_x"));
        slot.delete().unwrap();
        assert!(slot.load().unwrap().is_none());
    }
}
