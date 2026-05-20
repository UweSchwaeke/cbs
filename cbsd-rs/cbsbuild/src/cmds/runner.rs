// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild runner …` — host-side container lifecycle + the
//! in-container build entry point.
//!
//! Subcommands:
//!
//! - `runner run <descriptor.json>` — host-side spawn (alias of
//!   `cbsbuild build`). Loads config + secrets, reads the
//!   descriptor, dispatches to `cbscore::runner::run` (Phase 4
//!   Commit 3). On success the resulting [`RunReport`] is printed
//!   to stdout in pretty-JSON.
//! - `runner stop [--name NAME] [--all]` — wraps
//!   `cbscore::runner::stop` (Phase 4 Commit 2).
//! - `runner build <descriptor.json>` — **in-container entry
//!   point** invoked by the host runner via
//!   `--entrypoint /runner/cbsbuild`. Loads the mounted config +
//!   secrets, dispatches to [`cbscore::builder::run_build`]
//!   (Phase 5 Commit 7), then serialises the resulting
//!   [`BuildArtifactReport`] to
//!   `<config.paths.scratch>/build-report.json` (which the host
//!   runner reads back after the container exits — matches the
//!   Phase 4 Commit 3 mount-table aliasing).
//!
//! SIGTERM cooperative cancellation: the `runner run` host-side
//! path inherits Phase 4 Commit 3's
//! [`tokio::select!`]-driven SIGTERM handler; the `runner build`
//! in-container path installs its own [`tokio::signal::ctrl_c`] /
//! `SignalKind::terminate` handler that drops the `run_build`
//! future cooperatively.

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use cbscore::builder::{BuildOptions, run_build};
use cbscore::runner::{self, DEFAULT_STOP_TIMEOUT, RunOpts};
use cbscore::versions::desc::read_descriptor;
use clap::{Args, Subcommand};

use super::shared::{dump_yaml_path, load_config_and_secrets};

const TARGET_CBSBUILD_RUNNER: &str = "cbsbuild::runner";

/// In-container path the runner-build handler writes its
/// [`BuildArtifactReport`] to. The host runner reads back from
/// `<config.paths.scratch>/build-report.json` (Phase 4 Commit 3's
/// mount table aliases `/runner/scratch` to the host's
/// `config.paths.scratch` directory, so the two refer to the same
/// file).
const REPORT_BASENAME: &str = "build-report.json";

/// `cbsbuild runner …` subcommand enum.
#[derive(Debug, Subcommand)]
pub enum RunnerCommand {
    /// Run a build (host-side spawn).
    Run(RunArgs),
    /// Stop a container or every cbscore-prefixed container.
    Stop(StopArgs),
    /// In-container build entry — invoked by the host runner via
    /// `--entrypoint /runner/cbsbuild`.
    Build(RunArgs),
}

/// Shared shape for `runner run` and `runner build`.
#[derive(Debug, Args)]
pub struct RunArgs {
    /// Path to the version-descriptor JSON.
    pub descriptor: Utf8PathBuf,
    /// Skip the in-container rpmbuild step.
    #[arg(long = "skip-build")]
    pub skip_build: bool,
    /// Clear each component's scratch dir before fetching sources.
    #[arg(long = "force")]
    pub force: bool,
    /// TLS verification for registry pushes (default on).
    #[arg(long = "tls-verify", default_value_t = true, action = clap::ArgAction::Set)]
    pub tls_verify: bool,
    /// Builder image reference (the cbscore builder image). When
    /// omitted on `runner run`, the host runner falls back to the
    /// descriptor's `image:` block. `runner build` ignores this
    /// flag — the in-container path doesn't spawn anything.
    #[arg(long = "image")]
    pub image: Option<String>,
}

/// `cbsbuild runner stop` arguments.
#[derive(Debug, Args)]
pub struct StopArgs {
    /// Stop a single container by name.
    #[arg(long = "name", conflicts_with = "all")]
    pub name: Option<String>,
    /// Stop every cbscore-prefixed container (the `--all` form).
    #[arg(long = "all", conflicts_with = "name")]
    pub all: bool,
}

/// `cbsbuild runner …` handler.
pub async fn handle(cmd: RunnerCommand, config_path: &Utf8Path) -> Result<()> {
    match cmd {
        RunnerCommand::Run(args) => handle_run(args, config_path).await,
        RunnerCommand::Stop(args) => handle_stop(args).await,
        RunnerCommand::Build(args) => handle_in_container_build(args, config_path).await,
    }
}

/// Shared `runner run` / `build` host-side handler. Same behaviour
/// as `cbsbuild build` (which delegates here).
pub async fn handle_run(args: RunArgs, config_path: &Utf8Path) -> Result<()> {
    let (cfg, secrets) = load_config_and_secrets(config_path).await?;
    let desc = read_descriptor(&args.descriptor)
        .await
        .with_context(|| format!("reading descriptor at '{}'", args.descriptor))?;

    let image_ref = args.image.clone().unwrap_or_else(|| {
        format!(
            "{}/{}:{}",
            desc.image.registry, desc.image.name, desc.image.tag,
        )
    });
    let opts = RunOpts {
        image_ref,
        user_args: Vec::new(),
        ..RunOpts::default()
    };
    tracing::info!(
        target: TARGET_CBSBUILD_RUNNER,
        descriptor = %args.descriptor,
        skip_build = args.skip_build,
        force = args.force,
        tls_verify = args.tls_verify,
        "cbsbuild runner run: dispatching to cbscore::runner::run",
    );
    let report = runner::run(&desc, &cfg, &secrets, &opts).await?;
    let body = serde_json::to_string_pretty(&serde_json::json!({
        "container_name": report.container_name,
        "exit_code": report.exit_code,
        "build_report": report.build_report,
    }))
    .context("serialising RunReport for display")?;
    println!("{body}");
    Ok(())
}

async fn handle_stop(args: StopArgs) -> Result<()> {
    let name = args.name.as_deref();
    if name.is_none() && !args.all {
        anyhow::bail!("cbsbuild runner stop: one of --name NAME or --all is required");
    }
    let ctx = name.map_or_else(
        || "stopping all cbscore-prefixed containers".to_string(),
        |n| format!("stopping container '{n}'"),
    );
    runner::stop(name, DEFAULT_STOP_TIMEOUT)
        .await
        .context(ctx)?;
    Ok(())
}

/// In-container `runner build` handler — invoked by the host
/// runner via `--entrypoint /runner/cbsbuild`. Drives
/// [`cbscore::builder::run_build`] then writes the
/// [`BuildArtifactReport`] to the host-mounted scratch path so the
/// host runner can read it back after the container exits.
async fn handle_in_container_build(args: RunArgs, config_path: &Utf8Path) -> Result<()> {
    let (cfg, secrets) = load_config_and_secrets(config_path).await?;
    let desc = read_descriptor(&args.descriptor)
        .await
        .with_context(|| format!("reading descriptor at '{}'", args.descriptor))?;
    let opts = BuildOptions {
        skip_build: args.skip_build,
        force: args.force,
    };
    let report_path = cfg.paths.scratch.join(REPORT_BASENAME);

    tracing::info!(
        target: TARGET_CBSBUILD_RUNNER,
        descriptor = %args.descriptor,
        report_path = %report_path,
        "cbsbuild runner build: dispatching to cbscore::builder::run_build",
    );

    // SIGTERM cooperative cancellation: tokio::select! drives the
    // run_build future against a SIGTERM signal so the host
    // runner's `podman stop --time 1` propagates cleanly into the
    // container. On SIGTERM the run_build future is dropped, which
    // triggers each per-stage RAII guard's cleanup
    // (Phase 5's BuildahWorkingContainer, Phase 2's async_run_cmd
    // drop guard, etc.).
    let report_result = tokio::select! {
        result = run_build(&desc, &cfg, &secrets, &opts) => result,
        () = wait_for_sigterm() => {
            tracing::warn!(
                target: TARGET_CBSBUILD_RUNNER,
                "SIGTERM received — dropping run_build future",
            );
            return Err(anyhow::anyhow!(
                "cbsbuild runner build: cancelled by SIGTERM"
            ));
        }
    };

    let report = report_result.with_context(|| format!("building version '{}'", desc.version))?;

    // Serialise via serde_json::to_string_pretty (matches the
    // descriptor write side — 2-space indent, no trailing newline).
    let report_json =
        serde_json::to_string_pretty(&report).context("serialising BuildArtifactReport")?;
    tokio::fs::write(&report_path, &report_json)
        .await
        .with_context(|| format!("writing build report to '{report_path}'"))?;
    dump_yaml_path(&report_path);
    Ok(())
}

#[cfg(unix)]
async fn wait_for_sigterm() {
    use tokio::signal::unix::{SignalKind, signal};
    let Ok(mut term) = signal(SignalKind::terminate()) else {
        // If the signal handler can't be installed (sandboxed
        // tests, etc.), park forever so the SIGTERM branch never
        // fires. Phase 4's outer timeout still caps runtime.
        std::future::pending::<()>().await;
        return;
    };
    let _ = term.recv().await;
}

#[cfg(not(unix))]
async fn wait_for_sigterm() {
    std::future::pending::<()>().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stop_without_name_or_all_errors() {
        let args = StopArgs {
            name: None,
            all: false,
        };
        let Err(err) = handle_stop(args).await else {
            panic!("expected error");
        };
        assert!(err.to_string().contains("--name NAME or --all"));
    }
}
