// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by version-descriptor IO and parsing.

use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors loading, parsing, or writing a `VersionDescriptor` JSON file.
///
/// `AlreadyExists` is the EEXIST-style refusal-to-overwrite raised by
/// `cbsbuild versions create` when the descriptor file already exists
/// on disk. The two `schema_version` variants parallel
/// [`ConfigError`]'s identically-named variants but use snake-case
/// `schema_version` (descriptors are JSON with snake-case keys). The
/// three `…DescriptorRoot…` variants (`NoDescriptorRoot`,
/// `DescriptorRootResolve`, `DescriptorRootNotUtf8`) belong to the
/// configurable-descriptor-root resolver in
/// [`cbscore::versions::resolve_root`][resolve] — they surface when
/// neither `--versions-dir` nor `paths.versions` is set and the
/// resolver cannot fall back to a git checkout, or when an operator-
/// supplied root cannot be canonicalised.
///
/// [`ConfigError`]: crate::config::ConfigError
/// [resolve]: https://docs.rs/cbscore/latest/cbscore/versions/fn.resolve_root.html
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

    /// No descriptor-store root could be resolved. Raised by
    /// `cbscore::versions::resolve_root` when none of the three
    /// precedence layers (CLI `--versions-dir`, `Config.paths.versions`,
    /// `<git-rev-parse --show-toplevel>/_versions`) yielded a path.
    ///
    /// The four-line `Display` text is operator-actionable and pinned
    /// by design 004 §OQ5 — every line names a different override
    /// surface the operator can set.
    #[error(
        "cannot resolve descriptor store location.\n  no --versions-dir flag was supplied,\n  no `paths.versions` field is set in cbs-build.config.yaml,\n  and the current directory ({cwd}) is not inside a git checkout.\n  set one of the above to choose where descriptors live."
    )]
    NoDescriptorRoot {
        /// Current working directory when the resolver was called.
        /// Rendered as `<unknown>` if `std::env::current_dir()` itself
        /// failed (deleted cwd, etc.) — the resolver captures this
        /// best-effort and never propagates the underlying `io::Error`
        /// so the operator-facing text stays clean.
        cwd: Utf8PathBuf,
    },

    /// An operator-supplied descriptor root could not be canonicalised
    /// — most commonly `ENOENT` because the directory does not yet
    /// exist. The operator can fix this by `mkdir -p`-ing the target
    /// before passing it through `--versions-dir` / `paths.versions`.
    #[error(
        "cannot resolve descriptor root '{path}': {source} (hint: `mkdir -p '{path}'` before running again)"
    )]
    DescriptorRootResolve {
        /// Operator-supplied path that failed to canonicalise.
        path: Utf8PathBuf,
        /// Underlying canonicalize error from the filesystem.
        source: std::io::Error,
    },

    /// The descriptor root canonicalised successfully but the resolved
    /// absolute path contains non-UTF-8 bytes. Rare on Linux operator
    /// hosts but representable; surfacing the error explicitly avoids
    /// silently corrupting the descriptor-store layout.
    #[error("descriptor root '{path}' is not valid UTF-8 after canonicalisation")]
    DescriptorRootNotUtf8 {
        /// Lossy string form of the offending path (the original is
        /// `OsString` and not constructible from `String`, so the
        /// captured form here is intentionally lossy — the variant
        /// exists to surface the failure, not to round-trip the path).
        path: String,
    },
}
