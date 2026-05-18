// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Runner-subsystem types (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Phase 4 adds the
//! runner state machine in `cbscore::runner`.

pub mod errors;

pub use errors::RunnerError;
