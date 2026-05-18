// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbs.component.yaml` value + error surface (zero IO).
//!
//! Phase 1 lands the value-side types in this module and the error
//! taxonomy in [`errors`]. The IO function
//! `cbscore::core::component::load_components` lands in Phase 5
//! Commit 2.

pub mod errors;

pub use errors::ComponentError;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// Container-section subset of a `cbs.component.yaml` file.
///
/// # Examples
///
/// ```
/// use cbscore_types::core::component::CoreComponentContainersSection;
/// use camino::Utf8PathBuf;
///
/// let c = CoreComponentContainersSection {
///     path: Utf8PathBuf::from("containers/ceph.yaml"),
/// };
/// let json = serde_json::to_string(&c).unwrap();
/// let parsed: CoreComponentContainersSection =
///     serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CoreComponentContainersSection {
    /// Path to the container descriptor YAML, relative to the
    /// component directory.
    pub path: Utf8PathBuf,
}

/// RPM-build subset of a `cbs.component.yaml` `build` section.
///
/// # Examples
///
/// ```
/// use cbscore_types::core::component::CoreComponentBuildRPMSection;
///
/// let b = CoreComponentBuildRPMSection {
///     build: "make rpm".into(),
///     release_rpm: "ceph-release".into(),
/// };
/// let json = serde_json::to_string(&b).unwrap();
/// let parsed: CoreComponentBuildRPMSection = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, b);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CoreComponentBuildRPMSection {
    /// Shell command building the source RPM(s).
    pub build: String,
    /// Release-RPM artifact name. Wire key: `release-rpm`.
    pub release_rpm: String,
}

/// Build subset of a `cbs.component.yaml` file.
///
/// # Examples
///
/// ```
/// use cbscore_types::core::component::CoreComponentBuildSection;
///
/// let b = CoreComponentBuildSection {
///     rpm: None,
///     get_version: "git describe".into(),
///     deps: "ceph-common".into(),
/// };
/// let json = serde_json::to_string(&b).unwrap();
/// let parsed: CoreComponentBuildSection = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, b);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CoreComponentBuildSection {
    /// Optional RPM-build subsection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm: Option<CoreComponentBuildRPMSection>,
    /// Shell command extracting the version string from a source
    /// checkout. Wire key: `get-version`.
    pub get_version: String,
    /// Operator-facing dependency list (informational).
    pub deps: String,
}

/// Top-level value of a `cbs.component.yaml` file.
///
/// `cbs.component.yaml` is kebab-case on the wire (per design 002
/// §Wire-Format Versioning), so this struct carries
/// `#[serde(rename_all = "kebab-case")]`.
///
/// # Examples
///
/// ```
/// use cbscore_types::core::component::{
///     CoreComponent, CoreComponentBuildSection, CoreComponentContainersSection,
/// };
/// use camino::Utf8PathBuf;
///
/// let c = CoreComponent {
///     name: "ceph".into(),
///     repo: "https://github.com/ceph/ceph.git".into(),
///     build: CoreComponentBuildSection {
///         rpm: None,
///         get_version: "git describe".into(),
///         deps: "".into(),
///     },
///     containers: CoreComponentContainersSection {
///         path: Utf8PathBuf::from("containers/ceph.yaml"),
///     },
/// };
/// let json = serde_json::to_string(&c).unwrap();
/// let parsed: CoreComponent = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CoreComponent {
    /// Component name (key in `load_components` `HashMap`).
    pub name: String,
    /// Source repository URL.
    pub repo: String,
    /// Build instructions.
    pub build: CoreComponentBuildSection,
    /// Container production instructions.
    pub containers: CoreComponentContainersSection,
}

/// Component file location + parsed value, as returned by
/// `cbscore::core::component::load_components` (Phase 5 Commit 2).
///
/// # Examples
///
/// ```
/// use cbscore_types::core::component::{
///     CoreComponent, CoreComponentBuildSection,
///     CoreComponentContainersSection, CoreComponentLoc,
/// };
/// use camino::Utf8PathBuf;
///
/// let comp = CoreComponent {
///     name: "ceph".into(),
///     repo: "https://example.com/ceph.git".into(),
///     build: CoreComponentBuildSection {
///         rpm: None,
///         get_version: "git describe".into(),
///         deps: "".into(),
///     },
///     containers: CoreComponentContainersSection {
///         path: Utf8PathBuf::from("containers/ceph.yaml"),
///     },
/// };
/// let l = CoreComponentLoc {
///     path: Utf8PathBuf::from("/components/ceph/cbs.component.yaml"),
///     comp,
/// };
/// let json = serde_json::to_string(&l).unwrap();
/// let parsed: CoreComponentLoc = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, l);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CoreComponentLoc {
    /// Absolute path of the `cbs.component.yaml` file that produced
    /// [`comp`](Self::comp).
    pub path: Utf8PathBuf,
    /// Parsed component value.
    pub comp: CoreComponent,
}
