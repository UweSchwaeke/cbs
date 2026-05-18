// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by the four-stage builder pipeline.

use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors surfaced by the builder pipeline stages.
///
/// Phase 1 lands one variant — `MissingScript` — surfaced by the
/// prepare stage when a required build script is absent. Each Phase 5
/// commit pins its own per-stage variant text.
///
/// # Examples
///
/// ```
/// use cbscore_types::builder::BuilderError;
/// use camino::Utf8PathBuf;
///
/// let err = BuilderError::MissingScript {
///     path: Utf8PathBuf::from("/scratch/ceph/build.sh"),
/// };
/// assert_eq!(
///     err.to_string(),
///     "required build script not found at /scratch/ceph/build.sh",
/// );
/// ```
#[derive(Debug, Error)]
pub enum BuilderError {
    /// A required build script does not exist at the expected path.
    #[error("required build script not found at {path}")]
    MissingScript {
        /// Path the prepare stage looked up.
        path: Utf8PathBuf,
    },
}
