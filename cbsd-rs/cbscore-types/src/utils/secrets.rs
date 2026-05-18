// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Secrets manager / credential-family error / value surface (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Commit 3 of this
//! phase adds the four credential-family types
//! (`GitCreds`, `StorageCreds`, `SigningCreds`, `RegistryCreds`).
//! Phase 3 Commit 3 adds the IO-bearing `SecretsMgr` in
//! `cbscore::secrets`.

pub mod errors;

pub use errors::SecretsError;
