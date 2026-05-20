// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Builder pipeline — the four-stage in-container build workflow.
//!
//! Phase 5 Commit 7 closes the M1.4 milestone with [`run_build`]
//! chaining prepare → rpmbuild → `containers::build` → signing →
//! upload into a single async entry point that the Phase 6
//! `cbsbuild runner build` CLI invokes inside the runner
//! container.
//!
//! Per-stage modules:
//!
//! - [`prepare`] — sources + repo resolution + patch walker
//!   (Commit 1).
//! - [`rpmbuild`] — per-component RPM builds (Commit 3).
//! - [`signing`] — RPM GPG signing (Commit 5).
//! - [`upload`] — S3 publish + image push (Commit 6).
//! - [`report`] — [`BuildArtifactReport`] assembly (this commit).

pub mod prepare;
pub mod report;
pub mod rpmbuild;
pub mod signing;
pub mod upload;
pub mod utils;

use cbscore_types::builder::BuilderError;
use cbscore_types::builder::report::BuildArtifactReport;
use cbscore_types::config::Config;
use cbscore_types::versions::VersionDescriptor;

use crate::containers::build::build_image;
use crate::secrets::SecretsMgr;

const TARGET_BUILDER: &str = "cbscore::builder";

/// Caller-supplied options for [`run_build`](self::run_build) and
/// each stage's `run` entry point.
///
/// `skip_build` short-circuits the `rpmbuild` stage and propagates
/// through `signing` + `upload` so the pipeline still produces a
/// well-formed (empty) artifact report. Operators set this when they
/// want to exercise the prepare stage in isolation (e.g. to validate
/// component refs without paying the rpmbuild cost).
///
/// `force` tells `prepare` to clear `config.paths.scratch/<component>`
/// before fetching sources, so a re-run starts from a clean tree.
///
/// # Examples
///
/// ```
/// use cbscore::builder::BuildOptions;
///
/// let opts = BuildOptions::default();
/// assert!(!opts.skip_build);
/// assert!(!opts.force);
/// ```
#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    /// Skip the in-container rpmbuild step and propagate the
    /// no-op signal through downstream stages.
    pub skip_build: bool,
    /// Clear each component's scratch dir before fetching sources.
    pub force: bool,
}

/// Run the full builder pipeline against `desc`.
///
/// Stages chain in strict order per design 002 §Build Pipeline:
///
/// 1. [`prepare::run`] — sources + repo resolution.
/// 2. [`rpmbuild::run`] — per-component RPM builds.
/// 3. [`crate::containers::build::build_image`] — container image
///    assembly via buildah.
/// 4. [`signing::run`] — RPM GPG signing (optional; no-op when
///    `config.signing.gpg` is `None`).
/// 5. [`upload::run`] — S3 publish + image push (optional; no-op
///    when `config.storage` is `None`).
///
/// On any stage error the chain short-circuits and returns
/// `Err(BuilderError)` immediately. Per design 002, the per-component
/// scratch dir is **left in place** — operators inspect the
/// scratch contents to debug failures, and re-run with
/// `opts.force = true` to start clean.
///
/// On success the assembled [`BuildArtifactReport`] carries every
/// per-stage output projected into the wire-format shape Phase 4's
/// runner reads from `/runner/<name>.report.json`.
///
/// # Errors
///
/// Returns the first [`BuilderError`] from any stage in the chain.
///
/// # Examples
///
/// ```no_run
/// use cbscore::builder::{run_build, BuildOptions};
/// use cbscore::secrets::SecretsMgr;
/// use cbscore_types::config::Config;
/// use cbscore_types::versions::VersionDescriptor;
///
/// # async fn demo(
/// #     desc: &VersionDescriptor,
/// #     cfg: &Config,
/// #     secrets: &SecretsMgr,
/// # ) -> Result<(), cbscore_types::builder::BuilderError> {
/// let report = run_build(desc, cfg, secrets, &BuildOptions::default()).await?;
/// println!("built {} ({} components)", report.version, report.components.len());
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::builder",
    skip(desc, config, secrets, opts),
    fields(version = %desc.version, skip_build = opts.skip_build, force = opts.force),
)]
pub async fn run_build(
    desc: &VersionDescriptor,
    config: &Config,
    secrets: &SecretsMgr,
    opts: &BuildOptions,
) -> Result<BuildArtifactReport, BuilderError> {
    tracing::info!(
        target: TARGET_BUILDER,
        version = %desc.version,
        "run_build: starting pipeline",
    );

    let prep = prepare::run(desc, config, secrets, opts).await?;
    let rpms = rpmbuild::run(desc, config, &prep, opts).await?;
    let image = build_image(desc)
        .await
        .map_err(|e| BuilderError::Other(format!("containers::build: {e}")))?;
    let signed = signing::run(desc, config, secrets, &rpms).await?;
    let uploaded = upload::run(desc, config, secrets, &rpms, &signed, &image).await?;

    let report = report::assemble(desc, config, &prep, &rpms, &image, &signed, &uploaded);
    tracing::info!(
        target: TARGET_BUILDER,
        version = %desc.version,
        components = report.components.len(),
        skipped = report.skipped,
        "run_build: pipeline complete",
    );
    Ok(report)
}
