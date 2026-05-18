// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Build-artifact report types.
//!
//! The build report is the structured JSON the runner writes to
//! `/runner/<name>.report.json` at the end of a successful container
//! run; the host runner reads it back as the `RunReport.build_report`
//! payload that cbsd-worker forwards over its WebSocket.

use serde::{Deserialize, Serialize};

/// Container image produced by the build.
///
/// # Examples
///
/// ```
/// use cbscore_types::builder::report::ContainerImageReport;
///
/// let c = ContainerImageReport {
///     name: "harbor.clyso.com/ces-devel/ceph".into(),
///     tag: "v19.2.3-dev.1".into(),
///     pushed: true,
/// };
/// let json = serde_json::to_string(&c).unwrap();
/// let parsed: ContainerImageReport = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerImageReport {
    /// Registry-qualified image name.
    pub name: String,
    /// Image tag.
    pub tag: String,
    /// `true` if the image was pushed to the registry.
    pub pushed: bool,
}

/// Location of the release descriptor in S3.
///
/// # Examples
///
/// ```
/// use cbscore_types::builder::report::ReleaseDescriptorReport;
///
/// let r = ReleaseDescriptorReport {
///     s3_path: "releases/19.2.3.json".into(),
///     bucket: "cbs-releases".into(),
/// };
/// let json = serde_json::to_string(&r).unwrap();
/// let parsed: ReleaseDescriptorReport = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, r);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseDescriptorReport {
    /// S3 object key for the release descriptor.
    pub s3_path: String,
    /// S3 bucket where the descriptor was written.
    pub bucket: String,
}

/// One component included in the build report.
///
/// # Examples
///
/// ```
/// use cbscore_types::builder::report::ComponentReport;
///
/// let c = ComponentReport {
///     name: "ceph".into(),
///     version: "19.2.3-42.g5a0b003".into(),
///     sha1: "5a0b003a".into(),
///     repo_url: "https://github.com/ceph/ceph.git".into(),
///     rpms_s3_path: Some("s3://artifacts/ceph/19.2.3/".into()),
/// };
/// let json = serde_json::to_string(&c).unwrap();
/// let parsed: ComponentReport = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentReport {
    /// Component name (matches `CoreComponent.name`).
    pub name: String,
    /// Long version string, e.g. `19.2.3-42.g5a0b003`.
    pub version: String,
    /// Git SHA1 of the built source.
    pub sha1: String,
    /// Source repository URL.
    pub repo_url: String,
    /// S3 path to the RPM artifacts. `None` if not uploaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpms_s3_path: Option<String>,
}

/// Summary of artifacts produced by a build — the structured terminator
/// the in-container `cbsbuild runner build` writes to
/// `/runner/<name>.report.json`.
///
/// `schema_version` replaces Python's `report_version` field (renamed
/// per design 002 §Wire-Format Versioning so the `schema_version`
/// convention is uniform across every wire format the Rust port
/// emits).
///
/// # Examples
///
/// ```
/// use cbscore_types::builder::report::BuildArtifactReport;
///
/// let r = BuildArtifactReport {
///     schema_version: 1,
///     version: "19.2.3".into(),
///     skipped: false,
///     container_image: None,
///     release_descriptor: None,
///     components: vec![],
/// };
/// let json = serde_json::to_string(&r).unwrap();
/// let parsed: BuildArtifactReport = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, r);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildArtifactReport {
    /// Wire-format version marker.
    pub schema_version: u64,
    /// Release version string.
    pub version: String,
    /// `true` if the build was skipped (image already existed).
    pub skipped: bool,
    /// Container image info (populated for both skipped and full
    /// builds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_image: Option<ContainerImageReport>,
    /// Release-descriptor location. `None` when the build was skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_descriptor: Option<ReleaseDescriptorReport>,
    /// Components included in the build (empty when skipped).
    #[serde(default)]
    pub components: Vec<ComponentReport>,
}
