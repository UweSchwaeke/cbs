// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Root cross-cutting error variants shared across subsystems.

use thiserror::Error;

/// Cross-cutting error variants that span multiple cbscore-rs subsystems.
///
/// Subsystem-specific errors (config IO, runner, builder, …) live in
/// their own per-subsystem `errors` module. This root type carries
/// only the variants that don't belong to any one subsystem.
///
/// # Examples
///
/// ```
/// use cbscore_types::errors::CbsError;
///
/// let err = CbsError::MalformedVersion("v1.foo".into());
/// assert_eq!(err.to_string(), "malformed version: v1.foo");
/// ```
#[derive(Debug, Error)]
pub enum CbsError {
    /// A version string failed to parse against the
    /// `[prefix-]vM.m.p[-suffix]` regex.
    #[error("malformed version: {0}")]
    MalformedVersion(String),

    /// The referenced version is not known to the descriptor store.
    #[error("no such version: {0}")]
    NoSuchVersion(String),

    /// The repository referenced by a component is not recognised.
    #[error("unknown repository: {0}")]
    UnknownRepository(String),
}
