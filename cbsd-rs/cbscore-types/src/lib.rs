// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Shared zero-IO types for the cbscore-rs Rust port.
//!
//! Provides the wire-format descriptors, config types, and error
//! taxonomy consumed by [`cbscore`], [`cbsbuild`], and `cbsd-worker`.
//! Performs no IO and depends only on `serde`-derived types so the
//! dependency graph of every downstream consumer stays lean.
//!
//! Modules are added incrementally by the seq-002 plan; this initial
//! commit ships an empty crate root so the workspace compiles end to
//! end before the type and error surface lands in later commits.
