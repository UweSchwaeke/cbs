// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild config …` — config-file lifecycle.
//!
//! - `config init` — interactive prompt flow when no `--for-*`
//!   bypass mode is supplied; bypass-mode behaviour preserved
//!   when one of `--for-systemd-install` / `--for-containerized-run`
//!   is set. Per-field flags (`--components`, `--scratch`,
//!   `--containers-scratch`, `--ccache`, `--versions-dir`,
//!   `--vault`, `--secrets`) suppress the matching prompts.
//! - `config init-vault` — interactive flow to produce a
//!   `cbs-build.vault.yaml` companion file (vault address +
//!   auth method + credentials). Added by seq-003 Commit 2.
//! - `config show` — pretty-print the loaded config.
//! - `config check` — validate required fields and exit 0
//!   / non-zero.

pub(crate) mod init;
pub(crate) mod init_vault;
pub(crate) mod prompts;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use cbscore::config;
use cbscore_types::config::{Config, PathsConfig};
use clap::{Args, Subcommand};

/// `cbsbuild config …` subcommand enum.
#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Initialise a config interactively, or from a bypass mode +
    /// overrides. With no flags, walks the operator through prompts
    /// for every required field. With `--for-systemd-install` or
    /// `--for-containerized-run`, pre-fills the matching template
    /// and skips every prompt. Per-field flags compose on top in
    /// both modes.
    Init(InitArgs),
    /// Interactively produce a `cbs-build.vault.yaml` companion
    /// file (vault address + auth method + credentials).
    InitVault(init_vault::InitVaultArgs),
    /// Pretty-print the loaded config.
    Show,
    /// Validate the loaded config; exits 0 on success.
    Check,
}

/// `cbsbuild config init` arguments.
///
/// Three modes, all backed by the same flag set:
///
/// - **Bypass mode** — one of `--for-systemd-install` or
///   `--for-containerized-run`. Pre-fills every path field from
///   the corresponding template and skips every prompt. Per-field
///   flags compose on top, overriding the template's pre-fill for
///   that field.
/// - **Interactive mode** — no `--for-*` flag. Walks the operator
///   through prompts for every required field. Per-field flags
///   suppress the matching prompts (e.g. `--components /comp`
///   skips the components prompts and uses `/comp` directly).
/// - **Mixed mode** — interactive prompts for unset fields,
///   per-field flag values verbatim for set fields.
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
    /// Override `paths.versions` (interactive mode skips the
    /// "Versions path" prompt when supplied).
    #[arg(long = "versions-dir")]
    pub versions_dir: Option<Utf8PathBuf>,
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
        ConfigCommand::InitVault(args) => init_vault::handle_init_vault(args).await,
        ConfigCommand::Show => handle_show(config_path).await,
        ConfigCommand::Check => handle_check(config_path).await,
    }
}

async fn handle_init(args: InitArgs, config_path: &Utf8Path) -> Result<()> {
    if args.for_systemd_install && args.for_containerized_run {
        bail!(
            "cbsbuild config init: --for-systemd-install and \
             --for-containerized-run are mutually exclusive"
        );
    }

    if args.for_systemd_install || args.for_containerized_run {
        return handle_init_bypass(args, config_path).await;
    }

    // No `--for-*` flag → interactive prompt flow (seq-003).
    let mut prompter = prompts::DialoguerPrompter;
    init::config_init(&mut prompter, &args, config_path).await
}

/// Bypass-mode `cbsbuild config init` — write the template + the
/// per-field overrides, no prompts. Preserves M1 byte-identical
/// behaviour modulo the `paths.versions` pre-fill that seq-003
/// adds per OQ-A.1 (per-template prefix-matching).
async fn handle_init_bypass(args: InitArgs, config_path: &Utf8Path) -> Result<()> {
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
    if let Some(p) = args.versions_dir {
        cfg.paths.versions = Some(p);
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
/// design 004 §Bypass-mode pre-fill.
///
/// `paths.versions` lands at `/var/lib/cbsd/_versions` —
/// per-template prefix-matching per seq-003 plan OQ-A.1.
pub(crate) fn systemd_install_template() -> Config {
    Config {
        paths: PathsConfig {
            components: vec!["/etc/cbsd/components".into()],
            scratch: "/var/lib/cbsd/scratch".into(),
            scratch_containers: "/var/lib/cbsd/scratch-containers".into(),
            ccache: Some("/var/lib/cbsd/ccache".into()),
            versions: Some("/var/lib/cbsd/_versions".into()),
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
///
/// `paths.versions` lands at `/cbs/_versions` — per-template
/// prefix-matching per seq-003 plan OQ-A.1.
pub(crate) fn containerized_run_template() -> Config {
    Config {
        paths: PathsConfig {
            components: vec!["/cbs/components".into()],
            scratch: "/cbs/scratch".into(),
            scratch_containers: "/cbs/scratch-containers".into(),
            ccache: Some("/cbs/ccache".into()),
            versions: Some("/cbs/_versions".into()),
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
    fn systemd_template_pre_fills_versions_under_var_lib_cbsd() {
        // seq-003 plan OQ-A.1: per-template prefix-matching pre-fill.
        let cfg = systemd_install_template();
        assert_eq!(
            cfg.paths.versions.as_deref(),
            Some(Utf8Path::new("/var/lib/cbsd/_versions")),
        );
    }

    #[test]
    fn containerized_template_pre_fills_versions_under_cbs() {
        // seq-003 plan OQ-A.1: per-template prefix-matching pre-fill.
        let cfg = containerized_run_template();
        assert_eq!(
            cfg.paths.versions.as_deref(),
            Some(Utf8Path::new("/cbs/_versions")),
        );
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
