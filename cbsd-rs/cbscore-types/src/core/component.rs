// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbs.component.yaml` value + error surface (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Phase 5 Commit 2
//! adds the `load_components` IO function in
//! `cbscore::core::component`, plus the `CoreComponent` /
//! `CoreComponentLoc` struct definitions in this module.

pub mod errors;

pub use errors::ComponentError;
