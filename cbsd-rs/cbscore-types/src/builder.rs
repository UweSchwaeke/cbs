// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Builder-pipeline types (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Phase 5 adds the
//! per-stage pipeline (`prepare → rpmbuild → containers → signing →
//! upload`) in `cbscore::builder`, plus the `BuildArtifactReport` type
//! in `cbscore-types::builder::report`.

pub mod errors;

pub use errors::BuilderError;
