// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Release-descriptor types (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Phase 5 adds the
//! S3-publish orchestrator in `cbscore::releases`.

pub mod errors;

pub use errors::ReleaseError;
