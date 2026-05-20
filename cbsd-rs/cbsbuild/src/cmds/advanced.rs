// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild advanced …` — operator-side debug escape hatches.
//!
//! Per design 002 line 1240–1241 ("escape-hatch subcommands;
//! rare"). Writes to **stderr** by default so stdout stays
//! parseable for the wrapping handler; `--json` switches each
//! handler to machine-readable output on stdout.
//!
//! Phase 6 Commit 4 lands `dump-config` and
//! `dump-resolved-secrets` — the M1 minimal set sufficient for
//! the M1 smoke gate (Commit 6).

use anyhow::{Context, Result};
use camino::Utf8Path;
use clap::{Args, Subcommand};

use super::shared::load_config_and_secrets;

/// `cbsbuild advanced …` subcommand enum.
#[derive(Debug, Subcommand)]
pub(crate) enum AdvancedCommand {
    /// Dump the loaded config — useful for inspecting bypass-mode
    /// outputs and resolved defaults.
    DumpConfig(DumpArgs),
    /// Dump the resolved secrets payload — vault refs replaced by
    /// their plain form.
    DumpResolvedSecrets(DumpArgs),
}

/// Shared shape for the `advanced` dump subcommands.
#[derive(Debug, Args)]
pub(crate) struct DumpArgs {
    /// Emit machine-readable JSON on stdout instead of
    /// human-readable text on stderr.
    #[arg(long = "json")]
    pub json: bool,
}

/// `cbsbuild advanced …` handler.
pub(crate) async fn handle(cmd: AdvancedCommand, config_path: &Utf8Path) -> Result<()> {
    match cmd {
        AdvancedCommand::DumpConfig(args) => handle_dump_config(args, config_path).await,
        AdvancedCommand::DumpResolvedSecrets(args) => {
            handle_dump_resolved_secrets(args, config_path).await
        }
    }
}

async fn handle_dump_config(args: DumpArgs, config_path: &Utf8Path) -> Result<()> {
    let (cfg, _secrets) = load_config_and_secrets(config_path).await?;
    if args.json {
        let body =
            serde_json::to_string_pretty(&cfg).context("serialising config for --json output")?;
        println!("{body}");
    } else {
        // Human-readable view goes to stderr so callers piping
        // stdout aren't surprised by debug-shape noise.
        eprintln!("{cfg:#?}");
    }
    Ok(())
}

async fn handle_dump_resolved_secrets(args: DumpArgs, config_path: &Utf8Path) -> Result<()> {
    let (_cfg, secrets) = load_config_and_secrets(config_path).await?;
    // Vault-ref resolution would happen here once the Phase 5
    // follow-up wires SecretsMgr's typed Vault-secret resolver;
    // M1 dumps whatever the load_files step assembled.
    if args.json {
        let body = serde_json::to_string_pretty(secrets.secrets())
            .context("serialising resolved secrets for --json output")?;
        println!("{body}");
    } else {
        // The Secrets value itself deliberately does NOT derive
        // Debug (CLAUDE.md Correctness Invariant 5 — credential
        // redaction); print structured per-family counts only.
        // Operators take the --json path for the full payload
        // when they're sure the terminal is safe.
        let s = secrets.secrets();
        eprintln!(
            "resolved-secrets summary:\n  \
             git:      {} entries\n  \
             storage:  {} entries\n  \
             signing:  {} entries\n  \
             registry: {} entries\n\
             (pass --json to dump the full payload to stdout)",
            s.git.len(),
            s.storage.len(),
            s.signing.len(),
            s.registry.len(),
        );
    }
    Ok(())
}
