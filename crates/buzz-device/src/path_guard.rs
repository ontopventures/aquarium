//! Canonicalize-and-prefix path fence. Symlinks that escape the root fail.

use crate::DeviceError;
use buzz_core::device::relative_path_is_safe;
use std::path::{Path, PathBuf};

/// Resolve `root/rel` so the canonical result stays inside `root`.
pub fn resolve_under_root(root: &Path, rel: &str) -> Result<PathBuf, DeviceError> {
    relative_path_is_safe(rel).map_err(DeviceError::Protocol)?;
    let root = std::fs::canonicalize(root)
        .map_err(|e| DeviceError::Path(format!("allowed root is not accessible: {e}")))?;
    if !root.is_dir() {
        return Err(DeviceError::Path("allowed root is not a directory".into()));
    }
    let candidate = root.join(rel);
    if candidate.exists() {
        let resolved = std::fs::canonicalize(&candidate)
            .map_err(|e| DeviceError::Path(format!("path resolve failed: {e}")))?;
        if !resolved.starts_with(&root) {
            return Err(DeviceError::Path(
                "resolved path escaped the allowed root".into(),
            ));
        }
        return Ok(resolved);
    }
    let parent = candidate.parent().unwrap_or(&root);
    if parent.exists() {
        let resolved_parent = std::fs::canonicalize(parent)
            .map_err(|e| DeviceError::Path(format!("parent resolve failed: {e}")))?;
        if !resolved_parent.starts_with(&root) {
            return Err(DeviceError::Path(
                "path parent escaped the allowed root".into(),
            ));
        }
    } else if !parent.starts_with(&root) {
        return Err(DeviceError::Path(
            "path parent is outside the allowed root".into(),
        ));
    }
    Ok(candidate)
}

/// Confirm an existing path still sits inside `root` after symlink resolution.
pub fn assert_existing_inside(root: &Path, path: &Path) -> Result<PathBuf, DeviceError> {
    let root = std::fs::canonicalize(root)
        .map_err(|e| DeviceError::Path(format!("allowed root is not accessible: {e}")))?;
    let resolved = std::fs::canonicalize(path)
        .map_err(|e| DeviceError::Path(format!("path resolve failed: {e}")))?;
    if !resolved.starts_with(&root) {
        return Err(DeviceError::Path(
            "resolved path escaped the allowed root".into(),
        ));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn rejects_dotdot_and_absolute() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_under_root(tmp.path(), "../escape").is_err());
        assert!(resolve_under_root(tmp.path(), "/tmp").is_err());
    }

    #[test]
    fn accepts_nested_relative() {
        let tmp = tempfile::tempdir().unwrap();
        let resolved = resolve_under_root(tmp.path(), "tanks/a").unwrap();
        assert!(resolved.starts_with(tmp.path().canonicalize().unwrap()));
    }

    #[test]
    fn rejects_symlink_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = tmp.path().join("link");
        symlink(outside.path(), &link).unwrap();
        let err = resolve_under_root(tmp.path(), "link").unwrap_err();
        assert!(err.to_string().contains("escaped"));
    }
}
