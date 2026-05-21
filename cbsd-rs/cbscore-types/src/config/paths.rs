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
///     versions: Some(Utf8PathBuf::from("/srv/cbs/versions")),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ccache: Option<Utf8PathBuf>,
    /// Optional root for the version-descriptor store (the directory
    /// under which `cbsbuild versions create` writes
    /// `<type>/<VERSION>.json`).
    ///
    /// When `None`, `cbsbuild versions create` falls back to
    /// `<git-rev-parse --show-toplevel>/_versions` for Python parity.
    /// When `Some(p)`, the resolver canonicalises `p` and uses it as
    /// the root (CLI `--versions-dir` overrides this).
    ///
    /// Both `#[serde(default)]` (so existing YAML files that omit the
    /// field deserialise as `None`) and
    /// `#[serde(skip_serializing_if = "Option::is_none")]` (so
    /// re-serialising a deserialised file omits the field rather than
    /// emitting `versions: null`) are mandatory: the wire-format
    /// round-trip stability documented in design 004 §OQ6 depends on
    /// both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub versions: Option<Utf8PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips a `PathsConfig` that carries `versions = Some(_)`
    /// through YAML byte-stably.
    #[test]
    fn versions_set_round_trips_through_yaml() {
        let p = PathsConfig {
            components: vec![Utf8PathBuf::from("/c")],
            scratch: Utf8PathBuf::from("/s"),
            scratch_containers: Utf8PathBuf::from("/s/c"),
            ccache: None,
            versions: Some(Utf8PathBuf::from("/srv/cbs/versions")),
        };
        let yaml = serde_saphyr::to_string(&p).unwrap();
        assert!(
            yaml.contains("versions: /srv/cbs/versions"),
            "expected `versions:` in YAML output: {yaml}",
        );
        let parsed: PathsConfig = serde_saphyr::from_str(&yaml).unwrap();
        assert_eq!(parsed, p);
    }

    /// Confirms the load-bearing serde attributes:
    /// `#[serde(default)]` lets the field be omitted on the wire, and
    /// `#[serde(skip_serializing_if = "Option::is_none")]` keeps the
    /// re-serialised YAML free of any `versions:` line so old binaries
    /// see byte-identical input.
    #[test]
    fn versions_unset_serialises_absent_from_yaml() {
        let p = PathsConfig {
            components: vec![Utf8PathBuf::from("/c")],
            scratch: Utf8PathBuf::from("/s"),
            scratch_containers: Utf8PathBuf::from("/s/c"),
            ccache: None,
            versions: None,
        };
        let yaml = serde_saphyr::to_string(&p).unwrap();
        assert!(
            !yaml.contains("versions:"),
            "expected `versions:` absent from YAML when field is None: {yaml}",
        );
    }

    /// YAML that omits the `versions` key deserialises as `None`
    /// (load-bearing for backwards compatibility — pre-seq-004 operator
    /// configs round-trip through the new binary unchanged).
    #[test]
    fn versions_omitted_in_input_deserialises_as_none() {
        let yaml = "\
            components: [/c]\n\
            scratch: /s\n\
            scratch-containers: /s/c\n\
        ";
        let parsed: PathsConfig = serde_saphyr::from_str(yaml).unwrap();
        assert_eq!(parsed.versions, None);
        assert_eq!(parsed.ccache, None);
    }
}
