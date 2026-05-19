// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Core-component IO — the on-disk loader for `cbs.component.yaml`
//! files.
//!
//! Phase 5 Commit 2 lands [`component::load_components`]; this module
//! exists so the public path `cbscore::core::component` resolves.
//! Phase 5 Commit 1 (`builder::prepare`) consumed
//! `config.paths.components` as a list of search roots; once the
//! orchestrator wiring (Commit 7) adds the
//! `load_components` -> orchestrator boundary, the same map flows in
//! as a stage input.

pub mod component;
