// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by the image-descriptor + sign / sync flow.

use thiserror::Error;

/// Errors surfaced by `cbscore::images`.
///
/// Phase 1 lands a single placeholder variant; Phase 5 extends the
/// enum with manifest-signing / skopeo-copy variants.
///
/// # Examples
///
/// ```
/// use cbscore_types::images::ImageDescriptorError;
///
/// let err = ImageDescriptorError::Invalid(
///     "image manifest digest mismatch".into(),
/// );
/// assert_eq!(
///     err.to_string(),
///     "image descriptor error: image manifest digest mismatch",
/// );
/// ```
#[derive(Debug, Error)]
pub enum ImageDescriptorError {
    /// Generic image-side error message, pending per-stage refinement.
    #[error("image descriptor error: {0}")]
    Invalid(String),
}
