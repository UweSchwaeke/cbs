// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Vault-server configuration subset of cbs-build.vault.yaml.

use serde::{Deserialize, Serialize};

/// Vault userpass auth (username + password).
///
/// The `password` field is plain-text in `cbs-build.vault.yaml` on
/// purpose — this is the bootstrap credential that lets the secrets
/// manager reach Vault. Operators protect the vault YAML with
/// filesystem permissions.
///
/// # Examples
///
/// ```
/// use cbscore_types::config::VaultUserPassConfig;
///
/// let v = VaultUserPassConfig {
///     username: "deploy".into(),
///     password: "hunter2".into(),
/// };
/// let json = serde_json::to_string(&v).unwrap();
/// let parsed: VaultUserPassConfig = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, v);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VaultUserPassConfig {
    /// Vault username.
    pub username: String,
    /// Vault password.
    pub password: String,
}

/// Vault `AppRole` auth (role-id + secret-id).
///
/// # Examples
///
/// ```
/// use cbscore_types::config::VaultAppRoleConfig;
///
/// let v = VaultAppRoleConfig {
///     role_id: "abc-123".into(),
///     secret_id: "shh".into(),
/// };
/// let json = serde_json::to_string(&v).unwrap();
/// let parsed: VaultAppRoleConfig = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, v);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VaultAppRoleConfig {
    /// `AppRole` role-id. Wire key: `role-id`.
    pub role_id: String,
    /// `AppRole` secret-id. Wire key: `secret-id`.
    pub secret_id: String,
}

/// Vault-server configuration consumed by `cbscore::utils::vault`.
///
/// Selection of auth method follows the explicit order documented in
/// Phase 3 Commit 2: token → `AppRole` → userpass. The wrapper picks the
/// first field that's populated.
///
/// # Examples
///
/// ```
/// use cbscore_types::config::VaultConfig;
///
/// let v = VaultConfig {
///     vault_addr: "https://vault.example.com".into(),
///     auth_user: None,
///     auth_approle: None,
///     auth_token: Some("hvs.AAAA".into()),
/// };
/// let json = serde_json::to_string(&v).unwrap();
/// let parsed: VaultConfig = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, v);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VaultConfig {
    /// Vault server URL. Wire key: `vault-addr`.
    pub vault_addr: String,
    /// Optional userpass auth credentials. Wire key: `auth-user`.
    #[serde(default)]
    pub auth_user: Option<VaultUserPassConfig>,
    /// Optional `AppRole` auth credentials. Wire key: `auth-approle`.
    #[serde(default)]
    pub auth_approle: Option<VaultAppRoleConfig>,
    /// Optional explicit token (highest-precedence auth path).
    /// Wire key: `auth-token`.
    #[serde(default)]
    pub auth_token: Option<String>,
}
