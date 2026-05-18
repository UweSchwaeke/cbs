// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `schema-version`-tagged wrapper for `cbs.component.yaml`
//! ([`CoreComponent`]).

use camino::Utf8Path;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::core::component::{ComponentError, CoreComponent};
use crate::versioned::{ExtractError, extract_schema_version, serialize_versioned};

const KEBAB_TAG: &str = "schema-version";

/// Wire-marker wrapper for [`CoreComponent`] — the on-disk
/// `cbs.component.yaml` format. Surface mirrors
/// [`crate::config::VersionedConfig`] but reports
/// [`ComponentError`]s.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::core::component::{
///     CoreComponent, CoreComponentBuildSection, CoreComponentContainersSection,
///     VersionedCoreComponent,
/// };
///
/// let c = CoreComponent {
///     name: "ceph".into(),
///     repo: "https://example.com/ceph.git".into(),
///     build: CoreComponentBuildSection {
///         rpm: None,
///         get_version: "git describe".into(),
///         deps: "".into(),
///     },
///     containers: CoreComponentContainersSection {
///         path: "containers/ceph.yaml".into(),
///     },
/// };
/// let yaml = serde_saphyr::to_string(
///     &VersionedCoreComponent::new(c.clone()),
/// ).unwrap();
/// assert!(yaml.starts_with("schema-version: 1\n"));
///
/// let raw: serde_value::Value = serde_saphyr::from_str(&yaml).unwrap();
/// let parsed = VersionedCoreComponent::from_value(
///     raw,
///     Utf8Path::new("/components/ceph/cbs.component.yaml"),
/// )
/// .unwrap()
/// .into_latest();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedCoreComponent {
    /// Current schema version. Carries a fully-deserialized [`CoreComponent`].
    V1(CoreComponent),
}

impl VersionedCoreComponent {
    /// Maximum `schema-version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap a [`CoreComponent`] at the current schema version.
    #[must_use]
    pub fn new(comp: CoreComponent) -> Self {
        Self::V1(comp)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> CoreComponent {
        match self {
            Self::V1(c) => c,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`], producing typed
    /// [`ComponentError`]s.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentError::MissingSchemaVersion`] if the marker
    /// key is absent; [`ComponentError::UnknownSchemaVersion`] if the
    /// marker exceeds [`Self::CURRENT`]; [`ComponentError::Yaml`] if
    /// the inner payload fails to deserialize as a [`CoreComponent`].
    pub fn from_value(value: serde_value::Value, path: &Utf8Path) -> Result<Self, ComponentError> {
        let marker = extract_schema_version(&value, KEBAB_TAG).map_err(|e| match e {
            ExtractError::Missing => ComponentError::MissingSchemaVersion {
                path: path.to_owned(),
            },
            ExtractError::NotMap | ExtractError::NotInteger => ComponentError::Yaml {
                path: path.to_owned(),
                message: e.to_string(),
            },
        })?;
        if marker > Self::CURRENT {
            return Err(ComponentError::UnknownSchemaVersion {
                path: path.to_owned(),
                found: marker,
                max_supported: Self::CURRENT,
            });
        }
        let c = CoreComponent::deserialize(value.into_deserializer()).map_err(
            |e: serde_value::DeserializerError| ComponentError::Yaml {
                path: path.to_owned(),
                message: e.to_string(),
            },
        )?;
        Ok(Self::V1(c))
    }
}

impl Serialize for VersionedCoreComponent {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(c) => serialize_versioned(s, KEBAB_TAG, 1, c),
        }
    }
}
