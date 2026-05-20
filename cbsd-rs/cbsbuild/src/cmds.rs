// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Subcommand handlers + the central dispatch entry point.
//!
//! Each submodule defines:
//!
//! - A clap-derive `Args` / `Subcommand` struct or enum mirroring the
//!   Python `cbsbuild` flag tree byte-for-byte.
//! - A `handle(args, cli)` async function that returns
//!   `Result<(), anyhow::Error>`.
//!
//! Phase 6 Commit 1 lands the trees with `bail!("not yet
//! implemented: …")` bodies; commits 2–4 populate them.

pub mod advanced;
pub mod build;
pub mod config;
pub mod runner;
pub mod shared;
pub mod versions;

use anyhow::Result;

use crate::cli::{Cli, Command};

/// Route the parsed [`Cli`] to the matching subcommand handler.
#[allow(clippy::unused_async)] // dispatched-to handlers are stubs until commits 2–4
pub async fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Build(args) => build::handle(args, &cli.config).await,
        Command::Runner(cmd) => runner::handle(cmd, &cli.config).await,
        Command::Versions(cmd) => versions::handle(cmd, &cli.config).await,
        Command::Config(cmd) => config::handle(cmd, &cli.config).await,
        Command::Advanced(cmd) => advanced::handle(cmd, &cli.config).await,
    }
}
