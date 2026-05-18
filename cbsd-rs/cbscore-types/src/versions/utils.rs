// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Pure version-helpers value surface (zero IO).
//!
//! Phase 1 lands the [`VersionType`] enum only; the parse-family
//! helpers (`parse_version`, `get_major_version`, `get_minor_version`,
//! `normalize_version`, `parse_component_refs`) require the `regex`
//! crate and live in `cbscore::versions::utils`, landing in Phase 2
//! Commit 5.

use serde::{Deserialize, Serialize};

/// Kind of version a descriptor represents.
///
/// The wire form is lowercase (`"release"`, `"dev"`, `"test"`,
/// `"ci"`), matching Python `cbscore.versions.utils.VersionType`'s
/// `enum.StrEnum` values byte-for-byte.
///
/// The variant strings also double as the per-type subdirectory names
/// under the descriptor store (`_versions/release/`, `_versions/dev/`,
/// …) — see seq-004 `VersionType::as_dir_name`.
///
/// # Examples
///
/// ```
/// use cbscore_types::versions::VersionType;
///
/// let v = VersionType::Dev;
/// let json = serde_json::to_string(&v).unwrap();
/// assert_eq!(json, r#""dev""#);
/// let parsed: VersionType = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, v);
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    /// General-availability release.
    Release,
    /// Development snapshot.
    Dev,
    /// Test build.
    Test,
    /// Continuous-integration build.
    Ci,
}
