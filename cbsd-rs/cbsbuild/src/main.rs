// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild` CLI — Rust port of the Python `cbsbuild` entry point.
//!
//! Phase 6 Commit 1 lands the clap tree scaffold plus the
//! cross-cutting setup (tokio runtime, tracing subscriber, exit-code
//! mapping). Subcommand handlers are stubs that return
//! `bail!("not yet implemented")`; commits 2–4 fill them in.
//!
//! Exit codes (per design 002 lines 1246–1248):
//!
//! - `0` — success.
//! - `22` (`errno::EINVAL`) — missing config / invalid input.
//! - `131` (`errno::ENOTRECOVERABLE`) — any other unhandled error.

#![warn(missing_docs)]

mod cli;
mod cmds;
mod logging;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;

/// `errno::ENOTRECOVERABLE` — unhandled error exit code.
const EXIT_UNRECOVERABLE: u8 = 131;

/// `errno::EINVAL` — invalid-input / missing-config exit code.
const EXIT_INVAL: u8 = 22;

/// Tokio multi-thread main. Worker count defaults to `num_cpus`;
/// the runtime awaits idle before returning so background tasks
/// finish on normal exit.
#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // tracing-appender's non-blocking writer returns a WorkerGuard
    // whose Drop flushes the background thread. Hold it in main()'s
    // scope so the last log lines reach disk on any exit path.
    let _log_guard = match logging::init_logging(cli.debug) {
        Ok(guard) => guard,
        Err(e) => {
            eprintln!("cbsbuild: failed to initialise logging: {e:#}");
            return ExitCode::from(EXIT_UNRECOVERABLE);
        }
    };

    match cmds::dispatch(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("cbsbuild: {err:#}");
            ExitCode::from(classify_exit(&err))
        }
    }
}

/// Map a handler error to an exit code per design 002 lines
/// 1246–1248.
///
/// "Missing config" failures (anything whose top-level message
/// starts with `config file not found` — matches
/// `cbscore_types::config::ConfigError::NotFound`'s pinned
/// Display text — or that opens with `missing config`) map to
/// [`EXIT_INVAL`]. Everything else maps to [`EXIT_UNRECOVERABLE`].
fn classify_exit(err: &anyhow::Error) -> u8 {
    let msg = err.to_string();
    if msg.starts_with("config file not found") || msg.starts_with("missing config") {
        EXIT_INVAL
    } else {
        EXIT_UNRECOVERABLE
    }
}
