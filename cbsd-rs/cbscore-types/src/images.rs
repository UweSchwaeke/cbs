// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Image-descriptor types and errors (zero IO).
//!
//! - [`desc`] — value-side types ([`ImageDescriptor`], [`ImageLocations`]).
//! - [`errors`] — the [`ImageDescriptorError`] taxonomy.

pub mod desc;
pub mod errors;

pub use desc::{ImageDescriptor, ImageLocations};
pub use errors::ImageDescriptorError;
