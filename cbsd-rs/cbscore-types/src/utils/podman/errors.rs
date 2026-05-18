// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by `cbscore::utils::podman`.

use thiserror::Error;

/// Errors surfaced by the podman async wrappers.
///
/// Phase 1 lands a single `Failed { retcode, stderr }` variant that
/// captures both the non-zero exit code and the raw stderr payload;
/// Phase 2 Commit 2 may extend with podman-specific subcategories
/// (e.g. cidfile-missing, container-already-stopped) as the wrappers
/// land.
///
/// # Examples
///
/// ```
/// use cbscore_types::utils::podman::PodmanError;
///
/// let err = PodmanError::Failed {
///     retcode: 125,
///     stderr: "Error: no such image: my/builder:el9".into(),
/// };
/// assert_eq!(
///     err.to_string(),
///     "podman exited with code 125: Error: no such image: my/builder:el9",
/// );
/// ```
#[derive(Debug, Error)]
pub enum PodmanError {
    /// A podman invocation surfaced a non-zero exit code.
    #[error("podman exited with code {retcode}: {stderr}")]
    Failed {
        /// Process exit code reported by podman.
        retcode: i32,
        /// Captured stderr payload (truncated by the subprocess driver
        /// before reaching this variant per the secret-redaction rules
        /// in `cbscore::utils::subprocess`).
        stderr: String,
    },
}
