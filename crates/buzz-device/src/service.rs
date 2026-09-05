//! Device host: authorize, journal, execute. Never falls back to the caller.

use crate::git_checkout::{create_worktree, list_worktrees, CheckoutEvidence};
use crate::journal::{Journal, JournalEntry, RequestState};
use crate::session::{cancel_session, spawn_fixture_agent, SessionEvidence};
use crate::wire::{fingerprint_request, DeviceReceipt, DeviceRequest, ReceiptStatus};
use crate::{DeviceError, GrantFile};
use buzz_core::device::{check_request_freshness, DeviceOp};
use std::path::{Path, PathBuf};

/// Outcome of handling one request.
#[derive(Debug, Clone)]
pub struct HandleOutcome {
    /// Durable receipt.
    pub receipt: DeviceReceipt,
    /// Whether a mutation was attempted.
    pub mutated: bool,
}

/// Long-lived host state.
pub struct DeviceService {
    /// Grant file contents.
    pub grant: GrantFile,
    journal: Journal,
    allowed_root: PathBuf,
    agent_command: Option<Vec<String>>,
}

impl DeviceService {
    /// Open a service from isolated state + grant.
    pub fn open(
        state_dir: &Path,
        grant: GrantFile,
        agent_command: Option<Vec<String>>,
    ) -> Result<Self, DeviceError> {
        if grant.allowed_roots.len() != 1 {
            return Err(DeviceError::Grant(
                "prototype requires exactly one allowed root".into(),
            ));
        }
        let allowed_root = PathBuf::from(&grant.allowed_roots[0]);
        Ok(Self {
            journal: Journal::open(state_dir)?,
            grant,
            allowed_root,
            agent_command,
        })
    }

    /// Reconcile `executing` rows after restart. Never blindly respawns.
    pub fn reconcile_on_start(&self) -> Result<(), DeviceError> {
        for (_, mut row) in self.journal.load_all()? {
            if row.state != RequestState::Executing {
                continue;
            }
            row.state = RequestState::Uncertain;
            row.error = Some(
                "host restarted while executing; outcome is uncertain and will not be retried"
                    .into(),
            );
            if row.op == DeviceOp::CreateCheckout.as_str() {
                if let Some(path) = row.outcome.get("path").and_then(|v| v.as_str()) {
                    if Path::new(path).join(".git").exists()
                        || Path::new(path).join("README").exists()
                    {
                        row.state = RequestState::Succeeded;
                        row.error = None;
                    }
                }
            }
            self.journal.put(&row)?;
        }
        Ok(())
    }
}

/// Handle one decrypted request from `actor_pubkey_hex` targeting `device_pubkey_hex`.
pub fn handle_request(
    service: &DeviceService,
    actor_pubkey_hex: &str,
    device_pubkey_hex: &str,
    request: &DeviceRequest,
    now_ms: u64,
) -> Result<HandleOutcome, DeviceError> {
    let grant = service.grant.to_grant();
    if request.device_id != grant.device_id {
        return reject(
            service,
            request,
            actor_pubkey_hex,
            "device_id does not match this host",
            ReceiptStatus::Rejected,
        );
    }
    if request.grant_generation != grant.generation {
        return reject(
            service,
            request,
            actor_pubkey_hex,
            "grant generation mismatch",
            ReceiptStatus::Rejected,
        );
    }
    if let Err(err) = check_request_freshness(&request.request_id, now_ms) {
        return reject(
            service,
            request,
            actor_pubkey_hex,
            &err.to_string(),
            ReceiptStatus::Rejected,
        );
    }
    if let Err(err) = grant.authorize(actor_pubkey_hex, device_pubkey_hex, now_ms) {
        return reject(
            service,
            request,
            actor_pubkey_hex,
            &err.to_string(),
            ReceiptStatus::Rejected,
        );
    }
    let fingerprint = fingerprint_request(request)?;
    if let Some(existing) = service.journal.get(&request.request_id)? {
        if existing.fingerprint != fingerprint {
            let receipt = DeviceReceipt {
                v: request.v,
                request_id: request.request_id.clone(),
                fingerprint: existing.fingerprint,
                status: ReceiptStatus::Conflict,
                evidence: existing.outcome,
                error: Some("request id reused with different parameters".into()),
            };
            return Ok(HandleOutcome {
                receipt,
                mutated: false,
            });
        }
        return Ok(HandleOutcome {
            receipt: receipt_from_entry(&existing),
            mutated: false,
        });
    }

    let op = DeviceOp::parse(&request.op)
        .ok_or_else(|| DeviceError::Transport(format!("unknown op {}", request.op)))?;
    let mut entry = JournalEntry {
        request_id: request.request_id.clone(),
        fingerprint: fingerprint.clone(),
        state: RequestState::Accepted,
        op: request.op.clone(),
        actor_pubkey_hex: actor_pubkey_hex.to_string(),
        grant_generation: request.grant_generation,
        outcome: serde_json::Value::Null,
        error: None,
    };
    service.journal.put(&entry)?;
    entry.state = RequestState::Executing;
    service.journal.put(&entry)?;

    match execute(service, op, request) {
        Ok(evidence) => {
            entry.state = RequestState::Succeeded;
            entry.outcome = evidence.clone();
            service.journal.put(&entry)?;
            Ok(HandleOutcome {
                receipt: DeviceReceipt {
                    v: request.v,
                    request_id: request.request_id.clone(),
                    fingerprint,
                    status: ReceiptStatus::Succeeded,
                    evidence,
                    error: None,
                },
                mutated: matches!(
                    op,
                    DeviceOp::CreateCheckout | DeviceOp::StartSession | DeviceOp::CancelSession
                ),
            })
        }
        Err(err) => {
            entry.state = RequestState::Failed;
            entry.error = Some(err.to_string());
            service.journal.put(&entry)?;
            Ok(HandleOutcome {
                receipt: DeviceReceipt {
                    v: request.v,
                    request_id: request.request_id.clone(),
                    fingerprint,
                    status: ReceiptStatus::Failed,
                    evidence: serde_json::Value::Null,
                    error: Some(err.to_string()),
                },
                mutated: false,
            })
        }
    }
}

fn reject(
    service: &DeviceService,
    request: &DeviceRequest,
    actor_pubkey_hex: &str,
    error: &str,
    status: ReceiptStatus,
) -> Result<HandleOutcome, DeviceError> {
    let fingerprint = fingerprint_request(request).unwrap_or_else(|_| "invalid".into());
    if let Ok(Some(existing)) = service.journal.get(&request.request_id) {
        if matches!(
            existing.state,
            RequestState::Succeeded
                | RequestState::Failed
                | RequestState::Conflict
                | RequestState::Uncertain
                | RequestState::Executing
        ) {
            return Ok(HandleOutcome {
                receipt: DeviceReceipt {
                    v: request.v,
                    request_id: request.request_id.clone(),
                    fingerprint: existing.fingerprint,
                    status: ReceiptStatus::Rejected,
                    evidence: serde_json::Value::Null,
                    error: Some(error.to_string()),
                },
                mutated: false,
            });
        }
    }
    let entry = JournalEntry {
        request_id: request.request_id.clone(),
        fingerprint: fingerprint.clone(),
        state: match status {
            ReceiptStatus::Conflict => RequestState::Conflict,
            _ => RequestState::Rejected,
        },
        op: request.op.clone(),
        actor_pubkey_hex: actor_pubkey_hex.to_string(),
        grant_generation: request.grant_generation,
        outcome: serde_json::Value::Null,
        error: Some(error.to_string()),
    };
    let _ = service.journal.put(&entry);
    Ok(HandleOutcome {
        receipt: DeviceReceipt {
            v: request.v,
            request_id: request.request_id.clone(),
            fingerprint,
            status,
            evidence: serde_json::Value::Null,
            error: Some(error.to_string()),
        },
        mutated: false,
    })
}

fn receipt_from_entry(entry: &JournalEntry) -> DeviceReceipt {
    let status = match entry.state {
        RequestState::Succeeded => ReceiptStatus::Succeeded,
        RequestState::Failed => ReceiptStatus::Failed,
        RequestState::Rejected => ReceiptStatus::Rejected,
        RequestState::Conflict => ReceiptStatus::Conflict,
        RequestState::Uncertain => ReceiptStatus::Uncertain,
        RequestState::Accepted | RequestState::Executing => ReceiptStatus::Uncertain,
    };
    DeviceReceipt {
        v: 1,
        request_id: entry.request_id.clone(),
        fingerprint: entry.fingerprint.clone(),
        status,
        evidence: entry.outcome.clone(),
        error: entry.error.clone(),
    }
}

fn execute(
    service: &DeviceService,
    op: DeviceOp,
    request: &DeviceRequest,
) -> Result<serde_json::Value, DeviceError> {
    match op {
        DeviceOp::InspectCapabilities => Ok(serde_json::json!({
            "device_id": service.grant.device_id,
            "device_pubkey_hex": service.grant.device_pubkey_hex,
            "online": true,
            "protocol_version": buzz_core::device::DEVICE_PROTOCOL_VERSION,
            "harnesses": ["fixture-agent"],
            "setup_readiness": "ready",
            "grant_generation": service.grant.generation,
        })),
        DeviceOp::InspectRequest => {
            let target = request
                .params
                .get("request_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    DeviceError::Transport("inspect_request needs params.request_id".into())
                })?;
            match service.journal.get(target)? {
                Some(entry) => Ok(serde_json::to_value(receipt_from_entry(&entry))
                    .map_err(|e| DeviceError::Journal(e.to_string()))?),
                None => Err(DeviceError::Journal("request not found".into())),
            }
        }
        DeviceOp::CreateCheckout => {
            let tank_id = required_str(&request.params, "tank_id")?;
            let branch = required_str(&request.params, "branch")?;
            let relpath = required_str(&request.params, "relpath")?;
            let start_rev = request
                .params
                .get("start_rev")
                .and_then(|v| v.as_str())
                .unwrap_or("HEAD");
            let repo_relpath = request
                .params
                .get("repo_relpath")
                .and_then(|v| v.as_str())
                .unwrap_or("repo");
            let evidence: CheckoutEvidence = create_worktree(
                &service.allowed_root,
                repo_relpath,
                relpath,
                branch,
                start_rev,
            )?;
            let repo = service.allowed_root.join(repo_relpath);
            let list = list_worktrees(&repo).unwrap_or_default();
            Ok(serde_json::json!({
                "tank_id": tank_id,
                "path": evidence.path,
                "branch": evidence.branch,
                "head": evidence.head,
                "host": evidence.host,
                "worktree_list": list,
            }))
        }
        DeviceOp::StartSession => {
            let checkout = required_str(&request.params, "checkout_path")?;
            let session_id = request
                .params
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let evidence: SessionEvidence = spawn_fixture_agent(
                &service.allowed_root,
                Path::new(checkout),
                &session_id,
                service.agent_command.as_deref(),
            )?;
            Ok(serde_json::to_value(evidence).map_err(|e| DeviceError::Agent(e.to_string()))?)
        }
        DeviceOp::CancelSession => {
            let pid = request
                .params
                .get("pid")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| DeviceError::Agent("cancel_session needs params.pid".into()))?;
            cancel_session(pid as u32)?;
            Ok(serde_json::json!({"cancelled_pid": pid}))
        }
    }
}

fn required_str<'a>(params: &'a serde_json::Value, key: &str) -> Result<&'a str, DeviceError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| DeviceError::Transport(format!("missing params.{key}")))
}
