//! Local NIP-01 event fan-out. Transport fixture, not isolated Buzz-relay proof.

use crate::DeviceError;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::{accept_async, tungstenite::Message};

/// Run a loopback multiplexer that challenges AUTH then fans EVENT frames out.
pub async fn run_mux(bind: SocketAddr) -> Result<(), DeviceError> {
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    let (tx, _) = broadcast::channel::<String>(256);
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| DeviceError::Transport(e.to_string()))?;
        let tx = tx.clone();
        let rx = tx.subscribe();
        tokio::spawn(async move {
            if let Err(error) = handle_conn(stream, tx, rx).await {
                tracing::warn!("mux connection ended: {error}");
            }
        });
    }
}

async fn handle_conn(
    stream: TcpStream,
    tx: broadcast::Sender<String>,
    mut rx: broadcast::Receiver<String>,
) -> Result<(), DeviceError> {
    let mut ws = accept_async(stream)
        .await
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    ws.send(Message::Text(
        json!(["AUTH", "aquarium-device-mux"]).to_string().into(),
    ))
    .await
    .map_err(|e| DeviceError::Transport(e.to_string()))?;

    let pending: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));

    loop {
        tokio::select! {
            incoming = ws.next() => {
                let Some(frame) = incoming else { break; };
                let Message::Text(text) = frame.map_err(|e| DeviceError::Transport(e.to_string()))? else {
                    continue;
                };
                let parsed: Vec<Value> = serde_json::from_str(&text)
                    .map_err(|e| DeviceError::Transport(e.to_string()))?;
                let kind = parsed.first().and_then(|v| v.as_str()).unwrap_or("");
                match kind {
                    "AUTH" => {
                        let id = parsed
                            .get(1)
                            .and_then(|v| v.get("id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("auth");
                        ws.send(Message::Text(json!(["OK", id, true, ""]).to_string().into()))
                            .await
                            .map_err(|e| DeviceError::Transport(e.to_string()))?;
                    }
                    "EVENT" => {
                        let event = parsed.get(1).cloned().unwrap_or(Value::Null);
                        let id = event.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let encoded = json!(event).to_string();
                        let _ = tx.send(encoded);
                        // Fixture only: live fan-out. No stored history; subscribers
                        // must REQ before the publisher sends EVENT.
                        ws.send(Message::Text(json!(["OK", id, true, ""]).to_string().into()))
                            .await
                            .map_err(|e| DeviceError::Transport(e.to_string()))?;
                    }
                    "REQ" => {
                        let sub = parsed
                            .get(1)
                            .and_then(|v| v.as_str())
                            .unwrap_or("sub")
                            .to_string();
                        let filter = parsed.get(2).cloned().unwrap_or(json!({}));
                        pending.lock().await.push((sub.clone(), filter));
                        ws.send(Message::Text(json!(["EOSE", sub]).to_string().into()))
                            .await
                            .map_err(|e| DeviceError::Transport(e.to_string()))?;
                    }
                    "CLOSE" => {}
                    _ => {}
                }
            }
            fanout = rx.recv() => {
                let Ok(event_json) = fanout else { continue; };
                let event: Value = serde_json::from_str(&event_json)
                    .map_err(|e| DeviceError::Transport(e.to_string()))?;
                let filters = pending.lock().await.clone();
                for (sub, filter) in filters {
                    if event_matches(&event, &filter) {
                        ws.send(Message::Text(
                            json!(["EVENT", sub, event.clone()]).to_string().into(),
                        ))
                        .await
                        .map_err(|e| DeviceError::Transport(e.to_string()))?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn event_matches(event: &Value, filter: &Value) -> bool {
    if let Some(kinds) = filter.get("kinds").and_then(|v| v.as_array()) {
        let kind = event.get("kind").and_then(|v| v.as_u64());
        if !kinds.iter().any(|k| k.as_u64() == kind) {
            return false;
        }
    }
    if let Some(authors) = filter.get("authors").and_then(|v| v.as_array()) {
        let pubkey = event.get("pubkey").and_then(|v| v.as_str());
        if !authors.iter().any(|a| a.as_str() == pubkey) {
            return false;
        }
    }
    if let Some(ids) = filter.get("#p").and_then(|v| v.as_array()) {
        let tags = event.get("tags").and_then(|v| v.as_array());
        let Some(tags) = tags else { return false };
        let has = tags.iter().any(|tag| {
            tag.get(0).and_then(|v| v.as_str()) == Some("p")
                && ids
                    .iter()
                    .any(|want| tag.get(1).and_then(|v| v.as_str()) == want.as_str())
        });
        if !has {
            return false;
        }
    }
    true
}

/// Bind an ephemeral localhost port and return it before serving.
pub async fn bind_local() -> Result<(TcpListener, SocketAddr), DeviceError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    let addr = listener
        .local_addr()
        .map_err(|e| DeviceError::Transport(e.to_string()))?;
    Ok((listener, addr))
}

/// Serve mux on an already-bound listener (tests).
pub async fn run_mux_listener(listener: TcpListener) -> Result<(), DeviceError> {
    let (tx, _) = broadcast::channel::<String>(256);
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| DeviceError::Transport(e.to_string()))?;
        let tx = tx.clone();
        let rx = tx.subscribe();
        tokio::spawn(async move {
            let _ = handle_conn(stream, tx, rx).await;
        });
    }
}
