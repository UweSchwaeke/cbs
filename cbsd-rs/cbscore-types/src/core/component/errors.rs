// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by the `cbs.component.yaml` loader.

use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors surfaced by `cbscore::core::component::load_components`.
///
/// `Yaml::message` is the stringified `serde_saphyr` parser error
/// captured at load time; the `Error::source()` chain terminates at
/// this variant because the parser type stays in the `cbscore`
/// library crate (cbscore-types is free of format crates in its
/// `[dependencies]`). Lost-source is intentional: operator-actionable
/// info survives in the stringified message.
///
/// Component-name comparison is case-sensitive: two component files
/// declaring `name: ceph` and `name: Ceph` are **distinct** components,
/// not duplicates, so `DuplicateComponentName` triggers only on exact
/// byte-equality.
///
/// # Examples
///
/// ```
/// use cbscore_types::core::component::ComponentError;
/// use camino::Utf8PathBuf;
///
/// let err = ComponentError::DuplicateComponentName {
///     name: "ceph".into(),
///     first: Utf8PathBuf::from("/components/ceph-a/cbs.component.yaml"),
///     second: Utf8PathBuf::from("/components/ceph-b/cbs.component.yaml"),
/// };
/// assert_eq!(
///     err.to_string(),
///     "duplicate component name 'ceph': first defined at /components/ceph-a/cbs.component.yaml, redefined at /components/ceph-b/cbs.component.yaml",
/// );
///
/// let err = ComponentError::MissingSchemaVersion {
///     path: Utf8PathBuf::from("/components/ceph/cbs.component.yaml"),
/// };
/// assert_eq!(
///     err.to_string(),
///     "missing 'schema-version' key in component file /components/ceph/cbs.component.yaml",
/// );
///
/// let err = ComponentError::Yaml {
///     path: Utf8PathBuf::from("/components/ceph/cbs.component.yaml"),
///     message: "expected mapping, got sequence".into(),
/// };
/// assert_eq!(
///     err.to_string(),
///     "YAML parse error in /components/ceph/cbs.component.yaml: expected mapping, got sequence",
/// );
/// ```
#[derive(Debug, Error)]
pub enum ComponentError {
    /// Directory-walk failure (permission denied, IO failure, …).
    #[error("component walk failed: {source}")]
    Walk {
        /// Underlying walker `std::io::Error`.
        #[from]
        source: std::io::Error,
    },

    /// Per-file YAML syntax or shape failure; lossy parser-error capture.
    #[error("YAML parse error in {path}: {message}")]
    Yaml {
        /// Component file whose YAML failed to parse.
        path: Utf8PathBuf,
        /// Stringified parser error (line/column / field info embedded).
        message: String,
    },

    /// The component file is missing the kebab-case `schema-version` key.
    #[error("missing 'schema-version' key in component file {path}")]
    MissingSchemaVersion {
        /// File whose schema-version marker is absent.
        path: Utf8PathBuf,
    },

    /// The component file declares a `schema-version` higher than the
    /// compiled-in maximum supported by this cbscore-rs build.
    #[error(
        "unsupported schema-version {found} in component file {path} (max supported: {max_supported}); upgrade cbscore-rs"
    )]
    UnknownSchemaVersion {
        /// File whose schema-version exceeds the supported range.
        path: Utf8PathBuf,
        /// schema-version value found on disk.
        found: u64,
        /// Highest schema-version this cbscore-rs build understands.
        max_supported: u64,
    },

    /// Two component files share the same `name:` field; the HashMap
    /// key would collide.
    #[error("duplicate component name '{name}': first defined at {first}, redefined at {second}")]
    DuplicateComponentName {
        /// Operator-chosen `name:` field that's repeated.
        name: String,
        /// File where the name was first encountered.
        first: Utf8PathBuf,
        /// File where the duplicate was found.
        second: Utf8PathBuf,
    },
}
