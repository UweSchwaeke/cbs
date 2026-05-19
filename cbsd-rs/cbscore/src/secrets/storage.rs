// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Storage-credential helpers — resolved S3 access keys + per-name
//! lookup for the builder's S3 upload pipeline.
//!
//! Phase 3 lands the resolved-form accessor; Phase 5's
//! `builder::upload` calls into it via the [`super::SecretsMgr`] handle.

use cbscore_types::utils::secrets::{StorageCreds, StoragePlainCreds};

/// Borrow the `(access_id, secret_id)` pair from a resolved S3
/// storage credential.
///
/// Returns `None` for [`StorageCreds::Vault`] entries — callers must
/// invoke [`super::SecretsMgr::resolve_vault_refs`] first.
///
/// # Examples
///
/// ```
/// use cbscore::secrets::storage::s3_creds;
/// use cbscore_types::utils::secrets::{StorageCreds, StoragePlainCreds};
///
/// let creds = StorageCreds::Plain(StoragePlainCreds::S3 {
///     access_id: "AKIA…".into(),
///     secret_id: "wJalr…".into(),
/// });
/// let (access, secret) = s3_creds(&creds).expect("plain");
/// assert!(access.starts_with("AKIA"));
/// # let _ = secret;
/// ```
#[must_use]
pub const fn s3_creds(creds: &StorageCreds) -> Option<(&str, &str)> {
    let StorageCreds::Plain(StoragePlainCreds::S3 {
        access_id,
        secret_id,
    }) = creds
    else {
        return None;
    };
    Some((access_id.as_str(), secret_id.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::utils::secrets::StorageVaultCreds;

    #[test]
    fn s3_creds_plain_returns_pair() {
        let creds = StorageCreds::Plain(StoragePlainCreds::S3 {
            access_id: "AKIA".into(),
            secret_id: "WJALR".into(),
        });
        assert_eq!(s3_creds(&creds), Some(("AKIA", "WJALR")));
    }

    #[test]
    fn s3_creds_vault_returns_none() {
        let creds = StorageCreds::Vault(StorageVaultCreds::S3 {
            key: "k".into(),
            access_id: "AKIA".into(),
            secret_id: "WJALR".into(),
        });
        assert!(s3_creds(&creds).is_none());
    }
}
