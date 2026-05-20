// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild build <descriptor.json>` — thin alias for
//! [`cbsbuild runner run`](super::runner::RunnerCommand::Run) per
//! design 002 line 1224. Both end up calling Phase 4's
//! `cbscore::runner::run`.

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use clap::Args;

use super::runner::{RunArgs, handle_run};

/// `cbsbuild build` argument set.
#[derive(Debug, Args)]
pub(crate) struct BuildArgs {
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
    /// `true` (default) verifies TLS for registry pushes.
    #[arg(long = "tls-verify", default_value_t = true, action = clap::ArgAction::Set)]
    pub tls_verify: bool,
    /// Builder image reference override. Falls back to the
    /// descriptor's `image:` block when absent.
    #[arg(long = "image")]
    pub image: Option<String>,
}

/// `cbsbuild build` handler — delegates verbatim to
/// [`super::runner::handle_run`] so the two commands share one
/// code path (matches the Python alias relationship).
pub(crate) async fn handle(args: BuildArgs, config_path: &Utf8Path) -> Result<()> {
    let run_args = RunArgs {
        descriptor: args.descriptor,
        skip_build: args.skip_build,
        force: args.force,
        tls_verify: args.tls_verify,
        image: args.image,
    };
    handle_run(run_args, config_path).await
}
