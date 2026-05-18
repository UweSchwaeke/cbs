// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by `cbscore::utils::subprocess::async_run_cmd`.

use std::time::Duration;

use thiserror::Error;

/// Errors surfaced by `cbscore::utils::subprocess::async_run_cmd`.
///
/// Non-zero exit codes are **not** an error — they surface via
/// `Ok(RunOutcome { rc, .. })` with `rc != 0` so the caller can
/// interpret per their own domain (e.g. `PodmanError::Failed` wraps a
/// non-zero podman exit). `CommandError` is reserved for subprocess
/// lifecycle failures (couldn't spawn, IO failure on a pipe, timeout).
///
/// # Examples
///
/// ```
/// use cbscore_types::utils::subprocess::CommandError;
/// use std::time::Duration;
///
/// let err = CommandError::Timeout { after: Duration::from_secs(60) };
/// assert_eq!(
///     err.to_string(),
///     "subprocess timed out after 60s",
/// );
/// ```
#[derive(Debug, Error)]
pub enum CommandError {
    /// `tokio::process::Command::spawn()` failed (binary not found,
    /// permission denied, …).
    #[error("could not spawn subprocess: {source}")]
    Spawn {
        /// Underlying `std::io::Error` from `spawn()`.
        source: std::io::Error,
    },

    /// IO failure on the child's stdout / stderr pipe, or while
    /// awaiting the child to exit.
    #[error("subprocess IO failure: {source}")]
    Io {
        /// Underlying `std::io::Error`.
        source: std::io::Error,
    },

    /// The inner per-subprocess `tokio::time::timeout` elapsed before
    /// the child exited.
    #[error("subprocess timed out after {}s", after.as_secs())]
    Timeout {
        /// Per-call timeout budget that elapsed.
        after: Duration,
    },
}
