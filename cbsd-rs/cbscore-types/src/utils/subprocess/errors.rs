// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by `cbscore::utils::subprocess::async_run_cmd`.

use std::time::Duration;

use thiserror::Error;

/// Errors surfaced by `cbscore::utils::subprocess::async_run_cmd`.
///
/// Phase 1 lands the `Timeout` variant — the inner per-subprocess
/// budget that the runner's two-layer timeout architecture
/// distinguishes from `RunnerError::Timeout`. Phase 2 Commit 1
/// extends the enum with `Spawn` and `NonZeroExit` variants as the
/// subprocess driver lands.
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
    /// The inner per-subprocess `tokio::time::timeout` elapsed before
    /// the child exited.
    #[error("subprocess timed out after {}s", after.as_secs())]
    Timeout {
        /// Per-call timeout budget that elapsed.
        after: Duration,
    },
}
