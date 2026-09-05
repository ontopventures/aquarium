//! Aquarium device-command protocol types.
//!
//! Zero I/O: request identity, parameter fingerprints, grant checks, and
//! relative-path grammar. Filesystem canonicalize and Git execution live in
//! `buzz-device`. These kinds are a separate command plane from NIP-AO
//! (kind 24200), NIP-AB pairing, and unused job kinds 43001–43006.

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Wire version for device request/receipt JSON bodies.
pub const DEVICE_PROTOCOL_VERSION: u32 = 1;
/// Reject request timestamps more than five minutes in the future.
pub const DEVICE_OPERATION_FUTURE_SKEW_MS: u64 = 5 * 60 * 1000;
/// Reject request timestamps older than 24 hours.
pub const DEVICE_MAX_NEW_OPERATION_AGE_MS: u64 = 24 * 60 * 60 * 1000;

const REQUEST_ID_PATTERN_LEN: usize = 13 + 1 + 32;

/// Device-command protocol errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DeviceProtocolError {
    /// Request id is not `{13-digit-ms}-{32 lowercase hex}`.
    #[error("invalid device request id")]
    InvalidRequestId,
    /// Request timestamp is too far in the future.
    #[error("device request timestamp is in the future")]
    RequestInFuture,
    /// Request timestamp is older than [`DEVICE_MAX_NEW_OPERATION_AGE_MS`].
    #[error("device request has expired")]
    RequestExpired,
    /// Grant is revoked or generation does not match.
    #[error("device grant is not active")]
    GrantInactive,
    /// Actor pubkey is not on the grant.
    #[error("actor is not authorized for this device")]
    UnauthorizedActor,
    /// Target device pubkey does not match the grant.
    #[error("request targets the wrong device")]
    WrongDevice,
    /// Relative path uses `..`, is absolute, or is empty.
    #[error("path is outside the allowed root grammar")]
    UnsafeRelativePath,
    /// JSON could not be canonicalized.
    #[error("device parameters are not canonical JSON")]
    InvalidParams,
}

/// Typed device operations. Names are the v1 contract, not prior APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceOp {
    /// Read capability/readiness metadata.
    InspectCapabilities,
    /// Create a git branch/worktree under an allowed root.
    CreateCheckout,
    /// Read a prior request's durable receipt.
    InspectRequest,
    /// Start one agent process whose cwd is an existing checkout.
    StartSession,
    /// Cancel that agent process.
    CancelSession,
}

impl DeviceOp {
    /// Wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InspectCapabilities => "inspect_capabilities",
            Self::CreateCheckout => "create_checkout",
            Self::InspectRequest => "inspect_request",
            Self::StartSession => "start_session",
            Self::CancelSession => "cancel_session",
        }
    }

    /// Parse a wire name. Unknown names are errors, not local fallbacks.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "inspect_capabilities" => Some(Self::InspectCapabilities),
            "create_checkout" => Some(Self::CreateCheckout),
            "inspect_request" => Some(Self::InspectRequest),
            "start_session" => Some(Self::StartSession),
            "cancel_session" => Some(Self::CancelSession),
            _ => None,
        }
    }
}

/// Owner grant that authorizes actors to command one execution host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceGrant {
    /// Stable device id (not a pubkey, not a tank id).
    pub device_id: String,
    /// Execution-host service pubkey, lowercase hex.
    pub device_pubkey_hex: String,
    /// Owner who issued the grant, lowercase hex.
    pub owner_pubkey_hex: String,
    /// Actor pubkeys allowed to issue commands, lowercase hex.
    pub actor_pubkeys: Vec<String>,
    /// Absolute allowed project roots (strings only; canonicalize at the host).
    pub allowed_roots: Vec<String>,
    /// Generation; replace to revoke.
    pub generation: u64,
    /// Optional grant expiry (unix ms).
    pub expires_at_ms: Option<u64>,
    /// Explicit revocation flag.
    pub revoked: bool,
}

impl DeviceGrant {
    /// Authorize `actor` to command `device_pubkey` at `now_ms`.
    pub fn authorize(
        &self,
        actor_pubkey_hex: &str,
        device_pubkey_hex: &str,
        now_ms: u64,
    ) -> Result<(), DeviceProtocolError> {
        if self.revoked {
            return Err(DeviceProtocolError::GrantInactive);
        }
        if let Some(expires) = self.expires_at_ms {
            if now_ms > expires {
                return Err(DeviceProtocolError::GrantInactive);
            }
        }
        if !eq_hex(&self.device_pubkey_hex, device_pubkey_hex) {
            return Err(DeviceProtocolError::WrongDevice);
        }
        let actor = actor_pubkey_hex.trim().to_ascii_lowercase();
        let owner = self.owner_pubkey_hex.trim().to_ascii_lowercase();
        if actor == owner
            || self
                .actor_pubkeys
                .iter()
                .any(|candidate| eq_hex(candidate, &actor))
        {
            Ok(())
        } else {
            Err(DeviceProtocolError::UnauthorizedActor)
        }
    }
}

fn eq_hex(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

/// Parse `{millis}-{32 hex}` and return the embedded timestamp.
pub fn parse_request_timestamp(request_id: &str) -> Result<u64, DeviceProtocolError> {
    if request_id.len() != REQUEST_ID_PATTERN_LEN {
        return Err(DeviceProtocolError::InvalidRequestId);
    }
    let (ts, rest) = request_id
        .split_once('-')
        .ok_or(DeviceProtocolError::InvalidRequestId)?;
    if ts.len() != 13 || rest.len() != 32 {
        return Err(DeviceProtocolError::InvalidRequestId);
    }
    if !ts.bytes().all(|b| b.is_ascii_digit())
        || !rest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(DeviceProtocolError::InvalidRequestId);
    }
    ts.parse::<u64>()
        .map_err(|_| DeviceProtocolError::InvalidRequestId)
}

/// Reject expired or future-dated request ids.
pub fn check_request_freshness(request_id: &str, now_ms: u64) -> Result<(), DeviceProtocolError> {
    let created = parse_request_timestamp(request_id)?;
    if created > now_ms.saturating_add(DEVICE_OPERATION_FUTURE_SKEW_MS) {
        return Err(DeviceProtocolError::RequestInFuture);
    }
    if now_ms.saturating_sub(created) > DEVICE_MAX_NEW_OPERATION_AGE_MS {
        return Err(DeviceProtocolError::RequestExpired);
    }
    Ok(())
}

/// SHA-256 hex fingerprint of canonical `{op, device_id, grant_generation, params}`.
pub fn parameter_fingerprint(
    op: DeviceOp,
    device_id: &str,
    grant_generation: u64,
    params: &serde_json::Value,
) -> Result<String, DeviceProtocolError> {
    let canonical_params = canonicalize_value(params)?;
    let mut body = serde_json::Map::new();
    body.insert(
        "device_id".to_string(),
        serde_json::Value::String(device_id.to_string()),
    );
    body.insert(
        "grant_generation".to_string(),
        serde_json::Value::Number(grant_generation.into()),
    );
    body.insert(
        "op".to_string(),
        serde_json::Value::String(op.as_str().to_string()),
    );
    body.insert("params".to_string(), canonical_params);
    let encoded = serde_json::to_string(&serde_json::Value::Object(body))
        .map_err(|_| DeviceProtocolError::InvalidParams)?;
    let digest = Sha256::digest(encoded.as_bytes());
    Ok(hex::encode(digest))
}

fn canonicalize_value(value: &serde_json::Value) -> Result<serde_json::Value, DeviceProtocolError> {
    match value {
        serde_json::Value::Object(map) => {
            let mut ordered = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                let child = map.get(key).ok_or(DeviceProtocolError::InvalidParams)?;
                ordered.insert(key.clone(), canonicalize_value(child)?);
            }
            Ok(serde_json::Value::Object(ordered))
        }
        serde_json::Value::Array(items) => Ok(serde_json::Value::Array(
            items
                .iter()
                .map(canonicalize_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        other => Ok(other.clone()),
    }
}

/// Reject absolute paths and `..` components in a host-relative worktree path.
pub fn relative_path_is_safe(rel: &str) -> Result<(), DeviceProtocolError> {
    let trimmed = rel.trim();
    if trimmed.is_empty() {
        return Err(DeviceProtocolError::UnsafeRelativePath);
    }
    let path = std::path::Path::new(trimmed);
    if path.is_absolute() {
        return Err(DeviceProtocolError::UnsafeRelativePath);
    }
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => {
                let part = part.to_string_lossy();
                if part.is_empty() || part.contains('\0') {
                    return Err(DeviceProtocolError::UnsafeRelativePath);
                }
            }
            std::path::Component::CurDir => {}
            _ => return Err(DeviceProtocolError::UnsafeRelativePath),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant() -> DeviceGrant {
        DeviceGrant {
            device_id: "dev-1".into(),
            device_pubkey_hex: "aa".repeat(32),
            owner_pubkey_hex: "bb".repeat(32),
            actor_pubkeys: vec!["cc".repeat(32)],
            allowed_roots: vec!["/tmp/aquarium-fixture".into()],
            generation: 1,
            expires_at_ms: None,
            revoked: false,
        }
    }

    #[test]
    fn request_id_round_trip() {
        let id = "1700000000000-0123456789abcdef0123456789abcdef";
        assert_eq!(parse_request_timestamp(id).unwrap(), 1_700_000_000_000);
        check_request_freshness(id, 1_700_000_000_000).unwrap();
    }

    #[test]
    fn expired_request_is_rejected() {
        let id = "1000000000000-0123456789abcdef0123456789abcdef";
        let now = 1000000000000 + DEVICE_MAX_NEW_OPERATION_AGE_MS + 1;
        assert_eq!(
            check_request_freshness(id, now),
            Err(DeviceProtocolError::RequestExpired)
        );
    }

    #[test]
    fn future_request_is_rejected() {
        let id = "2000000000000-0123456789abcdef0123456789abcdef";
        assert_eq!(
            check_request_freshness(id, 1_000_000_000_000),
            Err(DeviceProtocolError::RequestInFuture)
        );
    }

    #[test]
    fn fingerprint_is_stable_across_key_order() {
        let a = serde_json::json!({"branch": "tank", "tank_id": "t1"});
        let b = serde_json::json!({"tank_id": "t1", "branch": "tank"});
        let left = parameter_fingerprint(DeviceOp::CreateCheckout, "dev-1", 1, &a).unwrap();
        let right = parameter_fingerprint(DeviceOp::CreateCheckout, "dev-1", 1, &b).unwrap();
        assert_eq!(left, right);
        let different = parameter_fingerprint(
            DeviceOp::CreateCheckout,
            "dev-1",
            1,
            &serde_json::json!({"branch": "other", "tank_id": "t1"}),
        )
        .unwrap();
        assert_ne!(left, different);
    }

    #[test]
    fn grant_allows_owner_and_listed_actor() {
        let g = grant();
        let now = 1_700_000_000_000;
        g.authorize(&g.owner_pubkey_hex, &g.device_pubkey_hex, now)
            .unwrap();
        g.authorize(&g.actor_pubkeys[0], &g.device_pubkey_hex, now)
            .unwrap();
        assert_eq!(
            g.authorize(&"dd".repeat(32), &g.device_pubkey_hex, now),
            Err(DeviceProtocolError::UnauthorizedActor)
        );
        assert_eq!(
            g.authorize(&g.owner_pubkey_hex, &"ee".repeat(32), now),
            Err(DeviceProtocolError::WrongDevice)
        );
    }

    #[test]
    fn revoked_or_expired_grant_is_inactive() {
        let mut g = grant();
        g.revoked = true;
        assert_eq!(
            g.authorize(&g.owner_pubkey_hex, &g.device_pubkey_hex, 1),
            Err(DeviceProtocolError::GrantInactive)
        );
        g.revoked = false;
        g.expires_at_ms = Some(10);
        assert_eq!(
            g.authorize(&g.owner_pubkey_hex, &g.device_pubkey_hex, 11),
            Err(DeviceProtocolError::GrantInactive)
        );
    }

    #[test]
    fn relative_path_rejects_escape() {
        relative_path_is_safe("tanks/tank-a").unwrap();
        assert_eq!(
            relative_path_is_safe("../outside"),
            Err(DeviceProtocolError::UnsafeRelativePath)
        );
        assert_eq!(
            relative_path_is_safe("/absolute"),
            Err(DeviceProtocolError::UnsafeRelativePath)
        );
        assert_eq!(
            relative_path_is_safe(""),
            Err(DeviceProtocolError::UnsafeRelativePath)
        );
    }
}
