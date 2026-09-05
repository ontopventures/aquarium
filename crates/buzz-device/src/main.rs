//! `buzz-device` — isolated Aquarium device host, controller, and mux fixture.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use buzz_core::coord::{CoordPeers, CoordPhase, CoordStatus, COORD_PROTOCOL_VERSION};
use buzz_core::device::DEVICE_PROTOCOL_VERSION;
use buzz_core::kind::{KIND_DEVICE_RECEIPT, KIND_DEVICE_REQUEST};
use buzz_device::{
    coord_filter, decrypt_receipt, decrypt_request, fingerprint_request, generate_request_id,
    handle_coord, handle_request, parse_coord_event, publish_advertisement, publish_coord,
    publish_receipt, publish_request, run_agent_fixture, run_mux, run_mux_listener, CoordBind,
    CoordJournal, DeviceReceipt, DeviceRequest, DeviceService, GrantFile, HandleOutcome,
    ReceiptStatus,
};
use buzz_ws_client::{NostrWsConnection, RelayMessage};
use clap::{Parser, Subcommand};
use nostr::{Filter, Keys, PublicKey, ToBech32};
use serde_json::json;
use tokio::time::timeout;

#[derive(Parser)]
#[command(name = "buzz-device", about = "Aquarium device-command prototype")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execution host. The only process that runs git or agents.
    Serve {
        /// Isolated state directory (journal).
        #[arg(long)]
        state_dir: PathBuf,
        /// Grant JSON (execution authority).
        #[arg(long)]
        grant: PathBuf,
        /// Host nsec file (fixture keys only).
        #[arg(long)]
        nsec_file: PathBuf,
        /// Relay or mux WebSocket URL.
        #[arg(long)]
        relay: String,
        /// Optional argv for a real harness instead of the fixture agent.
        #[arg(long)]
        agent_command: Option<String>,
    },
    /// Initiating client. Never runs git.
    Ctl {
        /// Actor nsec file (fixture keys only).
        #[arg(long)]
        nsec_file: PathBuf,
        /// Selected execution-host pubkey (required; no local fallback).
        #[arg(long)]
        device_pubkey: String,
        /// Device id.
        #[arg(long)]
        device_id: String,
        /// Relay or mux WebSocket URL.
        #[arg(long)]
        relay: String,
        /// Grant generation.
        #[arg(long, default_value_t = 1)]
        grant_generation: u64,
        /// Request id; generated if omitted.
        #[arg(long)]
        request_id: Option<String>,
        #[command(subcommand)]
        op: CtlOp,
    },
    /// Local signed-event multiplexer (not Buzz-relay acceptance).
    Mux {
        #[arg(long, default_value = "127.0.0.1:0")]
        bind: String,
    },
    /// Fixture agent process whose cwd must be the checkout.
    AgentFixture {
        #[arg(long)]
        session_id: String,
    },
    /// Write a new fixture nsec and print its pubkey. Not for existing user keys.
    Keygen {
        /// Destination nsec file.
        #[arg(long)]
        nsec_file: PathBuf,
    },
    /// Leader: delegate a bounded same-tank assignment and continue after the result.
    CoordLeader {
        #[arg(long)]
        nsec_file: PathBuf,
        #[arg(long)]
        worker_pubkey: String,
        #[arg(long)]
        conversation_id: String,
        #[arg(long)]
        tank_id: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        relay: String,
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        assignment_id: Option<String>,
    },
    /// Worker: ack, perform bounded work in cwd, report result. YOLO does not skip peer auth.
    CoordWorker {
        #[arg(long)]
        nsec_file: PathBuf,
        #[arg(long)]
        leader_pubkey: String,
        #[arg(long)]
        conversation_id: String,
        #[arg(long)]
        tank_id: String,
        #[arg(long)]
        relay: String,
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        cwd: PathBuf,
    },
}

#[derive(Subcommand)]
enum CtlOp {
    InspectCapabilities,
    CreateCheckout {
        #[arg(long)]
        tank_id: String,
        #[arg(long)]
        branch: String,
        #[arg(long)]
        relpath: String,
        #[arg(long, default_value = "repo")]
        repo_relpath: String,
        #[arg(long, default_value = "HEAD")]
        start_rev: String,
    },
    InspectRequest {
        #[arg(long)]
        target_request_id: String,
    },
    StartSession {
        #[arg(long)]
        checkout_path: String,
        #[arg(long)]
        session_id: Option<String>,
    },
    CancelSession {
        #[arg(long)]
        pid: u32,
    },
}

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter("buzz_device=info")
        .init();
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Commands::AgentFixture { session_id } => {
            run_agent_fixture(&session_id)?;
            Ok(())
        }
        Commands::Keygen { nsec_file } => {
            let keys = Keys::generate();
            if let Some(parent) = nsec_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&nsec_file, format!("{}\n", keys.secret_key().to_bech32()?))?;
            println!("{}", keys.public_key().to_hex());
            Ok(())
        }
        Commands::Mux { bind } => {
            let addr: std::net::SocketAddr = bind.parse()?;
            if addr.port() == 0 {
                let listener = tokio::net::TcpListener::bind(addr).await?;
                println!("{}", listener.local_addr()?);
                run_mux_listener(listener).await?;
            } else {
                println!("{addr}");
                run_mux(addr).await?;
            }
            Ok(())
        }
        Commands::CoordLeader {
            nsec_file,
            worker_pubkey,
            conversation_id,
            tank_id,
            task,
            relay,
            state_dir,
            cwd,
            assignment_id,
        } => {
            coord_leader(
                nsec_file,
                worker_pubkey,
                conversation_id,
                tank_id,
                task,
                relay,
                state_dir,
                cwd,
                assignment_id,
            )
            .await
        }
        Commands::CoordWorker {
            nsec_file,
            leader_pubkey,
            conversation_id,
            tank_id,
            relay,
            state_dir,
            cwd,
        } => {
            coord_worker(
                nsec_file,
                leader_pubkey,
                conversation_id,
                tank_id,
                relay,
                state_dir,
                cwd,
            )
            .await
        }
        Commands::Serve {
            state_dir,
            grant,
            nsec_file,
            relay,
            agent_command,
        } => serve(state_dir, grant, nsec_file, relay, agent_command).await,
        Commands::Ctl {
            nsec_file,
            device_pubkey,
            device_id,
            relay,
            grant_generation,
            request_id,
            op,
        } => {
            ctl(
                nsec_file,
                device_pubkey,
                device_id,
                relay,
                grant_generation,
                request_id,
                op,
            )
            .await
        }
    }
}

async fn serve(
    state_dir: PathBuf,
    grant_path: PathBuf,
    nsec_file: PathBuf,
    relay: String,
    agent_command: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let grant = GrantFile::load(&grant_path)?;
    let keys = load_keys(&nsec_file)?;
    let host_hex = keys.public_key().to_hex();
    if !host_hex.eq_ignore_ascii_case(&grant.device_pubkey_hex) {
        return Err("nsec file does not match grant device_pubkey_hex".into());
    }
    let agent_command =
        agent_command.map(|s| s.split_whitespace().map(str::to_string).collect::<Vec<_>>());
    let service = DeviceService::open(&state_dir, grant, agent_command)?;
    service.reconcile_on_start()?;
    let mut conn = NostrWsConnection::connect_authenticated(&relay, &keys, None).await?;
    let filter = Filter::new()
        .kind(nostr::Kind::Custom(KIND_DEVICE_REQUEST as u16))
        .pubkey(keys.public_key());
    conn.send_raw(&json!(["REQ", "device-in", filter])).await?;
    let advertisement =
        publish_advertisement(&keys, &service.grant.device_id, service.grant.generation)?;
    conn.send_event(advertisement).await?;
    tracing::info!("device host online as {host_hex}");
    loop {
        match timeout(
            Duration::from_secs(30),
            conn.next_event(Duration::from_secs(30)),
        )
        .await
        {
            Ok(Ok(RelayMessage::Event { event, .. })) => {
                if u32::from(event.kind.as_u16()) != KIND_DEVICE_REQUEST {
                    continue;
                }
                let p_ok = event.tags.iter().any(|tag| {
                    let items = tag.as_slice();
                    items.first().map(String::as_str) == Some("p")
                        && items
                            .get(1)
                            .is_some_and(|value| value.eq_ignore_ascii_case(&host_hex))
                });
                if !p_ok {
                    continue;
                }
                let actor = event.pubkey.to_hex();
                let request = match decrypt_request(&keys, &event) {
                    Ok(request) => request,
                    Err(error) => {
                        tracing::warn!("decrypt failed: {error}");
                        continue;
                    }
                };
                let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
                let outcome = match handle_request(&service, &actor, &host_hex, &request, now) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        tracing::warn!("handle_request failed: {error}");
                        HandleOutcome {
                            receipt: DeviceReceipt {
                                v: request.v,
                                request_id: request.request_id.clone(),
                                fingerprint: fingerprint_request(&request)
                                    .unwrap_or_else(|_| "invalid".into()),
                                status: ReceiptStatus::Failed,
                                evidence: serde_json::Value::Null,
                                error: Some(error.to_string()),
                            },
                            mutated: false,
                        }
                    }
                };
                let actor_pk = PublicKey::from_hex(&actor)?;
                let receipt_event =
                    publish_receipt(&keys, &actor_pk, &service.grant.device_id, &outcome.receipt)?;
                conn.send_event(receipt_event).await?;
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                tracing::warn!("relay read: {error}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(_) => {}
        }
    }
}

async fn ctl(
    nsec_file: PathBuf,
    device_pubkey: String,
    device_id: String,
    relay: String,
    grant_generation: u64,
    request_id: Option<String>,
    op: CtlOp,
) -> Result<(), Box<dyn std::error::Error>> {
    if device_pubkey.trim().is_empty() {
        return Err("device_pubkey is required; refusing to run locally".into());
    }
    let keys = load_keys(&nsec_file)?;
    let device_pk = PublicKey::from_hex(device_pubkey.trim())?;
    let (op_name, params) = match op {
        CtlOp::InspectCapabilities => ("inspect_capabilities", json!({})),
        CtlOp::CreateCheckout {
            tank_id,
            branch,
            relpath,
            repo_relpath,
            start_rev,
        } => (
            "create_checkout",
            json!({
                "tank_id": tank_id,
                "branch": branch,
                "relpath": relpath,
                "repo_relpath": repo_relpath,
                "start_rev": start_rev,
            }),
        ),
        CtlOp::InspectRequest { target_request_id } => (
            "inspect_request",
            json!({ "request_id": target_request_id }),
        ),
        CtlOp::StartSession {
            checkout_path,
            session_id,
        } => (
            "start_session",
            json!({
                "checkout_path": checkout_path,
                "session_id": session_id,
            }),
        ),
        CtlOp::CancelSession { pid } => ("cancel_session", json!({ "pid": pid })),
    };
    let request = DeviceRequest {
        v: DEVICE_PROTOCOL_VERSION,
        request_id: request_id.unwrap_or_else(generate_request_id),
        op: op_name.to_string(),
        grant_generation,
        device_id: device_id.clone(),
        params,
    };
    let mut conn = NostrWsConnection::connect_authenticated(&relay, &keys, None).await?;
    let filter = Filter::new()
        .kind(nostr::Kind::Custom(KIND_DEVICE_RECEIPT as u16))
        .pubkey(keys.public_key());
    conn.send_raw(&json!(["REQ", "device-out", filter])).await?;
    let event = publish_request(&keys, &device_pk, &device_id, &request)?;
    conn.send_event(event).await?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for device receipt; not running locally".into());
        }
        match conn.next_event(remaining).await? {
            RelayMessage::Event { event, .. } => {
                if u32::from(event.kind.as_u16()) != KIND_DEVICE_RECEIPT {
                    continue;
                }
                if event.pubkey != device_pk {
                    continue;
                }
                if let Ok(receipt) = decrypt_receipt(&keys, &event) {
                    if receipt.request_id == request.request_id {
                        println!("{}", serde_json::to_string_pretty(&receipt)?);
                        if receipt.status == ReceiptStatus::Succeeded {
                            return Ok(());
                        }
                        return Err(receipt
                            .error
                            .unwrap_or_else(|| receipt.status.status_label())
                            .into());
                    }
                }
            }
            RelayMessage::Eose { .. } => {}
            other => tracing::debug!("ctl ignored {other:?}"),
        }
    }
}

async fn coord_leader(
    nsec_file: PathBuf,
    worker_pubkey: String,
    conversation_id: String,
    tank_id: String,
    task: String,
    relay: String,
    state_dir: PathBuf,
    cwd: PathBuf,
    assignment_id: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let keys = load_keys(&nsec_file)?;
    let worker_pk = PublicKey::from_hex(worker_pubkey.trim())?;
    let assignment_id = assignment_id.unwrap_or_else(generate_request_id);
    let journal = CoordJournal::open(&state_dir)?;
    let peers = CoordPeers {
        leader_pubkey_hex: keys.public_key().to_hex(),
        worker_pubkey_hex: worker_pk.to_hex(),
    };
    let bind = CoordBind {
        self_pubkey_hex: keys.public_key().to_hex(),
        conversation_id: conversation_id.clone(),
        tank_id: tank_id.clone(),
    };
    let mut conn = NostrWsConnection::connect_authenticated(&relay, &keys, None).await?;
    conn.send_raw(&json!([
        "REQ",
        "coord-in",
        coord_filter(keys.public_key(), &conversation_id)
    ]))
    .await?;
    let body = json!({
        "v": COORD_PROTOCOL_VERSION,
        "assignment_id": assignment_id,
        "task": task,
        "hops": 0,
    });
    let event = publish_coord(
        &keys,
        &worker_pk,
        &conversation_id,
        &tank_id,
        &assignment_id,
        CoordPhase::Delegate,
        &body,
    )?;
    conn.send_event(event).await?;
    let mut saw_ack = false;
    let mut saw_result = false;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline && !(saw_ack && saw_result) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match conn.next_event(remaining).await? {
            RelayMessage::Event { event, .. } => {
                let Some(message) = parse_coord_event(&event) else {
                    continue;
                };
                if message.assignment_id != assignment_id {
                    continue;
                }
                let (status, _) =
                    handle_coord(&journal, &peers, &bind, &message, Some(&cwd), Some(&cwd))?;
                match message.phase {
                    CoordPhase::Ack if status == CoordStatus::Succeeded => saw_ack = true,
                    CoordPhase::Result if status == CoordStatus::Succeeded => saw_result = true,
                    _ => {}
                }
            }
            RelayMessage::Eose { .. } => {}
            _ => {}
        }
    }
    if !saw_result {
        return Err("timed out waiting for worker succeeded result; not looping".into());
    }
    let cont = json!({
        "v": COORD_PROTOCOL_VERSION,
        "assignment_id": assignment_id,
        "task": task,
        "hops": 0,
        "status": "succeeded",
    });
    let continue_msg = publish_coord(
        &keys,
        &worker_pk,
        &conversation_id,
        &tank_id,
        &assignment_id,
        CoordPhase::Continue,
        &cont,
    )?;
    if let Some(parsed) = parse_coord_event(&continue_msg) {
        handle_coord(&journal, &peers, &bind, &parsed, Some(&cwd), Some(&cwd))?;
    }
    conn.send_event(continue_msg).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "assignment_id": assignment_id,
            "status": "succeeded",
            "continued": true,
        }))?
    );
    Ok(())
}

async fn coord_worker(
    nsec_file: PathBuf,
    leader_pubkey: String,
    conversation_id: String,
    tank_id: String,
    relay: String,
    state_dir: PathBuf,
    cwd: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let keys = load_keys(&nsec_file)?;
    let leader_pk = PublicKey::from_hex(leader_pubkey.trim())?;
    let journal = CoordJournal::open(&state_dir)?;
    let peers = CoordPeers {
        leader_pubkey_hex: leader_pk.to_hex(),
        worker_pubkey_hex: keys.public_key().to_hex(),
    };
    let bind = CoordBind {
        self_pubkey_hex: keys.public_key().to_hex(),
        conversation_id: conversation_id.clone(),
        tank_id: tank_id.clone(),
    };
    let mut conn = NostrWsConnection::connect_authenticated(&relay, &keys, None).await?;
    conn.send_raw(&json!([
        "REQ",
        "coord-in",
        coord_filter(keys.public_key(), &conversation_id)
    ]))
    .await?;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match conn.next_event(remaining).await? {
            RelayMessage::Event { event, .. } => {
                let Some(message) = parse_coord_event(&event) else {
                    continue;
                };
                if message.phase != CoordPhase::Delegate {
                    continue;
                }
                let (status, _) =
                    handle_coord(&journal, &peers, &bind, &message, Some(&cwd), Some(&cwd))?;
                if status == CoordStatus::Rejected {
                    continue;
                }
                let task = message
                    .body
                    .get("task")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let ack = publish_coord(
                    &keys,
                    &leader_pk,
                    &conversation_id,
                    &tank_id,
                    &message.assignment_id,
                    CoordPhase::Ack,
                    &json!({
                        "v": COORD_PROTOCOL_VERSION,
                        "assignment_id": message.assignment_id,
                        "task": task,
                        "hops": 0,
                    }),
                )?;
                conn.send_event(ack).await?;
                let result_body = json!({
                    "v": COORD_PROTOCOL_VERSION,
                    "assignment_id": message.assignment_id,
                    "task": task,
                    "hops": 0,
                    "status": "succeeded",
                });
                let result_event = publish_coord(
                    &keys,
                    &leader_pk,
                    &conversation_id,
                    &tank_id,
                    &message.assignment_id,
                    CoordPhase::Result,
                    &result_body,
                )?;
                if let Some(parsed) = parse_coord_event(&result_event) {
                    handle_coord(&journal, &peers, &bind, &parsed, Some(&cwd), Some(&cwd))?;
                }
                conn.send_event(result_event).await?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "assignment_id": message.assignment_id,
                        "status": "succeeded",
                    }))?
                );
                return Ok(());
            }
            RelayMessage::Eose { .. } => {}
            _ => {}
        }
    }
    Err("timed out waiting for leader assignment; not looping".into())
}

fn load_keys(path: &PathBuf) -> Result<Keys, Box<dyn std::error::Error>> {
    let raw = std::fs::read_to_string(path)?;
    Ok(Keys::parse(raw.trim())?)
}
