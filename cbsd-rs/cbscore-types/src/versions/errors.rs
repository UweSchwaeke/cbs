// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by version-descriptor IO and parsing.

use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors loading, parsing, or writing a `VersionDescriptor` JSON file.
///
/// `AlreadyExists` is the EEXIST-style refusal-to-overwrite raised by
/// `cbsbuild versions create` / seq-004's write site when the
/// descriptor file already exists on disk. The two `schema_version`
/// variants parallel [`ConfigError`]'s identically-named variants but
/// use snake-case `schema_version` (descriptors are JSON with
/// snake-case keys).
///
/// seq-004 Commit 2 adds three further variants to this enum
/// (`NoDescriptorRoot`, `DescriptorRootResolve`, `DescriptorRootNotUtf8`)
/// for the configurable-descriptor-root resolver; the canonical home
/// for every `VersionError` variant is this file.
///
/// [`ConfigError`]: crate::config::ConfigError
///
/// # Examples
///
/// ```
/// use cbscore_types::versions::VersionError;
/// use camino::Utf8PathBuf;
///
/// let err = VersionError::AlreadyExists {
///     path: Utf8PathBuf::from("/var/cbs/_versions/dev/19.2.3.json"),
/// };
/// assert_eq!(
///     err.to_string(),
///     "refusing to overwrite existing descriptor at /var/cbs/_versions/dev/19.2.3.json",
/// );
///
/// let err = VersionError::MissingSchemaVersion {
///     path: Utf8PathBuf::from("/var/cbs/_versions/dev/19.2.3.json"),
/// };
/// assert_eq!(
///     err.to_string(),
///     "missing 'schema_version' key in /var/cbs/_versions/dev/19.2.3.json",
/// );
///
/// let err = VersionError::UnknownSchemaVersion {
///     path: Utf8PathBuf::from("/var/cbs/_versions/dev/19.2.3.json"),
///     found: 99,
///     max_supported: 1,
/// };
/// assert_eq!(
///     err.to_string(),
///     "unsupported schema_version 99 in /var/cbs/_versions/dev/19.2.3.json (max supported: 1); upgrade cbscore-rs",
/// );
/// ```
#[derive(Debug, Error)]
pub enum VersionError {
    /// The on-disk descriptor file is not a well-formed
    /// `VersionDescriptor` (JSON parse error, type mismatch, …).
    #[error("invalid version descriptor at {path}: {message}")]
    InvalidDescriptor {
        /// Path of the offending descriptor file.
        path: Utf8PathBuf,
        /// Underlying parser-error message (lossy capture; the
        /// source-error chain terminates here since cbscore-types is
        /// free of `serde_json` in its [`dependencies`] graph).
        ///
        /// [`dependencies`]: https://doc.rust-lang.org/cargo/reference/manifest.html#the-dependencies-section
        message: String,
    },

    /// No descriptor file exists at the supplied path.
    #[error("no version descriptor at {path}")]
    NoSuchDescriptor {
        /// Path that was looked up and not found.
        path: Utf8PathBuf,
    },

    /// `cbsbuild versions create` refuses to overwrite an existing
    /// descriptor; matches Python EEXIST surface.
    #[error("refusing to overwrite existing descriptor at {path}")]
    AlreadyExists {
        /// Descriptor path that already exists.
        path: Utf8PathBuf,
    },

    /// The descriptor file is missing the snake-case `schema_version` key.
    #[error("missing 'schema_version' key in {path}")]
    MissingSchemaVersion {
        /// Descriptor whose `schema_version` marker is absent.
        path: Utf8PathBuf,
    },

    /// The descriptor file declares a `schema_version` higher than the
    /// compiled-in maximum supported by this cbscore-rs build.
    #[error(
        "unsupported schema_version {found} in {path} (max supported: {max_supported}); upgrade cbscore-rs"
    )]
    UnknownSchemaVersion {
        /// Descriptor whose `schema_version` exceeds the supported range.
        path: Utf8PathBuf,
        /// `schema_version` value found on disk.
        found: u64,
        /// Highest `schema_version` this cbscore-rs build understands.
        max_supported: u64,
    },
}
