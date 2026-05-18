// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Container-descriptor types and errors (zero IO).
//!
//! - [`desc`] — value-side types for the container descriptor YAML.
//! - [`errors`] — the [`ContainerError`] taxonomy.

pub mod desc;
pub mod errors;

pub use desc::{
    ContainerConfig, ContainerDescriptor, ContainerPackages, ContainerPackagesEntry, ContainerPre,
    ContainerRepo, ContainerScript,
};
pub use errors::ContainerError;
