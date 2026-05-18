// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Configuration loading and storage types (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; later commits add
//! the `Config` / `PathsConfig` / `StorageConfig` / `VaultConfig` /
//! `LoggingConfig` struct definitions.

pub mod errors;

pub use errors::ConfigError;
