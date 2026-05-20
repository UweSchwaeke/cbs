// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Container production — `buildah`-driven image build orchestrator.
//!
//! Phase 5 Commit 4 lands:
//!
//! - [`build`] — the [`build_image`](build::build_image) driver and
//!   the [`BuildahWorkingContainer`](build::BuildahWorkingContainer)
//!   RAII guard that ensures the live buildah working container is
//!   `buildah unmount`-ed + `buildah rm`-ed on any failure /
//!   future-drop path.
//! - [`component`] — per-container-component helpers
//!   (`ComponentContainer` shape, the apply-pre / install-packages /
//!   apply-post sub-stages live here).
//! - [`repos`] — resolution of `ContainerRepo.source` entries
//!   (`copr://` / `file://` / `http(s)://` / etc.) into the
//!   container-side artefacts the Containerfile's `dnf` lines
//!   consume.
//!
//! Phase 5 Commit 6 (`builder::upload`) consumes
//! [`ContainerImageReport`](build::ContainerImageReport) to push the
//! locally-built image to the destination registry.

pub mod build;
pub mod component;
pub mod repos;
