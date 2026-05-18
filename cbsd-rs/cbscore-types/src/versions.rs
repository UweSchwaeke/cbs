// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Version-descriptor types and helpers (zero IO).
//!
//! - [`desc`] — value-side types for `<root>/<type>/<VERSION>.json`.
//! - [`utils`] — pure helpers ([`VersionType`] enum here; the
//!   `regex`-based parse family lives in `cbscore::versions::utils`).
//! - [`errors`] — the [`VersionError`] taxonomy.

pub mod desc;
pub mod errors;
pub mod utils;
pub mod versioned;

pub use desc::{VersionComponent, VersionDescriptor, VersionImage, VersionSignedOffBy};
pub use errors::VersionError;
pub use utils::VersionType;
pub use versioned::VersionedVersionDescriptor;
