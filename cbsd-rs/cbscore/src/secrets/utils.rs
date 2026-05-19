// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Shared utilities for the secrets manager and per-family modules.
//!
//! Centralises the "owner-only write" pattern: every secrets-bearing
//! file written by cbscore-rs (the runner-mounted secrets dump, the
//! SSH-key tempfile, the GPG keyring tempfile) lands at mode 0600
//! via an atomic create + rename, so a concurrent reader can never
//! observe a partial or wider-than-owner file.

use camino::Utf8Path;
use cbscore_types::utils::secrets::SecretsError;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

/// Write `contents` to `path` with mode 0600, atomically.
///
/// Serialises to a sibling tempfile (`<path>.<random>.tmp`), `fsync`s
/// it, then renames over the final path. The tempfile is created with
/// `O_CREAT | O_TRUNC | O_WRONLY` and mode `0o600`, so the file is
/// owner-only from the moment it exists at the final path — secrets
/// are never world-readable even transiently. Rename within the same
/// directory is atomic on Linux (POSIX `rename(2)` guarantee).
///
/// # Errors
///
/// Returns [`SecretsError::Manager`] wrapping the underlying IO error
/// if any step (tempfile create, write, fsync, rename) fails.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
/// use cbscore::secrets::utils::write_secure_file;
///
/// # async fn demo() -> Result<(), cbscore_types::utils::secrets::SecretsError> {
/// write_secure_file(
///     Utf8Path::new("/run/secrets/cbs-build.secrets.yaml"),
///     b"git: {}\nstorage: {}\nsigning: {}\nregistry: {}\n",
/// )
/// .await?;
/// # Ok(()) }
/// ```
pub async fn write_secure_file(path: &Utf8Path, contents: &[u8]) -> Result<(), SecretsError> {
    let parent = path.parent().ok_or_else(|| {
        SecretsError::Manager(format!(
            "cannot derive parent directory of '{path}' for atomic write",
        ))
    })?;
    let tmp_path = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name().unwrap_or("cbs-secrets"),
        std::process::id(),
    ));
    let mut tmp = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp_path)
        .await
        .map_err(|e| {
            SecretsError::Manager(format!(
                "create tempfile '{tmp_path}' for atomic write: {e}"
            ))
        })?;
    tmp.write_all(contents)
        .await
        .map_err(|e| SecretsError::Manager(format!("write tempfile '{tmp_path}': {e}")))?;
    tmp.sync_all()
        .await
        .map_err(|e| SecretsError::Manager(format!("fsync tempfile '{tmp_path}': {e}")))?;
    drop(tmp);
    tokio::fs::rename(&tmp_path, path).await.map_err(|e| {
        SecretsError::Manager(format!(
            "rename tempfile '{tmp_path}' to '{path}' (final): {e}",
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn write_secure_file_lands_at_mode_0600() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("secret.yaml")).expect("utf8 path");
        write_secure_file(&path, b"hello\n").await.expect("write");
        let meta = tokio::fs::metadata(&path).await.expect("stat");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
        let body = tokio::fs::read(&path).await.expect("read");
        assert_eq!(body, b"hello\n");
    }

    #[tokio::test]
    async fn write_secure_file_overwrites_existing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("secret.yaml")).expect("utf8 path");
        write_secure_file(&path, b"first").await.expect("first");
        write_secure_file(&path, b"second").await.expect("second");
        let body = tokio::fs::read(&path).await.expect("read");
        assert_eq!(body, b"second");
    }
}
