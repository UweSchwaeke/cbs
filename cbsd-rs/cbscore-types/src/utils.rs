// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Subsystem-wrapper error / value surface (zero IO).
//!
//! Phase 1 lands the error taxonomy for each wrapper; Phase 2 adds the
//! IO-bearing wrappers themselves in `cbscore::utils`. The
//! secrets / signing / registry submodules add their data types in
//! Phase 1 Commit 3.

pub mod buildah;
pub mod podman;
pub mod secrets;
pub mod subprocess;
