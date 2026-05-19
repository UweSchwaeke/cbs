// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Git-secret-specific helpers — extracting an SSH key from a
//! resolved [`GitPlainCreds::Ssh`] entry into an owner-only tempfile
//! the host-side `git` invocation can `ssh-add` from.
//!
//! Phase 3 Commit 3 lands the SSH-key extract; Phase 5's builder
//! pipeline will wire it into `git clone` / `git fetch` calls via
//! `GIT_SSH_COMMAND="ssh -i <path>"`.

use camino::Utf8Path;
use cbscore_types::utils::secrets::{GitPlainCreds, SecretsError};

use super::utils::write_secure_file;

/// Write the SSH private key from a [`GitPlainCreds::Ssh`] entry to
/// `path` at mode 0600.
///
/// Returns [`SecretsError::Manager`] for non-SSH variants — callers
/// should match on the entry shape first and only invoke this helper
/// for SSH credentials.
///
/// # Errors
///
/// - [`SecretsError::Manager`] when `creds` is not [`GitPlainCreds::Ssh`].
/// - [`SecretsError::Manager`] wrapping the underlying IO error from
///   the secure-tempfile write.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
/// use cbscore::secrets::git::write_ssh_key;
/// use cbscore_types::utils::secrets::GitPlainCreds;
///
/// # async fn demo() -> Result<(), cbscore_types::utils::secrets::SecretsError> {
/// let creds = GitPlainCreds::Ssh {
///     username: "git".into(),
///     ssh_key: "-----BEGIN OPENSSH PRIVATE KEY-----\n...\n".into(),
/// };
/// write_ssh_key(&creds, Utf8Path::new("/tmp/cbs-git-ssh.key")).await?;
/// # Ok(()) }
/// ```
pub async fn write_ssh_key(creds: &GitPlainCreds, path: &Utf8Path) -> Result<(), SecretsError> {
    let GitPlainCreds::Ssh { ssh_key, .. } = creds else {
        return Err(SecretsError::Manager(
            "write_ssh_key called on non-SSH git creds".into(),
        ));
    };
    write_secure_file(path, ssh_key.as_bytes()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn write_ssh_key_lands_at_mode_0600() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("id")).expect("utf8 path");
        let creds = GitPlainCreds::Ssh {
            username: "git".into(),
            ssh_key: "FAKE-KEY-PAYLOAD".into(),
        };
        write_ssh_key(&creds, &path).await.expect("write");
        let meta = tokio::fs::metadata(&path).await.expect("stat");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let body = tokio::fs::read_to_string(&path).await.expect("read");
        assert_eq!(body, "FAKE-KEY-PAYLOAD");
    }

    #[tokio::test]
    async fn write_ssh_key_rejects_non_ssh_variant() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("id")).expect("utf8 path");
        let creds = GitPlainCreds::Token {
            username: "git".into(),
            token: "FAKE".into(),
        };
        let Err(SecretsError::Manager(msg)) = write_ssh_key(&creds, &path).await else {
            panic!("expected Manager error on non-SSH variant");
        };
        assert!(msg.contains("non-SSH"));
    }
}
