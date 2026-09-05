//! In-process host proofs: real git worktrees, authz, retry, no local fallback.

use buzz_core::device::DEVICE_PROTOCOL_VERSION;
use buzz_device::{
    generate_request_id, handle_request, DeviceRequest, DeviceService, GrantFile, ReceiptStatus,
};
use nostr::Keys;
use std::path::Path;
use std::process::Command;

fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    assert!(Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(dir)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["config", "user.email", "device-prototype@example.test"])
        .current_dir(dir)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["config", "user.name", "Device Prototype"])
        .current_dir(dir)
        .status()
        .unwrap()
        .success());
    std::fs::write(dir.join("README"), "fixture\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "README"])
        .current_dir(dir)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .status()
        .unwrap()
        .success());
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64
}

fn request_id() -> String {
    generate_request_id()
}

fn setup() -> (tempfile::TempDir, DeviceService, Keys, Keys) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    init_repo(&repo);
    let host = Keys::generate();
    let owner = Keys::generate();
    let grant = GrantFile {
        device_id: "dev-1".into(),
        device_pubkey_hex: host.public_key().to_hex(),
        owner_pubkey_hex: owner.public_key().to_hex(),
        actor_pubkeys: vec![],
        allowed_roots: vec![tmp.path().display().to_string()],
        generation: 1,
        expires_at_ms: None,
        revoked: false,
    };
    let service = DeviceService::open(tmp.path(), grant, None).unwrap();
    (tmp, service, host, owner)
}

fn checkout_req(device_id: &str, relpath: &str, branch: &str) -> DeviceRequest {
    DeviceRequest {
        v: DEVICE_PROTOCOL_VERSION,
        request_id: request_id(),
        op: "create_checkout".into(),
        grant_generation: 1,
        device_id: device_id.into(),
        params: serde_json::json!({
            "tank_id": "tank-1",
            "branch": branch,
            "relpath": relpath,
            "repo_relpath": "repo",
            "start_rev": "HEAD",
        }),
    }
}

#[test]
fn authorized_create_checkout_makes_one_worktree() {
    let (tmp, service, host, owner) = setup();
    let req = checkout_req("dev-1", "tanks/t1", "aquarium/t1");
    let outcome = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(outcome.receipt.status, ReceiptStatus::Succeeded);
    let path = outcome.receipt.evidence["path"].as_str().unwrap();
    assert!(Path::new(path).join("README").exists());
    let head = outcome.receipt.evidence["head"].as_str().unwrap();
    assert_eq!(head.len(), 40);
    let list = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(tmp.path().join("repo"))
        .output()
        .unwrap();
    let listed = String::from_utf8_lossy(&list.stdout);
    assert!(listed.contains(path) || listed.contains("tanks/t1"));
}

#[test]
fn retry_same_id_does_not_duplicate_worktree() {
    let (tmp, service, host, owner) = setup();
    let req = checkout_req("dev-1", "tanks/t1", "aquarium/t1");
    let first = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    let second = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(second.receipt.status, ReceiptStatus::Succeeded);
    assert!(!second.mutated);
    assert_eq!(
        first.receipt.evidence["path"],
        second.receipt.evidence["path"]
    );
    let mut count = 0;
    for entry in std::fs::read_dir(tmp.path().join("tanks")).unwrap() {
        if entry.unwrap().path().is_dir() {
            count += 1;
        }
    }
    assert_eq!(count, 1);
}

#[test]
fn different_params_same_id_conflict() {
    let (_tmp, service, host, owner) = setup();
    let mut req = checkout_req("dev-1", "tanks/t1", "aquarium/t1");
    handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    req.params["branch"] = serde_json::json!("aquarium/other");
    let outcome = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(outcome.receipt.status, ReceiptStatus::Conflict);
    assert!(!outcome.mutated);
}

#[test]
fn unauthorized_cannot_overwrite_successful_journal() {
    let (tmp, service, host, owner) = setup();
    let req = checkout_req("dev-1", "tanks/t1", "aquarium/t1");
    let ok = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(ok.receipt.status, ReceiptStatus::Succeeded);
    let stranger = Keys::generate();
    let denied = handle_request(
        &service,
        &stranger.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(denied.receipt.status, ReceiptStatus::Rejected);
    let replay = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(replay.receipt.status, ReceiptStatus::Succeeded);
    assert!(tmp.path().join("tanks/t1/README").exists());
}

#[test]
fn unauthorized_actor_does_not_mutate() {
    let (tmp, service, host, _owner) = setup();
    let stranger = Keys::generate();
    let req = checkout_req("dev-1", "tanks/nope", "aquarium/nope");
    let outcome = handle_request(
        &service,
        &stranger.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(outcome.receipt.status, ReceiptStatus::Rejected);
    assert!(!Path::new(&tmp.path().join("tanks/nope")).exists());
}

#[test]
fn wrong_device_pubkey_does_not_mutate() {
    let (tmp, service, _host, owner) = setup();
    let other = Keys::generate();
    let req = checkout_req("dev-1", "tanks/wrong", "aquarium/wrong");
    let outcome = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &other.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(outcome.receipt.status, ReceiptStatus::Rejected);
    assert!(!tmp.path().join("tanks/wrong").exists());
}

#[test]
fn path_escape_is_rejected() {
    let (tmp, service, host, owner) = setup();
    let mut req = checkout_req("dev-1", "../escape", "aquarium/escape");
    req.request_id = request_id();
    let outcome = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_ne!(outcome.receipt.status, ReceiptStatus::Succeeded);
    assert!(!outcome.mutated);
    assert!(!tmp.path().join("tanks").exists());
}

#[test]
fn expired_request_is_rejected() {
    let (tmp, service, host, owner) = setup();
    let mut req = checkout_req("dev-1", "tanks/old", "aquarium/old");
    req.request_id = "1000000000000-0123456789abcdef0123456789abcdef".into();
    let outcome = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(outcome.receipt.status, ReceiptStatus::Rejected);
    assert!(!tmp.path().join("tanks/old").exists());
}

#[test]
fn revoked_grant_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(&tmp.path().join("repo"));
    let host = Keys::generate();
    let owner = Keys::generate();
    let grant = GrantFile {
        device_id: "dev-1".into(),
        device_pubkey_hex: host.public_key().to_hex(),
        owner_pubkey_hex: owner.public_key().to_hex(),
        actor_pubkeys: vec![],
        allowed_roots: vec![tmp.path().display().to_string()],
        generation: 2,
        expires_at_ms: None,
        revoked: true,
    };
    let service = DeviceService::open(tmp.path(), grant, None).unwrap();
    let mut req = checkout_req("dev-1", "tanks/revoked", "aquarium/revoked");
    req.grant_generation = 2;
    let outcome = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    assert_eq!(outcome.receipt.status, ReceiptStatus::Rejected);
    assert!(!tmp.path().join("tanks/revoked").exists());
}

#[test]
fn start_session_cwd_is_checkout() {
    let (tmp, service, host, owner) = setup();
    let req = checkout_req("dev-1", "tanks/t1", "aquarium/t1");
    let created = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &req,
        now_ms(),
    )
    .unwrap();
    let path = created.receipt.evidence["path"]
        .as_str()
        .unwrap()
        .to_string();
    let agent_cmd = vec![
        "/bin/sh".into(),
        "-c".into(),
        "pwd > .aquarium-session-cwd; exec sleep 30".into(),
    ];
    let service = DeviceService::open(
        tmp.path(),
        GrantFile {
            device_id: "dev-1".into(),
            device_pubkey_hex: host.public_key().to_hex(),
            owner_pubkey_hex: owner.public_key().to_hex(),
            actor_pubkeys: vec![],
            allowed_roots: vec![tmp.path().display().to_string()],
            generation: 1,
            expires_at_ms: None,
            revoked: false,
        },
        Some(agent_cmd),
    )
    .unwrap();
    let start = DeviceRequest {
        v: DEVICE_PROTOCOL_VERSION,
        request_id: request_id(),
        op: "start_session".into(),
        grant_generation: 1,
        device_id: "dev-1".into(),
        params: serde_json::json!({
            "checkout_path": path,
            "session_id": "sess-1",
        }),
    };
    let outcome = handle_request(
        &service,
        &owner.public_key().to_hex(),
        &host.public_key().to_hex(),
        &start,
        now_ms(),
    )
    .unwrap();
    assert_eq!(outcome.receipt.status, ReceiptStatus::Succeeded);
    let cwd = outcome.receipt.evidence["cwd"].as_str().unwrap();
    assert_eq!(cwd, path);
    let pid = outcome.receipt.evidence["pid"].as_u64().unwrap();
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
}
