// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Configuration types (zero IO).
//!
//! Phase 1 lands the value-side types here in [`Config`],
//! [`SigningConfig`], [`LoggingConfig`], plus the sub-module
//! [`paths::PathsConfig`], [`storage::StorageConfig`] and
//! [`vault::VaultConfig`]. The IO side (`Config::load` / `Config::store`)
//! lives in `cbscore::config` and is added by Phase 3 Commit 4.

pub mod errors;
pub mod paths;
pub mod storage;
pub mod vault;
pub mod versioned;

pub use errors::ConfigError;
pub use paths::PathsConfig;
pub use storage::{RegistryStorageConfig, S3LocationConfig, S3StorageConfig, StorageConfig};
pub use vault::{VaultAppRoleConfig, VaultConfig, VaultUserPassConfig};
pub use versioned::{VersionedConfig, VersionedVaultConfig};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// Top-level `cbs-build.config.yaml` value.
///
/// All optional sections default to absent; a minimal config carries
/// only the required `paths:` block.
///
/// # Examples
///
/// Round-trip a hand-crafted [`Config`] value through `serde_json`:
///
/// ```
/// use cbscore_types::config::{Config, PathsConfig};
/// use camino::Utf8PathBuf;
///
/// let cfg = Config {
///     paths: PathsConfig {
///         components: vec![Utf8PathBuf::from("/components")],
///         scratch: Utf8PathBuf::from("/scratch"),
///         scratch_containers: Utf8PathBuf::from("/scratch/containers"),
///         ccache: None,
///         versions: None,
///     },
///     storage: None,
///     signing: None,
///     logging: None,
///     secrets: vec![],
///     vault: None,
/// };
/// let json = serde_json::to_string(&cfg).unwrap();
/// let parsed: Config = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, cfg);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    /// Filesystem paths the build pipeline reads and writes.
    pub paths: PathsConfig,
    /// Optional S3 and registry destinations for release artifacts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageConfig>,
    /// Optional signing-key references; `None` disables signing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing: Option<SigningConfig>,
    /// Optional file-appender logging configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingConfig>,
    /// Per-deployment secret-file paths consumed by the secrets manager.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<Utf8PathBuf>,
    /// Optional `cbs-build.vault.yaml` path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<Utf8PathBuf>,
}

/// Signing-key references resolved from the secrets file at run time.
///
/// Both fields are operator-chosen names that index into the secrets
/// store; `None` means "no signing of this kind" and the corresponding
/// stage in the build pipeline becomes a no-op.
///
/// # Examples
///
/// ```
/// use cbscore_types::config::SigningConfig;
///
/// let s = SigningConfig {
///     gpg: Some("rpm-signing".into()),
///     transit: None,
/// };
/// let json = serde_json::to_string(&s).unwrap();
/// let parsed: SigningConfig = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, s);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SigningConfig {
    /// Name of the GPG signing-secret entry, if RPM signing is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpg: Option<String>,
    /// Name of the Vault Transit signing-secret entry, if manifest
    /// signing is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transit: Option<String>,
}

/// File-appender logging configuration.
///
/// `LoggingConfig.log_file` is the absolute path of the rolling log
/// the file appender writes to. `Config.logging: Option<LoggingConfig>`
/// is `#[serde(default)]`, so omitting the whole `logging:` section
/// produces `None` (no file appender — stdout/stderr only).
///
/// # Examples
///
/// ```
/// use cbscore_types::config::LoggingConfig;
/// use camino::Utf8PathBuf;
///
/// let l = LoggingConfig { log_file: Utf8PathBuf::from("/var/log/cbs.log") };
/// let json = serde_json::to_string(&l).unwrap();
/// let parsed: LoggingConfig = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, l);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LoggingConfig {
    /// Absolute path of the rolling log file. Wire key: `log-file`.
    pub log_file: Utf8PathBuf,
}
