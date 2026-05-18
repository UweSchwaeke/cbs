// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `schema_version`-tagged wrapper for the snake-case
//! [`ImageDescriptor`] format.

use camino::Utf8Path;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::images::{ImageDescriptor, ImageDescriptorError};
use crate::versioned::{ExtractError, extract_schema_version, serialize_versioned};

const SNAKE_TAG: &str = "schema_version";

/// Wire-marker wrapper for [`ImageDescriptor`].
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::images::{ImageDescriptor, VersionedImageDescriptor};
///
/// let d = ImageDescriptor { releases: vec![], images: vec![] };
/// let json = serde_json::to_string(
///     &VersionedImageDescriptor::new(d.clone()),
/// )
/// .unwrap();
/// assert!(json.starts_with(r#"{"schema_version":1"#));
///
/// let raw: serde_value::Value = serde_json::from_str(&json).unwrap();
/// let parsed = VersionedImageDescriptor::from_value(
///     raw,
///     Utf8Path::new("/images/desc.json"),
/// )
/// .unwrap()
/// .into_latest();
/// assert_eq!(parsed, d);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedImageDescriptor {
    /// Current schema version. Carries a fully-deserialized [`ImageDescriptor`].
    V1(ImageDescriptor),
}

impl VersionedImageDescriptor {
    /// Maximum `schema_version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap an [`ImageDescriptor`] at the current schema version.
    #[must_use]
    pub fn new(desc: ImageDescriptor) -> Self {
        Self::V1(desc)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> ImageDescriptor {
        match self {
            Self::V1(d) => d,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`].
    ///
    /// # Errors
    ///
    /// Returns [`ImageDescriptorError::Invalid`] for any failure today
    /// (missing marker, unknown marker, or inner-deserialize error).
    pub fn from_value(
        value: serde_value::Value,
        path: &Utf8Path,
    ) -> Result<Self, ImageDescriptorError> {
        let marker = extract_schema_version(&value, SNAKE_TAG).map_err(|e| match e {
            ExtractError::Missing => {
                ImageDescriptorError::Invalid(format!("{path}: missing 'schema_version' key"))
            }
            ExtractError::NotMap | ExtractError::NotInteger => {
                ImageDescriptorError::Invalid(format!("{path}: {e}"))
            }
        })?;
        if marker > Self::CURRENT {
            return Err(ImageDescriptorError::Invalid(format!(
                "{path}: unsupported schema_version {marker} (max supported: {})",
                Self::CURRENT
            )));
        }
        let d = ImageDescriptor::deserialize(value.into_deserializer()).map_err(
            |e: serde_value::DeserializerError| {
                ImageDescriptorError::Invalid(format!("{path}: {e}"))
            },
        )?;
        Ok(Self::V1(d))
    }
}

impl Serialize for VersionedImageDescriptor {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(d) => serialize_versioned(s, SNAKE_TAG, 1, d),
        }
    }
}
