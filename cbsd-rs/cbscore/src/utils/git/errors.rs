// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by `cbscore::utils::git`.
//!
//! Stays inside the `cbscore` crate (not `cbscore-types`) per design
//! 001 §Lift-out invariants — `utils::git` is a future
//! `cbscommon-rs` candidate, so its error type travels with the
//! module rather than living in the shared types crate.

use cbscore_types::utils::subprocess::CommandError;
use thiserror::Error;

/// Errors surfaced by the async git wrappers.
#[derive(Debug, Error)]
pub enum GitError {
    /// `git` exited with a non-zero status. Captures both the exit
    /// code and the stderr payload for operator-actionable diagnostics.
    #[error("git exited with code {retcode}: {stderr}")]
    Failed {
        /// Process exit code reported by `git`.
        retcode: i32,
        /// Captured stderr payload.
        stderr: String,
    },

    /// Underlying subprocess driver failure (spawn / IO / timeout).
    #[error(transparent)]
    Command(#[from] CommandError),

    /// `repo_root()` invoked outside any git checkout.
    #[error("not inside a git repository")]
    NotInRepo,
}
