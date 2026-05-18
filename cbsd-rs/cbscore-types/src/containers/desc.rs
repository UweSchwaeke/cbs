// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Container-descriptor types (the YAML at `containers.path` in
//! `cbs.component.yaml`).
//!
//! Wire format is JSON with snake_case keys (no `rename_all` —
//! descriptors keep serde's default).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A named shell script entry on a container descriptor.
///
/// # Examples
///
/// ```
/// use cbscore_types::containers::desc::ContainerScript;
///
/// let s = ContainerScript {
///     name: "install".into(),
///     run: "dnf install -y foo".into(),
/// };
/// let json = serde_json::to_string(&s).unwrap();
/// let parsed: ContainerScript = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, s);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerScript {
    /// Operator-chosen script name.
    pub name: String,
    /// Shell command line executed inside the container.
    pub run: String,
}

/// One repository entry inside [`ContainerPre`].
///
/// Discrimination of `file://` / `http(s)://` / `copr://` happens at
/// the `cbscore::containers::repos` consumer (Phase 5 Commit 4) by
/// inspecting [`source`](Self::source); the wire format is a single
/// flat shape so existing on-disk container descriptors parse
/// unchanged (no `type:` tag introduced).
///
/// # Examples
///
/// ```
/// use cbscore_types::containers::desc::ContainerRepo;
///
/// let r = ContainerRepo {
///     name: "ceph-test-repo".into(),
///     source: "https://example.com/test.repo".into(),
///     dest: Some("/etc/yum.repos.d/test.repo".into()),
/// };
/// let json = serde_json::to_string(&r).unwrap();
/// let parsed: ContainerRepo = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, r);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerRepo {
    /// Operator-chosen repo identifier.
    pub name: String,
    /// Repo source URL — `file://`, `http(s)://`, or `copr://`.
    pub source: String,
    /// Destination path inside the container (required for `file://` /
    /// `http(s)://`; absent for `copr://`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
}

/// Pre-install section of a container descriptor.
///
/// Runs before the main `dnf install` of `packages` — installs GPG
/// keys, registers extra repos, and runs `scripts` to set up the
/// build environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerPre {
    /// GPG key URLs imported before package installation.
    #[serde(default)]
    pub keys: Vec<String>,
    /// Packages installed during the pre stage.
    #[serde(default)]
    pub packages: Vec<String>,
    /// Extra repositories registered during the pre stage.
    #[serde(default)]
    pub repos: Vec<ContainerRepo>,
    /// Shell scripts run after key import and repo registration.
    #[serde(default)]
    pub scripts: Vec<ContainerScript>,
}

/// One entry of [`ContainerPackages::required`] /
/// [`ContainerPackages::optional`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerPackagesEntry {
    /// Operator-facing section label.
    pub section: String,
    /// Packages installed for this section.
    pub packages: Vec<String>,
    /// Optional conditional expression deciding whether the section
    /// applies to a given build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cond: Option<String>,
}

/// Package-install instructions for a container descriptor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerPackages {
    /// Required packages (build fails if installation fails).
    #[serde(default)]
    pub required: Vec<ContainerPackagesEntry>,
    /// Optional packages (build continues if installation fails).
    #[serde(default)]
    pub optional: Vec<ContainerPackagesEntry>,
}

/// Container runtime configuration (env, labels, annotations).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Container env vars.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// OCI labels.
    #[serde(default)]
    pub labels: HashMap<String, String>,
    /// OCI annotations.
    #[serde(default)]
    pub annotations: HashMap<String, String>,
}

/// Top-level container descriptor — the YAML referenced by
/// `cbs.component.yaml`'s `containers.path`.
///
/// # Examples
///
/// ```
/// use cbscore_types::containers::desc::{
///     ContainerDescriptor, ContainerPackages, ContainerPre,
/// };
///
/// let c = ContainerDescriptor {
///     config: None,
///     pre: ContainerPre::default(),
///     packages: ContainerPackages::default(),
///     post: vec![],
/// };
/// let json = serde_json::to_string(&c).unwrap();
/// let parsed: ContainerDescriptor = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerDescriptor {
    /// Optional container runtime configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ContainerConfig>,
    /// Pre-install stage (keys / repos / packages / scripts).
    pub pre: ContainerPre,
    /// Main `dnf install` stage.
    pub packages: ContainerPackages,
    /// Post-install scripts.
    #[serde(default)]
    pub post: Vec<ContainerScript>,
}
