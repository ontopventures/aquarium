//! Two OS processes through the signed-event mux. Not isolated Buzz-relay proof.

use nostr::{Keys, ToBech32};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn wait_line(child: &mut Child, timeout: Duration) -> String {
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);
    let start = Instant::now();
    let mut line = String::new();
    while start.elapsed() < timeout {
        line.clear();
        if reader.read_line(&mut line).ok().is_some_and(|n| n > 0) {
            return line.trim().to_string();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for mux bind line");
}

fn init_repo(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "device-prototype@example.test"],
        vec!["config", "user.name", "Device Prototype"],
    ] {
        assert!(Command::new("git")
            .args(&args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
    }
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

#[test]
fn controller_does_not_create_worktree_locally() {
    let bin = env!("CARGO_BIN_EXE_buzz-device");
    let tmp = tempfile::tempdir().unwrap();
    init_repo(&tmp.path().join("repo"));
    let host = Keys::generate();
    let owner = Keys::generate();
    let host_nsec = tmp.path().join("host.nsec");
    let owner_nsec = tmp.path().join("owner.nsec");
    std::fs::write(&host_nsec, host.secret_key().to_bech32().unwrap()).unwrap();
    std::fs::write(&owner_nsec, owner.secret_key().to_bech32().unwrap()).unwrap();
    let grant = serde_json::json!({
        "device_id": "dev-1",
        "device_pubkey_hex": host.public_key().to_hex(),
        "owner_pubkey_hex": owner.public_key().to_hex(),
        "actor_pubkeys": [],
        "allowed_roots": [tmp.path().display().to_string()],
        "generation": 1,
        "revoked": false
    });
    let grant_path = tmp.path().join("grant.json");
    std::fs::write(&grant_path, serde_json::to_vec_pretty(&grant).unwrap()).unwrap();

    let mut mux = Command::new(bin)
        .args(["mux", "--bind", "127.0.0.1:0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let addr = wait_line(&mut mux, Duration::from_secs(5));
    let relay = format!("ws://{addr}");

    let mut serve = Command::new(bin)
        .args([
            "serve",
            "--state-dir",
            tmp.path().join("state").to_str().unwrap(),
            "--grant",
            grant_path.to_str().unwrap(),
            "--nsec-file",
            host_nsec.to_str().unwrap(),
            "--relay",
            &relay,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));

    let ctl_cwd = tempfile::tempdir().unwrap();
    let ctl = Command::new(bin)
        .current_dir(ctl_cwd.path())
        .args([
            "ctl",
            "--nsec-file",
            owner_nsec.to_str().unwrap(),
            "--device-pubkey",
            &host.public_key().to_hex(),
            "--device-id",
            "dev-1",
            "--relay",
            &relay,
            "--request-id",
            "1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "create-checkout",
            "--tank-id",
            "tank-1",
            "--repository-id",
            "repo",
            "--branch",
            "aquarium/t1",
            "--relpath",
            "tanks/t1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&ctl.stdout);
    let stderr = String::from_utf8_lossy(&ctl.stderr);
    assert!(ctl.status.success(), "ctl failed: {stderr} {stdout}");
    assert!(stdout.contains("succeeded") || stdout.contains("path"));
    let worktree = PathBuf::from(tmp.path()).join("tanks/t1");
    assert!(
        worktree.join("README").exists(),
        "host must create the worktree"
    );
    assert!(
        !ctl_cwd.path().join("tanks").exists(),
        "ctl cwd must not grow a local worktree"
    );

    let _ = serve.kill();
    let _ = serve.wait();
    let _ = mux.kill();
    let _ = mux.wait();
}

#[test]
fn unauthorized_ctl_does_not_mutate_host() {
    let bin = env!("CARGO_BIN_EXE_buzz-device");
    let tmp = tempfile::tempdir().unwrap();
    init_repo(&tmp.path().join("repo"));
    let host = Keys::generate();
    let owner = Keys::generate();
    let stranger = Keys::generate();
    let host_nsec = tmp.path().join("host.nsec");
    let stranger_nsec = tmp.path().join("stranger.nsec");
    std::fs::write(&host_nsec, host.secret_key().to_bech32().unwrap()).unwrap();
    std::fs::write(&stranger_nsec, stranger.secret_key().to_bech32().unwrap()).unwrap();
    let grant = serde_json::json!({
        "device_id": "dev-1",
        "device_pubkey_hex": host.public_key().to_hex(),
        "owner_pubkey_hex": owner.public_key().to_hex(),
        "actor_pubkeys": [],
        "allowed_roots": [tmp.path().display().to_string()],
        "generation": 1,
        "revoked": false
    });
    let grant_path = tmp.path().join("grant.json");
    std::fs::write(&grant_path, serde_json::to_vec_pretty(&grant).unwrap()).unwrap();

    let mut mux = Command::new(bin)
        .args(["mux", "--bind", "127.0.0.1:0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let addr = wait_line(&mut mux, Duration::from_secs(5));
    let relay = format!("ws://{addr}");
    let mut serve = Command::new(bin)
        .args([
            "serve",
            "--state-dir",
            tmp.path().join("state").to_str().unwrap(),
            "--grant",
            grant_path.to_str().unwrap(),
            "--nsec-file",
            host_nsec.to_str().unwrap(),
            "--relay",
            &relay,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));

    let ctl = Command::new(bin)
        .args([
            "ctl",
            "--nsec-file",
            stranger_nsec.to_str().unwrap(),
            "--device-pubkey",
            &host.public_key().to_hex(),
            "--device-id",
            "dev-1",
            "--relay",
            &relay,
            "--request-id",
            "1788581600001-cccccccccccccccccccccccccccccccc",
            "create-checkout",
            "--tank-id",
            "tank-1",
            "--repository-id",
            "repo",
            "--branch",
            "aquarium/nope",
            "--relpath",
            "tanks/nope",
        ])
        .output()
        .unwrap();
    assert!(!ctl.status.success());
    assert!(!tmp.path().join("tanks/nope").exists());
    let _ = serve.kill();
    let _ = serve.wait();
    let _ = mux.kill();
    let _ = mux.wait();
}

#[test]
fn missing_device_pubkey_is_a_cli_error() {
    let bin = env!("CARGO_BIN_EXE_buzz-device");
    let out = Command::new(bin)
        .args([
            "ctl",
            "--nsec-file",
            "/dev/null",
            "--device-id",
            "x",
            "--relay",
            "ws://127.0.0.1:1",
            "inspect-capabilities",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn coord_leader_worker_through_mux_is_fixture_transport() {
    let bin = env!("CARGO_BIN_EXE_buzz-device");
    let tmp = tempfile::tempdir().unwrap();
    let tank = tmp.path();
    let leader = Keys::generate();
    let worker = Keys::generate();
    let leader_nsec = tmp.path().join("leader.nsec");
    let worker_nsec = tmp.path().join("worker.nsec");
    std::fs::write(&leader_nsec, leader.secret_key().to_bech32().unwrap()).unwrap();
    std::fs::write(&worker_nsec, worker.secret_key().to_bech32().unwrap()).unwrap();

    let mut mux = Command::new(bin)
        .args(["mux", "--bind", "127.0.0.1:0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let addr = wait_line(&mut mux, Duration::from_secs(5));
    let relay = format!("ws://{addr}");
    let conversation = "00000000-0000-4000-8000-000000000001";
    let assignment = "1788581600001-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let mut worker_proc = Command::new(bin)
        .args([
            "coord-worker",
            "--nsec-file",
            worker_nsec.to_str().unwrap(),
            "--leader-pubkey",
            &leader.public_key().to_hex(),
            "--conversation-id",
            conversation,
            "--tank-id",
            "tank-coord",
            "--relay",
            &relay,
            "--state-dir",
            tmp.path().join("wstate").to_str().unwrap(),
            "--cwd",
            tank.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));

    let leader_out = Command::new(bin)
        .args([
            "coord-leader",
            "--nsec-file",
            leader_nsec.to_str().unwrap(),
            "--worker-pubkey",
            &worker.public_key().to_hex(),
            "--conversation-id",
            conversation,
            "--tank-id",
            "tank-coord",
            "--task",
            "write-hello",
            "--relay",
            &relay,
            "--state-dir",
            tmp.path().join("lstate").to_str().unwrap(),
            "--cwd",
            tank.to_str().unwrap(),
            "--assignment-id",
            assignment,
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&leader_out.stdout);
    let stderr = String::from_utf8_lossy(&leader_out.stderr);
    assert!(
        leader_out.status.success(),
        "coord-leader failed: {stderr} {stdout}"
    );
    assert!(stdout.contains("continued"));
    assert!(tank.join(format!("COORD-{assignment}.txt")).exists());
    assert!(tank.join(format!("CONTINUE-{assignment}.txt")).exists());
    let _ = worker_proc.kill();
    let _ = worker_proc.wait();
    let _ = mux.kill();
    let _ = mux.wait();
}
