// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild runner …` — host-side container lifecycle + the
//! in-container build entry point.
//!
//! Subcommands:
//!
//! - `runner run <descriptor.json>` — host-side spawn (alias of
//!   `cbsbuild build`).
//! - `runner stop [--name NAME] [--all]` — host-side stop.
//! - `runner build <descriptor.json>` — **in-container entry
//!   point** invoked by Phase 4's host runner via
//!   `--entrypoint /runner/cbsbuild`. Runs the four-stage
//!   `builder::run_build` pipeline against the mounted config +
//!   secrets and writes the `BuildArtifactReport` to the pinned
//!   path `/runner/<name>.report.json`.
//!
//! Implementation lands in Phase 6 Commit 3.

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Subcommand};

/// `cbsbuild runner …` subcommand enum.
#[derive(Debug, Subcommand)]
pub enum RunnerCommand {
    /// Run a build (host-side spawn).
    Run(RunArgs),
    /// Stop a container or every cbscore-prefixed container.
    Stop(StopArgs),
    /// In-container build entry — invoked by the host runner via
    /// `--entrypoint /runner/cbsbuild`.
    Build(RunArgs),
}

/// Shared shape for `runner run` and `runner build`.
#[derive(Debug, Args)]
pub struct RunArgs {
    /// Path to the version-descriptor JSON.
    pub descriptor: Utf8PathBuf,

    /// Skip the in-container rpmbuild step.
    #[arg(long = "skip-build")]
    pub skip_build: bool,

    /// Clear each component's scratch dir before fetching sources.
    #[arg(long = "force")]
    pub force: bool,

    /// TLS verification for registry pushes (default on).
    #[arg(long = "tls-verify", default_value_t = true, action = clap::ArgAction::Set)]
    pub tls_verify: bool,
}

/// `cbsbuild runner stop` arguments.
#[derive(Debug, Args)]
pub struct StopArgs {
    /// Stop a single container by name.
    #[arg(long = "name", conflicts_with = "all")]
    pub name: Option<String>,

    /// Stop every cbscore-prefixed container (the `--all` form).
    #[arg(long = "all", conflicts_with = "name")]
    pub all: bool,
}

/// `cbsbuild runner …` handler — stub.
#[allow(clippy::unused_async)] // stub becomes real-async in commits 2–4
pub async fn handle(cmd: RunnerCommand, _config: &Utf8Path) -> Result<()> {
    match cmd {
        RunnerCommand::Run(_) => {
            bail!("not yet implemented: cbsbuild runner run (Phase 6 Commit 3)")
        }
        RunnerCommand::Stop(_) => {
            bail!("not yet implemented: cbsbuild runner stop (Phase 6 Commit 3)")
        }
        RunnerCommand::Build(_) => {
            bail!("not yet implemented: cbsbuild runner build (Phase 6 Commit 3)")
        }
    }
}
