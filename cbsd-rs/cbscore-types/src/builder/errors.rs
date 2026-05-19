// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by the four-stage builder pipeline.

use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors surfaced by the builder pipeline stages.
///
/// Phase 1 landed `MissingScript`; Phase 5 commits add their per-stage
/// variants (`Io`, `Git`, `Patch`, …) as each stage starts surfacing
/// them. Carrying a generic `Other(String)` here keeps wrapping calls
/// from [`VersionError`](crate::versions::VersionError),
/// [`ComponentError`](crate::core::component::ComponentError), and
/// [`SecretsError`](crate::utils::secrets::SecretsError) concise
/// without proliferating per-wrap variants.
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

    /// Generic IO failure during a stage's work (scratch-dir create
    /// or clear, fixture file read, …). The wrapped path identifies
    /// the offending location; the wrapped `source` carries the
    /// underlying `std::io::Error`.
    #[error("builder IO failure at {path}: {source}")]
    Io {
        /// Path of the offending IO operation.
        path: Utf8PathBuf,
        /// Underlying `std::io::Error`.
        source: std::io::Error,
    },

    /// A required core-component definition is missing from the
    /// loaded `HashMap<String, CoreComponent>` set — the descriptor
    /// references a component name not provisioned by the
    /// `components/` tree.
    #[error("descriptor references unknown core component '{name}'")]
    MissingComponent {
        /// Operator-chosen component name from the descriptor.
        name: String,
    },

    /// Generic wrap for upstream errors (`GitError`, `VersionError`,
    /// `S3Error`, …) — the wrapping caller renders the underlying
    /// error's message into a single string. The source chain
    /// terminates here; callers that need typed chain traversal use
    /// the per-upstream-error variants once those are added by their
    /// owning Phase 5 commit.
    #[error("{0}")]
    Other(String),
}
