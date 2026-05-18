// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by the container-production subsystem.

use thiserror::Error;

/// Errors surfaced by `cbscore::containers`.
///
/// Phase 1 lands a single placeholder variant; Phase 5 extends the
/// enum with per-stage variants (UnsupportedRepoType,
/// BuildahWorkingContainer cleanup failures, etc.).
///
/// # Examples
///
/// ```
/// use cbscore_types::containers::ContainerError;
///
/// let err = ContainerError::Invalid(
///     "container descriptor missing 'base-image' field".into(),
/// );
/// assert_eq!(
///     err.to_string(),
///     "container error: container descriptor missing 'base-image' field",
/// );
/// ```
#[derive(Debug, Error)]
pub enum ContainerError {
    /// Generic container-side error message, pending per-stage refinement.
    #[error("container error: {0}")]
    Invalid(String),
}
