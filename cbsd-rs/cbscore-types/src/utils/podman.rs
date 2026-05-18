// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Podman-wrapper error / value surface (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Phase 2 Commit 2
//! adds the `podman_run` / `podman_stop` / `podman_pull` /
//! `podman_image_inspect` async wrappers in `cbscore::utils::podman`.

pub mod errors;

pub use errors::PodmanError;
