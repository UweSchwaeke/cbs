// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Async wrappers around `HashiCorp` Vault via the [`vaultrs`] crate.
//!
//! Pure-Rust port of `cbscore/utils/vault.py` (~184 lines). Mirrors the
//! Python wrapper's semantics one-for-one with two intentional
//! enhancements:
//!
//! - **KV v1 + v2 auto-detect.** Python hardcodes KV v2; the Rust
//!   wrapper queries the mount listing to pick the right read API per
//!   mount. Operator-facing payload shape (a flat
//!   `HashMap<String, String>`) is identical across versions.
//! - **Transit signing.** [`transit_sign`] is wired here in Phase 3
//!   so Phase 5's `images::signing` has the primitive it needs; the
//!   Python equivalent lives in `cbscore/builder/signing.py` (Phase 5
//!   port).
//!
//! # Auth precedence
//!
//! When a [`VaultConfig`](cbscore_types::config::vault::VaultConfig)
//! carries multiple auth fields, the wrapper picks the first
//! populated one in the order **token → `AppRole` → userpass**. Matches
//! Python `get_vault_from_config` and design 002 §Vault line 636.
//!
//! # No token caching
//!
//! Every operation re-authenticates against Vault — no token is
//! retained across calls. Keeps the security posture identical to
//! the Python cutover: minimal in-memory token window, full Vault
//! audit signal on every operation, and zero blast radius from a
//! stolen token on a long-lived `cbsd-worker` (Phase 7 context).
//! The cost is one extra Vault RTT per resolution pass — negligible
//! for a build tool.
//!
//! # No built-in retry
//!
//! Unlike [`crate::utils::s3`] (which inherits aws-sdk-s3's
//! transparent retry), every `vaultrs` call goes through once and
//! surfaces transient errors immediately. A
//! [`VaultError::RequestFailed`] on a known-reliable Vault deployment
//! is often a one-off — `cbsbuild build` is safe to re-run.

pub mod errors;

pub use errors::VaultError;

use std::collections::HashMap;

use cbscore_types::config::vault::VaultConfig;
use vaultrs::client::{VaultClient, VaultClientSettingsBuilder};
use vaultrs::error::ClientError;

/// Tracing target for every event in this module.
const TARGET_UTILS_VAULT: &str = "cbscore::utils::vault";

/// Which KV API version backs a given mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KvVersion {
    V1,
    V2,
}

/// Build a token-authenticated [`VaultClient`] without contacting
/// the server (no auth handshake required for static tokens).
fn build_token_client(addr: &str, token: &str) -> Result<VaultClient, VaultError> {
    let settings = VaultClientSettingsBuilder::default()
        .address(addr)
        .token(token)
        .build()
        .map_err(|_| VaultError::InvalidAddress {
            addr: addr.to_owned(),
        })?;
    VaultClient::new(settings).map_err(|e| VaultError::AuthFailed {
        method: "token",
        source: e,
    })
}

/// Authenticate against an `AppRole` backend, returning a client whose
/// session token was minted by Vault as part of the login call.
async fn build_approle_client(
    addr: &str,
    role_id: &str,
    secret_id: &str,
) -> Result<VaultClient, VaultError> {
    // Bootstrap a tokenless client just to drive the login call.
    let bootstrap_settings = VaultClientSettingsBuilder::default()
        .address(addr)
        .token("") // empty — login replaces it
        .build()
        .map_err(|_| VaultError::InvalidAddress {
            addr: addr.to_owned(),
        })?;
    let bootstrap = VaultClient::new(bootstrap_settings).map_err(|e| VaultError::AuthFailed {
        method: "approle",
        source: e,
    })?;
    let auth = vaultrs::auth::approle::login(&bootstrap, "approle", role_id, secret_id)
        .await
        .map_err(|e| VaultError::AuthFailed {
            method: "approle",
            source: e,
        })?;
    build_token_client(addr, &auth.client_token)
}

/// Authenticate against a userpass backend.
async fn build_userpass_client(
    addr: &str,
    username: &str,
    password: &str,
) -> Result<VaultClient, VaultError> {
    let bootstrap_settings = VaultClientSettingsBuilder::default()
        .address(addr)
        .token("")
        .build()
        .map_err(|_| VaultError::InvalidAddress {
            addr: addr.to_owned(),
        })?;
    let bootstrap = VaultClient::new(bootstrap_settings).map_err(|e| VaultError::AuthFailed {
        method: "userpass",
        source: e,
    })?;
    let auth = vaultrs::auth::userpass::login(&bootstrap, "userpass", username, password)
        .await
        .map_err(|e| VaultError::AuthFailed {
            method: "userpass",
            source: e,
        })?;
    build_token_client(addr, &auth.client_token)
}

/// Authenticate per the `token → AppRole → userpass` precedence and
/// return a ready-to-use [`VaultClient`].
async fn authenticate(config: &VaultConfig) -> Result<VaultClient, VaultError> {
    if config.vault_addr.is_empty() {
        return Err(VaultError::InvalidAddress {
            addr: config.vault_addr.clone(),
        });
    }
    if let Some(token) = config.auth_token.as_deref()
        && !token.is_empty()
    {
        return build_token_client(&config.vault_addr, token);
    }
    if let Some(approle) = config.auth_approle.as_ref() {
        return build_approle_client(&config.vault_addr, &approle.role_id, &approle.secret_id)
            .await;
    }
    if let Some(userpass) = config.auth_user.as_ref() {
        return build_userpass_client(&config.vault_addr, &userpass.username, &userpass.password)
            .await;
    }
    Err(VaultError::NoAuthMethod)
}

/// Detect whether `mount` is a KV v1 or KV v2 secret backend by
/// inspecting the mount listing's `options.version` field. Falls
/// back to v1 when the field is absent (matching Vault's default for
/// older KV v1 mounts that have no `options` block).
async fn detect_kv_version(client: &VaultClient, mount: &str) -> Result<KvVersion, VaultError> {
    let mounts = vaultrs::sys::mount::list(client).await?;
    let key = if mount.ends_with('/') {
        mount.to_owned()
    } else {
        format!("{mount}/")
    };
    let entry = mounts.get(&key).ok_or_else(|| VaultError::BadResponse {
        message: format!("mount '{mount}' not present in mount list"),
    })?;
    if entry.mount_type != "kv" {
        return Err(VaultError::BadResponse {
            message: format!(
                "mount '{mount}' has type '{}', expected 'kv'",
                entry.mount_type
            ),
        });
    }
    let is_v2 = entry
        .options
        .as_ref()
        .and_then(|m| m.get("version"))
        .is_some_and(|v| v == "2");
    Ok(if is_v2 { KvVersion::V2 } else { KvVersion::V1 })
}

/// Map a `ClientError` from a KV read into the right [`VaultError`]
/// variant. 404 / `code: 404` API responses become
/// [`VaultError::PathNotFound`]; everything else surfaces as
/// [`VaultError::RequestFailed`].
fn classify_kv_error(err: ClientError, mount: &str, path: &str) -> VaultError {
    if matches!(&err, ClientError::APIError { code: 404, .. }) {
        VaultError::PathNotFound {
            mount: mount.to_owned(),
            path: path.to_owned(),
        }
    } else {
        VaultError::RequestFailed { source: err }
    }
}

// ---------------------------------------------------------------------
// kv_read
// ---------------------------------------------------------------------

/// Read a KV secret at `mount/path` from Vault and return its
/// flat string map.
///
/// Supports both KV v1 and v2 mounts — detected once per call from
/// the mount listing. The return shape (a flat
/// `HashMap<String, String>`) is identical across versions: for KV
/// v2, the wrapper reads the latest version's `data` sub-tree
/// transparently.
///
/// # Errors
///
/// - [`VaultError::PathNotFound`] when the secret does not exist at
///   `mount/path`.
/// - [`VaultError::AuthFailed`] when the configured auth method
///   fails.
/// - [`VaultError::RequestFailed`] for transport / 5xx /
///   unexpected-response errors.
/// - [`VaultError::BadResponse`] when the mount listing entry does
///   not describe a KV backend.
///
/// # Examples
///
/// ```no_run
/// use cbscore::utils::vault::kv_read;
/// use cbscore_types::config::vault::VaultConfig;
///
/// # async fn demo(cfg: &VaultConfig) -> Result<(), cbscore::utils::vault::VaultError> {
/// let secret = kv_read(cfg, "ces-kv", "git/ceph-mirror").await?;
/// let user = secret.get("username").map_or("", String::as_str);
/// # let _ = user;
/// # Ok(()) }
/// ```
#[tracing::instrument(level = "debug", target = "cbscore::utils::vault", skip(config))]
pub async fn kv_read(
    config: &VaultConfig,
    mount: &str,
    path: &str,
) -> Result<HashMap<String, String>, VaultError> {
    let client = authenticate(config).await?;
    let version = detect_kv_version(&client, mount).await?;
    tracing::debug!(
        target: TARGET_UTILS_VAULT,
        mount, path, ?version,
        "KV read",
    );
    match version {
        KvVersion::V2 => vaultrs::kv2::read::<HashMap<String, String>>(&client, mount, path)
            .await
            .map_err(|e| classify_kv_error(e, mount, path)),
        KvVersion::V1 => vaultrs::kv1::get::<HashMap<String, String>>(&client, mount, path)
            .await
            .map_err(|e| classify_kv_error(e, mount, path)),
    }
}

// ---------------------------------------------------------------------
// transit_sign
// ---------------------------------------------------------------------

/// Sign `input` (base64-encoded) using Vault Transit's `sign`
/// endpoint and return the Vault-formatted signature
/// (e.g. `vault:v1:<base64>`).
///
/// `mount` is the Transit secrets-engine mount path (typically
/// `transit`); `key_name` is the named transit key under that mount.
/// Both fields originate from
/// [`SigningCreds::Transit`](cbscore_types::utils::secrets::SigningCreds)
/// at the call site.
///
/// Per-call auth applies (no token caching) — same security posture
/// as [`kv_read`].
///
/// # Errors
///
/// - [`VaultError::AuthFailed`] when the configured auth method
///   fails.
/// - [`VaultError::RequestFailed`] for any other Vault error
///   (mount missing, key missing, transport).
///
/// # Examples
///
/// ```no_run
/// use cbscore::utils::vault::transit_sign;
/// use cbscore_types::config::vault::VaultConfig;
///
/// # async fn demo(cfg: &VaultConfig) -> Result<(), cbscore::utils::vault::VaultError> {
/// let sig = transit_sign(
///     cfg,
///     "transit",
///     "rpm-signing-key",
///     "SGVsbG8sIHdvcmxkIQ==",
/// )
/// .await?;
/// assert!(sig.starts_with("vault:"));
/// # Ok(()) }
/// ```
#[tracing::instrument(level = "debug", target = "cbscore::utils::vault", skip(config, input))]
pub async fn transit_sign(
    config: &VaultConfig,
    mount: &str,
    key_name: &str,
    input: &str,
) -> Result<String, VaultError> {
    let client = authenticate(config).await?;
    let resp = vaultrs::transit::data::sign(&client, mount, key_name, input, None)
        .await
        .map_err(|e| VaultError::RequestFailed { source: e })?;
    tracing::debug!(
        target: TARGET_UTILS_VAULT,
        mount, key_name,
        "Transit sign complete",
    );
    Ok(resp.signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::config::vault::{VaultAppRoleConfig, VaultConfig, VaultUserPassConfig};

    fn empty_config() -> VaultConfig {
        VaultConfig {
            vault_addr: "http://127.0.0.1:8200".into(),
            auth_user: None,
            auth_approle: None,
            auth_token: None,
        }
    }

    // `VaultClient` does not derive `Debug` (vaultrs design choice),
    // so `.unwrap_err()` / `.unwrap()` don't compile here — they need
    // the `Ok` payload's `Debug` impl for the panic message. Tests
    // pattern-match against `Result` directly instead.

    #[tokio::test]
    async fn no_auth_method_when_all_fields_empty() {
        let cfg = empty_config();
        let Err(err) = authenticate(&cfg).await else {
            panic!("expected NoAuthMethod error, got Ok");
        };
        assert!(matches!(err, VaultError::NoAuthMethod));
    }

    #[tokio::test]
    async fn invalid_address_when_addr_empty() {
        let mut cfg = empty_config();
        cfg.vault_addr = String::new();
        cfg.auth_token = Some("hvs.fake".into());
        let Err(err) = authenticate(&cfg).await else {
            panic!("expected InvalidAddress error, got Ok");
        };
        assert!(matches!(err, VaultError::InvalidAddress { .. }));
    }

    #[tokio::test]
    async fn token_method_wins_when_multiple_set() {
        // Token + AppRole + userpass populated → token path runs first
        // (succeeds locally because token auth needs no Vault RTT).
        let cfg = VaultConfig {
            vault_addr: "http://127.0.0.1:8200".into(),
            auth_token: Some("hvs.token".into()),
            auth_approle: Some(VaultAppRoleConfig {
                role_id: "r".into(),
                secret_id: "s".into(),
            }),
            auth_user: Some(VaultUserPassConfig {
                username: "u".into(),
                password: "p".into(),
            }),
        };
        // Token auth is purely client-side (no Vault RTT) so the
        // call resolves to `Ok` without a live server. The lack of
        // a `Debug` impl on `VaultClient` blocks `.unwrap()` —
        // discard the client via the `Ok(_)` arm instead.
        assert!(authenticate(&cfg).await.is_ok());
    }

    #[tokio::test]
    async fn empty_token_falls_through_to_approle() {
        // An empty-string `auth_token` is treated as "unset" (matches
        // Python's truthy check on the Optional[str] field).
        let cfg = VaultConfig {
            vault_addr: "http://127.0.0.1:8200".into(),
            auth_token: Some(String::new()),
            auth_approle: Some(VaultAppRoleConfig {
                role_id: "r".into(),
                secret_id: "s".into(),
            }),
            auth_user: None,
        };
        // AppRole login WILL fail (no Vault), but the error must be
        // AuthFailed { method: "approle", .. } — proving fall-through.
        let Err(err) = authenticate(&cfg).await else {
            panic!("expected AuthFailed error, got Ok");
        };
        assert!(matches!(
            err,
            VaultError::AuthFailed {
                method: "approle",
                ..
            }
        ));
    }

    #[test]
    fn classify_kv_error_404_becomes_path_not_found() {
        let err = ClientError::APIError {
            code: 404,
            errors: vec!["missing".into()],
        };
        let mapped = classify_kv_error(err, "ces-kv", "git/missing");
        assert!(matches!(
            mapped,
            VaultError::PathNotFound { mount, path }
                if mount == "ces-kv" && path == "git/missing"
        ));
    }

    #[test]
    fn classify_kv_error_500_becomes_request_failed() {
        let err = ClientError::APIError {
            code: 500,
            errors: vec!["boom".into()],
        };
        let mapped = classify_kv_error(err, "ces-kv", "git/x");
        assert!(matches!(mapped, VaultError::RequestFailed { .. }));
    }
}
