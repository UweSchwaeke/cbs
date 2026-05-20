// Copyright (C) 2026  Clyso
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

//! In-process build executor — dispatches a `BuildDescriptor` to
//! the cbscore library's `runner::run` entry point.
//!
//! Phase 7 Commit 1b's cutover: replaces the Python subprocess
//! bridge (`tokio::process::Command::new("python3")
//! .arg("cbscore-wrapper.py")…`) with a direct call into
//! [`cbscore::runner::run`]. Tracing emissions from cbscore are
//! captured via the per-build [`super::dispatch::BuildDispatchLayer`]
//! and routed to the [`super::output::run_batcher`] task that
//! emits `WorkerMessage::BuildOutput` frames over the WebSocket.
//!
//! Cancellation: dropping the [`run_in_process`] future (via
//! `JoinHandle::abort` from the WS handler on a `BuildRevoke`)
//! triggers Phase 4 Commit 3's `runner::run` cleanup chain — the
//! podman child is killed synchronously inside the RAII drop
//! guard, no SIGTERM → SIGKILL escalation budget is required.

use std::collections::HashMap;
use std::path::Path;

use cbscore::secrets::SecretsMgr;
use cbscore::versions::create::{VersionCreateInput, version_create_helper};
use cbscore_types::versions::desc::VersionSignedOffBy;
use cbscore_types::versions::utils::VersionType;
use cbsd_proto::build::{BuildDescriptor, BuildId};
use cbsd_proto::ws::BuildFinishedStatus;
use tracing::Instrument;

use crate::config::ResolvedWorkerConfig;

/// Maximum size (in bytes) of the serialised `build_report` JSON.
/// Reports exceeding this limit are logged and discarded — matches
/// the pre-cutover cap in the retired `output.rs::stream_output`.
const MAX_REPORT_SIZE: usize = 65_536;

/// Outcome of [`run_in_process`] — the data the WS handler needs to
/// build the `WorkerMessage::BuildFinished` frame.
#[derive(Debug)]
pub(crate) struct RunOutcome {
    pub status: BuildFinishedStatus,
    pub error: Option<String>,
    pub build_report: Option<serde_json::Value>,
}

/// Errors from [`run_in_process`].
#[derive(Debug)]
pub(crate) enum ExecutorError {
    /// A required config field is missing.
    MissingConfig(&'static str),
    /// The descriptor carried a malformed `version_type` field.
    InvalidVersionType(String),
    /// cbscore returned an error during the build pipeline.
    Cbscore(String),
    /// `BuildDescriptor.build.os_version` was not in `elN` form.
    InvalidOsVersion(String),
}

impl std::fmt::Display for ExecutorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingConfig(field) => write!(f, "missing required config: {field}"),
            Self::InvalidVersionType(s) => write!(f, "invalid version_type '{s}'"),
            Self::Cbscore(msg) => write!(f, "cbscore: {msg}"),
            Self::InvalidOsVersion(s) => write!(f, "invalid os_version '{s}' (expected 'elN')"),
        }
    }
}

impl std::error::Error for ExecutorError {}

/// Dispatch a build to cbscore in-process. Returns the
/// [`RunOutcome`] the WS handler converts into a
/// `WorkerMessage::BuildFinished` frame.
///
/// The `instrument(span)` wrap is **mandatory**: the
/// [`super::dispatch::BuildDispatchLayer`] keys its per-build
/// channel lookup on the `build_id` field of the current span
/// chain. Without `instrument`, the events would have no parent
/// span, the layer would find no `build_id`, and every tracing
/// emission inside cbscore would be silently dropped.
pub(crate) async fn run_in_process(
    config: &ResolvedWorkerConfig,
    build_id: BuildId,
    descriptor: &BuildDescriptor,
    component_path: &Path,
    trace_id: &str,
) -> Result<RunOutcome, ExecutorError> {
    let span = tracing::info_span!(
        target: "cbscore",
        "cbsd_build",
        build_id = build_id.0,
        trace_id = trace_id,
    );

    async move {
        match run_inner(config, descriptor, component_path).await {
            Ok(report) => Ok(report),
            Err(err) => {
                tracing::error!(error = %err, "build failed");
                Err(err)
            }
        }
    }
    .instrument(span)
    .await
}

async fn run_inner(
    config: &ResolvedWorkerConfig,
    descriptor: &BuildDescriptor,
    component_path: &Path,
) -> Result<RunOutcome, ExecutorError> {
    // 1. Load cbscore Config + SecretsMgr.
    let cbscore_config_path = config
        .cbscore_config_path
        .as_ref()
        .ok_or(ExecutorError::MissingConfig("cbscore-config-path"))?;
    let cbscore_config_path = camino::Utf8PathBuf::from_path_buf(cbscore_config_path.clone())
        .map_err(|p| {
            ExecutorError::MissingConfig(
                "cbscore-config-path: non-UTF8 path (Box::leak to keep &'static)",
            )
            .with_path_hint(&p)
        })?;
    let mut cbs_config = cbscore::config::load(&cbscore_config_path)
        .await
        .map_err(|e| ExecutorError::Cbscore(format!("loading cbscore config: {e}")))?;

    // Override the components path to the unpacked tarball dir;
    // matches the Python wrapper's
    // `config.paths.components = [Path(component_path)]` step.
    let component_path_utf8 = camino::Utf8PathBuf::from_path_buf(component_path.to_owned())
        .map_err(|p| {
            ExecutorError::Cbscore(format!("non-UTF8 component_path '{}'", p.display()))
        })?;
    cbs_config.paths.components = vec![component_path_utf8];

    let secrets = SecretsMgr::load_files(&cbs_config.secrets)
        .await
        .map_err(|e| ExecutorError::Cbscore(format!("loading secrets: {e}")))?;

    // 2. Translate BuildDescriptor → VersionDescriptor via
    //    version_create_helper (Phase 6 Commit 2). Mirrors the
    //    Python wrapper's same call.
    let storage = cbs_config
        .storage
        .as_ref()
        .ok_or(ExecutorError::MissingConfig("config.storage"))?;
    let registry = storage
        .registry
        .as_ref()
        .ok_or(ExecutorError::MissingConfig("config.storage.registry"))?;

    let el_version = parse_el_version(&descriptor.build.os_version)?;
    let version_type = descriptor
        .version_type
        .as_ref()
        .map(version_type_to_cbscore)
        .ok_or_else(|| {
            ExecutorError::InvalidVersionType("descriptor.version_type is absent".into())
        })?;

    let mut component_refs: Vec<(String, String)> = Vec::with_capacity(descriptor.components.len());
    let mut component_repos: HashMap<String, String> = HashMap::new();
    for c in &descriptor.components {
        component_refs.push((c.name.clone(), c.git_ref.clone()));
        if let Some(repo) = &c.repo {
            component_repos.insert(c.name.clone(), repo.clone());
        }
        // When repo is None, version_create_helper requires a
        // component_repos entry — fall back to the components-dir
        // lookup. The Phase 6 Commit 2 helper checks the map first;
        // a missing entry surfaces as an explicit error there.
    }

    let input = VersionCreateInput {
        version: descriptor.version.clone(),
        version_type,
        component_refs,
        component_repos,
        signed_off_by: VersionSignedOffBy {
            user: descriptor.signed_off_by.user.clone(),
            email: descriptor.signed_off_by.email.clone(),
        },
        registry: registry.url.clone(),
        image_name: descriptor.dst_image.name.clone(),
        image_tag: Some(descriptor.dst_image.tag.clone()),
        distro: descriptor.build.distro.clone(),
        el_version,
    };

    let version_desc = version_create_helper(&input)
        .await
        .map_err(|e| ExecutorError::Cbscore(format!("version_create_helper: {e}")))?;

    // 3. Dispatch to cbscore::runner::run.
    let image_ref = format!(
        "{}/{}:{}",
        version_desc.image.registry, version_desc.image.name, version_desc.image.tag,
    );
    let opts = cbscore::runner::RunOpts {
        image_ref,
        user_args: Vec::new(),
        trace_id: None,
        ..cbscore::runner::RunOpts::default()
    };

    let run_report = cbscore::runner::run(&version_desc, &cbs_config, &secrets, &opts)
        .await
        .map_err(|e| ExecutorError::Cbscore(format!("runner::run: {e}")))?;

    // 4. Map cbscore RunReport → RunOutcome.
    let status = match run_report.exit_code {
        0 => BuildFinishedStatus::Success,
        _ => BuildFinishedStatus::Failure,
    };
    let mut build_report = run_report.build_report;
    if let Some(report) = build_report.as_ref() {
        let size = serde_json::to_string(report).map(|s| s.len()).unwrap_or(0);
        if size > MAX_REPORT_SIZE {
            tracing::warn!(
                size,
                limit = MAX_REPORT_SIZE,
                "build report exceeds 64 KB, discarding",
            );
            build_report = None;
        }
    }
    let error = if status == BuildFinishedStatus::Success {
        None
    } else {
        Some(format!("build exited with code {}", run_report.exit_code))
    };

    Ok(RunOutcome {
        status,
        error,
        build_report,
    })
}

/// Parse `elN` → integer N (e.g. `el9` → `9`).
fn parse_el_version(os_version: &str) -> Result<u32, ExecutorError> {
    os_version
        .strip_prefix("el")
        .and_then(|n| n.parse::<u32>().ok())
        .ok_or_else(|| ExecutorError::InvalidOsVersion(os_version.to_owned()))
}

/// Translate the cbsd-proto `VersionType` into the cbscore-types
/// `VersionType` (they're parallel enums; cbsd-proto's flavor
/// targets wire stability and cbscore-types' targets the cbscore
/// library API).
fn version_type_to_cbscore(t: &cbsd_proto::build::VersionType) -> VersionType {
    match t {
        cbsd_proto::build::VersionType::Dev => VersionType::Dev,
        cbsd_proto::build::VersionType::Test => VersionType::Test,
        cbsd_proto::build::VersionType::Ci => VersionType::Ci,
        cbsd_proto::build::VersionType::Release => VersionType::Release,
    }
}

impl ExecutorError {
    /// Internal helper used by the non-UTF8 path conversion; lets
    /// the call site retain the offending path for the operator
    /// diagnostic without leaking a `'static` allocation.
    fn with_path_hint(self, p: &std::path::Path) -> Self {
        Self::Cbscore(format!("{self}: '{}'", p.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_el_version_basic() {
        assert_eq!(parse_el_version("el9").unwrap(), 9);
        assert_eq!(parse_el_version("el10").unwrap(), 10);
    }

    #[test]
    fn parse_el_version_rejects_malformed() {
        assert!(parse_el_version("rhel9").is_err());
        assert!(parse_el_version("el").is_err());
        assert!(parse_el_version("elx").is_err());
        assert!(parse_el_version("").is_err());
    }

    #[test]
    fn version_type_translation_full_coverage() {
        assert_eq!(
            version_type_to_cbscore(&cbsd_proto::build::VersionType::Dev),
            VersionType::Dev,
        );
        assert_eq!(
            version_type_to_cbscore(&cbsd_proto::build::VersionType::Test),
            VersionType::Test,
        );
        assert_eq!(
            version_type_to_cbscore(&cbsd_proto::build::VersionType::Ci),
            VersionType::Ci,
        );
        assert_eq!(
            version_type_to_cbscore(&cbsd_proto::build::VersionType::Release),
            VersionType::Release,
        );
    }
}
