//! Bounded `git worktree add` on the execution host only.

use crate::path_guard::{assert_existing_inside, resolve_under_root};
use crate::DeviceError;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const STDOUT_CAP: usize = 1_048_576;
const STDERR_CAP: usize = 65_536;
const GIT_TIMEOUT: Duration = Duration::from_secs(60);

/// Evidence that a checkout exists on this host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutEvidence {
    /// Absolute worktree path.
    pub path: String,
    /// Branch name.
    pub branch: String,
    /// HEAD commit.
    pub head: String,
    /// Host identifier (hostname).
    pub host: String,
}

/// Create a branch/worktree under `allowed_root`.
pub fn create_worktree(
    allowed_root: &Path,
    repo_relpath: &str,
    worktree_relpath: &str,
    branch: &str,
    start_rev: &str,
) -> Result<CheckoutEvidence, DeviceError> {
    let repo = resolve_under_root(allowed_root, repo_relpath)?;
    if !repo.join(".git").exists() {
        return Err(DeviceError::Git(
            "repository path is not a git checkout".into(),
        ));
    }
    let worktree = resolve_under_root(allowed_root, worktree_relpath)?;
    if worktree.exists() {
        return Err(DeviceError::Git(format!(
            "worktree path already exists: {}",
            worktree.display()
        )));
    }
    if branch.starts_with('-') || start_rev.starts_with('-') {
        return Err(DeviceError::Git(
            "branch/rev must not start with '-'".into(),
        ));
    }
    if let Some(parent) = worktree.parent() {
        std::fs::create_dir_all(parent).map_err(|e| DeviceError::Path(e.to_string()))?;
    }
    let worktree_str = worktree
        .to_str()
        .ok_or_else(|| DeviceError::Git("worktree path is not utf-8".into()))?
        .to_string();
    run_git(
        &[
            "worktree",
            "add",
            "-b",
            branch,
            "--",
            worktree_str.as_str(),
            start_rev,
        ],
        Some(&repo),
    )?;
    let resolved = assert_existing_inside(allowed_root, &worktree)?;
    let head = run_git(&["rev-parse", "HEAD"], Some(&resolved))?
        .trim()
        .to_string();
    Ok(CheckoutEvidence {
        path: resolved.display().to_string(),
        branch: branch.to_string(),
        head,
        host: hostname(),
    })
}

/// `git worktree list --porcelain` from the repo.
pub fn list_worktrees(repo: &Path) -> Result<String, DeviceError> {
    run_git(&["worktree", "list", "--porcelain"], Some(repo))
}

fn hostname() -> String {
    std::process::Command::new("scutil")
        .args(["--get", "LocalHostName"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|out| String::from_utf8(out.stdout).ok())
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown-host".to_string())
}

fn run_git(args: &[&str], cwd: Option<&Path>) -> Result<String, DeviceError> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| DeviceError::Git(format!("spawn git: {e}")))?;
    let deadline = Instant::now() + GIT_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = read_capped(child.stdout.take(), STDOUT_CAP);
                let stderr = read_capped(child.stderr.take(), STDERR_CAP);
                if !status.success() {
                    return Err(DeviceError::Git(format!(
                        "git {} failed: {stderr}",
                        args.first().unwrap_or(&"")
                    )));
                }
                return Ok(stdout);
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(DeviceError::Git("git timed out after 60s".into()));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(DeviceError::Git(format!("wait git: {e}")));
            }
        }
    }
}

fn read_capped(pipe: Option<impl Read>, cap: usize) -> String {
    let Some(mut pipe) = pipe else {
        return String::new();
    };
    let mut buf = vec![0u8; 8192];
    let mut out = Vec::new();
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let remaining = cap.saturating_sub(out.len());
                out.extend_from_slice(&buf[..n.min(remaining)]);
                if out.len() >= cap {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
pub(crate) fn init_fixture_repo(dir: &Path) -> Result<(), DeviceError> {
    std::fs::create_dir_all(dir).map_err(|e| DeviceError::Path(e.to_string()))?;
    run_git(&["init", "-b", "main"], Some(dir))?;
    run_git(
        &["config", "user.email", "device-prototype@example.test"],
        Some(dir),
    )?;
    run_git(&["config", "user.name", "Device Prototype"], Some(dir))?;
    std::fs::write(dir.join("README"), "fixture\n")
        .map_err(|e| DeviceError::Path(e.to_string()))?;
    run_git(&["add", "README"], Some(dir))?;
    run_git(&["commit", "-m", "init"], Some(dir))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_worktree_is_idempotent_only_via_caller_journal() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        init_fixture_repo(&repo).unwrap();
        let evidence =
            create_worktree(tmp.path(), "repo", "tanks/t1", "aquarium/t1", "HEAD").unwrap();
        assert!(std::path::PathBuf::from(&evidence.path)
            .join("README")
            .exists());
        let list = list_worktrees(&repo).unwrap();
        assert!(list.contains("tanks/t1") || list.contains(&evidence.path));
        assert!(create_worktree(tmp.path(), "repo", "tanks/t1", "aquarium/t1", "HEAD").is_err());
    }
}
