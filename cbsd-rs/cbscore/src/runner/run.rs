// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Runner pipeline — state machine + podman invocation.
//!
//! Implements the full lifecycle of a single build container per
//! design 002 §Runner Subsystem (lines 741–864):
//!
//! 1. **Preparing.** Generate a run name, create the per-run
//!    tempfiles (descriptor, config, secrets) and the components
//!    aggregate tempdir, build the mount table.
//! 2. **Spawning.** Assemble the podman command via
//!    [`PodmanRunOpts`] and dispatch through
//!    [`podman_run`](crate::utils::podman::podman_run).
//! 3. **Running.** The podman child is driven by the Phase 2
//!    [`async_run_cmd`](crate::utils::subprocess::async_run_cmd)
//!    RAII guard, wrapped here by an outer
//!    [`tokio::time::timeout`] (4 hours by default) plus a
//!    [`tokio::select!`] that listens for SIGTERM.
//! 4. **Cleanup.** Two-tier: an explicit `async fn cleanup` runs at
//!    every normal return path (success, expected error, timeout,
//!    SIGTERM) and removes tempfiles + the components tempdir; an
//!    RAII guard's `Drop` impl runs the same removals
//!    synchronously on panic / future drop. A shared
//!    `Arc<AtomicBool>` coordinates the SIGTERM handler with the
//!    async cleanup so `podman_stop` is never called twice.
//!
//! The mount layout matches design 002 §Runner Subsystem (lines
//! 779–789) bit-for-bit; see [`run`] for the canonical table.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::config::Config;
use cbscore_types::runner::RunnerError;
use cbscore_types::versions::VersionDescriptor;
use tempfile::TempDir;
use uuid::Uuid;

use crate::config as config_io;
use crate::secrets::SecretsMgr;
use crate::utils::podman::{PodmanRunOpts, podman_run, podman_stop};
use crate::versions::desc::write_descriptor;

const TARGET_RUNNER_RUN: &str = "cbscore::runner::run";

/// Default outer-timeout for [`run`] — 4 hours, matching
/// `cbscore/runner.py:runner(timeout=4 * 3600, …)` and design 002
/// line 849.
pub const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(4 * 3600);

/// In-container path under which podman mounts the
/// `cbsbuild` binary (PID 1).
const IN_CONTAINER_CBSBUILD: &str = "/runner/cbsbuild";

/// In-container working directory pin.
const IN_CONTAINER_WORKDIR: &str = "/runner";

/// Caller-supplied options for [`run`].
///
/// The `trace_id` carries the cross-process correlation UUID
/// populated by `cbsd-worker` (Phase 7) when invoking the runner;
/// `None` for standalone `cbsbuild` invocations. The runner's
/// top-level `tracing::span!` carries `trace_id` as a structured
/// field — rendered as the UUID string when `Some`, or the
/// literal `"none"` when `None`. Consistent field-name policy
/// across standalone CLI and worker contexts.
#[derive(Debug, Clone)]
pub struct RunOpts {
    /// Outer runner-level timeout. Default: [`DEFAULT_RUN_TIMEOUT`].
    pub timeout: Duration,
    /// Passthrough args appended after `cbsbuild runner build`.
    /// Unvalidated — operator's escape hatch for flags cbscore-rs
    /// doesn't model. Unrecognised flags fail at the in-container
    /// clap parser.
    pub user_args: Vec<String>,
    /// Container image reference (the builder image — typically the
    /// `el9` cbscore builder image).
    pub image_ref: String,
    /// Optional cross-process trace correlation UUID.
    pub trace_id: Option<Uuid>,
}

impl Default for RunOpts {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_RUN_TIMEOUT,
            user_args: Vec::new(),
            image_ref: String::new(),
            trace_id: None,
        }
    }
}

/// Outcome from a completed [`run`].
///
/// `build_report` is read from the in-container
/// `BuildArtifactReport` JSON file at
/// `{config.paths.scratch}/build-report.json` after the container
/// exits — the in-container build writes it to
/// `/runner/scratch/build-report.json` which the runner's mount
/// table aliases to the host's `config.paths.scratch` directory,
/// so no separate file mount is required to surface it.
#[derive(Debug, Clone)]
pub struct RunReport {
    /// Container name (the `gen_run_name`-generated ID).
    pub container_name: String,
    /// Container exit code (0 = success).
    pub exit_code: i32,
    /// Parsed build artifact report, if the in-container build
    /// produced one and it was readable from the host.
    pub build_report: Option<serde_json::Value>,
}

/// RAII guard that owns the per-run tempfile paths + components
/// tempdir + container name. Its `Drop` impl runs sync best-effort
/// cleanup on panic or future drop; the explicit `async fn cleanup`
/// handles every normal return path.
///
/// The guard's `inner: Option<…>` shape lets [`cleanup`] take
/// ownership of the fields via `take()` so subsequent panics inside
/// `cleanup` do not re-trigger the `Drop` fallback.
struct CleanupGuard {
    inner: Option<CleanupState>,
}

struct CleanupState {
    descriptor_path: Utf8PathBuf,
    config_path: Utf8PathBuf,
    secrets_path: Utf8PathBuf,
    components_dir: TempDir,
    container_name: String,
    cleanup_flag: Arc<AtomicBool>,
}

impl CleanupGuard {
    const fn new(state: CleanupState) -> Self {
        Self { inner: Some(state) }
    }

    /// Take ownership of the cleanup state, defusing the
    /// `Drop`-side fallback. Subsequent panics inside the async
    /// cleanup path no longer re-trigger the sync fallback.
    fn defuse(mut self) -> Option<CleanupState> {
        self.inner.take()
    }
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let Some(state) = self.inner.take() else {
            return;
        };
        // Mark cleanup-in-progress so any concurrent SIGTERM
        // handler skips its own podman_stop call.
        state.cleanup_flag.store(true, Ordering::Release);
        // Sync best-effort fallback path (panic / future drop) —
        // every error is swallowed; this is a last-ditch attempt.
        let _ = std::fs::remove_file(state.descriptor_path.as_std_path());
        let _ = std::fs::remove_file(state.config_path.as_std_path());
        let _ = std::fs::remove_file(state.secrets_path.as_std_path());
        // The `TempDir` Drop impl removes the directory itself.
        drop(state.components_dir);
        // Fire-and-forget `podman stop` via a blocking subprocess
        // — we cannot await here, so use std::process::Command.
        let _ = std::process::Command::new("podman")
            .args(["stop", "--time", "1", &state.container_name])
            .status();
    }
}

/// Run a single build pass: spawn the podman container, drive it
/// to completion, collect the artifact report, and clean up.
///
/// # Errors
///
/// Returns [`RunnerError::BinaryNotFound`] when
/// `std::env::current_exe()` fails (the runner cannot mount
/// itself into the container without a host-side path).
///
/// Returns [`RunnerError::Timeout`] when the outer-timeout fires.
///
/// Returns [`RunnerError::Cancelled`] when SIGTERM is delivered to
/// the host runner process.
///
/// Returns [`RunnerError::Podman`] / [`RunnerError::Command`] on
/// container-spawn failure, non-zero exit, or subprocess driver
/// failure.
///
/// On error: the explicit `async cleanup` runs at the failure path,
/// removing every tempfile and stopping the container. The error
/// is then returned to the caller.
///
/// On success: returns a [`RunReport`] populated with the
/// container name, exit code, and the parsed build artifact report
/// (or `None` if the in-container build did not produce one).
///
/// # Mount layout
///
/// | Host path                              | Mount point                                |
/// | -------------------------------------- | ------------------------------------------ |
/// | tempfile `descriptor.json`             | `/runner/<name>.json`                      |
/// | `cbsbuild` binary (self)               | `/runner/cbsbuild` (PID 1)                 |
/// | tempfile config                        | `/runner/cbs-build.config.yaml`            |
/// | tempfile secrets                       | `/runner/cbs-build.secrets.yaml`           |
/// | `config.vault` path (if set)           | `/runner/cbs-build.vault.yaml`             |
/// | `config.paths.scratch`                 | `/runner/scratch`                          |
/// | `config.paths.scratch_containers`      | `/var/lib/containers:Z`                    |
/// | components aggregate tempdir           | `/runner/components`                       |
/// | `config.paths.ccache` (if set)         | `/runner/ccache`                           |
///
/// # Examples
///
/// ```no_run
/// use cbscore::runner::{run, RunOpts};
/// use cbscore::secrets::SecretsMgr;
/// use cbscore_types::config::Config;
/// use cbscore_types::versions::VersionDescriptor;
///
/// # async fn demo(
/// #     desc: &VersionDescriptor,
/// #     cfg: &Config,
/// #     secrets: &SecretsMgr,
/// # ) -> Result<(), cbscore_types::runner::RunnerError> {
/// let opts = RunOpts {
///     image_ref: "registry.example.com/ceph-builder:el9".into(),
///     ..Default::default()
/// };
/// let report = run(desc, cfg, secrets, &opts).await?;
/// assert_eq!(report.exit_code, 0);
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::runner::run",
    skip(desc, config, secrets),
    fields(
        version = %desc.version,
        trace_id = %trace_id_field(opts.trace_id.as_ref()),
    ),
)]
pub async fn run(
    desc: &VersionDescriptor,
    config: &Config,
    secrets: &SecretsMgr,
    opts: &RunOpts,
) -> Result<RunReport, RunnerError> {
    let Prepared {
        container_name,
        descriptor_path,
        descriptor_basename,
        config_path,
        secrets_path,
        components_dir,
        components_dir_path,
        cbsbuild_self_path,
    } = prepare(desc, config, secrets).await?;

    let podman_opts = build_podman_opts(
        &PodmanOptsInput {
            container_name: &container_name,
            descriptor_path: &descriptor_path,
            descriptor_basename: &descriptor_basename,
            config_path: &config_path,
            secrets_path: &secrets_path,
            components_dir_path: &components_dir_path,
            cbsbuild_self_path: &cbsbuild_self_path,
        },
        config,
        opts,
    );

    let cleanup_flag = Arc::new(AtomicBool::new(false));
    let guard = CleanupGuard::new(CleanupState {
        descriptor_path,
        config_path,
        secrets_path,
        components_dir,
        container_name: container_name.clone(),
        cleanup_flag: Arc::clone(&cleanup_flag),
    });

    let run_future = drive_run(podman_opts, container_name.clone());

    let outcome = run_with_signal_and_timeout(run_future, opts.timeout).await;

    // Read the in-container build report from the host-side scratch
    // mount BEFORE running cleanup — cleanup removes the per-run
    // tempfiles but leaves config.paths.scratch alone.
    let report_path = config.paths.scratch.join("build-report.json");
    let build_report = read_build_report(&report_path).await;

    // Defuse the guard and run the explicit async cleanup.
    if let Some(state) = guard.defuse()
        && let Err(e) = cleanup(state, cleanup_flag).await
    {
        // Per the plan: log cleanup failure when the run-stage
        // already failed; surface cleanup failure when the run
        // stage succeeded.
        if outcome.is_err() {
            tracing::warn!(
                target: TARGET_RUNNER_RUN,
                error = %e,
                "cleanup failed after run-stage error",
            );
        } else {
            return Err(e);
        }
    }

    let exit_code = outcome?;
    Ok(RunReport {
        container_name,
        exit_code,
        build_report,
    })
}

/// Helper for the [`tracing::instrument`] `fields` clause —
/// renders the optional UUID as a hyphenated string or
/// the literal `"none"`.
fn trace_id_field(id: Option<&Uuid>) -> String {
    id.map_or_else(|| "none".to_string(), ToString::to_string)
}

// ---------------------------------------------------------------------
// Preparing — per-run tempfiles + components dir
// ---------------------------------------------------------------------

struct Prepared {
    container_name: String,
    descriptor_path: Utf8PathBuf,
    descriptor_basename: String,
    config_path: Utf8PathBuf,
    secrets_path: Utf8PathBuf,
    components_dir: TempDir,
    components_dir_path: Utf8PathBuf,
    cbsbuild_self_path: Utf8PathBuf,
}

async fn prepare(
    desc: &VersionDescriptor,
    config: &Config,
    secrets: &SecretsMgr,
) -> Result<Prepared, RunnerError> {
    let container_name = super::gen_run_name(None);
    let descriptor_basename = format!("{}.json", desc.version);

    // Create one tempdir to host the three per-run tempfiles + the
    // components aggregate dir (each gets its own sub-tempdir).
    let staging = TempDir::new().map_err(|e| RunnerError::BinaryNotFound { source: e })?;
    let staging_path = Utf8PathBuf::from_path_buf(staging.path().to_owned()).map_err(|p| {
        RunnerError::BinaryNotFound {
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("non-UTF8 staging dir: {}", p.display()),
            ),
        }
    })?;

    let descriptor_path = staging_path.join(&descriptor_basename);
    write_descriptor(desc, &descriptor_path)
        .await
        .map_err(|e| {
            // Map VersionError to RunnerError::Command via a synthetic
            // CommandError; the wrapped message preserves the path.
            RunnerError::Command(cbscore_types::utils::subprocess::CommandError::Io {
                source: std::io::Error::other(e.to_string()),
            })
        })?;

    let config_path = staging_path.join("cbs-build.config.yaml");
    let in_container_config = container_facing_config(config);
    config_io::store(&in_container_config, &config_path)
        .await
        .map_err(|e| {
            RunnerError::Command(cbscore_types::utils::subprocess::CommandError::Io {
                source: std::io::Error::other(e.to_string()),
            })
        })?;

    let secrets_path = staging_path.join("cbs-build.secrets.yaml");
    secrets.dump_to_runner(&secrets_path).await.map_err(|e| {
        RunnerError::Command(cbscore_types::utils::subprocess::CommandError::Io {
            source: std::io::Error::other(e.to_string()),
        })
    })?;

    let components_dir = build_components_dir(&config.paths.components).await?;
    let components_dir_path = Utf8PathBuf::from_path_buf(components_dir.path().to_owned())
        .map_err(|p| RunnerError::BinaryNotFound {
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("non-UTF8 components dir: {}", p.display()),
            ),
        })?;

    // The staging tempdir's lifetime must outlive the per-run
    // tempfiles. Forget it intentionally — the per-run cleanup paths
    // remove each tempfile individually; the staging dir itself
    // leaks (small `/tmp/.tmpXXXXXX/`) until the process exits or
    // the OS reaps it. Acceptable tradeoff: forcing the staging dir
    // to live in `CleanupGuard` would balloon the guard's `Drop`
    // surface; the per-file removal contract is more precise.
    std::mem::forget(staging);

    let cbsbuild_self_path = std::env::current_exe()
        .map_err(|e| RunnerError::BinaryNotFound { source: e })
        .and_then(|p| {
            Utf8PathBuf::from_path_buf(p).map_err(|q| RunnerError::BinaryNotFound {
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("non-UTF8 cbsbuild path: {}", q.display()),
                ),
            })
        })?;

    Ok(Prepared {
        container_name,
        descriptor_path,
        descriptor_basename,
        config_path,
        secrets_path,
        components_dir,
        components_dir_path,
        cbsbuild_self_path,
    })
}

/// Build the in-container `Config` from the host-side `Config` by
/// remapping every path field to its in-container mount point.
fn container_facing_config(host: &Config) -> Config {
    let mut new_cfg = host.clone();
    new_cfg.paths.scratch = "/runner/scratch".into();
    new_cfg.paths.scratch_containers = "/var/lib/containers".into();
    new_cfg.paths.components = vec!["/runner/components".into()];
    new_cfg.paths.ccache = host.paths.ccache.as_ref().map(|_| "/runner/ccache".into());
    new_cfg.secrets = vec!["/runner/cbs-build.secrets.yaml".into()];
    new_cfg.vault = host
        .vault
        .as_ref()
        .map(|_| "/runner/cbs-build.vault.yaml".into());
    new_cfg
}

/// Walk each component-root in `paths` and copy every top-level
/// subdirectory into a fresh tempdir under its leaf name.
async fn build_components_dir(paths: &[Utf8PathBuf]) -> Result<TempDir, RunnerError> {
    let paths = paths.to_vec();
    tokio::task::spawn_blocking(move || -> std::io::Result<TempDir> {
        let staging = tempfile::Builder::new()
            .prefix("cbs-components-")
            .tempdir()?;
        for component_root in &paths {
            let entries = match std::fs::read_dir(component_root.as_std_path()) {
                Ok(it) => it,
                Err(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(io_err) => return Err(io_err),
            };
            for entry in entries {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let dst_path = staging.path().join(entry.file_name());
                copy_dir_recursive(&entry.path(), &dst_path)?;
            }
        }
        Ok(staging)
    })
    .await
    .map_err(|e| {
        RunnerError::Command(cbscore_types::utils::subprocess::CommandError::Io {
            source: std::io::Error::other(format!("join error: {e}")),
        })
    })?
    .map_err(|e| {
        RunnerError::Command(cbscore_types::utils::subprocess::CommandError::Io { source: e })
    })
}

/// Recursive directory copy — sync (called from `spawn_blocking`).
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &dst_path)?;
        }
        // Symlinks and other special files are skipped (matching
        // Python's shutil.copytree default).
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Spawning — assemble PodmanRunOpts
// ---------------------------------------------------------------------

/// Inputs to [`build_podman_opts`] — borrowed references to the
/// fields of [`Prepared`] so the destructured caller can hold
/// ownership while we assemble the options.
struct PodmanOptsInput<'a> {
    container_name: &'a str,
    descriptor_path: &'a Utf8Path,
    descriptor_basename: &'a str,
    config_path: &'a Utf8Path,
    secrets_path: &'a Utf8Path,
    components_dir_path: &'a Utf8Path,
    cbsbuild_self_path: &'a Utf8Path,
}

fn build_podman_opts(
    input: &PodmanOptsInput<'_>,
    config: &Config,
    opts: &RunOpts,
) -> PodmanRunOpts<'static> {
    let descriptor_mount = format!("/runner/{}", input.descriptor_basename);

    let mut volumes: HashMap<Utf8PathBuf, String> = HashMap::new();
    volumes.insert(input.descriptor_path.to_owned(), descriptor_mount.clone());
    volumes.insert(
        input.cbsbuild_self_path.to_owned(),
        IN_CONTAINER_CBSBUILD.into(),
    );
    volumes.insert(
        input.config_path.to_owned(),
        "/runner/cbs-build.config.yaml".into(),
    );
    volumes.insert(
        input.secrets_path.to_owned(),
        "/runner/cbs-build.secrets.yaml".into(),
    );
    if let Some(vault_path) = &config.vault {
        volumes.insert(vault_path.clone(), "/runner/cbs-build.vault.yaml".into());
    }
    volumes.insert(config.paths.scratch.clone(), "/runner/scratch".into());
    volumes.insert(
        config.paths.scratch_containers.clone(),
        "/var/lib/containers:Z".into(),
    );
    volumes.insert(
        input.components_dir_path.to_owned(),
        "/runner/components".into(),
    );
    if let Some(ccache) = &config.paths.ccache {
        volumes.insert(ccache.clone(), "/runner/ccache".into());
    }

    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("HOME".into(), "/runner".into());
    env.insert(
        "CBS_DEBUG".into(),
        std::env::var("CBS_DEBUG").unwrap_or_default(),
    );

    let mut args = vec![
        "--config".into(),
        "/runner/cbs-build.config.yaml".into(),
        "runner".into(),
        "build".into(),
        "--desc".into(),
        descriptor_mount,
    ];
    args.extend(opts.user_args.iter().cloned());

    PodmanRunOpts {
        image: opts.image_ref.clone(),
        cidfile: None,
        name: Some(input.container_name.to_owned()),
        entrypoint: Some(IN_CONTAINER_CBSBUILD.into()),
        env,
        volumes,
        devices: HashMap::new(),
        replace_if_exists: false,
        use_user_ns: false,
        use_host_network: true,
        unconfined: true,
        timeout: Some(opts.timeout),
        workdir: Some(IN_CONTAINER_WORKDIR.into()),
        args,
    }
}

// ---------------------------------------------------------------------
// Running — drive podman_run with timeout + SIGTERM coordination
// ---------------------------------------------------------------------

async fn drive_run(opts: PodmanRunOpts<'_>, container_name: String) -> Result<i32, RunnerError> {
    tracing::info!(
        target: TARGET_RUNNER_RUN,
        container = %container_name,
        "spawning builder container",
    );
    let outcome = podman_run(opts).await?;
    Ok(outcome.rc)
}

async fn run_with_signal_and_timeout<F>(future: F, timeout: Duration) -> Result<i32, RunnerError>
where
    F: std::future::Future<Output = Result<i32, RunnerError>>,
{
    let sigterm_fut = wait_for_sigterm();
    tokio::pin!(future);
    tokio::pin!(sigterm_fut);
    let select = async {
        tokio::select! {
            res = &mut future => res,
            () = &mut sigterm_fut => Err(RunnerError::Cancelled),
        }
    };
    tokio::time::timeout(timeout, select)
        .await
        .map_or(Err(RunnerError::Timeout), |res| res)
}

#[cfg(unix)]
async fn wait_for_sigterm() {
    use tokio::signal::unix::{SignalKind, signal};
    let Ok(mut term) = signal(SignalKind::terminate()) else {
        // If the signal handler can't be installed (e.g. inside a
        // sandboxed test), park forever so the SIGTERM branch never
        // fires. The outer-timeout will still cap runtime.
        std::future::pending::<()>().await;
        return;
    };
    let _ = term.recv().await;
}

#[cfg(not(unix))]
async fn wait_for_sigterm() {
    std::future::pending::<()>().await;
}

// ---------------------------------------------------------------------
// Cleanup
// ---------------------------------------------------------------------

/// Explicit async cleanup — runs on every normal return path
/// (success, expected error, timeout, SIGTERM). Takes the
/// `CleanupState` by value and immediately destructures it so a
/// later `?`-early-return inside this function does not re-trigger
/// the `Drop`-side fallback.
async fn cleanup(state: CleanupState, cleanup_flag: Arc<AtomicBool>) -> Result<(), RunnerError> {
    let CleanupState {
        descriptor_path,
        config_path,
        secrets_path,
        components_dir,
        container_name,
        cleanup_flag: _,
    } = state;
    cleanup_flag.store(true, Ordering::Release);
    // Stop the container first so any in-flight tempfile reads
    // unblock before we remove the tempfiles.
    let _ = podman_stop(Some(&container_name), super::DEFAULT_STOP_TIMEOUT).await;
    // Tempfile removals — best-effort; missing files are not
    // errors (the container may have already finished and the
    // tempfile may have been removed by a sibling code path).
    let _ = tokio::fs::remove_file(&descriptor_path).await;
    let _ = tokio::fs::remove_file(&config_path).await;
    let _ = tokio::fs::remove_file(&secrets_path).await;
    drop(components_dir);
    tracing::debug!(
        target: TARGET_RUNNER_RUN,
        container = %container_name,
        "cleanup complete",
    );
    Ok(())
}

async fn read_build_report(path: &Utf8Path) -> Option<serde_json::Value> {
    let bytes = tokio::fs::read(path).await.ok()?;
    let report: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    // Best-effort: remove the file once read so subsequent runs
    // don't pick up stale reports.
    let _ = tokio::fs::remove_file(path).await;
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::config::PathsConfig;

    fn sample_config() -> Config {
        Config {
            paths: PathsConfig {
                components: vec!["/srv/components".into()],
                scratch: "/srv/scratch".into(),
                scratch_containers: "/srv/scratch-containers".into(),
                ccache: None,
                versions: None,
            },
            storage: None,
            signing: None,
            logging: None,
            secrets: Vec::new(),
            vault: None,
        }
    }

    #[test]
    fn default_run_opts_timeout_is_four_hours() {
        let opts = RunOpts::default();
        assert_eq!(opts.timeout, Duration::from_secs(4 * 3600));
    }

    #[test]
    fn container_facing_config_remaps_paths() {
        let mut host = sample_config();
        host.paths.ccache = Some("/srv/ccache".into());
        host.vault = Some("/srv/vault.yaml".into());
        let cont = container_facing_config(&host);
        assert_eq!(cont.paths.scratch, "/runner/scratch");
        assert_eq!(cont.paths.scratch_containers, "/var/lib/containers");
        assert_eq!(
            cont.paths.components,
            vec![Utf8PathBuf::from("/runner/components")]
        );
        assert_eq!(
            cont.paths.ccache.as_deref(),
            Some(Utf8Path::new("/runner/ccache"))
        );
        assert_eq!(
            cont.secrets,
            vec![Utf8PathBuf::from("/runner/cbs-build.secrets.yaml")]
        );
        assert_eq!(
            cont.vault.as_deref(),
            Some(Utf8Path::new("/runner/cbs-build.vault.yaml")),
        );
    }

    #[test]
    fn container_facing_config_drops_optional_when_unset() {
        let cont = container_facing_config(&sample_config());
        assert!(cont.paths.ccache.is_none());
        assert!(cont.vault.is_none());
    }

    fn input<'a>(
        container_name: &'a str,
        descriptor_path: &'a Utf8Path,
        descriptor_basename: &'a str,
        config_path: &'a Utf8Path,
        secrets_path: &'a Utf8Path,
        components_dir_path: &'a Utf8Path,
        cbsbuild_self_path: &'a Utf8Path,
    ) -> PodmanOptsInput<'a> {
        PodmanOptsInput {
            container_name,
            descriptor_path,
            descriptor_basename,
            config_path,
            secrets_path,
            components_dir_path,
            cbsbuild_self_path,
        }
    }

    #[test]
    fn build_podman_opts_has_required_mounts_and_env() {
        let cfg = sample_config();
        let opts = RunOpts {
            image_ref: "registry.example.com/builder:el9".into(),
            user_args: vec!["--force".into()],
            ..Default::default()
        };
        let podman = build_podman_opts(
            &input(
                "ces_abcdefghij",
                Utf8Path::new("/tmp/staging/19.2.3-dev.1.json"),
                "19.2.3-dev.1.json",
                Utf8Path::new("/tmp/staging/cbs-build.config.yaml"),
                Utf8Path::new("/tmp/staging/cbs-build.secrets.yaml"),
                Utf8Path::new("/tmp/staging/components"),
                Utf8Path::new("/usr/bin/cbsbuild"),
            ),
            &cfg,
            &opts,
        );

        // Required mount points.
        let mounted: Vec<&String> = podman.volumes.values().collect();
        assert!(mounted.iter().any(|m| *m == "/runner/19.2.3-dev.1.json"));
        assert!(mounted.iter().any(|m| *m == IN_CONTAINER_CBSBUILD));
        assert!(
            mounted
                .iter()
                .any(|m| *m == "/runner/cbs-build.config.yaml")
        );
        assert!(
            mounted
                .iter()
                .any(|m| *m == "/runner/cbs-build.secrets.yaml")
        );
        assert!(mounted.iter().any(|m| *m == "/runner/scratch"));
        assert!(mounted.iter().any(|m| *m == "/var/lib/containers:Z"));
        assert!(mounted.iter().any(|m| *m == "/runner/components"));

        // Env vars.
        assert_eq!(podman.env.get("HOME").map(String::as_str), Some("/runner"));
        assert!(podman.env.contains_key("CBS_DEBUG"));

        // Workdir + entrypoint pins.
        assert_eq!(podman.workdir.as_deref(), Some(IN_CONTAINER_WORKDIR));
        assert_eq!(podman.entrypoint.as_deref(), Some(IN_CONTAINER_CBSBUILD));

        // User args appended after the canonical args.
        assert_eq!(podman.args.last(), Some(&"--force".to_string()));
        assert_eq!(
            &podman.args[..6],
            &[
                "--config",
                "/runner/cbs-build.config.yaml",
                "runner",
                "build",
                "--desc",
                "/runner/19.2.3-dev.1.json",
            ],
        );
    }

    #[test]
    fn build_podman_opts_includes_vault_mount_when_set() {
        let mut cfg = sample_config();
        cfg.vault = Some("/srv/vault.yaml".into());
        let opts = RunOpts {
            image_ref: "img:tag".into(),
            ..Default::default()
        };
        let podman = build_podman_opts(
            &input(
                "ces_abcdefghij",
                Utf8Path::new("/tmp/d.json"),
                "d.json",
                Utf8Path::new("/tmp/c.yaml"),
                Utf8Path::new("/tmp/s.yaml"),
                Utf8Path::new("/tmp/comp"),
                Utf8Path::new("/usr/bin/cbsbuild"),
            ),
            &cfg,
            &opts,
        );
        assert!(
            podman
                .volumes
                .values()
                .any(|v| v == "/runner/cbs-build.vault.yaml"),
            "expected vault mount when config.vault is set",
        );
    }

    #[test]
    fn trace_id_field_renders_none_or_uuid() {
        assert_eq!(trace_id_field(None), "none");
        let id = Uuid::nil();
        assert_eq!(trace_id_field(Some(&id)), id.to_string());
    }
}
