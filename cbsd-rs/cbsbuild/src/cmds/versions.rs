// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild versions …` — version-descriptor lifecycle.
//!
//! Subcommands:
//!
//! - `versions create <VERSION>` — resolve component refs via
//!   `git ls-remote`, build a `VersionDescriptor`, write to
//!   `<git-root>/_versions/<type>/<VERSION>.json`.
//! - `versions list [--path DIR]` — list known releases from S3.
//! - `versions show <descriptor.json>` — pretty-print a descriptor.
//! - `versions validate <descriptor.json>` — exit 0 on success,
//!   non-zero on validation error.
//!
//! Implementation lands in Phase 6 Commit 2.

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Subcommand};

/// `cbsbuild versions …` subcommand enum.
#[derive(Debug, Subcommand)]
pub enum VersionsCommand {
    /// Create a version descriptor for `VERSION`.
    Create(CreateArgs),
    /// List released versions from S3.
    List(ListArgs),
    /// Pretty-print a descriptor file.
    Show(ShowArgs),
    /// Validate a descriptor file and exit 0 / 1.
    Validate(ShowArgs),
}

/// `cbsbuild versions create` arguments.
#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Version string to create the descriptor for.
    pub version: String,

    /// Component refs in `component@ref` form. Repeatable.
    #[arg(short = 'c', long = "component", value_name = "COMPONENT@REF")]
    pub components: Vec<String>,

    /// Release type. Defaults to `dev`.
    #[arg(long = "type", default_value = "dev")]
    pub version_type: String,

    /// Optional title — falls back to a generated string when
    /// absent.
    #[arg(long = "title")]
    pub title: Option<String>,

    /// Optional override for the descriptor's `distro` field.
    #[arg(long = "distro")]
    pub distro: Option<String>,
}

/// `cbsbuild versions list` arguments.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Optional override for the S3 prefix to enumerate.
    #[arg(long = "path")]
    pub path: Option<String>,
}

/// `cbsbuild versions show` / `validate` arguments.
#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Path to the descriptor JSON.
    pub descriptor: Utf8PathBuf,
}

/// `cbsbuild versions …` handler — stub.
#[allow(clippy::unused_async)] // stub becomes real-async in commits 2–4
pub async fn handle(cmd: VersionsCommand, _config: &Utf8Path) -> Result<()> {
    match cmd {
        VersionsCommand::Create(_) => {
            bail!("not yet implemented: cbsbuild versions create (Phase 6 Commit 2)")
        }
        VersionsCommand::List(_) => {
            bail!("not yet implemented: cbsbuild versions list (Phase 6 Commit 2)")
        }
        VersionsCommand::Show(_) => {
            bail!("not yet implemented: cbsbuild versions show (Phase 6 Commit 2)")
        }
        VersionsCommand::Validate(_) => {
            bail!("not yet implemented: cbsbuild versions validate (Phase 6 Commit 2)")
        }
    }
}
