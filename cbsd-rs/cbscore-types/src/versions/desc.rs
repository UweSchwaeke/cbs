// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Version-descriptor types (`<root>/<type>/<VERSION>.json`).
//!
//! Wire format is JSON with `snake_case` keys (no `rename_all` —
//! descriptors keep serde's default Rust-identifier-as-wire-key per
//! design 002 §Wire-Format Versioning).

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::versions::utils::VersionType;

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

/// Build the on-disk path of a version descriptor under the configured
/// descriptor-store root.
///
/// The layout is `<root>/<type>/<VERSION>.json` (design 004 OQ3, locked
/// in by Python parity and the type-encoded-in-directory invariant).
/// This is the single source of truth for the layout — every other
/// code path that needs it imports this helper.
///
/// `root` must be **absolute**. The caller (typically the resolver in
/// `cbscore::versions::resolve_root`) is expected to canonicalise the
/// root before calling here; a relative root would silently produce a
/// path relative to the process cwd, which is almost never what an
/// operator means. The debug-assert below catches accidental
/// relative-root passes in test builds.
///
/// # Panics
///
/// Debug builds `debug_assert!` that `root.is_absolute()`. Release
/// builds skip the check and return whatever the join produces (per
/// `camino::Utf8Path::join` semantics — a relative root just yields
/// a relative result).
///
/// # Examples
///
/// ```
/// use camino::{Utf8Path, Utf8PathBuf};
/// use cbscore_types::versions::VersionType;
/// use cbscore_types::versions::desc::descriptor_path;
///
/// let path = descriptor_path(
///     Utf8Path::new("/srv/cbs/_versions"),
///     VersionType::Dev,
///     "19.2.3-dev1",
/// );
/// assert_eq!(
///     path,
///     Utf8PathBuf::from("/srv/cbs/_versions/dev/19.2.3-dev1.json"),
/// );
/// ```
#[must_use]
pub fn descriptor_path(root: &Utf8Path, ty: VersionType, version: &str) -> Utf8PathBuf {
    debug_assert!(
        root.is_absolute(),
        "descriptor_path expects an absolute root; got {root}",
    );
    root.join(ty.as_dir_name()).join(format!("{version}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks the layout for every `VersionType` variant. Any future
    /// change to either `VersionType::as_dir_name` or the chain inside
    /// `descriptor_path` would surface here.
    #[test]
    fn descriptor_path_layout_per_variant() {
        let root = Utf8Path::new("/r");
        assert_eq!(
            descriptor_path(root, VersionType::Release, "19.2.3"),
            Utf8PathBuf::from("/r/release/19.2.3.json"),
        );
        assert_eq!(
            descriptor_path(root, VersionType::Dev, "19.2.3-dev1"),
            Utf8PathBuf::from("/r/dev/19.2.3-dev1.json"),
        );
        assert_eq!(
            descriptor_path(root, VersionType::Test, "1.0.0-test"),
            Utf8PathBuf::from("/r/test/1.0.0-test.json"),
        );
        assert_eq!(
            descriptor_path(root, VersionType::Ci, "ci-abc123"),
            Utf8PathBuf::from("/r/ci/ci-abc123.json"),
        );
    }

    /// `UUIDv7` version strings (seq-005's planned shape — no in-tree
    /// callers yet, but the layout helper must already handle the
    /// opaque case unchanged).
    #[test]
    fn descriptor_path_accepts_opaque_version_string() {
        let root = Utf8Path::new("/r");
        let path = descriptor_path(
            root,
            VersionType::Dev,
            "0190b6a7-8d61-7000-8000-aabbccddeeff",
        );
        assert_eq!(
            path,
            Utf8PathBuf::from("/r/dev/0190b6a7-8d61-7000-8000-aabbccddeeff.json",),
        );
    }

    /// `descriptor_path` panics on a relative root in debug builds.
    /// The runtime guard catches the common operator-mistake of
    /// passing a relative path through `--versions-dir` (the resolver
    /// canonicalises before calling here, so this only fires if a
    /// caller bypasses the resolver).
    #[test]
    #[should_panic(expected = "descriptor_path expects an absolute root")]
    #[cfg(debug_assertions)]
    fn descriptor_path_rejects_relative_root_in_debug() {
        let _ = descriptor_path(Utf8Path::new("relative/root"), VersionType::Dev, "1.0.0");
    }
}
