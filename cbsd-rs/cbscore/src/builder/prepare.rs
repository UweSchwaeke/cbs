// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Prepare stage — first of the four-stage builder pipeline.
//!
//! For each component declared in the version descriptor:
//!
//! 1. Set up the per-component scratch dir under
//!    `config.paths.scratch/<component>` (clearing it first when
//!    `opts.force` is true).
//! 2. Clone the component's git repo into the scratch dir (or
//!    `git fetch` if it already exists).
//! 3. Switch to the descriptor's `ref_` and capture the resolved
//!    SHA into the per-component [`BuildComponentInfo`].
//! 4. Walk `components/<component>/patches/` and select the patch
//!    set that applies to the descriptor's `version` field per
//!    design 002 §Effects of `UUIDv7` VERSIONs §Patches and design
//!    005.
//!
//! Patch application itself (driving `git apply`) is deferred to
//! a follow-up commit that extends [`crate::utils::git`] with
//! `git_apply`; this commit lands the patch *walker* and the
//! source-fetch / SHA-capture flow.

use std::collections::HashMap;

use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::builder::BuilderError;
use cbscore_types::config::Config;
use cbscore_types::versions::VersionDescriptor;

use super::utils::{component_scratch_dir, ensure_dir, remove_dir_all_if_present};
use crate::secrets::SecretsMgr;
use crate::utils::git::{GitError, git_clone, git_fetch, git_rev_parse, git_switch};
use crate::utils::subprocess::CmdArg;
use crate::versions::utils::{get_major_version, get_minor_version};

const TARGET_BUILDER_PREPARE: &str = "cbscore::builder::prepare";

// ---------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------

/// Per-component preparation result — what downstream stages
/// (`rpmbuild`, `signing`, `upload`) need from the prepare step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildComponentInfo {
    /// Operator-chosen component name (matches the descriptor).
    pub name: String,
    /// Local scratch path the source was fetched into.
    pub repo_path: Utf8PathBuf,
    /// Upstream repo URL from the descriptor.
    pub repo_url: String,
    /// Descriptor-provided ref (branch / tag / SHA prefix).
    pub base_ref: String,
    /// Resolved SHA at `base_ref` after `git switch`.
    pub sha1: String,
    /// Patches selected by the walker, in apply order
    /// (highest-priority first per Python semantics).
    pub patches: Vec<Utf8PathBuf>,
}

/// Output of [`run`] — the per-component info map plus the
/// patch-walker results, keyed by component name.
#[derive(Debug, Clone, Default)]
pub struct PrepareReport {
    /// Map from component name to its [`BuildComponentInfo`].
    pub components: HashMap<String, BuildComponentInfo>,
}

// ---------------------------------------------------------------------
// Stage entry point
// ---------------------------------------------------------------------

/// Run the prepare stage against every component declared in
/// `desc.components`. Returns a [`PrepareReport`] carrying the
/// per-component info downstream stages consume.
///
/// `secrets` threads through to the underlying [`crate::utils::git`]
/// calls so private-repo source fetches can resolve their
/// `GitCreds` entry by host. M1 deployments with public-only repos
/// pass the manager through but never look anything up in it; the
/// param is present so the orchestrator's `prepare::run(...)` line
/// matches the other stages' signatures 1:1.
///
/// # Errors
///
/// Returns [`BuilderError::Io`] on scratch-dir setup failure (create
/// or remove), [`BuilderError::Other`] wrapping any underlying
/// [`GitError`] from clone / fetch / switch / rev-parse.
///
/// # Examples
///
/// ```no_run
/// use cbscore::builder::{prepare, BuildOptions};
/// use cbscore::secrets::SecretsMgr;
/// use cbscore_types::config::Config;
/// use cbscore_types::versions::VersionDescriptor;
///
/// # async fn demo(
/// #     desc: &VersionDescriptor,
/// #     cfg: &Config,
/// #     secrets: &SecretsMgr,
/// # ) -> Result<(), cbscore_types::builder::BuilderError> {
/// let report = prepare::run(desc, cfg, secrets, &BuildOptions::default()).await?;
/// for (name, info) in &report.components {
///     println!("{name}: {} @ {}", info.repo_url, info.sha1);
/// }
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::builder::prepare",
    skip(desc, config, secrets),
    fields(version = %desc.version),
)]
pub async fn run(
    desc: &VersionDescriptor,
    config: &Config,
    secrets: &SecretsMgr,
    opts: &super::BuildOptions,
) -> Result<PrepareReport, BuilderError> {
    let _ = secrets; // M1: public-only repos; param threaded for parity.
    let mut report = PrepareReport::default();
    for comp in &desc.components {
        let info = prepare_component(comp, desc, config, opts).await?;
        report.components.insert(comp.name.clone(), info);
    }
    tracing::debug!(
        target: TARGET_BUILDER_PREPARE,
        components = report.components.len(),
        "prepare stage complete",
    );
    Ok(report)
}

async fn prepare_component(
    comp: &cbscore_types::versions::desc::VersionComponent,
    desc: &VersionDescriptor,
    config: &Config,
    opts: &super::BuildOptions,
) -> Result<BuildComponentInfo, BuilderError> {
    let scratch = component_scratch_dir(&config.paths.scratch, &comp.name);

    if opts.force {
        tracing::debug!(
            target: TARGET_BUILDER_PREPARE,
            component = %comp.name,
            path = %scratch,
            "clearing scratch dir (force = true)",
        );
        remove_dir_all_if_present(&scratch).await?;
    }
    ensure_dir(&scratch).await?;

    // Clone (or fetch if a `.git` already exists). The clone helper
    // expects the destination dir to not exist, so we treat "scratch
    // path missing a .git subdir" as the "fresh clone" path and
    // "scratch path with .git" as the "fetch existing" path.
    let dot_git = scratch.join(".git");
    if dot_git.exists() {
        git_fetch(&scratch)
            .await
            .map_err(|e| git_wrap(&comp.name, &e))?;
    } else {
        // git_clone clones INTO the destination, so dest must be
        // either absent or empty. The ensure_dir above creates an
        // empty dir — that's fine for git clone since git's CLI is
        // happy to clone into an existing empty dir.
        git_clone(CmdArg::from(comp.repo.as_str()), &scratch)
            .await
            .map_err(|e| git_wrap(&comp.name, &e))?;
    }

    git_switch(&scratch, &comp.ref_, true)
        .await
        .map_err(|e| git_wrap(&comp.name, &e))?;

    let sha1 = git_rev_parse(&scratch, "HEAD")
        .await
        .map_err(|e| git_wrap(&comp.name, &e))?;

    let patches = patch_list_for_component(&config.paths.components, &comp.name, &desc.version);

    Ok(BuildComponentInfo {
        name: comp.name.clone(),
        repo_path: scratch,
        repo_url: comp.repo.clone(),
        base_ref: comp.ref_.clone(),
        sha1,
        patches,
    })
}

fn git_wrap(component: &str, err: &GitError) -> BuilderError {
    BuilderError::Other(format!("component '{component}': {err}"))
}

// ---------------------------------------------------------------------
// Patch walker
// ---------------------------------------------------------------------

/// Search each `config.paths.components` root for
/// `<root>/<component>/patches/`, walk the first match, and return
/// the ordered patch list per [`get_patch_list`].
///
/// Returns an empty `Vec` when no root contains a patches directory
/// for `component` — components without patches are valid.
fn patch_list_for_component(
    component_roots: &[Utf8PathBuf],
    component: &str,
    version: &str,
) -> Vec<Utf8PathBuf> {
    for root in component_roots {
        let patches_path = root.join(component).join("patches");
        if patches_path.exists() {
            return get_patch_list(&patches_path, version);
        }
    }
    Vec::new()
}

/// Walk `patches_path` and return the ordered list of patches that
/// apply to `version`, per the layered-priority scheme from Python
/// `cbscore.builder.prepare._get_patch_list`.
///
/// Top-level `*.patch` files always apply (priority 0). Each
/// subdirectory whose name exactly matches `version`, `version`'s
/// `major.minor.patch`, or `version`'s `major.minor` adds priority
/// 1; further-nested subdirectories matching the same rule add
/// priority 2, etc. Within each priority, files are sorted by the
/// leading `\d+-` prefix on their filename. The final order
/// concatenates priorities from highest to lowest — so a
/// `19.2.3/0001-foo.patch` applies before a top-level
/// `0001-foo.patch`.
///
/// When `version` is a UUIDv7-style string that
/// [`crate::versions::utils::parse_version`] cannot parse, the
/// major/minor lookups return `Err` and the walker logs a warn and
/// skips the version-keyed subdirs — only top-level patches apply.
/// Per design 005's spec; matches the malformed-version skip path
/// of the Python source.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore::builder::prepare::get_patch_list;
///
/// // Pointing at an empty / nonexistent patches dir is safe: empty Vec.
/// let v = get_patch_list(Utf8Path::new("/tmp/nonexistent-patches"), "ces-v19.2.3");
/// assert!(v.is_empty());
/// ```
#[must_use]
pub fn get_patch_list(patches_path: &Utf8Path, version: &str) -> Vec<Utf8PathBuf> {
    let mut by_prio: std::collections::BTreeMap<usize, Vec<(u64, Utf8PathBuf)>> =
        std::collections::BTreeMap::new();
    let major = get_major_version(version).ok();
    let minor = get_minor_version(version).ok().flatten();
    if major.is_none() {
        tracing::warn!(
            target: TARGET_BUILDER_PREPARE,
            version,
            "version-keyed patch subdirs skipped — VERSION does not parse \
             as `[prefix-]vM.m[.p][-suffix]`; only top-level patches apply",
        );
    }
    walk_patches(
        patches_path,
        0,
        version,
        major.as_deref(),
        minor.as_deref(),
        &mut by_prio,
    );
    let mut out = Vec::new();
    for (_, mut patches) in by_prio.into_iter().rev() {
        patches.sort_by_key(|(prio, _)| *prio);
        for (_, p) in patches {
            out.push(p);
        }
    }
    out
}

fn walk_patches(
    path: &Utf8Path,
    prio: usize,
    version: &str,
    major: Option<&str>,
    minor: Option<&str>,
    out: &mut std::collections::BTreeMap<usize, Vec<(u64, Utf8PathBuf)>>,
) {
    if prio > 0 {
        let Some(name) = path.file_name() else {
            return;
        };
        let matches =
            name == version || major.is_some_and(|m| m == name) || minor.is_some_and(|m| m == name);
        if !matches {
            return;
        }
    }
    let Ok(entries) = std::fs::read_dir(path.as_std_path()) else {
        return;
    };
    let mut local: Vec<(u64, Utf8PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        let raw_name = entry.file_name();
        let Some(name) = raw_name.to_str() else {
            continue;
        };
        if ft.is_dir() {
            let Ok(sub) = Utf8PathBuf::from_path_buf(entry.path()) else {
                continue;
            };
            walk_patches(&sub, prio + 1, version, major, minor, out);
            continue;
        }
        if !std::path::Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("patch"))
        {
            continue;
        }
        let Some(prefix) = patch_prefix(name) else {
            tracing::warn!(
                target: TARGET_BUILDER_PREPARE,
                file = name,
                "patch filename malformed (expected `\\d+-...patch`); skipping",
            );
            continue;
        };
        let Ok(p) = Utf8PathBuf::from_path_buf(entry.path()) else {
            continue;
        };
        local.push((prefix, p));
    }
    if !local.is_empty() {
        out.entry(prio).or_default().extend(local);
    }
}

/// Parse the leading `\d+` from a patch filename.
fn patch_prefix(name: &str) -> Option<u64> {
    let end = name.find(|c: char| !c.is_ascii_digit())?;
    if end == 0 {
        return None;
    }
    name[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(path: &Utf8Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent.as_std_path()).expect("create parent");
        }
        fs::write(path.as_std_path(), b"--- patch stub\n").expect("write");
    }

    #[test]
    fn patch_prefix_basic() {
        assert_eq!(patch_prefix("0001-foo.patch"), Some(1));
        assert_eq!(patch_prefix("42-bar.patch"), Some(42));
        assert_eq!(patch_prefix("no-leading-digit.patch"), None);
    }

    #[test]
    fn walker_top_level_only() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        touch(&root.join("0001-a.patch"));
        touch(&root.join("0002-b.patch"));
        // A subdir whose name doesn't match → ignored.
        touch(&root.join("unrelated/0001-x.patch"));

        let patches = get_patch_list(&root, "ces-v19.2.3");
        let names: Vec<_> = patches
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(names, vec!["0001-a.patch", "0002-b.patch"]);
    }

    #[test]
    fn walker_priority_order() {
        // Priority comes from nesting depth, not from which alias
        // (full version / major.minor.patch / major.minor) matched.
        // Operators express "more-specific-overrides-general" by
        // nesting subdirs: patches/19.2/19.2.3/* is two levels deep,
        // so it has higher priority than patches/19.2/* which is one.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        touch(&root.join("0001-top.patch"));
        touch(&root.join("19.2/0001-major.patch"));
        touch(&root.join("19.2/19.2.3/0001-minor.patch"));

        let patches = get_patch_list(&root, "ces-v19.2.3");
        let names: Vec<_> = patches
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        // Highest priority first: prio 2 (nested minor) → prio 1
        // (top-level major) → prio 0 (top-level).
        assert_eq!(
            names,
            vec!["0001-minor.patch", "0001-major.patch", "0001-top.patch"],
        );
    }

    #[test]
    fn walker_within_priority_sorted_by_prefix() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        touch(&root.join("0010-late.patch"));
        touch(&root.join("0001-early.patch"));
        touch(&root.join("0005-mid.patch"));

        let patches = get_patch_list(&root, "ces-v19.2.3");
        let names: Vec<_> = patches
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(
            names,
            vec!["0001-early.patch", "0005-mid.patch", "0010-late.patch"],
        );
    }

    #[test]
    fn walker_uuid_v7_only_top_level() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        touch(&root.join("0001-top.patch"));
        // 19.2 / 19.2.3 subdirs exist but VERSION is a UUID — no parse,
        // so they should not be entered.
        touch(&root.join("19.2/0001-major.patch"));
        touch(&root.join("19.2.3/0001-minor.patch"));

        let patches = get_patch_list(&root, "0193e1a8-7c2e-7000-b1c0-9f8c45d77ed4");
        let names: Vec<_> = patches
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(names, vec!["0001-top.patch"]);
    }

    #[test]
    fn walker_exact_match_priority_subdir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        let exact = "ces-v19.2.3-dev.1";
        touch(&root.join(format!("{exact}/0001-exact.patch")));
        touch(&root.join("0002-top.patch"));

        let patches = get_patch_list(&root, exact);
        let names: Vec<_> = patches
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        // Exact-match subdir runs at prio 1, top-level at prio 0.
        assert_eq!(names, vec!["0001-exact.patch", "0002-top.patch"]);
    }

    #[test]
    fn walker_malformed_patch_name_skipped() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        touch(&root.join("0001-good.patch"));
        // No leading digit — walker should warn and skip.
        touch(&root.join("hello.patch"));

        let patches = get_patch_list(&root, "ces-v19.2.3");
        let names: Vec<_> = patches
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(names, vec!["0001-good.patch"]);
    }
}
