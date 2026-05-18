// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `schema_version`-tagged wrapper for the snake-case
//! [`BuildArtifactReport`] format.

use camino::Utf8Path;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::builder::{BuildArtifactReport, BuilderError};
use crate::versioned::{extract_schema_version, serialize_versioned};

const SNAKE_TAG: &str = "schema_version";

/// Wire-marker wrapper for [`BuildArtifactReport`] — the JSON the
/// in-container build writes to `/runner/<name>.report.json`.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::builder::{BuildArtifactReport, VersionedBuildArtifactReport};
///
/// let r = BuildArtifactReport {
///     schema_version: 1,
///     version: "19.2.3".into(),
///     skipped: false,
///     container_image: None,
///     release_descriptor: None,
///     components: vec![],
/// };
/// let json = serde_json::to_string(
///     &VersionedBuildArtifactReport::new(r.clone()),
/// )
/// .unwrap();
/// assert!(json.starts_with(r#"{"schema_version":1"#));
///
/// let raw: serde_value::Value = serde_json::from_str(&json).unwrap();
/// let parsed = VersionedBuildArtifactReport::from_value(
///     raw,
///     Utf8Path::new("/runner/build.report.json"),
/// )
/// .unwrap()
/// .into_latest();
/// assert_eq!(parsed, r);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedBuildArtifactReport {
    /// Current schema version. Carries a fully-deserialized [`BuildArtifactReport`].
    V1(BuildArtifactReport),
}

impl VersionedBuildArtifactReport {
    /// Maximum `schema_version` this build of cbscore-rs understands.
    pub const CURRENT: u64 = 1;

    /// Wrap a [`BuildArtifactReport`] at the current schema version.
    #[must_use]
    pub const fn new(report: BuildArtifactReport) -> Self {
        Self::V1(report)
    }

    /// Unwrap into the latest-version payload.
    #[must_use]
    pub fn into_latest(self) -> BuildArtifactReport {
        match self {
            Self::V1(r) => r,
        }
    }

    /// Parse from a pre-parsed [`serde_value::Value`].
    ///
    /// # Errors
    ///
    /// Returns [`BuilderError::MissingScript`] (the only Phase-1
    /// variant of [`BuilderError`]) wrapping a synthetic path for any
    /// parse failure today; future schema versions may refine to
    /// per-stage variants. Phase 5 commits extend the enum and this
    /// dispatch.
    pub fn from_value(value: serde_value::Value, path: &Utf8Path) -> Result<Self, BuilderError> {
        // Phase 1's BuilderError has only MissingScript — overload it
        // for any wire-format failure on this report. Phase 5 Commit
        // adds dedicated report variants.
        let synth = || BuilderError::MissingScript {
            path: path.to_owned(),
        };
        let marker = extract_schema_version(&value, SNAKE_TAG).map_err(|_| synth())?;
        if marker > Self::CURRENT {
            return Err(synth());
        }
        let r = BuildArtifactReport::deserialize(value.into_deserializer())
            .map_err(|_: serde_value::DeserializerError| synth())?;
        Ok(Self::V1(r))
    }
}

impl Serialize for VersionedBuildArtifactReport {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V1(r) => serialize_versioned(s, SNAKE_TAG, 1, r),
        }
    }
}
