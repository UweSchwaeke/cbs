// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Secrets manager — loads, merges, and resolves the per-family
//! credential maps (`git`, `storage`, `signing`, `registry`)
//! consumed by the build pipeline.
//!
//! The wire-format types ([`Secrets`](cbscore_types::utils::secrets::Secrets),
//! [`GitCreds`](cbscore_types::utils::secrets::GitCreds), etc.) live
//! in [`cbscore_types`]; this crate owns only the IO and orchestration
//! glue (loading YAML files, merging multiple sources, resolving
//! Vault-referenced entries to their plain form, dumping the merged
//! set to a runner-mounted tempfile).
//!
//! Phase 3 Commit 3 lands the manager + per-family helper scaffolds;
//! Phase 5 fills in the GPG keyring import, transit-key reference
//! resolution, and other deeper integrations as the builder pipeline
//! starts using them.

pub mod git;
pub mod mgr;
pub mod models;
pub mod registry;
pub mod signing;
pub mod storage;
pub mod utils;

pub use mgr::SecretsMgr;
