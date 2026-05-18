// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Shared zero-IO types for the cbscore-rs Rust port.
//!
//! Provides the wire-format descriptors, config types, and error
//! taxonomy consumed by [`cbscore`], `cbsbuild`, and `cbsd-worker`.
//! Performs no IO and depends only on `serde`-derived types so the
//! dependency graph of every downstream consumer stays lean.
//!
//! [`cbscore`]: https://github.com/clyso/cbs

pub mod builder;
pub mod config;
pub mod containers;
pub mod core;
pub mod errors;
pub mod images;
pub mod logger;
pub mod releases;
pub mod runner;
pub mod utils;
pub mod versions;

// pub use errors::CbsError;
