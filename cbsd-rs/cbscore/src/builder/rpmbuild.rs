// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `rpmbuild` stage — second of the four-stage builder pipeline.
//!
//! For each component declared in the descriptor's `components`
//! list, this stage:
//!
//! 1. Sets up the per-component RPM topdir
//!    (`config.paths.scratch/rpms/<component>/<version>/{BUILD,
//!    SOURCES, RPMS, SRPMS, SPECS}`).
//! 2. When `opts.skip_build` is `false`, locates the component's
//!    `build_rpms.sh` driver script (via the same multi-root
//!    convention `prepare::patch_list_for_component` uses —
//!    `components/<name>/build_rpms.sh`) and runs it with
//!    `(repo_path, el_version, rpms_topdir, version)`.
//! 3. Walks the topdir's `RPMS` and `SRPMS` subdirs to collect the
//!    produced `.rpm` artefacts into [`RpmArtifact`] records.
//!
//! The actual `rpmbuild` CLI invocation lives inside the driver
//! script (matching Python `cbscore.builder.rpmbuild`); this stage
//! is the orchestration around it.
//!
//! `opts.skip_build = true` short-circuits the script invocation
//! while still setting up the topdir and returning an empty (but
//! well-formed) [`RpmbuildReport`]. Downstream stages see "no RPMs
//! produced" and become no-ops accordingly.

use std::collections::HashMap;

use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::builder::BuilderError;
use cbscore_types::config::Config;
use cbscore_types::releases::desc::ArchType;
use cbscore_types::versions::VersionDescriptor;
use tracing::{debug, info};

use super::prepare::{BuildComponentInfo, PrepareReport};
use super::utils::{ensure_dir, io_err};
use crate::utils::subprocess::{CmdArg, RunOpts, async_run_cmd};

const TARGET_BUILDER_RPMBUILD: &str = "cbscore::builder::rpmbuild";

/// Filename the rpmbuild stage looks for under each component
/// directory to drive the per-component build.
const BUILD_SCRIPT_NAME: &str = "build_rpms.sh";

/// RPM topdir sub-directories per
/// [Maximum RPM § rpmbuild topdir layout](https://rpm-software-management.github.io/rpm/manual/buildprocess.html).
const TOPDIR_SUBDIRS: &[&str] = &["BUILD", "SOURCES", "RPMS", "SRPMS", "SPECS"];

// ---------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------

/// A single RPM produced by the rpmbuild stage. Carries the
/// minimum metadata downstream stages (signing in Commit 5, upload
/// in Commit 6) need to act on the file without re-parsing the
/// path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpmArtifact {
    /// Absolute path to the `.rpm` file in the per-component
    /// topdir.
    pub path: Utf8PathBuf,
    /// Component name (matches the descriptor's
    /// `VersionComponent.name`).
    pub component: String,
    /// CPU architecture (always `X86_64` for M1 per
    /// [`ArchType`]).
    pub arch: ArchType,
    /// `true` for source RPMs (under `SRPMS/`), `false` for
    /// binary RPMs (under `RPMS/<arch>/`).
    pub is_srpm: bool,
}

/// Per-component build summary — wall-clock time + topdir path —
/// for the eventual `BuildArtifactReport` assembly in Phase 5
/// Commit 7. Patterns the Python `ComponentBuild` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentBuild {
    /// Component name.
    pub component: String,
    /// Per-component RPM topdir (the `_topdir` rpmbuild sees).
    pub rpms_path: Utf8PathBuf,
    /// Resolved version string (the `BuildComponentInfo.sha1` /
    /// long-version string Python tracks as
    /// `ComponentBuild.version`).
    pub version: String,
}

/// Output of [`run`]: every produced RPM plus the per-component
/// summaries.
#[derive(Debug, Clone, Default)]
pub struct RpmbuildReport {
    /// Flat list of every `.rpm` produced across all components.
    /// Self-describing — downstream stages consume this slice
    /// directly without re-parsing paths.
    pub rpms: Vec<RpmArtifact>,
    /// Per-component summaries, keyed by component name.
    pub component_builds: HashMap<String, ComponentBuild>,
}

// ---------------------------------------------------------------------
// Stage entry point
// ---------------------------------------------------------------------

/// Run the rpmbuild stage. Iterates `prep.components` in
/// dependency order from the descriptor and builds each.
///
/// # Errors
///
/// - [`BuilderError::Io`] on RPM topdir create / RPMS-walk failure.
/// - [`BuilderError::MissingScript`] when the per-component
///   `build_rpms.sh` driver is missing on a non-skip_build run.
/// - [`BuilderError::Other`] wrapping subprocess failures (driver
///   script exited non-zero, or `async_run_cmd` failed to spawn).
///
/// # Examples
///
/// ```no_run
/// use cbscore::builder::{rpmbuild, prepare, BuildOptions};
/// use cbscore_types::config::Config;
/// use cbscore_types::versions::VersionDescriptor;
///
/// # async fn demo(
/// #     desc: &VersionDescriptor,
/// #     cfg: &Config,
/// #     prep: &prepare::PrepareReport,
/// # ) -> Result<(), cbscore_types::builder::BuilderError> {
/// let report = rpmbuild::run(desc, cfg, prep, &BuildOptions::default()).await?;
/// println!("{} RPMs", report.rpms.len());
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::builder::rpmbuild",
    skip(desc, config, prep),
    fields(version = %desc.version, skip_build = opts.skip_build),
)]
pub async fn run(
    desc: &VersionDescriptor,
    config: &Config,
    prep: &PrepareReport,
    opts: &super::BuildOptions,
) -> Result<RpmbuildReport, BuilderError> {
    let mut report = RpmbuildReport::default();
    for comp in &desc.components {
        let Some(info) = prep.components.get(&comp.name) else {
            return Err(BuilderError::Other(format!(
                "rpmbuild: component '{}' missing from prepare report",
                comp.name,
            )));
        };
        let build = build_component(info, &desc.version, config, opts).await?;
        if !opts.skip_build {
            let collected = collect_rpms(&build.rpms_path, &comp.name)?;
            report.rpms.extend(collected);
        }
        report.component_builds.insert(comp.name.clone(), build);
    }
    info!(
        target: TARGET_BUILDER_RPMBUILD,
        components = report.component_builds.len(),
        rpms = report.rpms.len(),
        "rpmbuild stage complete",
    );
    Ok(report)
}

// ---------------------------------------------------------------------
// Per-component build
// ---------------------------------------------------------------------

async fn build_component(
    info: &BuildComponentInfo,
    version: &str,
    config: &Config,
    opts: &super::BuildOptions,
) -> Result<ComponentBuild, BuilderError> {
    let topdir = rpm_topdir(&config.paths.scratch, &info.name, version);
    setup_topdir(&topdir).await?;

    if opts.skip_build {
        debug!(
            target: TARGET_BUILDER_RPMBUILD,
            component = %info.name,
            "skip_build set, skipping build_rpms.sh invocation",
        );
        return Ok(ComponentBuild {
            component: info.name.clone(),
            rpms_path: topdir,
            version: version.to_owned(),
        });
    }

    let script = locate_build_script(&config.paths.components, &info.name)?;
    let argv = vec![
        CmdArg::from(script.as_str()),
        CmdArg::from(info.repo_path.as_str()),
        CmdArg::Plain(rpm_el_version_arg(version)),
        CmdArg::from(topdir.as_str()),
        CmdArg::from(version),
    ];
    info!(
        target: TARGET_BUILDER_RPMBUILD,
        component = %info.name,
        script = %script,
        "running build_rpms.sh",
    );
    let outcome = async_run_cmd(&argv, RunOpts::default())
        .await
        .map_err(|e| BuilderError::Other(format!("component '{}': {e}", info.name)))?;
    if outcome.rc != 0 {
        return Err(BuilderError::Other(format!(
            "component '{}': build_rpms.sh exited with code {} ({})",
            info.name,
            outcome.rc,
            outcome.stderr.trim(),
        )));
    }
    Ok(ComponentBuild {
        component: info.name.clone(),
        rpms_path: topdir,
        version: version.to_owned(),
    })
}

/// EL major version arg passed to `build_rpms.sh`. Phase 5 keeps
/// the value pinned to `9` — the M1 builder image targets el9. Per
/// design 002 the descriptor's `el_version` field is the source of
/// truth, but `desc.el_version` is not threaded through this commit
/// so the placeholder lives here until the orchestrator (Commit 7)
/// wires it.
fn rpm_el_version_arg(_version: &str) -> String {
    // TODO: thread `desc.el_version` through prep::Run -> rpmbuild
    // and read it here. Until then this is a fixed placeholder.
    "9".to_owned()
}

/// Derive the per-component RPM topdir as
/// `<scratch>/rpms/<component>/<version>`.
#[must_use]
pub fn rpm_topdir(scratch_root: &Utf8Path, component: &str, version: &str) -> Utf8PathBuf {
    scratch_root.join("rpms").join(component).join(version)
}

async fn setup_topdir(topdir: &Utf8Path) -> Result<(), BuilderError> {
    ensure_dir(topdir).await?;
    for sub in TOPDIR_SUBDIRS {
        ensure_dir(&topdir.join(sub)).await?;
    }
    Ok(())
}

/// Search each `config.paths.components` root for
/// `<root>/<component>/build_rpms.sh` and return the first match.
fn locate_build_script(
    component_roots: &[Utf8PathBuf],
    component: &str,
) -> Result<Utf8PathBuf, BuilderError> {
    for root in component_roots {
        let script = root.join(component).join(BUILD_SCRIPT_NAME);
        if script.exists() {
            return Ok(script);
        }
    }
    Err(BuilderError::MissingScript {
        path: component_roots.first().map_or_else(
            || {
                Utf8PathBuf::from(format!(
                    "(no components root) / {component} / {BUILD_SCRIPT_NAME}"
                ))
            },
            |r| r.join(component).join(BUILD_SCRIPT_NAME),
        ),
    })
}

// ---------------------------------------------------------------------
// RPMs collection
// ---------------------------------------------------------------------

fn collect_rpms(topdir: &Utf8Path, component: &str) -> Result<Vec<RpmArtifact>, BuilderError> {
    let mut out = Vec::new();
    collect_from(&topdir.join("RPMS"), component, false, &mut out)?;
    collect_from(&topdir.join("SRPMS"), component, true, &mut out)?;
    Ok(out)
}

fn collect_from(
    dir: &Utf8Path,
    component: &str,
    is_srpm: bool,
    out: &mut Vec<RpmArtifact>,
) -> Result<(), BuilderError> {
    let entries = match std::fs::read_dir(dir.as_std_path()) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io_err(dir, e)),
    };
    for entry in entries {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        let file_type = entry.file_type().map_err(|e| io_err(dir, e))?;
        if file_type.is_dir() {
            let Ok(sub) = Utf8PathBuf::from_path_buf(entry.path()) else {
                continue;
            };
            collect_from(&sub, component, is_srpm, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let raw_name = entry.file_name();
        let Some(name) = raw_name.to_str() else {
            continue;
        };
        if !std::path::Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rpm"))
        {
            continue;
        }
        let Ok(path) = Utf8PathBuf::from_path_buf(entry.path()) else {
            continue;
        };
        out.push(RpmArtifact {
            path,
            component: component.to_owned(),
            arch: ArchType::X86_64,
            is_srpm,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Utf8Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent.as_std_path()).expect("create parent");
        }
        fs::write(path.as_std_path(), contents).expect("write");
    }

    #[test]
    fn rpm_topdir_joins_correctly() {
        let p = rpm_topdir(Utf8Path::new("/srv/scratch"), "ceph", "19.2.3");
        assert_eq!(p.as_str(), "/srv/scratch/rpms/ceph/19.2.3");
    }

    #[tokio::test]
    async fn setup_topdir_creates_layout() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let topdir = Utf8PathBuf::from_path_buf(tmp.path().join("topdir")).expect("utf8 path");
        setup_topdir(&topdir).await.expect("setup");
        for sub in TOPDIR_SUBDIRS {
            assert!(topdir.join(sub).is_dir(), "expected subdir {sub}");
        }
    }

    #[test]
    fn locate_build_script_finds_in_first_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        let script = root.join("ceph").join(BUILD_SCRIPT_NAME);
        write(&script, "#!/bin/sh\n");
        let resolved =
            locate_build_script(std::slice::from_ref(&root), "ceph").expect("found script");
        assert_eq!(resolved, script);
    }

    #[test]
    fn locate_build_script_missing_yields_missing_script_err() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        let Err(err) = locate_build_script(std::slice::from_ref(&root), "absent") else {
            panic!("expected MissingScript, got Ok");
        };
        assert!(matches!(err, BuilderError::MissingScript { .. }));
    }

    #[test]
    fn collect_rpms_picks_binary_and_srpms() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let topdir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        write(
            &topdir
                .join("RPMS")
                .join("x86_64")
                .join("ceph-1.0-1.el9.x86_64.rpm"),
            "",
        );
        write(
            &topdir
                .join("RPMS")
                .join("noarch")
                .join("ceph-common-1.0-1.el9.noarch.rpm"),
            "",
        );
        write(&topdir.join("SRPMS").join("ceph-1.0-1.el9.src.rpm"), "");

        let rpms = collect_rpms(&topdir, "ceph").expect("collect");
        assert_eq!(rpms.len(), 3);
        let srpm_count = rpms.iter().filter(|r| r.is_srpm).count();
        assert_eq!(srpm_count, 1);
        let binary_count = rpms.iter().filter(|r| !r.is_srpm).count();
        assert_eq!(binary_count, 2);
        for r in &rpms {
            assert_eq!(r.component, "ceph");
            assert_eq!(r.arch, ArchType::X86_64);
        }
    }

    #[test]
    fn collect_rpms_handles_missing_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let topdir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        // No RPMS or SRPMS subdirs created.
        let rpms = collect_rpms(&topdir, "ceph").expect("collect");
        assert!(rpms.is_empty());
    }

    #[tokio::test]
    async fn run_skip_build_short_circuits() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let scratch = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        // Build a minimal Config + descriptor + PrepareReport.
        let cfg = Config {
            paths: cbscore_types::config::PathsConfig {
                components: vec![scratch.join("components")],
                scratch: scratch.clone(),
                scratch_containers: scratch.join("scratch-containers"),
                ccache: None,
            },
            storage: None,
            signing: None,
            logging: None,
            secrets: Vec::new(),
            vault: None,
        };
        let desc = VersionDescriptor {
            version: "19.2.3".into(),
            title: "t".into(),
            signed_off_by: cbscore_types::versions::desc::VersionSignedOffBy {
                user: "u".into(),
                email: "e".into(),
            },
            image: cbscore_types::versions::desc::VersionImage {
                registry: "r".into(),
                name: "n".into(),
                tag: "t".into(),
            },
            components: vec![cbscore_types::versions::desc::VersionComponent {
                name: "ceph".into(),
                repo: "https://example.com/ceph.git".into(),
                ref_: "v19.2.3".into(),
            }],
            distro: "centos".into(),
            el_version: 9,
        };
        let mut prep = PrepareReport::default();
        prep.components.insert(
            "ceph".into(),
            BuildComponentInfo {
                name: "ceph".into(),
                repo_path: scratch.join("repos").join("ceph"),
                repo_url: "https://example.com/ceph.git".into(),
                base_ref: "v19.2.3".into(),
                sha1: "deadbeef".into(),
                patches: Vec::new(),
            },
        );
        let opts = super::super::BuildOptions {
            skip_build: true,
            force: false,
        };
        let report = run(&desc, &cfg, &prep, &opts)
            .await
            .expect("skip-build run");
        assert!(report.rpms.is_empty());
        assert_eq!(report.component_builds.len(), 1);
        assert_eq!(
            report.component_builds.get("ceph").unwrap().rpms_path,
            rpm_topdir(&scratch, "ceph", "19.2.3"),
        );
        // Topdir should still have been set up even on skip.
        for sub in TOPDIR_SUBDIRS {
            assert!(rpm_topdir(&scratch, "ceph", "19.2.3").join(sub).is_dir());
        }
    }

    #[tokio::test]
    async fn run_missing_component_in_prep_errors() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let scratch = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        let cfg = Config {
            paths: cbscore_types::config::PathsConfig {
                components: vec![scratch.join("components")],
                scratch: scratch.clone(),
                scratch_containers: scratch.join("scratch-containers"),
                ccache: None,
            },
            storage: None,
            signing: None,
            logging: None,
            secrets: Vec::new(),
            vault: None,
        };
        let desc = VersionDescriptor {
            version: "19.2.3".into(),
            title: "t".into(),
            signed_off_by: cbscore_types::versions::desc::VersionSignedOffBy {
                user: "u".into(),
                email: "e".into(),
            },
            image: cbscore_types::versions::desc::VersionImage {
                registry: "r".into(),
                name: "n".into(),
                tag: "t".into(),
            },
            components: vec![cbscore_types::versions::desc::VersionComponent {
                name: "absent".into(),
                repo: "https://example.com/absent.git".into(),
                ref_: "main".into(),
            }],
            distro: "centos".into(),
            el_version: 9,
        };
        let prep = PrepareReport::default();
        let opts = super::super::BuildOptions::default();
        let Err(err) = run(&desc, &cfg, &prep, &opts).await else {
            panic!("expected error, got Ok");
        };
        assert!(matches!(err, BuilderError::Other(_)));
    }
}
