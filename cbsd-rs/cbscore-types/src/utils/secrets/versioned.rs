// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `schema-version`-tagged wrapper for the kebab-case [`Secrets`] YAML
//! format (`secrets.yaml`).

use camino::Utf8Path;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::config::ConfigError;
use crate::utils::secrets::Secrets;
use crate::versioned::{ExtractError, extract_schema_version, serialize_versioned};

const KEBAB_TAG: &str = "schema-version";

/// Wire-marker wrapper for [`Secrets`] — the on-disk `secrets.yaml`
/// format. Surface mirrors [`crate::config::VersionedConfig`].
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::utils::secrets::{Secrets, VersionedSecrets};
///
/// let s = Secrets::default();
/// let yaml = serde_saphyr::to_string(&VersionedSecrets::new(s.clone())).unwrap();
/// assert!(yaml.starts_with("schema-version: 1\n"));
///
/// let raw: serde_value::Value = serde_saphyr::from_str(&yaml).unwrap();
/// let parsed = VersionedSecrets::from_value(raw, Utf8Path::new("/secrets.yaml"))
///     .unwrap()
///     .into_latest();
/// assert!(parsed == s);
/// ```
#[derive(Clone)]
pub enum VersionedSecrets {
    /// Current schema version. Carries a fully-deserialized [`Secrets`].
    V1(Secrets),
}

impl VersionedSecrets {
    /// Maximum `schema-version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap a [`Secrets`] at the current schema version.
    #[must_use]
    pub fn new(secrets: Secrets) -> Self {
        Self::V1(secrets)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> Secrets {
        match self {
            Self::V1(s) => s,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`], producing typed
    /// [`ConfigError`]s. The secrets format reuses [`ConfigError`]
    /// since its kebab marker shares the same surface as the other
    /// kebab YAML formats.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::MissingSchemaVersion`] if the marker key
    /// is absent; [`ConfigError::UnknownSchemaVersion`] if the marker
    /// exceeds [`Self::CURRENT`]; [`ConfigError::Io`] if the inner
    /// payload fails to deserialize as a [`Secrets`].
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
        let s = Secrets::deserialize(value.into_deserializer()).map_err(
            |e: serde_value::DeserializerError| ConfigError::Io {
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            },
        )?;
        Ok(Self::V1(s))
    }
}

impl Serialize for VersionedSecrets {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(secrets) => serialize_versioned(s, KEBAB_TAG, 1, secrets),
        }
    }
}
