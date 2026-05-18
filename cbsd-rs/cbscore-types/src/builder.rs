// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Builder-pipeline types and errors (zero IO).
//!
//! - [`report`] — [`BuildArtifactReport`] + sub-types written to
//!   `/runner/<name>.report.json` at the end of a build.
//! - [`errors`] — the [`BuilderError`] taxonomy.

pub mod errors;
pub mod report;

pub use errors::BuilderError;
pub use report::{
    BuildArtifactReport, ComponentReport, ContainerImageReport, ReleaseDescriptorReport,
};
