// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Release publishing — translates a built version into the S3
//! object layout the operator's release-listing tooling consumes.
//!
//! Phase 5 Commit 6 lands the write path:
//! [`s3::upload_release`] orchestrates Phase 3 Commit 1's
//! [`utils::s3`](crate::utils::s3) primitives to upload per-component
//! RPMs and the release descriptor JSON to S3. The read path
//! (`check_release_exists`, `check_released_components` per design
//! 002 §S3 operations) lives in Phase 3 Commit 1's `utils::s3` and
//! is exposed at the CLI layer in Phase 6 — no Phase 5 caller
//! needs the read surface.
//!
//! Per design 002 §Releases & S3 lines 1106–1170: S3 keys land at
//! `s3://<bucket>/<loc>/<version>/<rpm-basename>` for RPMs and
//! `s3://<bucket>/<loc>/<version>/release.json` for the descriptor.

pub mod s3;
pub mod utils;
