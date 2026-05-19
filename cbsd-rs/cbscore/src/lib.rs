// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! cbscore library — subsystem wrappers, build pipeline, and runner.
//!
//! Houses the IO-bearing implementations on top of [`cbscore_types`]:
//! subprocess execution ([`utils::subprocess`]), podman / buildah /
//! skopeo / git wrappers, S3 + Vault + secrets manager, config IO,
//! the podman-based runner, and the four-stage build pipeline
//! (prepare → rpmbuild → containers → signing → upload).
//!
//! Modules are added incrementally by the seq-002 plan; later phases
//! extend this crate without revisiting the top-level structure.

pub mod builder;
pub mod config;
pub mod core;
pub mod images;
pub mod runner;
pub mod secrets;
pub mod utils;
pub mod versions;
