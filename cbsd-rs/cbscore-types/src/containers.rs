// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Container-production types (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Phase 5 adds the
//! container build / component / repo drivers in `cbscore::containers`.

pub mod errors;

pub use errors::ContainerError;
