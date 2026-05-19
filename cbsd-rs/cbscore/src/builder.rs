// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Builder pipeline — the four-stage in-container build workflow.
//!
//! Lands incrementally across Phase 5 of the seq-002 plan. Phase 5
//! Commit 1 lands [`prepare`] (sources + repo resolution) plus this
//! module entry and the public [`BuildOptions`] struct; subsequent
//! commits add `rpmbuild`, `signing`, `upload`, `report`, and the
//! [`run_build`] orchestrator.

pub mod prepare;
pub mod rpmbuild;
pub mod utils;

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
