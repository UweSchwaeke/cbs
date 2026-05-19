// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Subsystem wrappers — async drivers for external binaries (podman,
//! buildah, skopeo, git) on top of the [`subprocess`] foundation.
//!
//! Phase 2 lands `subprocess` (foundation) plus the per-binary
//! wrappers (`podman`, `buildah`, `git`) and the skopeo driver in
//! [`crate::images`]. Phase 3 adds the S3 and Vault wrappers
//! (`utils::s3`, `utils::vault`) and the secrets manager.

pub mod buildah;
pub mod git;
pub mod podman;
pub mod s3;
pub mod subprocess;
pub mod vault;
