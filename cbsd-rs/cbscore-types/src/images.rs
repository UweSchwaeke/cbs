// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Image-descriptor types (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Phase 5 adds the
//! sign / sync orchestrators in `cbscore::images`.

pub mod errors;

pub use errors::ImageDescriptorError;
