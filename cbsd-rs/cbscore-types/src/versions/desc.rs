// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Version-descriptor types (`<root>/<type>/<VERSION>.json`).
//!
//! Wire format is JSON with `snake_case` keys (no `rename_all` —
//! descriptors keep serde's default Rust-identifier-as-wire-key per
//! design 002 §Wire-Format Versioning).

use serde::{Deserialize, Serialize};

/// Author identity recorded on a version descriptor.
///
/// # Examples
///
/// ```
/// use cbscore_types::versions::desc::VersionSignedOffBy;
///
/// let s = VersionSignedOffBy {
///     user: "Alice".into(),
///     email: "alice@example.com".into(),
/// };
/// let json = serde_json::to_string(&s).unwrap();
/// let parsed: VersionSignedOffBy = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, s);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionSignedOffBy {
    /// User name on the sign-off line.
    pub user: String,
    /// User email on the sign-off line.
    pub email: String,
}

/// Container image reference (registry + name + tag) on a version
/// descriptor.
///
/// # Examples
///
/// ```
/// use cbscore_types::versions::desc::VersionImage;
///
/// let img = VersionImage {
///     registry: "quay.io".into(),
///     name: "ceph".into(),
///     tag: "19.2.3-dev1".into(),
/// };
/// let json = serde_json::to_string(&img).unwrap();
/// let parsed: VersionImage = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, img);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionImage {
    /// Container registry hostname.
    pub registry: String,
    /// Image name (without the registry prefix).
    pub name: String,
    /// Image tag.
    pub tag: String,
}

/// One per-component entry inside a version descriptor — pins the
/// component's source repository to a specific commit / ref.
///
/// # Examples
///
/// ```
/// use cbscore_types::versions::desc::VersionComponent;
///
/// let c = VersionComponent {
///     name: "ceph".into(),
///     repo: "https://github.com/ceph/ceph.git".into(),
///     ref_: "abcdef1234".into(),
/// };
/// let json = serde_json::to_string(&c).unwrap();
/// let parsed: VersionComponent = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, c);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionComponent {
    /// Component name (matches `CoreComponent.name`).
    pub name: String,
    /// Git repository URL.
    pub repo: String,
    /// Git ref (commit SHA, tag, or branch) the component is pinned to.
    /// Wire key: `ref` (`ref_` is the Rust identifier because `ref` is
    /// a reserved keyword).
    #[serde(rename = "ref")]
    pub ref_: String,
}

/// Top-level version descriptor — the JSON file written by
/// `cbsbuild versions create` under
/// `<root>/<type>/<VERSION>.json`.
///
/// # Examples
///
/// ```
/// use cbscore_types::versions::desc::{
///     VersionComponent, VersionDescriptor, VersionImage, VersionSignedOffBy,
/// };
///
/// let v = VersionDescriptor {
///     version: "19.2.3-dev1".into(),
///     title: "Ceph 19.2.3 dev1".into(),
///     signed_off_by: VersionSignedOffBy {
///         user: "Alice".into(),
///         email: "alice@example.com".into(),
///     },
///     image: VersionImage {
///         registry: "quay.io".into(),
///         name: "ceph".into(),
///         tag: "19.2.3-dev1".into(),
///     },
///     components: vec![VersionComponent {
///         name: "ceph".into(),
///         repo: "https://github.com/ceph/ceph.git".into(),
///         ref_: "abcdef1234".into(),
///     }],
///     distro: "centos".into(),
///     el_version: 9,
/// };
/// let json = serde_json::to_string(&v).unwrap();
/// let parsed: VersionDescriptor = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, v);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionDescriptor {
    /// Version string (`[prefix-]vM.m.p[-suffix]` for the parseable
    /// case, an opaque `UUIDv7` for the seq-005 path).
    pub version: String,
    /// Operator-facing release title.
    pub title: String,
    /// Sign-off identity.
    pub signed_off_by: VersionSignedOffBy,
    /// Container image produced by the build.
    pub image: VersionImage,
    /// Components pinned to specific refs for this build.
    pub components: Vec<VersionComponent>,
    /// Target distro name (e.g., `centos`, `rhel`).
    pub distro: String,
    /// Target distro major version (e.g., `9` for el9).
    pub el_version: u32,
}
