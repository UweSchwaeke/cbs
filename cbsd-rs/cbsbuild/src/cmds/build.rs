// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild build <descriptor.json>` — runs a containerized build.
//!
//! Thin alias for [`runner run`](super::runner::RunnerCommand::Run)
//! per design 002 line 1224. Both end up calling Phase 4's
//! `cbscore::runner::run`. Implementation lands in Phase 6
//! Commit 3.

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Args;

/// `cbsbuild build` argument set.
#[derive(Debug, Args)]
pub struct BuildArgs {
    /// Path to the version-descriptor JSON.
    pub descriptor: Utf8PathBuf,

    /// Skip the in-container rpmbuild step. Propagates to
    /// `BuildOptions.skip_build`.
    #[arg(long = "skip-build")]
    pub skip_build: bool,

    /// Clear each component's scratch dir before fetching sources.
    /// Propagates to `BuildOptions.force`.
    #[arg(long = "force")]
    pub force: bool,

    /// `true` (default) verifies TLS for registry pushes; pair
    /// `--no-tls-verify` disables it.
    #[arg(long = "tls-verify", default_value_t = true, action = clap::ArgAction::Set)]
    pub tls_verify: bool,
}

/// `cbsbuild build` handler — stub.
#[allow(clippy::unused_async)] // stub becomes real-async in commits 2–4
pub async fn handle(_args: BuildArgs, _config: &Utf8Path) -> Result<()> {
    bail!("not yet implemented: cbsbuild build (Phase 6 Commit 3)");
}
