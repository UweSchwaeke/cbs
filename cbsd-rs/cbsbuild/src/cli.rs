// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Top-level CLI definition (clap derive).
//!
//! Mirrors the Python `cbsbuild` flag set byte-for-byte per CLAUDE.md
//! Correctness Invariant 2. The only deliberate parity break is the
//! removed `--cbscore-path` flag — the Rust runner mounts the binary
//! itself, so the flag is no longer meaningful (design 002 lines
//! 1249–1255).

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

use crate::cmds::{advanced, build, config, runner, versions};

/// Default value for the `--config` flag — matches the Python
/// `cbsbuild`'s default (CLAUDE.md Correctness Invariant 2; design
/// 002 line 1244).
pub(crate) const DEFAULT_CONFIG_PATH: &str = "cbs-build.config.yaml";

/// `cbsbuild` — the build-orchestrator CLI.
///
/// Subcommands:
///
/// - `build` — runs a build against an explicit descriptor (thin
///   alias for `runner run`).
/// - `runner` — host-side container lifecycle (`run`, `stop`) plus
///   the in-container `runner build` entry point.
/// - `versions` — version-descriptor lifecycle: `create`, `list`,
///   `show`, `validate`.
/// - `config` — config-file lifecycle: `init` (bypass modes only),
///   `show`, `check`.
/// - `advanced` — operator-side debug escape hatches.
#[derive(Debug, Parser)]
#[command(name = "cbsbuild", about, version)]
pub(crate) struct Cli {
    /// Path to the cbsbuild config file.
    #[arg(short = 'c', long = "config", default_value = DEFAULT_CONFIG_PATH)]
    pub config: Utf8PathBuf,

    /// Enable debug logging. Honours the `CBS_DEBUG` env var per
    /// design 002 line 1245.
    #[arg(short = 'd', long = "debug", env = "CBS_DEBUG")]
    pub debug: bool,

    /// Subcommand to dispatch.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommand enum.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Run a build against a version descriptor.
    Build(build::BuildArgs),
    /// Host-side container lifecycle + in-container build entry.
    #[command(subcommand)]
    Runner(runner::RunnerCommand),
    /// Version-descriptor lifecycle.
    #[command(subcommand)]
    Versions(versions::VersionsCommand),
    /// Config-file lifecycle.
    #[command(subcommand)]
    Config(config::ConfigCommand),
    /// Operator-side debug escape hatches.
    #[command(subcommand)]
    Advanced(advanced::AdvancedCommand),
}
