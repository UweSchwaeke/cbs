// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `schema_version`-tagged wrappers for the snake-case [`ReleaseDesc`]
//! and [`ReleaseComponent`] JSON formats.

use camino::Utf8Path;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::releases::{ReleaseComponent, ReleaseDesc, ReleaseError};
use crate::versioned::{ExtractError, extract_schema_version, serialize_versioned};

const SNAKE_TAG: &str = "schema_version";

/// Wire-marker wrapper for [`ReleaseDesc`].
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::releases::{ReleaseDesc, VersionedReleaseDesc};
/// use std::collections::HashMap;
///
/// let r = ReleaseDesc { version: "19.2.3".into(), builds: HashMap::new() };
/// let json = serde_json::to_string(
///     &VersionedReleaseDesc::new(r.clone()),
/// )
/// .unwrap();
/// assert!(json.starts_with(r#"{"schema_version":1"#));
///
/// let raw: serde_value::Value = serde_json::from_str(&json).unwrap();
/// let parsed = VersionedReleaseDesc::from_value(
///     raw,
///     Utf8Path::new("/releases/19.2.3.json"),
/// )
/// .unwrap()
/// .into_latest();
/// assert_eq!(parsed, r);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedReleaseDesc {
    /// Current schema version. Carries a fully-deserialized [`ReleaseDesc`].
    V1(ReleaseDesc),
}

impl VersionedReleaseDesc {
    /// Maximum `schema_version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap a [`ReleaseDesc`] at the current schema version.
    #[must_use]
    pub const fn new(desc: ReleaseDesc) -> Self {
        Self::V1(desc)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> ReleaseDesc {
        match self {
            Self::V1(d) => d,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`].
    ///
    /// # Errors
    ///
    /// Returns [`ReleaseError::Invalid`] for any failure today.
    pub fn from_value(value: serde_value::Value, path: &Utf8Path) -> Result<Self, ReleaseError> {
        let marker = extract_schema_version(&value, SNAKE_TAG).map_err(|e| match e {
            ExtractError::Missing => {
                ReleaseError::Invalid(format!("{path}: missing 'schema_version' key"))
            }
            ExtractError::NotMap | ExtractError::NotInteger => {
                ReleaseError::Invalid(format!("{path}: {e}"))
            }
        })?;
        if marker > Self::CURRENT {
            return Err(ReleaseError::Invalid(format!(
                "{path}: unsupported schema_version {marker} (max supported: {})",
                Self::CURRENT
            )));
        }
        let d = ReleaseDesc::deserialize(value.into_deserializer()).map_err(
            |e: serde_value::DeserializerError| ReleaseError::Invalid(format!("{path}: {e}")),
        )?;
        Ok(Self::V1(d))
    }
}

impl Serialize for VersionedReleaseDesc {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(d) => serialize_versioned(s, SNAKE_TAG, 1, d),
        }
    }
}

// ---------------------------------------------------------------------

/// Wire-marker wrapper for [`ReleaseComponent`] — the per-component
/// JSON file written alongside [`ReleaseDesc`].
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::releases::{
///     ArchType, BuildType, ReleaseArtifacts, ReleaseComponent,
///     VersionedReleaseComponent,
/// };
///
/// let c = ReleaseComponent {
///     name: "ceph".into(),
///     version: "19.2.3".into(),
///     sha1: "abc123".into(),
///     arch: ArchType::X86_64,
///     build_type: BuildType::Rpm,
///     os_version: "el9".into(),
///     repo_url: "https://example.com/ceph.git".into(),
///     artifacts: ReleaseArtifacts {
///         loc: "s3://b/p".into(),
///         release_rpm_loc: "s3://b/p/rel.rpm".into(),
///     },
/// };
/// let json = serde_json::to_string(
///     &VersionedReleaseComponent::new(c.clone()),
/// )
/// .unwrap();
/// assert!(json.starts_with(r#"{"schema_version":1"#));
///
/// let raw: serde_value::Value = serde_json::from_str(&json).unwrap();
/// let parsed = VersionedReleaseComponent::from_value(
///     raw,
///     Utf8Path::new("/releases/ceph.json"),
/// )
/// .unwrap()
/// .into_latest();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedReleaseComponent {
    /// Current schema version. Carries a fully-deserialized
    /// [`ReleaseComponent`].
    V1(ReleaseComponent),
}

impl VersionedReleaseComponent {
    /// Maximum `schema_version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap a [`ReleaseComponent`] at the current schema version.
    #[must_use]
    pub const fn new(comp: ReleaseComponent) -> Self {
        Self::V1(comp)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> ReleaseComponent {
        match self {
            Self::V1(c) => c,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`].
    ///
    /// # Errors
    ///
    /// Returns [`ReleaseError::Invalid`] for any failure today.
    pub fn from_value(value: serde_value::Value, path: &Utf8Path) -> Result<Self, ReleaseError> {
        let marker = extract_schema_version(&value, SNAKE_TAG).map_err(|e| match e {
            ExtractError::Missing => {
                ReleaseError::Invalid(format!("{path}: missing 'schema_version' key"))
            }
            ExtractError::NotMap | ExtractError::NotInteger => {
                ReleaseError::Invalid(format!("{path}: {e}"))
            }
        })?;
        if marker > Self::CURRENT {
            return Err(ReleaseError::Invalid(format!(
                "{path}: unsupported schema_version {marker} (max supported: {})",
                Self::CURRENT
            )));
        }
        let c = ReleaseComponent::deserialize(value.into_deserializer()).map_err(
            |e: serde_value::DeserializerError| ReleaseError::Invalid(format!("{path}: {e}")),
        )?;
        Ok(Self::V1(c))
    }
}

impl Serialize for VersionedReleaseComponent {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(c) => serialize_versioned(s, SNAKE_TAG, 1, c),
        }
    }
}
