// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by the secrets manager.

use thiserror::Error;

/// Errors surfaced by `cbscore::secrets::SecretsMgr`.
///
/// Phase 1 lands a single `Manager` variant that wraps any
/// secrets-manager-level failure; Phase 3 Commit 3 extends the enum
/// with `VaultError`-wrapping and per-family resolution variants.
///
/// # Examples
///
/// ```
/// use cbscore_types::utils::secrets::SecretsError;
///
/// let err = SecretsError::Manager(
///     "secrets file missing required 'git' section".into(),
/// );
/// assert_eq!(
///     err.to_string(),
///     "secrets manager error: secrets file missing required 'git' section",
/// );
/// ```
#[derive(Debug, Error)]
pub enum SecretsError {
    /// Generic secrets-manager error message, pending per-stage refinement.
    #[error("secrets manager error: {0}")]
    Manager(String),
}
