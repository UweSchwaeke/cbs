// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `schema_version`-tagged wrapper for the snake-case
//! [`VersionDescriptor`] JSON format.

use camino::Utf8Path;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::versioned::{ExtractError, extract_schema_version, serialize_versioned};
use crate::versions::{VersionDescriptor, VersionError};

const SNAKE_TAG: &str = "schema_version";

/// Wire-marker wrapper for [`VersionDescriptor`] — the JSON file at
/// `<root>/<type>/<VERSION>.json`. Surface mirrors
/// [`crate::config::VersionedConfig`] but uses the snake-case tag
/// `schema_version` (descriptors are JSON with snake-case keys per
/// design 002 §Wire-Format Versioning).
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::versions::desc::{
///     VersionDescriptor, VersionImage, VersionSignedOffBy,
/// };
/// use cbscore_types::versions::VersionedVersionDescriptor;
///
/// let v = VersionDescriptor {
///     version: "19.2.3".into(),
///     title: "title".into(),
///     signed_off_by: VersionSignedOffBy {
///         user: "u".into(),
///         email: "u@e.com".into(),
///     },
///     image: VersionImage {
///         registry: "r".into(),
///         name: "n".into(),
///         tag: "t".into(),
///     },
///     components: vec![],
///     distro: "centos".into(),
///     el_version: 9,
/// };
/// let json = serde_json::to_string(
///     &VersionedVersionDescriptor::new(v.clone()),
/// )
/// .unwrap();
/// assert!(json.starts_with(r#"{"schema_version":1"#));
///
/// let raw: serde_value::Value = serde_json::from_str(&json).unwrap();
/// let parsed = VersionedVersionDescriptor::from_value(
///     raw,
///     Utf8Path::new("/_versions/dev/19.2.3.json"),
/// )
/// .unwrap()
/// .into_latest();
/// assert_eq!(parsed, v);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedVersionDescriptor {
    /// Current schema version. Carries a fully-deserialized
    /// [`VersionDescriptor`].
    V1(VersionDescriptor),
}

impl VersionedVersionDescriptor {
    /// Maximum `schema_version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap a [`VersionDescriptor`] at the current schema version.
    #[must_use]
    pub fn new(desc: VersionDescriptor) -> Self {
        Self::V1(desc)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> VersionDescriptor {
        match self {
            Self::V1(d) => d,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`], producing typed
    /// [`VersionError`]s.
    ///
    /// # Errors
    ///
    /// Returns [`VersionError::MissingSchemaVersion`] if the marker
    /// key is absent; [`VersionError::UnknownSchemaVersion`] if the
    /// marker exceeds [`Self::CURRENT`];
    /// [`VersionError::InvalidDescriptor`] if the inner payload fails
    /// to deserialize as a [`VersionDescriptor`].
    pub fn from_value(value: serde_value::Value, path: &Utf8Path) -> Result<Self, VersionError> {
        let marker = extract_schema_version(&value, SNAKE_TAG).map_err(|e| match e {
            ExtractError::Missing => VersionError::MissingSchemaVersion {
                path: path.to_owned(),
            },
            ExtractError::NotMap | ExtractError::NotInteger => VersionError::InvalidDescriptor {
                path: path.to_owned(),
                message: e.to_string(),
            },
        })?;
        if marker > Self::CURRENT {
            return Err(VersionError::UnknownSchemaVersion {
                path: path.to_owned(),
                found: marker,
                max_supported: Self::CURRENT,
            });
        }
        let d = VersionDescriptor::deserialize(value.into_deserializer()).map_err(
            |e: serde_value::DeserializerError| VersionError::InvalidDescriptor {
                path: path.to_owned(),
                message: e.to_string(),
            },
        )?;
        Ok(Self::V1(d))
    }
}

impl Serialize for VersionedVersionDescriptor {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(d) => serialize_versioned(s, SNAKE_TAG, 1, d),
        }
    }
}
