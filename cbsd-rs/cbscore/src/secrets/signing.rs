// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Signing-secret-specific helpers — GPG armor-key extraction,
//! transit-key reference resolution, and signing-identity lookups.
//!
//! Phase 3 lands the GPG armor-key extract helper (which the Phase 5
//! builder pipeline will pipe into `gpg --import` via stdin) and the
//! transit-key reference accessor. Heavier integrations like the
//! actual keyring spawn live with the builder.

use camino::Utf8Path;
use cbscore_types::utils::secrets::{
    SecretsError, SigningCreds, SigningPlainCreds, SigningVaultCreds,
};

use super::utils::write_secure_file;

/// Write the GPG-armor private (and optionally public) key bytes
/// from a [`SigningPlainCreds::GpgArmorKey`] entry to two tempfiles
/// at mode 0600.
///
/// Returns the paths of the written files in the
/// `(private_key_path, public_key_path)` order — `public_key_path`
/// is `None` when the credential entry carries no public key.
///
/// # Errors
///
/// - [`SecretsError::Manager`] when `creds` is not
///   [`SigningPlainCreds::GpgArmorKey`].
/// - [`SecretsError::Manager`] wrapping the underlying IO error from
///   the secure-tempfile write.
pub async fn write_gpg_keys(
    creds: &SigningPlainCreds,
    private_path: &Utf8Path,
    public_path: Option<&Utf8Path>,
) -> Result<(), SecretsError> {
    let SigningPlainCreds::GpgArmorKey {
        private_key,
        public_key,
        ..
    } = creds;
    write_secure_file(private_path, private_key.as_bytes()).await?;
    if let (Some(pk), Some(out)) = (public_key.as_deref(), public_path) {
        write_secure_file(out, pk.as_bytes()).await?;
    }
    Ok(())
}

/// Return the Vault Transit `(mount, key_name)` pair from a
/// [`SigningCreds::Vault(SigningVaultCreds::Transit { .. })`] entry.
///
/// Phase 5's `images::signing` calls
/// [`crate::utils::vault::transit_sign`] with these.
///
/// # Examples
///
/// ```
/// use cbscore::secrets::signing::transit_key_ref;
/// use cbscore_types::utils::secrets::{SigningCreds, SigningVaultCreds};
///
/// let creds = SigningCreds::Vault(SigningVaultCreds::Transit {
///     key: "rpm-signing-key".into(),
///     mount: "transit/".into(),
/// });
/// let (mount, key) = transit_key_ref(&creds).expect("transit creds");
/// assert_eq!(mount, "transit/");
/// assert_eq!(key, "rpm-signing-key");
/// ```
#[must_use]
pub const fn transit_key_ref(creds: &SigningCreds) -> Option<(&str, &str)> {
    let SigningCreds::Vault(SigningVaultCreds::Transit { mount, key }) = creds else {
        return None;
    };
    Some((mount.as_str(), key.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn write_gpg_keys_private_only_lands_at_0600() {
        let dir = tempfile::tempdir().expect("tempdir");
        let priv_path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("priv.asc")).expect("utf8 path");
        let creds = SigningPlainCreds::GpgArmorKey {
            private_key: "-----BEGIN PGP PRIVATE KEY BLOCK-----\nFAKE".into(),
            public_key: None,
            passphrase: None,
            email: "ops@example.com".into(),
        };
        write_gpg_keys(&creds, &priv_path, None)
            .await
            .expect("write");
        let mode = tokio::fs::metadata(&priv_path)
            .await
            .expect("stat")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[tokio::test]
    async fn write_gpg_keys_with_public_writes_both() {
        let dir = tempfile::tempdir().expect("tempdir");
        let priv_path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("priv.asc")).expect("utf8 path");
        let pub_path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("pub.asc")).expect("utf8 path");
        let creds = SigningPlainCreds::GpgArmorKey {
            private_key: "PRIVATE".into(),
            public_key: Some("PUBLIC".into()),
            passphrase: None,
            email: "ops@example.com".into(),
        };
        write_gpg_keys(&creds, &priv_path, Some(&pub_path))
            .await
            .expect("write");
        assert_eq!(
            tokio::fs::read_to_string(&priv_path).await.unwrap(),
            "PRIVATE",
        );
        assert_eq!(
            tokio::fs::read_to_string(&pub_path).await.unwrap(),
            "PUBLIC",
        );
    }

    #[test]
    fn transit_key_ref_extracts_pair() {
        let creds = SigningCreds::Vault(SigningVaultCreds::Transit {
            key: "k".into(),
            mount: "m/".into(),
        });
        assert_eq!(transit_key_ref(&creds), Some(("m/", "k")));
    }

    #[test]
    fn transit_key_ref_plain_returns_none() {
        let creds = SigningCreds::Plain(SigningPlainCreds::GpgArmorKey {
            private_key: "p".into(),
            public_key: None,
            passphrase: None,
            email: "e".into(),
        });
        assert!(transit_key_ref(&creds).is_none());
    }
}
