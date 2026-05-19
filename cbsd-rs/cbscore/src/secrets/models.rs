// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Single-file YAML load helper for the wire-format
//! [`Secrets`] container.
//!
//! Wire-format types and their `VersionedSecrets` wrapper live in
//! [`cbscore_types::utils::secrets`]; this module owns the file IO
//! glue. `Secrets::load` is the private helper called by
//! [`super::mgr::SecretsMgr::load_files`] — it deliberately does
//! **not** mirror [`cbscore::config::Config::load`]'s public API
//! because secrets files are always loaded through the manager
//! (which merges multiple sources and resolves vault refs).

use camino::Utf8Path;
use cbscore_types::utils::secrets::{Secrets, SecretsError, VersionedSecrets};

/// Load a [`Secrets`] payload from a YAML file at `path`.
///
/// Parses through [`VersionedSecrets`] so the wire-format
/// `schema-version` marker (kebab per design 002 §Wire-Format
/// Versioning) is enforced; missing / unknown / malformed markers
/// surface the underlying [`SecretsError`] (currently funnelled
/// through [`SecretsError::Manager`] until the enum gains typed
/// schema-version variants in a future commit).
///
/// # Errors
///
/// Returns [`SecretsError::Manager`] on file IO failure, YAML parse
/// failure, or schema-version dispatch failure. The wrapped message
/// names the offending path.
pub(super) async fn load_secrets_file(path: &Utf8Path) -> Result<Secrets, SecretsError> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| SecretsError::Manager(format!("read secrets file '{path}': {e}")))?;
    let value: serde_value::Value = serde_saphyr::from_slice(&bytes)
        .map_err(|e| SecretsError::Manager(format!("parse YAML at '{path}': {e}")))?;
    let versioned = VersionedSecrets::from_value(value, path)
        .map_err(|e| SecretsError::Manager(format!("schema dispatch for '{path}': {e}")))?;
    Ok(versioned.into_latest())
}
