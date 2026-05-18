// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `schema-version`-tagged wrappers for the kebab-case config and
//! vault YAML formats.
//!
//! On disk the marker is a YAML integer key under the kebab-case name
//! `schema-version` (per design 002 §Wire-Format Versioning); the
//! Rust field name is `schema_version` (snake) for ergonomic code.

use camino::Utf8Path;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::config::{Config, ConfigError, VaultConfig};
use crate::versioned::{ExtractError, extract_schema_version, serialize_versioned};

const KEBAB_TAG: &str = "schema-version";

/// Wire-marker wrapper for [`Config`] — the on-disk
/// `cbs-build.config.yaml` format.
///
/// Today only [`VersionedConfig::V1`] exists; future `V2` etc. variants
/// land at schema-bump time per design 002 §Wire-Format Versioning.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::config::{Config, PathsConfig, VersionedConfig};
///
/// let cfg = Config {
///     paths: PathsConfig {
///         components: vec![],
///         scratch: "/scratch".into(),
///         scratch_containers: "/scratch/containers".into(),
///         ccache: None,
///     },
///     storage: None,
///     signing: None,
///     logging: None,
///     secrets: vec![],
///     vault: None,
/// };
///
/// // Serialize as YAML with the schema-version marker prepended.
/// let yaml = serde_saphyr::to_string(&VersionedConfig::new(cfg.clone())).unwrap();
/// assert!(yaml.starts_with("schema-version: 1\n"));
///
/// // Round-trip via serde_value::Value + from_value (typed errors).
/// let v: serde_value::Value = serde_saphyr::from_str(&yaml).unwrap();
/// let parsed = VersionedConfig::from_value(v, Utf8Path::new("/cfg.yaml"))
///     .unwrap()
///     .into_latest();
/// assert_eq!(parsed, cfg);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedConfig {
    /// Current schema version. Carries a fully-deserialized [`Config`].
    V1(Config),
}

impl VersionedConfig {
    /// Maximum `schema-version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap a [`Config`] at the current schema version.
    #[must_use]
    pub const fn new(config: Config) -> Self {
        Self::V1(config)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> Config {
        match self {
            Self::V1(cfg) => cfg,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`], producing typed
    /// [`ConfigError`]s.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::MissingSchemaVersion`] if the marker key
    /// is absent; [`ConfigError::UnknownSchemaVersion`] if the marker
    /// exceeds [`Self::CURRENT`]; [`ConfigError::Io`] if the inner
    /// payload fails to deserialize as a [`Config`].
    pub fn from_value(value: serde_value::Value, path: &Utf8Path) -> Result<Self, ConfigError> {
        let marker = extract_schema_version(&value, KEBAB_TAG).map_err(|e| match e {
            ExtractError::Missing => ConfigError::MissingSchemaVersion {
                path: path.to_owned(),
            },
            ExtractError::NotMap | ExtractError::NotInteger => ConfigError::Io {
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            },
        })?;
        if marker > Self::CURRENT {
            return Err(ConfigError::UnknownSchemaVersion {
                path: path.to_owned(),
                found: marker,
                max_supported: Self::CURRENT,
            });
        }
        let cfg = Config::deserialize(value.into_deserializer()).map_err(
            |e: serde_value::DeserializerError| ConfigError::Io {
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            },
        )?;
        Ok(Self::V1(cfg))
    }
}

impl Serialize for VersionedConfig {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(cfg) => serialize_versioned(s, KEBAB_TAG, 1, cfg),
        }
    }
}

/// Wire-marker wrapper for [`VaultConfig`] — the on-disk
/// `cbs-build.vault.yaml` format.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::config::{VaultConfig, VersionedVaultConfig};
///
/// let v = VaultConfig {
///     vault_addr: "https://vault.example.com".into(),
///     auth_user: None,
///     auth_approle: None,
///     auth_token: Some("hvs.AAAA".into()),
/// };
///
/// let yaml = serde_saphyr::to_string(&VersionedVaultConfig::new(v.clone())).unwrap();
/// assert!(yaml.starts_with("schema-version: 1\n"));
///
/// let raw: serde_value::Value = serde_saphyr::from_str(&yaml).unwrap();
/// let parsed = VersionedVaultConfig::from_value(raw, Utf8Path::new("/vault.yaml"))
///     .unwrap()
///     .into_latest();
/// assert_eq!(parsed, v);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedVaultConfig {
    /// Current schema version. Carries a fully-deserialized [`VaultConfig`].
    V1(VaultConfig),
}

impl VersionedVaultConfig {
    /// Maximum `schema-version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap a [`VaultConfig`] at the current schema version.
    #[must_use]
    pub const fn new(vault: VaultConfig) -> Self {
        Self::V1(vault)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> VaultConfig {
        match self {
            Self::V1(v) => v,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`], producing typed
    /// [`ConfigError`]s.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::MissingSchemaVersion`] if the marker key
    /// is absent; [`ConfigError::UnknownSchemaVersion`] if the marker
    /// exceeds [`Self::CURRENT`]; [`ConfigError::Io`] if the inner
    /// payload fails to deserialize as a [`VaultConfig`].
    pub fn from_value(value: serde_value::Value, path: &Utf8Path) -> Result<Self, ConfigError> {
        let marker = extract_schema_version(&value, KEBAB_TAG).map_err(|e| match e {
            ExtractError::Missing => ConfigError::MissingSchemaVersion {
                path: path.to_owned(),
            },
            ExtractError::NotMap | ExtractError::NotInteger => ConfigError::Io {
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            },
        })?;
        if marker > Self::CURRENT {
            return Err(ConfigError::UnknownSchemaVersion {
                path: path.to_owned(),
                found: marker,
                max_supported: Self::CURRENT,
            });
        }
        let v = VaultConfig::deserialize(value.into_deserializer()).map_err(
            |e: serde_value::DeserializerError| ConfigError::Io {
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            },
        )?;
        Ok(Self::V1(v))
    }
}

impl Serialize for VersionedVaultConfig {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(v) => serialize_versioned(s, KEBAB_TAG, 1, v),
        }
    }
}
