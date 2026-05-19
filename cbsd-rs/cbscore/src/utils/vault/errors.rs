// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by `cbscore::utils::vault`.
//!
//! Same cbscore-internal rationale as [`crate::utils::s3::S3Error`]:
//! callers in `secrets::mgr` (Phase 3 Commit 3) and `images::sign`
//! (Phase 5) wrap [`VaultError`] into their domain errors via
//! `#[from]`.

use thiserror::Error;
use vaultrs::error::ClientError;

/// Errors surfaced by the async Vault wrappers.
///
/// # Examples
///
/// ```
/// use cbscore::utils::vault::VaultError;
///
/// let e = VaultError::PathNotFound {
///     mount: "ces-kv".into(),
///     path: "git/ceph-mirror".into(),
/// };
/// assert!(e.to_string().contains("ces-kv"));
/// ```
#[derive(Debug, Error)]
pub enum VaultError {
    /// `kv_read` got a 404 / "secret does not exist" response from
    /// Vault at the requested `mount/path`. Operator-actionable: the
    /// caller likely mis-typed the path or referenced a secret that
    /// hasn't been provisioned yet.
    #[error("vault path '{mount}/{path}' not found")]
    PathNotFound {
        /// KV mount name (e.g. `ces-kv`).
        mount: String,
        /// Per-mount secret path.
        path: String,
    },

    /// Token / `AppRole` / userpass login failed. `method` is one of
    /// `"token"`, `"approle"`, `"userpass"` for the operator-visible
    /// message.
    #[error("vault {method} auth failed: {source}")]
    AuthFailed {
        /// Auth method name for diagnostics.
        method: &'static str,
        /// Underlying vaultrs error.
        #[source]
        source: ClientError,
    },

    /// Transport / 5xx / unexpected-response error. The `#[from]`
    /// triggers the automatic `From<ClientError>` impl and marks the
    /// field as `#[source]` for `Error::source()` chain traversal.
    #[error("vault request failed: {source}")]
    RequestFailed {
        /// Wrapped vaultrs error.
        #[from]
        source: ClientError,
    },

    /// Vault returned a 200 with a body shape the wrapper didn't
    /// expect (e.g. missing `data` key on a KV v2 response, or a
    /// non-KV mount when KV was expected).
    #[error("vault returned an unexpected response: {message}")]
    BadResponse {
        /// Human-readable diagnostic.
        message: String,
    },

    /// `VaultConfig` carried no usable auth method — none of the
    /// `auth_token`, `auth_approle`, or `auth_user` fields were
    /// populated. Operator-actionable.
    #[error("vault config carries no auth method (token, approle, or userpass)")]
    NoAuthMethod,

    /// `VaultConfig::vault_addr` was empty or could not be parsed as
    /// a URL.
    #[error("vault config has invalid address '{addr}'")]
    InvalidAddress {
        /// The offending address string.
        addr: String,
    },
}
