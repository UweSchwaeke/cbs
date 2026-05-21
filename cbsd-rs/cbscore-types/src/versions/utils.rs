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
/// under the descriptor store (`<root>/release/`, `<root>/dev/`, …) —
/// see [`VersionType::as_dir_name`] for the accessor and
/// [`crate::versions::desc::descriptor_path`] for the path-builder
/// that uses it.
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

impl VersionType {
    /// Per-type subdirectory name under the descriptor store root.
    ///
    /// `cbsbuild versions create` writes its output to
    /// `<root>/<ty.as_dir_name()>/<VERSION>.json`. The returned
    /// string is identical to the serde wire form produced by
    /// `#[serde(rename_all = "lowercase")]` on this enum — locked in
    /// by design 004 OQ3 (the "type-encoded-in-directory" invariant)
    /// and asserted by the doctest below for all four variants.
    ///
    /// # Examples
    ///
    /// ```
    /// use cbscore_types::versions::VersionType;
    ///
    /// assert_eq!(VersionType::Release.as_dir_name(), "release");
    /// assert_eq!(VersionType::Dev.as_dir_name(), "dev");
    /// assert_eq!(VersionType::Test.as_dir_name(), "test");
    /// assert_eq!(VersionType::Ci.as_dir_name(), "ci");
    ///
    /// // The dir-name matches the serde wire string for every
    /// // variant: any future change to one without the other would
    /// // surface here.
    /// for v in [
    ///     VersionType::Release,
    ///     VersionType::Dev,
    ///     VersionType::Test,
    ///     VersionType::Ci,
    /// ] {
    ///     let wire = serde_json::to_string(&v).unwrap();
    ///     let trimmed = wire.trim_matches('"');
    ///     assert_eq!(v.as_dir_name(), trimmed);
    /// }
    /// ```
    #[must_use]
    pub const fn as_dir_name(&self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Dev => "dev",
            Self::Test => "test",
            Self::Ci => "ci",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks the four directory-name strings against accidental drift.
    /// The Python side hardcodes the same strings in
    /// `cbscore/versions/utils.py:VersionType`; changing them on
    /// either side would silently relocate every operator's existing
    /// descriptor tree.
    #[test]
    fn as_dir_name_returns_expected_strings() {
        assert_eq!(VersionType::Release.as_dir_name(), "release");
        assert_eq!(VersionType::Dev.as_dir_name(), "dev");
        assert_eq!(VersionType::Test.as_dir_name(), "test");
        assert_eq!(VersionType::Ci.as_dir_name(), "ci");
    }

    /// `as_dir_name()` and `serde_json::to_string(&v)` must agree —
    /// if a future serde rename breaks the wire form, the build of
    /// this test breaks alongside the doctest, before any operator
    /// sees the divergence.
    #[test]
    fn as_dir_name_matches_serde_wire_for_every_variant() {
        for v in [
            VersionType::Release,
            VersionType::Dev,
            VersionType::Test,
            VersionType::Ci,
        ] {
            let wire = serde_json::to_string(&v).unwrap();
            assert_eq!(v.as_dir_name(), wire.trim_matches('"'));
        }
    }
}
