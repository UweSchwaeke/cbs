// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Version-string helpers — port of `cbscore/versions/utils.py`.
//!
//! Submodules:
//!
//! - [`utils`] — pure-string parse family (`parse_version` and
//!   friends; Phase 2).
//! - [`desc`] — `VersionDescriptor` IO via `read_descriptor` /
//!   `write_descriptor` (Phase 4).
//! - [`create`] — `version_create_helper`, the builder that turns
//!   operator-supplied component refs into a `VersionDescriptor`
//!   (Phase 6).
//! - [`resolve`] — descriptor-store-root resolution (CLI flag, config
//!   field, then git-fallback) for `cbsbuild versions create`
//!   (seq-004); plus VERSION resolution (operator-supplied or `UUIDv7`
//!   default) for seq-005.
//!
//! [`resolve::resolve_root`], [`resolve::resolve_version`], and
//! [`utils::validate_version`] are re-exported at the module root
//! for ergonomic cross-crate access.

pub mod create;
pub mod desc;
pub mod resolve;
pub mod utils;

pub use resolve::{resolve_root, resolve_version};
pub use utils::validate_version;
