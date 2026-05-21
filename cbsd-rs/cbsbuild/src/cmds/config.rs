// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild config …` — config-file lifecycle.
//!
//! - `config init` with bypass-mode flags
//!   (`--for-systemd-install` / `--for-containerized-run`) plus
//!   per-field overrides. No interactive prompts — the
//!   prompt-based UX is seq-003 (post-M1) per design 002 §Open
//!   Questions lines 1424–1432.
//! - `config show` — pretty-print the loaded config.
//! - `config check` — validate required fields and exit 0
//!   / non-zero.

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use cbscore::config;
use cbscore_types::config::{Config, PathsConfig};
use clap::{Args, Subcommand};

/// `cbsbuild config …` subcommand enum.
#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Initialise a config from a bypass mode + overrides.
    Init(InitArgs),
    /// Pretty-print the loaded config.
    Show,
    /// Validate the loaded config; exits 0 on success.
    Check,
}

/// `cbsbuild config init` arguments. No interactive prompts — at
/// least one of the `--for-*` mode flags is required (running with
/// no flags returns an error with a usage hint).
#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    /// Pre-fill paths for a systemd-managed deployment.
    #[arg(long = "for-systemd-install")]
    pub for_systemd_install: bool,
    /// Pre-fill paths for a containerised deployment.
    #[arg(long = "for-containerized-run")]
    pub for_containerized_run: bool,
    /// Override `paths.components` (repeatable / colon-separated).
    #[arg(long = "components", value_delimiter = ':')]
    pub components: Vec<Utf8PathBuf>,
    /// Override `paths.scratch`.
    #[arg(long = "scratch")]
    pub scratch: Option<Utf8PathBuf>,
    /// Override `paths.scratch-containers`.
    #[arg(long = "containers-scratch")]
    pub containers_scratch: Option<Utf8PathBuf>,
    /// Override `paths.ccache`.
    #[arg(long = "ccache")]
    pub ccache: Option<Utf8PathBuf>,
    /// Override `vault` (path to `cbs-build.vault.yaml`).
    #[arg(long = "vault")]
    pub vault: Option<Utf8PathBuf>,
    /// Override `secrets` (repeatable / colon-separated paths).
    #[arg(long = "secrets", value_delimiter = ':')]
    pub secrets: Vec<Utf8PathBuf>,
}

/// `cbsbuild config …` handler.
pub(crate) async fn handle(cmd: ConfigCommand, config_path: &Utf8Path) -> Result<()> {
    match cmd {
        ConfigCommand::Init(args) => handle_init(args, config_path).await,
        ConfigCommand::Show => handle_show(config_path).await,
        ConfigCommand::Check => handle_check(config_path).await,
    }
}

async fn handle_init(args: InitArgs, config_path: &Utf8Path) -> Result<()> {
    if !args.for_systemd_install && !args.for_containerized_run {
        bail!(
            "cbsbuild config init: one of --for-systemd-install or \
             --for-containerized-run is required (interactive prompt UX is \
             seq-003, post-M1). Pass --help for the per-field override flags."
        );
    }
    if args.for_systemd_install && args.for_containerized_run {
        bail!(
            "cbsbuild config init: --for-systemd-install and \
             --for-containerized-run are mutually exclusive"
        );
    }

    let mut cfg = if args.for_systemd_install {
        systemd_install_template()
    } else {
        containerized_run_template()
    };

    // Per-field overrides compose with the chosen bypass mode.
    if !args.components.is_empty() {
        cfg.paths.components = args.components;
    }
    if let Some(p) = args.scratch {
        cfg.paths.scratch = p;
    }
    if let Some(p) = args.containers_scratch {
        cfg.paths.scratch_containers = p;
    }
    if let Some(p) = args.ccache {
        cfg.paths.ccache = Some(p);
    }
    if let Some(p) = args.vault {
        cfg.vault = Some(p);
    }
    if !args.secrets.is_empty() {
        cfg.secrets = args.secrets;
    }

    config::store(&cfg, config_path)
        .await
        .with_context(|| format!("writing config to '{config_path}'"))?;
    println!("{config_path}");
    Ok(())
}

async fn handle_show(config_path: &Utf8Path) -> Result<()> {
    let cfg = config::load(config_path)
        .await
        .with_context(|| format!("loading config at '{config_path}'"))?;
    let body = serde_json::to_string_pretty(&cfg).context("serialising config for display")?;
    println!("{body}");
    Ok(())
}

async fn handle_check(config_path: &Utf8Path) -> Result<()> {
    let cfg = config::load(config_path)
        .await
        .with_context(|| format!("loading config at '{config_path}'"))?;
    validate_config(&cfg)?;
    println!("ok: {config_path}");
    Ok(())
}

/// Bypass-mode pre-fill for a systemd-managed deployment per
/// design 004 §Bypass-mode pre-fill (excluding the `versions`
/// field, which is design 004 Migration step 5 and deferred to
/// seq-003).
fn systemd_install_template() -> Config {
    Config {
        paths: PathsConfig {
            components: vec!["/etc/cbsd/components".into()],
            scratch: "/var/lib/cbsd/scratch".into(),
            scratch_containers: "/var/lib/cbsd/scratch-containers".into(),
            ccache: Some("/var/lib/cbsd/ccache".into()),
            // Pre-fill of `versions` (design 004 OQ7) is owned by
            // seq-003, which adds the interactive prompt + the
            // bypass-mode pre-fill in lockstep. seq-004 keeps the
            // template structurally valid with `None`.
            versions: None,
        },
        storage: None,
        signing: None,
        logging: None,
        secrets: vec!["/etc/cbsd/secrets.yaml".into()],
        vault: Some("/etc/cbsd/vault.yaml".into()),
    }
}

/// Bypass-mode pre-fill for a containerised deployment per design
/// 004 §Bypass-mode pre-fill line 358.
fn containerized_run_template() -> Config {
    Config {
        paths: PathsConfig {
            components: vec!["/cbs/components".into()],
            scratch: "/cbs/scratch".into(),
            scratch_containers: "/cbs/scratch-containers".into(),
            ccache: Some("/cbs/ccache".into()),
            // See note in systemd_install_template.
            versions: None,
        },
        storage: None,
        signing: None,
        logging: None,
        secrets: vec!["/cbs/secrets.yaml".into()],
        vault: Some("/cbs/vault.yaml".into()),
    }
}

/// `Config` validator — asserts every required field is set to a
/// non-empty value. Surfaces a single aggregated error listing
/// each problem so `cbsbuild config check` prints them all at
/// once.
pub(crate) fn validate_config(cfg: &Config) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();
    if cfg.paths.components.is_empty() {
        errors.push("paths.components is empty".into());
    }
    for (i, p) in cfg.paths.components.iter().enumerate() {
        if p.as_str().is_empty() {
            errors.push(format!("paths.components[{i}] is empty"));
        }
    }
    if cfg.paths.scratch.as_str().is_empty() {
        errors.push("paths.scratch is empty".into());
    }
    if cfg.paths.scratch_containers.as_str().is_empty() {
        errors.push("paths.scratch-containers is empty".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        bail!("config invalid:\n  - {}", errors.join("\n  - "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_template_has_required_paths() {
        let cfg = systemd_install_template();
        assert!(!cfg.paths.components.is_empty());
        assert!(!cfg.paths.scratch.as_str().is_empty());
        assert!(!cfg.paths.scratch_containers.as_str().is_empty());
    }

    #[test]
    fn containerized_template_uses_cbs_prefix() {
        let cfg = containerized_run_template();
        assert!(cfg.paths.scratch.as_str().starts_with("/cbs/"));
        assert!(cfg.paths.components[0].as_str().starts_with("/cbs/"));
    }

    #[test]
    fn validate_config_rejects_empty_components() {
        let mut cfg = containerized_run_template();
        cfg.paths.components.clear();
        let Err(err) = validate_config(&cfg) else {
            panic!("expected validation failure");
        };
        assert!(err.to_string().contains("paths.components is empty"));
    }

    #[test]
    fn validate_config_accepts_complete_template() {
        let cfg = containerized_run_template();
        validate_config(&cfg).expect("complete template should validate");
    }

    #[test]
    fn validate_config_aggregates_multiple_errors() {
        let mut cfg = containerized_run_template();
        cfg.paths.components.clear();
        cfg.paths.scratch = Utf8PathBuf::new();
        let Err(err) = validate_config(&cfg) else {
            panic!("expected validation failure");
        };
        let msg = err.to_string();
        assert!(msg.contains("components is empty"));
        assert!(msg.contains("scratch is empty"));
    }
}
