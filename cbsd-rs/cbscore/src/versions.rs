// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Version-string helpers — port of `cbscore/versions/utils.py`.
//!
//! Phase 2 lands the pure-string parse family in [`utils`]; the
//! IO-bearing [`crate::utils::subprocess`]-backed wrappers are
//! elsewhere. Phase 4 adds `read_descriptor` / `write_descriptor` in
//! `cbscore::versions::desc`; Phase 6 adds `version_create_helper` in
//! `cbscore::versions::create`; seq-004 adds `resolve_root` in
//! `cbscore::versions::resolve`.

pub mod utils;
