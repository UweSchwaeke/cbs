// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by config-file IO.

use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors loading or parsing a `cbs-build.config.yaml` file.
///
/// `NotFound` is the structured shape `Config::load` maps
/// `std::io::ErrorKind::NotFound` to before propagating, so the CLI
/// can surface a clean operator message rather than an opaque
/// `io::Error`. Other IO errors (permission denied, IO failure
/// mid-read) propagate via the [`Io`](Self::Io) variant.
///
/// # Examples
///
/// ```
/// use cbscore_types::config::ConfigError;
/// use camino::Utf8PathBuf;
///
/// let err = ConfigError::NotFound {
///     path: Utf8PathBuf::from("/etc/cbs/cbs-build.config.yaml"),
/// };
/// assert_eq!(
///     err.to_string(),
///     "config file not found at /etc/cbs/cbs-build.config.yaml; create one with cbsbuild config init",
/// );
///
/// let err = ConfigError::MissingSchemaVersion {
///     path: Utf8PathBuf::from("/etc/cbs/cbs-build.config.yaml"),
/// };
/// assert_eq!(
///     err.to_string(),
///     "missing 'schema-version' key in /etc/cbs/cbs-build.config.yaml",
/// );
///
/// let err = ConfigError::UnknownSchemaVersion {
///     path: Utf8PathBuf::from("/etc/cbs/cbs-build.config.yaml"),
///     found: 99,
///     max_supported: 1,
/// };
/// assert_eq!(
///     err.to_string(),
///     "unsupported schema-version 99 in /etc/cbs/cbs-build.config.yaml (max supported: 1); upgrade cbscore-rs",
/// );
/// ```
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The config file does not exist at the expected path.
    #[error("config file not found at {path}; create one with cbsbuild config init")]
    NotFound {
        /// Operator-supplied or default config path that was looked up.
        path: Utf8PathBuf,
    },

    /// Generic IO failure (permission denied, IO failure mid-read, …)
    /// distinct from the structured `NotFound` case above.
    #[error("config IO failure: {source}")]
    Io {
        /// Underlying `std::io::Error`.
        #[from]
        source: std::io::Error,
    },

    /// The config file is missing the kebab-case `schema-version` key.
    #[error("missing 'schema-version' key in {path}")]
    MissingSchemaVersion {
        /// File whose schema-version marker is absent.
        path: Utf8PathBuf,
    },

    /// The config file declares a `schema-version` higher than the
    /// compiled-in maximum supported by this cbscore-rs build.
    #[error(
        "unsupported schema-version {found} in {path} (max supported: {max_supported}); upgrade cbscore-rs"
    )]
    UnknownSchemaVersion {
        /// File whose schema-version exceeds the supported range.
        path: Utf8PathBuf,
        /// schema-version value found on disk.
        found: u64,
        /// Highest schema-version this cbscore-rs build understands.
        max_supported: u64,
    },
}
