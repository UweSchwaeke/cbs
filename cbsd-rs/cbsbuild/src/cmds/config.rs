// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild config …` — config-file lifecycle.
//!
//! Subcommands:
//!
//! - `config init` (bypass modes only — `--for-systemd-install` /
//!   `--for-containerized-run` + per-field overrides).
//! - `config show` — pretty-print the loaded config.
//! - `config check` — validate the loaded config and exit
//!   0 / non-zero.
//!
//! No interactive prompts — the prompt-based UX is seq-003
//! (post-M1) per design 002 §Open Questions lines 1424–1432.
//! Implementation lands in Phase 6 Commit 4.

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Subcommand};

/// `cbsbuild config …` subcommand enum.
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Initialise a config from a bypass mode + overrides.
    Init(InitArgs),
    /// Pretty-print the loaded config.
    Show,
    /// Validate the loaded config; exits 0 on success.
    Check,
}

/// `cbsbuild config init` arguments. No interactive prompts — at
/// least one of the `--for-*` mode flags is required (running with
/// no flags returns an error with a usage hint).
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Pre-fill paths for a systemd-managed deployment.
    #[arg(long = "for-systemd-install")]
    pub for_systemd_install: bool,

    /// Pre-fill paths for a containerised deployment.
    #[arg(long = "for-containerized-run")]
    pub for_containerized_run: bool,

    /// Override `paths.components` (repeatable / colon-separated).
    #[arg(long = "components", value_delimiter = ':')]
    pub components: Vec<Utf8PathBuf>,

    /// Override `paths.scratch`.
    #[arg(long = "scratch")]
    pub scratch: Option<Utf8PathBuf>,

    /// Override `paths.scratch-containers`.
    #[arg(long = "containers-scratch")]
    pub containers_scratch: Option<Utf8PathBuf>,

    /// Override `paths.ccache`.
    #[arg(long = "ccache")]
    pub ccache: Option<Utf8PathBuf>,

    /// Override `vault` (path to `cbs-build.vault.yaml`).
    #[arg(long = "vault")]
    pub vault: Option<Utf8PathBuf>,

    /// Override `secrets` (repeatable / colon-separated paths).
    #[arg(long = "secrets", value_delimiter = ':')]
    pub secrets: Vec<Utf8PathBuf>,
}

/// `cbsbuild config …` handler — stub.
#[allow(clippy::unused_async)] // stub becomes real-async in commits 2–4
pub async fn handle(cmd: ConfigCommand, _config: &Utf8Path) -> Result<()> {
    match cmd {
        ConfigCommand::Init(_) => {
            bail!("not yet implemented: cbsbuild config init (Phase 6 Commit 4)")
        }
        ConfigCommand::Show => {
            bail!("not yet implemented: cbsbuild config show (Phase 6 Commit 4)")
        }
        ConfigCommand::Check => {
            bail!("not yet implemented: cbsbuild config check (Phase 6 Commit 4)")
        }
    }
}
