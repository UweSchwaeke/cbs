// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `version_create_helper` — assemble a [`VersionDescriptor`] from
//! operator-supplied component refs.
//!
//! Per design 002 §Version creation (lines 706–711) and the Phase
//! 6 Commit 2 plan: resolves each `(component, ref)` pair to a git
//! SHA via [`crate::utils::git::git_ls_remote`], then composes the
//! result into a [`VersionDescriptor`] ready for
//! [`crate::versions::desc::write_descriptor`].
//!
//! The Rust port resolves refs to SHAs at create time (deviation
//! from Python, which keeps the operator-supplied ref string in
//! the descriptor) — operators inspecting a freshly-minted
//! descriptor see the concrete commit a build will run against,
//! eliminating one source of "what did I actually build?"
//! confusion.

use std::collections::HashMap;

use cbscore_types::versions::VersionError;
use cbscore_types::versions::desc::{
    VersionComponent, VersionDescriptor, VersionImage, VersionSignedOffBy,
};
use cbscore_types::versions::utils::VersionType;

use crate::utils::git::git_ls_remote;
use crate::utils::subprocess::CmdArg;

const TARGET_VERSIONS_CREATE: &str = "cbscore::versions::create";

/// Inputs for [`version_create_helper`]. Bundles the many args
/// the descriptor needs so the helper stays at the 4-arg
/// function-hygiene cap per CLAUDE.md.
#[derive(Debug, Clone)]
pub struct VersionCreateInput {
    /// Operator-supplied VERSION (e.g. `19.2.3-dev.1`).
    pub version: String,
    /// Release type — drives the title and the on-disk
    /// `_versions/<type>/<VERSION>.json` path.
    pub version_type: VersionType,
    /// `(component-name, git-ref)` pairs. Order is preserved into
    /// the resulting descriptor's `components` Vec.
    pub component_refs: Vec<(String, String)>,
    /// Map of component name → git repo URL. Every entry in
    /// `component_refs` must have a matching key here.
    pub component_repos: HashMap<String, String>,
    /// Signed-off-by identity from the operator's `git config`.
    pub signed_off_by: VersionSignedOffBy,
    /// Builder image registry hostname.
    pub registry: String,
    /// Builder image name.
    pub image_name: String,
    /// Optional builder image tag. `None` falls back to `version`.
    pub image_tag: Option<String>,
    /// Build distribution (e.g. `centos`).
    pub distro: String,
    /// EL major version (e.g. `9`).
    pub el_version: u32,
}

/// Build a [`VersionDescriptor`] from `input`, resolving every
/// component ref to a git SHA via `git ls-remote`.
///
/// Lookup order for each `(name, ref)` pair:
///
/// 1. `git_ls_remote(<repo_url>)` returns a map of qualified refs
///    → SHAs (e.g. `refs/heads/main` → `abc1234…`).
/// 2. The function probes the qualified-ref keys in this order:
///    `refs/tags/<ref>`, `refs/heads/<ref>`, then the bare `<ref>`
///    name (operators sometimes pass a SHA prefix).
/// 3. The first hit wins; the resolved SHA lands in
///    [`VersionComponent::ref_`].
/// 4. If no qualifier matches and the ref looks like a hex SHA
///    (all `[0-9a-f]`, length ≥ 7), it's kept verbatim.
/// 5. Otherwise [`VersionError::InvalidDescriptor`] surfaces with
///    a diagnostic naming the unresolved ref.
///
/// # Errors
///
/// - [`VersionError::InvalidDescriptor`] when a component name in
///   `component_refs` has no matching `component_repos` entry, or
///   when a ref cannot be resolved.
/// - [`VersionError::InvalidDescriptor`] wrapping any [`GitError`]
///   from `git_ls_remote` (network / permission / not-found).
///
/// # Examples
///
/// ```no_run
/// use cbscore::versions::create::{version_create_helper, VersionCreateInput};
/// use cbscore_types::versions::utils::VersionType;
/// use cbscore_types::versions::desc::VersionSignedOffBy;
///
/// # async fn demo() -> Result<(), cbscore_types::versions::VersionError> {
/// let input = VersionCreateInput {
///     version: "19.2.3-dev.1".into(),
///     version_type: VersionType::Dev,
///     component_refs: vec![("ceph".into(), "main".into())],
///     component_repos: [
///         ("ceph".to_string(), "https://github.com/ceph/ceph".to_string()),
///     ]
///     .into_iter()
///     .collect(),
///     signed_off_by: VersionSignedOffBy {
///         user: "ops".into(),
///         email: "ops@example.com".into(),
///     },
///     registry: "quay.io".into(),
///     image_name: "ceph-builder".into(),
///     image_tag: None,
///     distro: "centos".into(),
///     el_version: 9,
/// };
/// let desc = version_create_helper(&input).await?;
/// assert_eq!(desc.version, "19.2.3-dev.1");
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::versions::create",
    skip(input),
    fields(version = %input.version),
)]
pub async fn version_create_helper(
    input: &VersionCreateInput,
) -> Result<VersionDescriptor, VersionError> {
    let title = make_title(&input.version, input.version_type);

    let mut components: Vec<VersionComponent> = Vec::with_capacity(input.component_refs.len());
    for (name, ref_) in &input.component_refs {
        let repo =
            input
                .component_repos
                .get(name)
                .ok_or_else(|| VersionError::InvalidDescriptor {
                    path: "<version_create_helper>".into(),
                    message: format!("component '{name}' has no matching repo URL"),
                })?;
        let resolved = resolve_ref(repo, ref_).await?;
        tracing::debug!(
            target: TARGET_VERSIONS_CREATE,
            component = %name,
            ref_ = %ref_,
            sha = %resolved,
            "resolved component ref",
        );
        components.push(VersionComponent {
            name: name.clone(),
            repo: repo.clone(),
            ref_: resolved,
        });
    }

    let image_tag = input
        .image_tag
        .clone()
        .unwrap_or_else(|| input.version.clone());
    Ok(VersionDescriptor {
        version: input.version.clone(),
        title,
        signed_off_by: input.signed_off_by.clone(),
        image: VersionImage {
            registry: input.registry.clone(),
            name: input.image_name.clone(),
            tag: image_tag,
        },
        components,
        distro: input.distro.clone(),
        el_version: input.el_version,
    })
}

/// Resolve `ref_` against `repo` via `git ls-remote`.
async fn resolve_ref(repo: &str, ref_: &str) -> Result<String, VersionError> {
    let remote_refs =
        git_ls_remote(CmdArg::from(repo))
            .await
            .map_err(|e| VersionError::InvalidDescriptor {
                path: "<version_create_helper>".into(),
                message: format!("git ls-remote {repo}: {e}"),
            })?;
    for qualified in [
        format!("refs/tags/{ref_}"),
        format!("refs/heads/{ref_}"),
        ref_.to_owned(),
    ] {
        if let Some(sha) = remote_refs.get(&qualified) {
            return Ok(sha.clone());
        }
    }
    if looks_like_sha(ref_) {
        return Ok(ref_.to_owned());
    }
    Err(VersionError::InvalidDescriptor {
        path: "<version_create_helper>".into(),
        message: format!("ref '{ref_}' did not resolve in '{repo}'"),
    })
}

/// `true` when `s` looks like a hex git SHA (>= 7 lowercase hex
/// chars).
fn looks_like_sha(s: &str) -> bool {
    s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Build the descriptor's `title` field.
///
/// Two shapes:
///
/// - Supplied VERSION (or any non-UUIDv7 input) → `"Release
///   {type_desc} version {version}"`, matching the existing
///   Python-parity format string the Rust port has emitted since
///   M1 cut.
/// - `UUIDv7` input → `"Release {type_desc} version created at
///   {iso8601}"`, where `{iso8601}` is the `UUIDv7`'s embedded
///   48-bit timestamp formatted as ISO 8601 UTC at seconds
///   precision. Per design 005 §Title, this gives operators a
///   readable creation-time stamp instead of the raw UUID in
///   `versions list` output.
///
/// Stays private and infallible; the `UUIDv7` branch falls through
/// to the existing format string on any miss (parse failure, wrong
/// UUID version, out-of-range timestamp).
fn make_title(version: &str, version_type: VersionType) -> String {
    let type_desc = match version_type {
        VersionType::Dev => "Development",
        VersionType::Test => "Test",
        VersionType::Ci => "CI",
        VersionType::Release => "Release",
    };
    if let Ok(uuid) = uuid::Uuid::parse_str(version)
        && uuid.get_version() == Some(uuid::Version::SortRand)
        && let Some(ts) = crate::versions::resolve::uuid_v7_timestamp(&uuid)
    {
        let formatted = ts.format("%Y-%m-%dT%H:%M:%SZ");
        return format!("Release {type_desc} version created at {formatted}");
    }
    format!("Release {type_desc} version {version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_sha_accepts_short_and_full_hexes() {
        assert!(looks_like_sha("abc1234"));
        assert!(looks_like_sha("abc12345678901234567890123456789012345abc"));
    }

    #[test]
    fn looks_like_sha_rejects_short_or_non_hex() {
        assert!(!looks_like_sha("main"));
        assert!(!looks_like_sha("abc"));
        assert!(!looks_like_sha("abc123g"));
        assert!(!looks_like_sha("HEAD"));
    }

    #[test]
    fn make_title_includes_type_and_version() {
        let t = make_title("19.2.3", VersionType::Dev);
        assert_eq!(t, "Release Development version 19.2.3");
        let t = make_title("19.2.3", VersionType::Release);
        assert_eq!(t, "Release Release version 19.2.3");
    }

    /// `UUIDv7` input minted at a fixed Unix timestamp produces the
    /// "created at <iso8601>" title body (design 005 §Title). The
    /// constant `1_777_895_100` = 2026-05-04T11:45:00Z UTC.
    #[test]
    fn make_title_uuidv7_emits_created_at_iso8601() {
        let ts = uuid::Timestamp::from_unix_time(1_777_895_100, 0, 0, 0);
        let uuid = uuid::Uuid::new_v7(ts);
        let title = make_title(&uuid.to_string(), VersionType::Dev);
        assert_eq!(
            title,
            "Release Development version created at 2026-05-04T11:45:00Z",
        );
    }

    /// `UUIDv4` input falls through to the passthrough format with the
    /// literal UUID string. Pins the seq-005 design choice that only
    /// `UUIDv7` triggers the created-at branch, not "any UUID".
    #[test]
    fn make_title_uuidv4_falls_through_to_passthrough() {
        let v4 = uuid::Uuid::new_v4().to_string();
        let title = make_title(&v4, VersionType::Dev);
        assert_eq!(title, format!("Release Development version {v4}"));
    }

    /// Non-UUID malformed input falls through cleanly. `Uuid::parse_str`
    /// rejects the input and the passthrough format runs.
    #[test]
    fn make_title_non_uuid_falls_through() {
        let title = make_title("foobar", VersionType::Dev);
        assert_eq!(title, "Release Development version foobar");
    }
}
