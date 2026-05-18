// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Buildah-wrapper error / value surface (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Phase 2 Commit 2
//! adds the `buildah_from` / `buildah_commit` / `buildah_unmount`
//! async wrappers in `cbscore::utils::buildah`.

pub mod errors;

pub use errors::BuildahError;
