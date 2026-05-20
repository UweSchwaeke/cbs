// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild advanced …` — operator-side debug escape hatches.
//!
//! Per design 002 line 1240–1241 ("escape-hatch subcommands;
//! rare"). Writes to stderr by default so stdout stays parseable;
//! `--json` switches each handler to machine-readable output.
//!
//! Phase 6 Commit 4 lands the minimal `dump-config` and
//! `dump-resolved-secrets` subcommands. This commit ships the
//! scaffold.

use anyhow::{Result, bail};
use camino::Utf8Path;
use clap::{Args, Subcommand};

/// `cbsbuild advanced …` subcommand enum.
#[derive(Debug, Subcommand)]
pub enum AdvancedCommand {
    /// Dump the loaded config — useful for inspecting bypass-mode
    /// outputs and resolved defaults.
    DumpConfig(DumpArgs),
    /// Dump the resolved secrets payload — vault refs replaced by
    /// their plain form.
    DumpResolvedSecrets(DumpArgs),
}

/// Shared shape for the `advanced` dump subcommands.
#[derive(Debug, Args)]
pub struct DumpArgs {
    /// Emit machine-readable JSON instead of human-readable text.
    #[arg(long = "json")]
    pub json: bool,
}

/// `cbsbuild advanced …` handler — stub.
#[allow(clippy::unused_async)] // stub becomes real-async in commits 2–4
pub async fn handle(cmd: AdvancedCommand, _config: &Utf8Path) -> Result<()> {
    match cmd {
        AdvancedCommand::DumpConfig(_) => {
            bail!("not yet implemented: cbsbuild advanced dump-config (Phase 6 Commit 4)")
        }
        AdvancedCommand::DumpResolvedSecrets(_) => {
            bail!(
                "not yet implemented: cbsbuild advanced dump-resolved-secrets \
                 (Phase 6 Commit 4)"
            )
        }
    }
}
