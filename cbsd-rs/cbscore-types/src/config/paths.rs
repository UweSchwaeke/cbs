// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Filesystem-path subset of cbs-build.config.yaml.

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// Filesystem paths the build pipeline reads and writes.
///
/// `scratch_containers` and `ccache` are kebab-cased on the wire
/// (`scratch-containers`, `ccache`) via the container-level
/// `#[serde(rename_all = "kebab-case")]` attribute.
///
/// # Examples
///
/// ```
/// use cbscore_types::config::PathsConfig;
/// use camino::Utf8PathBuf;
///
/// let p = PathsConfig {
///     components: vec![Utf8PathBuf::from("/components")],
///     scratch: Utf8PathBuf::from("/scratch"),
///     scratch_containers: Utf8PathBuf::from("/scratch/containers"),
///     ccache: Some(Utf8PathBuf::from("/ccache")),
/// };
/// let json = serde_json::to_string(&p).unwrap();
/// let parsed: PathsConfig = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, p);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PathsConfig {
    /// Directories searched for `cbs.component.yaml` files.
    pub components: Vec<Utf8PathBuf>,
    /// Scratch directory for source fetches + per-component build trees.
    pub scratch: Utf8PathBuf,
    /// Scratch directory the builder container's writable layer mounts.
    /// Wire key: `scratch-containers`.
    pub scratch_containers: Utf8PathBuf,
    /// Optional ccache root for incremental C/C++ builds.
    #[serde(default)]
    pub ccache: Option<Utf8PathBuf>,
}
