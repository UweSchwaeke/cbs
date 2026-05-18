// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by the podman-based runner.

use thiserror::Error;

use crate::utils::podman::PodmanError;
use crate::utils::subprocess::CommandError;

/// Errors surfaced by `cbscore::runner::run`.
///
/// The variants follow the two-layer timeout architecture documented
/// in Phase 4 Commit 3: `Timeout` is the *outer* runner-level budget
/// (whole-runner, 4 hours by default per design 002 line 849), while
/// `CommandError::Timeout` (surfaced via [`Command`](Self::Command))
/// is the *inner* per-subprocess budget. The two variants are not
/// interchangeable.
///
/// # Examples
///
/// ```
/// use cbscore_types::runner::RunnerError;
/// use std::io;
///
/// let inner = io::Error::new(io::ErrorKind::NotFound, "no such file");
/// let err = RunnerError::BinaryNotFound { source: inner };
/// assert_eq!(
///     err.to_string(),
///     "could not locate the cbsbuild binary on disk: no such file",
/// );
/// ```
#[derive(Debug, Error)]
pub enum RunnerError {
    /// SIGTERM landed on the runner or the future was dropped by an
    /// outer cancellation.
    #[error("runner cancelled")]
    Cancelled,

    /// The outer runner-level `tokio::time::timeout` elapsed before the
    /// build finished.
    #[error("runner timed out")]
    Timeout,

    /// `std::env::current_exe()` failed: the runner cannot mount itself
    /// into the builder container without a host-side path.
    #[error("could not locate the cbsbuild binary on disk: {source}")]
    BinaryNotFound {
        /// Underlying `std::io::Error` from the procfs lookup.
        source: std::io::Error,
    },

    /// A subprocess invocation surfaced an error; transparently wraps
    /// [`CommandError`].
    #[error(transparent)]
    Command(#[from] CommandError),

    /// A podman invocation surfaced an error; transparently wraps
    /// [`PodmanError`].
    #[error(transparent)]
    Podman(#[from] PodmanError),
}
