//! Agent process ownership on the execution host. Cwd is the checkout.

use crate::path_guard::assert_existing_inside;
use crate::DeviceError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};

/// Evidence that a session process is bound to a checkout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvidence {
    /// Session id.
    pub session_id: String,
    /// Process id.
    pub pid: u32,
    /// Working directory of the process.
    pub cwd: String,
    /// Path of the cwd proof file written by the fixture agent.
    pub cwd_proof_file: String,
}

/// Spawn the fixture agent (or `--agent-command`) with cwd = checkout.
pub fn spawn_fixture_agent(
    allowed_root: &Path,
    checkout: &Path,
    session_id: &str,
    agent_command: Option<&[String]>,
) -> Result<SessionEvidence, DeviceError> {
    let cwd = assert_existing_inside(allowed_root, checkout)?;
    let mut cmd = if let Some(parts) = agent_command {
        let (first, rest) = parts
            .split_first()
            .ok_or_else(|| DeviceError::Agent("agent command is empty".into()))?;
        let mut cmd = Command::new(first);
        cmd.args(rest);
        cmd
    } else {
        let exe =
            std::env::current_exe().map_err(|e| DeviceError::Agent(format!("current_exe: {e}")))?;
        let mut cmd = Command::new(exe);
        cmd.arg("agent-fixture");
        cmd.arg("--session-id");
        cmd.arg(session_id);
        cmd
    };
    cmd.current_dir(&cwd);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| DeviceError::Agent(format!("spawn: {e}")))?;
    let pid = child.id();
    let proof = cwd.join(".aquarium-session-cwd");
    if let Err(error) = wait_for_file(&proof, 50) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    std::mem::forget(child);
    let recorded = std::fs::read_to_string(&proof)
        .map_err(|e| DeviceError::Agent(format!("cwd proof: {e}")))?;
    let recorded = recorded.trim().to_string();
    let expected = cwd.display().to_string();
    if recorded != expected {
        return Err(DeviceError::Agent(format!(
            "agent cwd {recorded} != checkout {expected}"
        )));
    }
    Ok(SessionEvidence {
        session_id: session_id.to_string(),
        pid,
        cwd: expected,
        cwd_proof_file: proof.display().to_string(),
    })
}

/// Terminate a previously spawned session. `pid` must be a host-tracked session.
pub fn cancel_session(pid: u32) -> Result<(), DeviceError> {
    if pid == 0 {
        return Err(DeviceError::Agent("refusing to signal pid 0".into()));
    }
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .map_err(|e| DeviceError::Agent(format!("kill: {e}")))?;
    if !status.success() {
        // ESRCH / already-stopped is success for cancel.
        return Ok(());
    }
    let _ = status;
    Ok(())
}

/// Fixture agent body: write cwd, then wait for SIGTERM.
pub fn run_agent_fixture(session_id: &str) -> Result<(), DeviceError> {
    let cwd = std::env::current_dir().map_err(|e| DeviceError::Agent(e.to_string()))?;
    std::fs::write(cwd.join(".aquarium-session-id"), session_id)
        .map_err(|e| DeviceError::Agent(e.to_string()))?;
    std::fs::write(
        cwd.join(".aquarium-session-cwd"),
        format!("{}\n", cwd.display()),
    )
    .map_err(|e| DeviceError::Agent(e.to_string()))?;
    std::fs::write(
        cwd.join(".aquarium-session-pid"),
        format!("{}\n", std::process::id()),
    )
    .map_err(|e| DeviceError::Agent(e.to_string()))?;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn wait_for_file(path: &Path, attempts: u32) -> Result<(), DeviceError> {
    for _ in 0..attempts {
        if path.exists() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err(DeviceError::Agent(format!(
        "timed out waiting for {}",
        path.display()
    )))
}
