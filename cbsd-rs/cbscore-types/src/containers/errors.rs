// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by the container-production subsystem.

use thiserror::Error;

/// Errors surfaced by `cbscore::containers`.
///
/// Phase 1 landed the placeholder `Invalid` variant; Phase 5 adds the
/// per-stage variants: `UnsupportedRepoType` (for unknown
/// `ContainerRepo.source` schemes — surfaces operator-actionable
/// errors instead of an `unreachable!()` macro), `Buildah` (subprocess
/// failures from `buildah from` / `commit` / `unmount` / `rm`), and
/// `Io` (filesystem failures while assembling the build context).
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
///
/// let err = ContainerError::UnsupportedRepoType {
///     name: "epel".into(),
///     value: "ftp://mirror.example.com/epel.repo".into(),
/// };
/// assert!(err.to_string().contains("ftp://"));
/// ```
#[derive(Debug, Error)]
pub enum ContainerError {
    /// Generic container-side error message, pending per-stage refinement.
    #[error("container error: {0}")]
    Invalid(String),

    /// A [`ContainerRepo`](super::desc::ContainerRepo) entry's
    /// `source` does not match a supported scheme (`copr://`,
    /// `file://`, or `http(s)://`).
    #[error(
        "unsupported repo type for '{name}': '{value}' (expected copr:// / file:// / http(s)://)"
    )]
    UnsupportedRepoType {
        /// Operator-chosen repo identifier from the descriptor.
        name: String,
        /// The unrecognised `source` value.
        value: String,
    },

    /// A buildah subprocess (from / commit / unmount / rm) failed
    /// during container assembly.
    #[error("buildah subprocess failed: {0}")]
    Buildah(String),

    /// Filesystem failure during container build context assembly
    /// (tempdir create, RPM staging into the build root, ...).
    #[error("container IO failure: {source}")]
    Io {
        /// Underlying `std::io::Error`.
        #[from]
        source: std::io::Error,
    },
}
