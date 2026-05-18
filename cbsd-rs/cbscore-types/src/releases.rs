// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Release-descriptor types and errors (zero IO).
//!
//! - [`desc`] — value-side types ([`ReleaseDesc`], [`ReleaseComponent`],
//!   [`ReleaseBuildEntry`], [`ReleaseArtifacts`], [`BuildInfo`],
//!   [`ArchType`], [`BuildType`]).
//! - [`errors`] — the [`ReleaseError`] taxonomy.

pub mod desc;
pub mod errors;
pub mod versioned;

pub use desc::{
    ArchType, BuildInfo, BuildType, ReleaseArtifacts, ReleaseBuildEntry, ReleaseComponent,
    ReleaseDesc,
};
pub use errors::ReleaseError;
pub use versioned::{VersionedReleaseComponent, VersionedReleaseDesc};
