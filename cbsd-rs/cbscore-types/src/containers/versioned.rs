// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `schema_version`-tagged wrapper for the snake-case
//! [`ContainerDescriptor`] format.

use camino::Utf8Path;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::containers::{ContainerDescriptor, ContainerError};
use crate::versioned::{ExtractError, extract_schema_version, serialize_versioned};

const SNAKE_TAG: &str = "schema_version";

/// Wire-marker wrapper for [`ContainerDescriptor`]. Surface mirrors
/// [`crate::versions::VersionedVersionDescriptor`] but reports
/// [`ContainerError`]s.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::containers::{
///     ContainerDescriptor, ContainerPackages, ContainerPre,
///     VersionedContainerDescriptor,
/// };
///
/// let c = ContainerDescriptor {
///     config: None,
///     pre: ContainerPre::default(),
///     packages: ContainerPackages::default(),
///     post: vec![],
/// };
/// let json = serde_json::to_string(
///     &VersionedContainerDescriptor::new(c.clone()),
/// )
/// .unwrap();
/// assert!(json.starts_with(r#"{"schema_version":1"#));
///
/// let raw: serde_value::Value = serde_json::from_str(&json).unwrap();
/// let parsed = VersionedContainerDescriptor::from_value(
///     raw,
///     Utf8Path::new("/containers/ceph.json"),
/// )
/// .unwrap()
/// .into_latest();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedContainerDescriptor {
    /// Current schema version. Carries a fully-deserialized [`ContainerDescriptor`].
    V1(ContainerDescriptor),
}

impl VersionedContainerDescriptor {
    /// Maximum `schema_version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap a [`ContainerDescriptor`] at the current schema version.
    #[must_use]
    pub const fn new(desc: ContainerDescriptor) -> Self {
        Self::V1(desc)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> ContainerDescriptor {
        match self {
            Self::V1(d) => d,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`].
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::Invalid`] for any failure today
    /// (missing marker, unknown marker, or inner-deserialize error);
    /// future schema versions may refine to per-variant errors.
    pub fn from_value(value: serde_value::Value, path: &Utf8Path) -> Result<Self, ContainerError> {
        let marker = extract_schema_version(&value, SNAKE_TAG).map_err(|e| match e {
            ExtractError::Missing => {
                ContainerError::Invalid(format!("{path}: missing 'schema_version' key"))
            }
            ExtractError::NotMap | ExtractError::NotInteger => {
                ContainerError::Invalid(format!("{path}: {e}"))
            }
        })?;
        if marker > Self::CURRENT {
            return Err(ContainerError::Invalid(format!(
                "{path}: unsupported schema_version {marker} (max supported: {})",
                Self::CURRENT
            )));
        }
        let d = ContainerDescriptor::deserialize(value.into_deserializer()).map_err(
            |e: serde_value::DeserializerError| ContainerError::Invalid(format!("{path}: {e}")),
        )?;
        Ok(Self::V1(d))
    }
}

impl Serialize for VersionedContainerDescriptor {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(d) => serialize_versioned(s, SNAKE_TAG, 1, d),
        }
    }
}
