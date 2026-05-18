// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by the S3 release publisher.

use thiserror::Error;

/// Errors surfaced by `cbscore::releases`.
///
/// Phase 1 lands a single placeholder variant; Phase 5 extends the
/// enum with `S3Error`-wrapping variants for the upload paths.
///
/// # Examples
///
/// ```
/// use cbscore_types::releases::ReleaseError;
///
/// let err = ReleaseError::Invalid(
///     "release descriptor missing 'version' field".into(),
/// );
/// assert_eq!(
///     err.to_string(),
///     "release error: release descriptor missing 'version' field",
/// );
/// ```
#[derive(Debug, Error)]
pub enum ReleaseError {
    /// Generic release-side error message, pending per-stage refinement.
    #[error("release error: {0}")]
    Invalid(String),
}
