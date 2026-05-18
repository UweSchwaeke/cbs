// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Version-descriptor types and helpers (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; later commits add
//! the `VersionDescriptor`, `VersionType`, and supporting struct
//! definitions plus the `descriptor_path` helper.

pub mod errors;

pub use errors::VersionError;
