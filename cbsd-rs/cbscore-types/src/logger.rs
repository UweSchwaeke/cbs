// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Tracing target hierarchy + [`EnvFilter`] constructor for cbscore-rs.
//!
//! This module is the canonical home of the `TARGET_*` constants every
//! cbscore-rs subsystem tags its spans/events with, the canonical
//! `FIELD_*` span-field names every subsystem populates, and
//! [`debug_filter`] which builds the [`EnvFilter`] the binary boundary
//! installs.
//!
//! The module performs no IO and never installs a global subscriber —
//! the binary (`cbsbuild`, `cbc`, …) is responsible for installing the
//! subscriber via
//! `tracing_subscriber::registry().with(<layer>).with(debug_filter()).init()`.
//!
//! Adding a new subsystem target means amending the [`TARGET_*`](#constants)
//! enumeration here; the enumeration is the canonical allowlist.

use tracing_subscriber::EnvFilter;

// --- target constants -----------------------------------------------------

/// Root tracing target for the cbscore-rs library.
pub const TARGET_CBSCORE: &str = "cbscore";

/// Config-IO subsystem (`cbscore::config::Config::{load, store}`).
pub const TARGET_CONFIG: &str = "cbscore::config";

/// `cbs.component.yaml` loader (`cbscore::core::component`).
pub const TARGET_CORE_COMPONENT: &str = "cbscore::core::component";

/// Secrets manager (`cbscore::secrets::SecretsMgr`).
pub const TARGET_SECRETS: &str = "cbscore::secrets";

/// Podman-based runner subsystem (`cbscore::runner`).
pub const TARGET_RUNNER: &str = "cbscore::runner";

/// Builder pipeline root (`cbscore::builder`).
pub const TARGET_BUILDER: &str = "cbscore::builder";

/// Prepare stage + patch walker (`cbscore::builder::prepare`).
pub const TARGET_BUILDER_PREPARE: &str = "cbscore::builder::prepare";

/// rpmbuild stage (`cbscore::builder::rpmbuild`).
pub const TARGET_BUILDER_RPMBUILD: &str = "cbscore::builder::rpmbuild";

/// RPM signing stage (`cbscore::builder::signing`).
pub const TARGET_BUILDER_SIGNING: &str = "cbscore::builder::signing";

/// S3 upload stage (`cbscore::builder::upload`).
pub const TARGET_BUILDER_UPLOAD: &str = "cbscore::builder::upload";

/// Container production subsystem (`cbscore::containers`).
pub const TARGET_CONTAINERS: &str = "cbscore::containers";

/// Skopeo driver (`cbscore::images::skopeo`).
pub const TARGET_IMAGES_SKOPEO: &str = "cbscore::images::skopeo";

/// Image manifest signing (`cbscore::images::signing`).
pub const TARGET_IMAGES_SIGNING: &str = "cbscore::images::signing";

/// Image copy + sign orchestration (`cbscore::images::sync`).
pub const TARGET_IMAGES_SYNC: &str = "cbscore::images::sync";

/// S3 release publishing (`cbscore::releases`).
pub const TARGET_RELEASES: &str = "cbscore::releases";

/// Buildah wrapper (`cbscore::utils::buildah`).
pub const TARGET_UTILS_BUILDAH: &str = "cbscore::utils::buildah";

/// Git wrapper (`cbscore::utils::git`).
pub const TARGET_UTILS_GIT: &str = "cbscore::utils::git";

/// Podman wrapper (`cbscore::utils::podman`).
pub const TARGET_UTILS_PODMAN: &str = "cbscore::utils::podman";

/// S3 wrapper (`cbscore::utils::s3`).
pub const TARGET_UTILS_S3: &str = "cbscore::utils::s3";

/// Subprocess driver (`cbscore::utils::subprocess`).
pub const TARGET_UTILS_SUBPROCESS: &str = "cbscore::utils::subprocess";

/// Vault wrapper (`cbscore::utils::vault`).
pub const TARGET_UTILS_VAULT: &str = "cbscore::utils::vault";

/// Version helpers + seq-004 resolver (`cbscore::versions`).
pub const TARGET_VERSIONS: &str = "cbscore::versions";

// --- canonical span-field names ------------------------------------------

/// Cross-process build-correlation UUID (literal `"none"` for standalone CLI).
pub const FIELD_TRACE_ID: &str = "trace_id";

/// UUID identifying the cbsd-worker build that drives the cbscore-rs pass.
pub const FIELD_BUILD_ID: &str = "build_id";

/// Component name on per-component spans inside the builder pipeline.
pub const FIELD_COMPONENT: &str = "component";

/// Builder-pipeline stage (`"prepare"`, `"rpmbuild"`, `"containers"`,
/// `"signing"`, `"upload"`).
pub const FIELD_STAGE: &str = "stage";

/// File path on per-file warnings or errors (e.g. component-loader WARN events).
pub const FIELD_PATH: &str = "path";

// --- filter constructor --------------------------------------------------

/// Build the [`EnvFilter`] for cbscore-rs's tracing subscriber.
///
/// Resolution order:
/// 1. `RUST_LOG`, if set and parseable, wins.
/// 2. Otherwise, if `CBS_DEBUG` is set in the environment, the default
///    directive is `cbscore=debug`.
/// 3. Otherwise, the default directive is `cbscore=info`.
///
/// The returned filter is **not** installed — the binary boundary owns
/// subscriber installation via
/// `tracing_subscriber::registry().with(<layer>).with(debug_filter()).init()`.
/// Returning rather than installing keeps `cbscore-types` free of
/// global-state mutation and runs the env-var read at the binary's
/// explicit invocation rather than at module load.
///
/// # Examples
///
/// ```
/// use cbscore_types::logger::debug_filter;
///
/// let _filter = debug_filter();
/// ```
#[must_use]
pub fn debug_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let default = if std::env::var_os("CBS_DEBUG").is_some() {
            "cbscore=debug"
        } else {
            "cbscore=info"
        };
        EnvFilter::new(default)
    })
}
