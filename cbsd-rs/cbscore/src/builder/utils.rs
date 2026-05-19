// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Shared builder helpers — scratch-dir setup, per-component path
//! derivation, common error-wrapping shims that the per-stage
//! modules ([`super::prepare`], future `rpmbuild`, `signing`,
//! `upload`) reuse.

use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::builder::BuilderError;

/// Map an [`std::io::Error`] to a [`BuilderError::Io`] capturing the
/// path that the failed operation targeted.
#[must_use]
pub fn io_err(path: &Utf8Path, source: std::io::Error) -> BuilderError {
    BuilderError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Derive the per-component scratch sub-path under
/// `config.paths.scratch/<component>`.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore::builder::utils::component_scratch_dir;
///
/// let p = component_scratch_dir(Utf8Path::new("/srv/scratch"), "ceph");
/// assert_eq!(p.as_str(), "/srv/scratch/ceph");
/// ```
#[must_use]
pub fn component_scratch_dir(scratch_root: &Utf8Path, component: &str) -> Utf8PathBuf {
    scratch_root.join(component)
}

/// Create `path` (and parent dirs as needed) idempotently. Maps any
/// IO failure to [`BuilderError::Io`].
///
/// # Errors
///
/// Returns [`BuilderError::Io`] when `create_dir_all` fails.
pub async fn ensure_dir(path: &Utf8Path) -> Result<(), BuilderError> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|e| io_err(path, e))
}

/// Recursively remove `path`. Missing-path is a no-op (matches
/// `rm -rf`). Maps any other IO failure to [`BuilderError::Io`].
///
/// # Errors
///
/// Returns [`BuilderError::Io`] when the underlying remove fails
/// for a reason other than "path does not exist".
pub async fn remove_dir_all_if_present(path: &Utf8Path) -> Result<(), BuilderError> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(path, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_dir_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = Utf8PathBuf::from_path_buf(tmp.path().join("a/b/c")).expect("utf8 path");
        ensure_dir(&path).await.expect("create");
        ensure_dir(&path).await.expect("re-create");
        assert!(path.exists());
    }

    #[tokio::test]
    async fn remove_missing_is_ok() {
        let path = Utf8PathBuf::from("/nonexistent/builder/test/dir");
        remove_dir_all_if_present(&path).await.expect("noop");
    }

    #[tokio::test]
    async fn remove_existing_clears_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = Utf8PathBuf::from_path_buf(tmp.path().join("a/b")).expect("utf8 path");
        tokio::fs::create_dir_all(&path).await.expect("create");
        tokio::fs::write(path.join("file.txt"), b"hello")
            .await
            .expect("write");
        remove_dir_all_if_present(&path).await.expect("remove");
        assert!(!path.exists());
    }

    #[test]
    fn component_scratch_dir_joins() {
        assert_eq!(
            component_scratch_dir(Utf8Path::new("/srv/scratch"), "ceph").as_str(),
            "/srv/scratch/ceph",
        );
    }
}
