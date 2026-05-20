// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Tracing-subscriber configuration for `cbsbuild`.
//!
//! Per design 002 §Logging (lines 1184–1210):
//!
//! - Default filter: `cbscore=info` — the cbscore-internal modules
//!   are the operator-relevant scope.
//! - `CBS_DEBUG=1` (or `--debug` / `-d`) bumps the filter to
//!   `cbscore=debug`.
//! - Optional file appender wired in Commit 4 once
//!   `config.logging.log_file` flows through the dispatch (the M1
//!   scaffold logs to stderr only).
//!
//! Returns a [`tracing_appender::non_blocking::WorkerGuard`] (or an
//! Option-wrapped equivalent) so the caller can hold it across
//! `main`'s scope. Dropping the guard flushes the background
//! writer thread.

use anyhow::Result;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialise the tracing subscriber.
///
/// `debug` is `true` when `--debug` / `-d` is set or `CBS_DEBUG=1`
/// is exported. The default filter targets the `cbscore` crate's
/// modules; everything else (axum, aws-sdk-s3, etc.) stays at the
/// subscriber's default level.
///
/// # Errors
///
/// Returns an error when the subscriber fails to install (typically
/// because a global subscriber was already set by another caller —
/// shouldn't happen under `cbsbuild`'s single-process model, but
/// surfaced as an error rather than a panic).
pub(crate) fn init_logging(debug: bool) -> Result<WorkerGuard> {
    let filter_str = if debug {
        "cbscore=debug"
    } else {
        "cbscore=info"
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter_str));

    let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stderr());

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(non_blocking))
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber init: {e}"))?;

    Ok(guard)
}
