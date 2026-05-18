// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by `cbscore::utils::buildah`.

use thiserror::Error;

/// Errors surfaced by the buildah async wrappers.
///
/// Mirrors [`PodmanError`]'s `Failed { retcode, stderr }` shape; Phase 2
/// Commit 2 may extend with buildah-specific subcategories as the
/// wrappers land.
///
/// [`PodmanError`]: crate::utils::podman::PodmanError
///
/// # Examples
///
/// ```
/// use cbscore_types::utils::buildah::BuildahError;
///
/// let err = BuildahError::Failed {
///     retcode: 1,
///     stderr: "error mounting container: not a working container".into(),
/// };
/// assert_eq!(
///     err.to_string(),
///     "buildah exited with code 1: error mounting container: not a working container",
/// );
/// ```
#[derive(Debug, Error)]
pub enum BuildahError {
    /// A buildah invocation surfaced a non-zero exit code.
    #[error("buildah exited with code {retcode}: {stderr}")]
    Failed {
        /// Process exit code reported by buildah.
        retcode: i32,
        /// Captured stderr payload.
        stderr: String,
    },
}
