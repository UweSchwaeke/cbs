// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Release-descriptor types — the JSON written to S3 by the release
//! publisher.
//!
//! Wire format: JSON, `snake_case` keys (no `rename_all` on the
//! containing structs). Enum variants are lowered to their wire form
//! via per-enum `rename_all` per design 002 §Releases struct sketch.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Build architecture.
///
/// `snake_case` on the enum is a Rust→Rust identifier convention, not
/// a wire-format change: the variant name `X86_64` would otherwise
/// serialise to `x8664` in JSON.
///
/// # Examples
///
/// ```
/// use cbscore_types::releases::desc::ArchType;
///
/// let a = ArchType::X86_64;
/// let json = serde_json::to_string(&a).unwrap();
/// assert_eq!(json, r#""x86_64""#);
/// let parsed: ArchType = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, a);
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(non_camel_case_types)]
pub enum ArchType {
    /// Intel/AMD 64-bit.
    X86_64,
}

/// Build artifact type.
///
/// # Examples
///
/// ```
/// use cbscore_types::releases::desc::BuildType;
///
/// let b = BuildType::Rpm;
/// let json = serde_json::to_string(&b).unwrap();
/// assert_eq!(json, r#""rpm""#);
/// let parsed: BuildType = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, b);
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildType {
    /// RPM package build.
    Rpm,
}

/// Build-target identification (architecture + build type + OS).
///
/// # Examples
///
/// ```
/// use cbscore_types::releases::desc::{ArchType, BuildInfo, BuildType};
///
/// let b = BuildInfo {
///     arch: ArchType::X86_64,
///     build_type: BuildType::Rpm,
///     os_version: "el9".into(),
/// };
/// let json = serde_json::to_string(&b).unwrap();
/// let parsed: BuildInfo = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, b);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildInfo {
    /// Build architecture.
    pub arch: ArchType,
    /// Build artifact type.
    pub build_type: BuildType,
    /// Target OS version label (e.g., `el9`, `el10`).
    pub os_version: String,
}

/// Locations of an RPM release's artifacts in S3.
///
/// The canonical Rust type name is [`ReleaseArtifacts`]; the design
/// keeps the door open for future build-type-specific artifact shapes
/// by reserving the name. RPM is the only shape today.
///
/// # Examples
///
/// ```
/// use cbscore_types::releases::desc::ReleaseArtifacts;
///
/// let a = ReleaseArtifacts {
///     loc: "s3://release-bucket/19.2.3/x86_64/".into(),
///     release_rpm_loc: "s3://release-bucket/19.2.3/release.rpm".into(),
/// };
/// let json = serde_json::to_string(&a).unwrap();
/// let parsed: ReleaseArtifacts = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, a);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseArtifacts {
    /// S3 key prefix where the build artifacts (per-component RPMs)
    /// live.
    pub loc: String,
    /// S3 key for the release RPM itself.
    pub release_rpm_loc: String,
}

/// One per-component build entry in a release descriptor.
///
/// Flattens Python's `ReleaseComponentHeader + BuildInfo + repo_url +
/// artifacts` (multiple inheritance) into one Rust struct. Canonical
/// Rust type name per design 002 §Releases struct sketch — replaces
/// the earlier `ReleaseComponentVersion` draft name.
///
/// # Examples
///
/// ```
/// use cbscore_types::releases::desc::{
///     ArchType, BuildType, ReleaseArtifacts, ReleaseComponent,
/// };
///
/// let c = ReleaseComponent {
///     name: "ceph".into(),
///     version: "19.2.3".into(),
///     sha1: "abc1234".into(),
///     arch: ArchType::X86_64,
///     build_type: BuildType::Rpm,
///     os_version: "el9".into(),
///     repo_url: "https://github.com/ceph/ceph.git".into(),
///     artifacts: ReleaseArtifacts {
///         loc: "s3://b/p/".into(),
///         release_rpm_loc: "s3://b/p/release.rpm".into(),
///     },
/// };
/// let json = serde_json::to_string(&c).unwrap();
/// let parsed: ReleaseComponent = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseComponent {
    /// Component name (matches `CoreComponent.name`).
    pub name: String,
    /// Component version string.
    pub version: String,
    /// Git SHA1 the component built from.
    pub sha1: String,
    /// Build architecture.
    pub arch: ArchType,
    /// Build artifact type.
    pub build_type: BuildType,
    /// Target OS version label.
    pub os_version: String,
    /// Source repository URL.
    pub repo_url: String,
    /// S3 locations of the build artifacts.
    pub artifacts: ReleaseArtifacts,
}

/// One per-arch build entry within a release descriptor — pairs build
/// metadata with the per-component build outputs.
///
/// # Examples
///
/// ```
/// use cbscore_types::releases::desc::{
///     ArchType, BuildType, ReleaseBuildEntry,
/// };
/// use std::collections::HashMap;
///
/// let b = ReleaseBuildEntry {
///     arch: ArchType::X86_64,
///     build_type: BuildType::Rpm,
///     os_version: "el9".into(),
///     components: HashMap::new(),
/// };
/// let json = serde_json::to_string(&b).unwrap();
/// let parsed: ReleaseBuildEntry = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, b);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseBuildEntry {
    /// Build architecture (flattened from `BuildInfo`).
    pub arch: ArchType,
    /// Build artifact type (flattened from `BuildInfo`).
    pub build_type: BuildType,
    /// Target OS version label (flattened from `BuildInfo`).
    pub os_version: String,
    /// Per-component build outputs, keyed by component name.
    pub components: HashMap<String, ReleaseComponent>,
}

/// Top-level release descriptor — the JSON written to S3 at
/// `release.json` for each published release.
///
/// # Examples
///
/// ```
/// use cbscore_types::releases::desc::ReleaseDesc;
/// use std::collections::HashMap;
///
/// let d = ReleaseDesc {
///     version: "19.2.3".into(),
///     builds: HashMap::new(),
/// };
/// let json = serde_json::to_string(&d).unwrap();
/// let parsed: ReleaseDesc = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, d);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseDesc {
    /// Release version string.
    pub version: String,
    /// Per-architecture build entries, keyed by `ArchType`.
    pub builds: HashMap<ArchType, ReleaseBuildEntry>,
}
