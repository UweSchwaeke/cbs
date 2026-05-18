// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Subprocess + secret-redaction value / error surface (zero IO).
//!
//! Phase 1 lands the error taxonomy in [`errors`]; Phase 2 Commit 1
//! adds `SecureArg`, `CmdArg`, `RunOpts`, `RunOutcome`, and
//! `async_run_cmd` in `cbscore::utils::subprocess`.

pub mod errors;

pub use errors::CommandError;
