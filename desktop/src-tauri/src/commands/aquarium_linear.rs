//! Native Linear client for Aquarium: OS secret slot + official GraphQL.
//!
//! Personal API keys use `Authorization: <API_KEY>` against
//! `https://api.linear.app/graphql` (Orca personal-key flow). `source: linear`
//! and `connected: true` are emitted only after a successful viewer query.
//! Synthetic/mock data is `source: mock` and `connected: false` so the UI
//! cannot auto-bind it as live. Production never special-cases a key prefix.
//! No live HTTP without a stored key. Keys never appear in results or errors.

use reqwest::header::{HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

use crate::secret_store::SecretStore;

/// Keychain blob slot. Never the identity nsec key.
pub const LINEAR_SECRET_SLOT: &str = "aquarium-linear-api-key";
const IDENTITY_SLOT: &str = "identity";
/// Official Linear GraphQL endpoint.
pub const LINEAR_GRAPHQL_URL: &str = "https://api.linear.app/graphql";

const CONNECT_QUERY: &str = r#"query AquariumConnect {
  viewer { id name }
  organization { id name urlKey }
}"#;

const SEARCH_QUERY: &str = r#"query AquariumSearch($term: String!) {
  searchIssues(term: $term, first: 25) {
    nodes {
      ... on Issue {
        id
        identifier
        title
        url
        state { name }
        project { name }
      }
    }
  }
}"#;

const LIST_QUERY: &str = r#"query AquariumIssues {
  issues(first: 25) {
    nodes {
      id
      identifier
      title
      url
      state { name }
      project { name }
    }
  }
}"#;

const GET_QUERY: &str = r#"query AquariumIssue($id: String!) {
  issue(id: $id) {
    id
    identifier
    title
    url
    state { name }
    project { name }
  }
}"#;

/// Linear connection DTO. Never includes the API key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinearConnection {
    /// Provenance. `linear` only after GraphQL success; mock is never live.
    pub source: String,
    /// Whether a usable Linear workspace was verified.
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
    /// Provenance. `linear` only from GraphQL; mock fixtures stay mock.
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

/// How the native client talks to Linear.
#[derive(Debug, Clone)]
pub(crate) enum LinearTransport {
    /// Official (or injected) GraphQL HTTP.
    Live { endpoint: String },
    /// Test/dev-only explicit mock. Never `source: linear`.
    Mock,
}

fn json_has_secret(value: &Value, secret: &str) -> bool {
    let encoded = value.to_string();
    !secret.is_empty() && encoded.contains(secret)
}

fn redact_secret(message: &str, secret: &str) -> String {
    if secret.is_empty() || !message.contains(secret) {
        return message.to_string();
    }
    message.replace(secret, "[redacted]")
}

fn disconnected(message: &str) -> LinearConnection {
    LinearConnection {
        source: "linear".into(),
        connected: false,
        workspace_name: None,
        message: message.into(),
    }
}

fn mock_connection() -> LinearConnection {
    LinearConnection {
        source: "mock".into(),
        connected: false,
        workspace_name: None,
        message: "mock Linear fixture; not a live workspace".into(),
    }
}

fn mock_issues() -> Vec<LinearIssue> {
    vec![LinearIssue {
        source: "mock".into(),
        id: "fixture-issue-1".into(),
        identifier: "AQU-1".into(),
        title: "Fixture tank".into(),
        status: "Todo".into(),
        project_name: Some("Aquarium".into()),
        url: None,
        tank_id: None,
    }]
}

fn linear_mode_from_env() -> LinearTransport {
    #[cfg(debug_assertions)]
    {
        // Explicit mock only. A custom endpoint must not mint source:linear
        // against a stand-in server in a debug desktop.
        if std::env::var("AQUARIUM_LINEAR_MOCK").ok().as_deref() == Some("1") {
            return LinearTransport::Mock;
        }
    }
    LinearTransport::Live {
        endpoint: LINEAR_GRAPHQL_URL.to_string(),
    }
}

fn issue_from_graphql(node: &Value, source: &str) -> Option<LinearIssue> {
    let id = node.get("id")?.as_str()?.to_string();
    let identifier = node.get("identifier")?.as_str()?.to_string();
    if id.is_empty() || identifier.is_empty() {
        return None;
    }
    Some(LinearIssue {
        source: source.into(),
        id,
        identifier,
        title: node
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        status: node
            .get("state")
            .and_then(|state| state.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        project_name: node
            .get("project")
            .and_then(|project| project.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        url: node.get("url").and_then(Value::as_str).map(str::to_string),
        tank_id: None,
    })
}

async fn graphql(
    endpoint: &str,
    api_key: &str,
    query: &str,
    variables: Value,
) -> Result<Value, String> {
    let header = HeaderValue::from_str(api_key)
        .map_err(|_| "linear api key is not a valid HTTP header".to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "linear http client unavailable".to_string())?;
    let response = client
        .post(endpoint)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, header)
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .map_err(|err| redact_secret(&classify_linear_http_error(&err), api_key))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| redact_secret(&classify_linear_http_error(&err), api_key))?;
    let body = redact_secret(&body, api_key);
    if !status.is_success() {
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err("linear rejected the API key".into());
        }
        return Err(format!("linear http {}", status.as_u16()));
    }
    let parsed: Value =
        serde_json::from_str(&body).map_err(|_| "linear returned a non-JSON body".to_string())?;
    if parsed
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err("linear graphql returned errors".into());
    }
    Ok(parsed)
}

fn classify_linear_http_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "linear unreachable: request timed out".into()
    } else if err.is_connect() {
        "linear unreachable: could not connect".into()
    } else {
        "linear unreachable: network error".into()
    }
}

fn connection_from_viewer(payload: &Value) -> Result<LinearConnection, String> {
    let data = payload
        .get("data")
        .ok_or_else(|| "linear connect missing data".to_string())?;
    let viewer = data
        .get("viewer")
        .filter(|viewer| viewer.is_object() && viewer.get("id").and_then(Value::as_str).is_some());
    if viewer.is_none() {
        return Err("linear connect did not return a viewer".into());
    }
    let workspace_name = data
        .get("organization")
        .and_then(|org| org.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(LinearConnection {
        source: "linear".into(),
        connected: true,
        workspace_name,
        message: "connected".into(),
    })
}

impl LinearTransport {
    async fn verify(&self, api_key: &str) -> Result<LinearConnection, String> {
        match self {
            LinearTransport::Mock => Ok(mock_connection()),
            LinearTransport::Live { endpoint } => {
                let payload = graphql(endpoint, api_key, CONNECT_QUERY, json!({})).await?;
                connection_from_viewer(&payload)
            }
        }
    }

    async fn search(&self, api_key: &str, query: &str) -> Result<Vec<LinearIssue>, String> {
        match self {
            LinearTransport::Mock => {
                let q = query.to_ascii_lowercase();
                Ok(mock_issues()
                    .into_iter()
                    .filter(|issue| {
                        q.is_empty()
                            || issue.title.to_ascii_lowercase().contains(&q)
                            || issue.identifier.to_ascii_lowercase().contains(&q)
                    })
                    .collect())
            }
            LinearTransport::Live { endpoint } => {
                let payload = if query.trim().is_empty() {
                    graphql(endpoint, api_key, LIST_QUERY, json!({})).await?
                } else {
                    graphql(endpoint, api_key, SEARCH_QUERY, json!({ "term": query })).await?
                };
                let nodes = if query.trim().is_empty() {
                    payload.pointer("/data/issues/nodes")
                } else {
                    payload.pointer("/data/searchIssues/nodes")
                };
                Ok(nodes
                    .and_then(Value::as_array)
                    .map(|nodes| {
                        nodes
                            .iter()
                            .filter_map(|node| issue_from_graphql(node, "linear"))
                            .collect()
                    })
                    .unwrap_or_default())
            }
        }
    }

    async fn get(&self, api_key: &str, id: &str) -> Result<Option<LinearIssue>, String> {
        match self {
            LinearTransport::Mock => Ok(mock_issues()
                .into_iter()
                .find(|issue| issue.id == id || issue.identifier == id)),
            LinearTransport::Live { endpoint } => {
                let payload = graphql(endpoint, api_key, GET_QUERY, json!({ "id": id })).await?;
                Ok(payload
                    .pointer("/data/issue")
                    .filter(|node| !node.is_null())
                    .and_then(|node| issue_from_graphql(node, "linear")))
            }
        }
    }
}

fn refuse_if_secret<T: Serialize>(value: &T, secret: &str, op: &str) -> Result<(), String> {
    let json = serde_json::to_value(value).unwrap_or(Value::Null);
    if json_has_secret(&json, secret) {
        return Err(format!("linear {op} refused to emit a credential"));
    }
    Ok(())
}

pub(crate) async fn connect_with(
    slot: &impl LinearSecretSlot,
    transport: &LinearTransport,
    api_key: String,
) -> Result<LinearConnection, String> {
    let key = api_key.trim().to_string();
    if key.is_empty() {
        return Err("linear api key is required".into());
    }
    let conn = transport
        .verify(&key)
        .await
        .map_err(|err| redact_secret(&err, &key))?;
    refuse_if_secret(&conn, &key, "connect")?;
    match conn.source.as_str() {
        "linear" if conn.connected => slot.store(&key)?,
        "mock" => slot.store(&key)?,
        _ => {}
    }
    Ok(conn)
}

pub(crate) async fn disconnect_with(
    slot: &impl LinearSecretSlot,
) -> Result<LinearConnection, String> {
    slot.delete()?;
    Ok(disconnected("not connected"))
}

pub(crate) async fn connection_with(
    slot: &impl LinearSecretSlot,
    transport: &LinearTransport,
) -> Result<LinearConnection, String> {
    let Some(key) = slot.load()? else {
        return Ok(disconnected("not connected"));
    };
    let conn = transport
        .verify(&key)
        .await
        .unwrap_or_else(|message| disconnected(&redact_secret(&message, &key)));
    refuse_if_secret(&conn, &key, "connection")?;
    Ok(conn)
}

pub(crate) async fn search_with(
    slot: &impl LinearSecretSlot,
    transport: &LinearTransport,
    query: String,
) -> Result<Vec<LinearIssue>, String> {
    let key = slot
        .load()?
        .ok_or_else(|| "linear is not connected".to_string())?;
    let issues = transport
        .search(&key, &query)
        .await
        .map_err(|err| redact_secret(&err, &key))?;
    refuse_if_secret(&issues, &key, "search")?;
    Ok(issues)
}

pub(crate) async fn get_with(
    slot: &impl LinearSecretSlot,
    transport: &LinearTransport,
    id: String,
) -> Result<Option<LinearIssue>, String> {
    let key = slot
        .load()?
        .ok_or_else(|| "linear is not connected".to_string())?;
    let issue = transport
        .get(&key, &id)
        .await
        .map_err(|err| redact_secret(&err, &key))?;
    refuse_if_secret(&issue, &key, "get issue")?;
    Ok(issue)
}

/// Store a Linear personal key and verify it against Linear GraphQL.
#[tauri::command]
pub async fn aquarium_linear_connect(api_key: String) -> Result<LinearConnection, String> {
    connect_with(&KeyringLinearSlot, &linear_mode_from_env(), api_key).await
}

/// Remove the Linear key from the OS keyring.
#[tauri::command]
pub async fn aquarium_linear_disconnect() -> Result<LinearConnection, String> {
    disconnect_with(&KeyringLinearSlot).await
}

/// Connection status. Re-verifies when a key is stored. Never returns the key.
#[tauri::command]
pub async fn aquarium_linear_connection() -> Result<LinearConnection, String> {
    connection_with(&KeyringLinearSlot, &linear_mode_from_env()).await
}

/// Search issues using the stored key natively.
#[tauri::command]
pub async fn aquarium_linear_search_issues(query: String) -> Result<Vec<LinearIssue>, String> {
    search_with(&KeyringLinearSlot, &linear_mode_from_env(), query).await
}

/// Fetch one issue using the stored key natively.
#[tauri::command]
pub async fn aquarium_linear_get_issue(id: String) -> Result<Option<LinearIssue>, String> {
    get_with(&KeyringLinearSlot, &linear_mode_from_env(), id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    struct MapSlot(Mutex<Option<String>>);

    impl LinearSecretSlot for MapSlot {
        fn store(&self, value: &str) -> Result<(), String> {
            *self.0.lock().expect("map slot") = Some(value.to_string());
            Ok(())
        }
        fn load(&self) -> Result<Option<String>, String> {
            Ok(self.0.lock().expect("map slot").clone())
        }
        fn delete(&self) -> Result<(), String> {
            *self.0.lock().expect("map slot") = None;
            Ok(())
        }
    }

    struct MockGraphql {
        endpoint: String,
        requests: Arc<AtomicUsize>,
        shutdown: Arc<std::sync::atomic::AtomicBool>,
        addr: std::net::SocketAddr,
        thread: Option<thread::JoinHandle<()>>,
    }

    impl Drop for MockGraphql {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::SeqCst);
            let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
            if let Some(handle) = self.thread.take() {
                let _ = handle.join();
            }
        }
    }

    fn spawn_mock(expected_key: &str) -> MockGraphql {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock linear");
        listener
            .set_nonblocking(false)
            .expect("blocking mock listener");
        let addr = listener.local_addr().expect("mock addr");
        let requests = Arc::new(AtomicUsize::new(0));
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let expected = expected_key.to_string();
        let reqs = Arc::clone(&requests);
        let stop = Arc::clone(&shutdown);
        let thread = thread::spawn(move || {
            listener.set_nonblocking(true).ok();
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        reqs.fetch_add(1, Ordering::SeqCst);
                        handle_mock(&mut stream, &expected);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        MockGraphql {
            endpoint: format!("http://{addr}/graphql"),
            requests,
            shutdown,
            addr,
            thread: Some(thread),
        }
    }

    fn handle_mock(stream: &mut TcpStream, expected_key: &str) {
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        let mut buf = Vec::new();
        let mut tmp = [0u8; 2048];
        let header_end;
        loop {
            match stream.read(&mut tmp) {
                Ok(0) => return,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
                Err(_) => return,
            }
            if let Some(idx) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = idx + 4;
                break;
            }
            if buf.len() > 64 * 1024 {
                return;
            }
        }
        let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    (name.eq_ignore_ascii_case("content-length"))
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
            })
            .unwrap_or(0);
        while buf.len() < header_end + content_length {
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
                Err(_) => break,
            }
        }
        let authorized = headers.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("authorization") && value.trim() == expected_key
            })
        });
        let body = String::from_utf8_lossy(&buf[header_end..]);
        let (status, payload) = if !authorized {
            (
                "401 Unauthorized",
                json!({"errors":[{"message":"Authentication required"}]}).to_string(),
            )
        } else if body.contains("searchIssues") {
            let term = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("variables")
                        .and_then(|vars| vars.get("term"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default()
                .to_ascii_lowercase();
            let issue = json!({
                "id": "issue-1",
                "identifier": "AQU-1",
                "title": "Real-shaped tank",
                "url": "https://linear.app/aquarium-test/issue/AQU-1",
                "state": { "name": "Todo" },
                "project": { "name": "Aquarium" }
            });
            let nodes =
                if term.is_empty() || "real-shaped tank".contains(&term) || "aqu-1".contains(&term)
                {
                    vec![issue]
                } else {
                    vec![]
                };
            (
                "200 OK",
                json!({ "data": { "searchIssues": { "nodes": nodes } } }).to_string(),
            )
        } else if body.contains("issues(") {
            (
                "200 OK",
                json!({
                    "data": {
                        "issues": {
                            "nodes": [{
                                "id": "issue-1",
                                "identifier": "AQU-1",
                                "title": "Real-shaped tank",
                                "url": "https://linear.app/aquarium-test/issue/AQU-1",
                                "state": { "name": "Todo" },
                                "project": { "name": "Aquarium" }
                            }]
                        }
                    }
                })
                .to_string(),
            )
        } else if body.contains("issue(") {
            let id = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("variables")
                        .and_then(|vars| vars.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            let node = if id == "issue-1" || id == "AQU-1" {
                json!({
                    "id": "issue-1",
                    "identifier": "AQU-1",
                    "title": "Real-shaped tank",
                    "url": "https://linear.app/aquarium-test/issue/AQU-1",
                    "state": { "name": "Todo" },
                    "project": { "name": "Aquarium" }
                })
            } else {
                Value::Null
            };
            ("200 OK", json!({ "data": { "issue": node } }).to_string())
        } else {
            (
                "200 OK",
                json!({
                    "data": {
                        "viewer": { "id": "user-1", "name": "Test User" },
                        "organization": { "id": "org-1", "name": "Aquarium Test Org", "urlKey": "aquarium-test" }
                    }
                })
                .to_string(),
            )
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        let _ = stream.write_all(response.as_bytes());
    }

    fn production_source(path: &str) -> &'static str {
        include_str!("aquarium_linear.rs")
            .split("#[cfg(test)]")
            .next()
            .expect(path)
    }

    #[test]
    fn production_has_no_magic_fixture_prefix() {
        let production = production_source("linear production");
        assert!(
            !production.contains("lin_fixture_"),
            "production must not special-case a fixture key prefix"
        );
        assert!(!production.contains("fn aquarium_linear_secret_get"));
        assert!(
            !production.contains("AQUARIUM_LINEAR_ENDPOINT"),
            "debug/production must not treat a custom endpoint as live Linear"
        );
        assert_eq!(LINEAR_GRAPHQL_URL, "https://api.linear.app/graphql");
    }

    #[test]
    fn env_endpoint_does_not_override_official_url() {
        let mode = linear_mode_from_env();
        match mode {
            LinearTransport::Mock => {}
            LinearTransport::Live { endpoint } => {
                assert_eq!(endpoint, LINEAR_GRAPHQL_URL);
            }
        }
    }

    #[test]
    fn linear_slot_is_not_identity_nsec() {
        assert_eq!(LINEAR_SECRET_SLOT, "aquarium-linear-api-key");
        assert_ne!(LINEAR_SECRET_SLOT, IDENTITY_SLOT);
        assert!(!LINEAR_SECRET_SLOT.contains("nsec"));
        assert!(!LINEAR_SECRET_SLOT.contains("identity"));
    }

    #[tokio::test]
    async fn explicit_mock_mode_is_source_mock_not_live() {
        let slot = MapSlot(Mutex::new(None));
        let key = "lin_fixture_demo_secret";
        let conn = connect_with(&slot, &LinearTransport::Mock, key.into())
            .await
            .unwrap();
        let json = serde_json::to_value(&conn).unwrap();
        assert!(!json_has_secret(&json, key));
        assert_eq!(conn.source, "mock");
        assert!(!conn.connected, "mock must not auto-bind as live Linear");
        let issues = search_with(&slot, &LinearTransport::Mock, "tank".into())
            .await
            .unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].source, "mock");
        assert!(issues.iter().all(|issue| issue.source == "mock"));
    }

    #[tokio::test]
    async fn magic_prefix_without_http_success_is_not_live_linear() {
        let server = spawn_mock("lin_api_real_test_key");
        let slot = MapSlot(Mutex::new(None));
        let transport = LinearTransport::Live {
            endpoint: server.endpoint.clone(),
        };
        let conn = connect_with(&slot, &transport, "lin_fixture_not_real".into())
            .await
            .unwrap_or_else(|message| disconnected(&message));
        assert_ne!(
            (conn.source.as_str(), conn.connected),
            ("linear", true),
            "a lin_fixture_ prefix must not skip verification: {conn:?}"
        );
        assert!(!conn.connected);
        assert!(!conn.message.contains("lin_fixture_not_real"));
        assert!(server.requests.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn viewer_success_via_mock_http_is_source_linear() {
        let key = "lin_api_real_test_key";
        let server = spawn_mock(key);
        let slot = MapSlot(Mutex::new(None));
        let transport = LinearTransport::Live {
            endpoint: server.endpoint.clone(),
        };
        let conn = connect_with(&slot, &transport, key.into()).await.unwrap();
        assert_eq!(conn.source, "linear");
        assert!(conn.connected);
        assert_eq!(conn.workspace_name.as_deref(), Some("Aquarium Test Org"));
        assert!(!json_has_secret(&serde_json::to_value(&conn).unwrap(), key));
        let issues = search_with(&slot, &transport, "tank".into()).await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].source, "linear");
        assert_eq!(issues[0].identifier, "AQU-1");
        let issue = get_with(&slot, &transport, "AQU-1".into())
            .await
            .unwrap()
            .expect("issue");
        assert_eq!(issue.source, "linear");
        assert_eq!(issue.id, "issue-1");
        assert!(server.requests.load(Ordering::SeqCst) >= 3);
    }

    #[tokio::test]
    async fn no_http_without_stored_key() {
        let server = spawn_mock("lin_api_real_test_key");
        let slot = MapSlot(Mutex::new(None));
        let transport = LinearTransport::Live {
            endpoint: server.endpoint.clone(),
        };
        let err = search_with(&slot, &transport, "tank".into())
            .await
            .unwrap_err();
        assert!(err.contains("not connected"), "{err}");
        assert_eq!(server.requests.load(Ordering::SeqCst), 0);
        let conn = connection_with(&slot, &transport).await.unwrap();
        assert!(!conn.connected);
        assert_eq!(server.requests.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn rejected_key_does_not_echo_secret() {
        let server = spawn_mock("lin_api_real_test_key");
        let slot = MapSlot(Mutex::new(None));
        let transport = LinearTransport::Live {
            endpoint: server.endpoint.clone(),
        };
        let key = "lin_api_wrong_secret_value";
        let err = connect_with(&slot, &transport, key.into())
            .await
            .unwrap_err();
        assert!(!err.contains(key), "{err}");
        assert!(err.contains("rejected") || err.contains("linear"), "{err}");
        assert!(slot.load().unwrap().is_none());
    }

    #[tokio::test]
    async fn map_slot_round_trip_delete() {
        let slot = MapSlot(Mutex::new(None));
        slot.store("lin_api_x").unwrap();
        assert_eq!(slot.load().unwrap().as_deref(), Some("lin_api_x"));
        disconnect_with(&slot).await.unwrap();
        assert!(slot.load().unwrap().is_none());
    }
}
