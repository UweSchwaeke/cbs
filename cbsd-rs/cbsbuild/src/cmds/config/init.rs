// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild config init` interactive flow.
//!
//! Mirrors Python `config_init` (`cbscore/src/cbscore/cmds/config.py`
//! lines 251–309) step-by-step. Driven by a [`Prompter`] so the
//! sub-functions can be unit-tested with a
//! [`crate::cmds::config::prompts::ScriptedPrompter`] rather than a
//! TTY.
//!
//! The flow only fires when no `--for-*` bypass mode is supplied
//! (the dispatcher in [`super::handle_init`] routes between the two
//! paths). Per-field flags suppress the matching prompts; supplied
//! values land verbatim in the resulting [`Config`].
//!
//! ## Final-confirmation step exit codes
//!
//! Step 8 (overwrite-confirm) and Step 10 (write-confirm) bail with
//! `bail!(...)` on operator decline, mirroring Python's
//! `sys.exit(errno.ENOTRECOVERABLE)`. The bin's `classify_exit`
//! routes anything that isn't a typed `ConfigError::NotFound` /
//! `VersionError::NoSuchDescriptor` to `EXIT_UNRECOVERABLE = 131`,
//! which matches `errno.ENOTRECOVERABLE`. The mapping is correct
//! without further wiring.

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::config::{
    Config, PathsConfig, RegistryStorageConfig, S3LocationConfig, S3StorageConfig, SigningConfig,
    StorageConfig, VersionedConfig,
};

use super::InitArgs;
use super::prompts::{Prompter, validate_url};

/// Top-level `cbsbuild config init` interactive flow.
///
/// 1–5. Sub-functions assemble [`PathsConfig`] / `Option<StorageConfig>`
///      / `Option<SigningConfig>` / `Vec<secret-path>`, each
///      respecting per-field flag overrides.
/// 6.   Build the [`Config`] struct, threading `args.vault` through
///      to `Config.vault`.
/// 7.   Suffix normalisation — if `config_path` doesn't end in
///      `.yaml`, rename to `.yaml` with an echo. Load-bearing:
///      `cbscore::config::load` picks the deserialiser by extension.
/// 8.   Overwrite-confirm if the target exists. Decline → bail.
/// 9.   Print rendered YAML preview to stdout.
/// 10.  Write-confirm. Decline → bail.
/// 11.  `cbscore::config::store` does the actual atomic write
///      (parent-directory creation included).
/// 12.  Echo the final "wrote config file to '${path}'" line.
///
/// # Errors
///
/// Returns `Err(_)` for prompter IO failures, decline-to-overwrite
/// (step 8), decline-to-write (step 10), serialisation failures
/// (step 9), or `cbscore::config::store` failures (step 11).
pub(crate) async fn config_init<P: Prompter>(
    prompter: &mut P,
    args: &InitArgs,
    config_path: &Utf8Path,
) -> Result<()> {
    let cwd_std = std::env::current_dir().context("resolving current working directory")?;
    let cwd = Utf8PathBuf::from_path_buf(cwd_std).map_err(|p| {
        anyhow::anyhow!(
            "current working directory '{}' is not valid UTF-8",
            p.display()
        )
    })?;

    let paths = config_init_paths(prompter, &cwd, args)?;
    let storage = config_init_storage(prompter)?;
    let signing = config_init_signing(prompter)?;
    let secrets = config_init_secrets_paths(prompter, &args.secrets)?;

    let cfg = Config {
        paths,
        storage,
        signing,
        logging: None,
        secrets,
        vault: args.vault.clone(),
    };

    // Step 7: suffix normalisation.
    let config_path = normalise_yaml_suffix(config_path);

    // Step 8: overwrite confirm if file exists.
    if config_path.exists() {
        let overwrite = prompter
            .confirm("Config file exists, overwrite?", false)
            .context("prompting for overwrite confirm")?;
        if !overwrite {
            bail!("do not write config file to '{config_path}'");
        }
    }

    // Step 9: print rendered YAML preview.
    println!("config:\n");
    let preview = serde_saphyr::to_string(&VersionedConfig::new(cfg.clone()))
        .context("serialising config preview")?;
    println!("{preview}");

    // Step 10: final write-confirm.
    let write_prompt = format!("Write config to '{config_path}'?");
    let write = prompter
        .confirm(&write_prompt, true)
        .context("prompting for write confirm")?;
    if !write {
        bail!("do not write config files");
    }

    // Step 11: store. Step 12: echo.
    cbscore::config::store(&cfg, &config_path)
        .await
        .with_context(|| format!("writing config to '{config_path}'"))?;
    println!("wrote config file to '{config_path}'");
    Ok(())
}

/// Suffix-normalise the operator-supplied config path.
///
/// `cbscore::config::load` picks the deserialiser by extension —
/// writing YAML to a `.json`-named file produces a parse failure
/// on the next invocation. So if the path doesn't end in `.yaml`,
/// echo a warning and return the path with the `.yaml` extension.
/// Mirrors Python `config_init` lines 283–288.
fn normalise_yaml_suffix(config_path: &Utf8Path) -> Utf8PathBuf {
    if config_path.extension() == Some("yaml") {
        return config_path.to_owned();
    }
    let new_path = config_path.with_extension("yaml");
    println!("config at '{config_path}' not YAML, use '{new_path}' instead.");
    new_path
}

/// Prompt-driven assembly of [`PathsConfig`].
///
/// Each prompt is suppressed when the corresponding [`InitArgs`]
/// flag is supplied. The components block has slightly richer UX:
/// if `${cwd}/components` exists, the operator gets a single
/// "use that?" `Confirm` instead of typing the path; additional
/// paths are loop-collected.
///
/// # Errors
///
/// Returns prompter IO failures via the [`Prompter`] return type.
pub(crate) fn config_init_paths<P: Prompter>(
    prompter: &mut P,
    cwd: &Utf8Path,
    args: &InitArgs,
) -> Result<PathsConfig> {
    let components = if args.components.is_empty() {
        prompt_components(prompter, cwd)?
    } else {
        args.components.clone()
    };

    let scratch = match &args.scratch {
        Some(p) => p.clone(),
        None => Utf8PathBuf::from(
            prompter
                .input("Scratch path", None)
                .context("prompting for scratch path")?,
        ),
    };

    let scratch_containers = match &args.containers_scratch {
        Some(p) => p.clone(),
        None => Utf8PathBuf::from(
            prompter
                .input("Scratch containers path", None)
                .context("prompting for scratch containers path")?,
        ),
    };

    let ccache = match &args.ccache {
        Some(p) => Some(p.clone()),
        None => {
            if prompter
                .confirm("Specify ccache path?", false)
                .context("prompting for ccache confirm")?
            {
                Some(Utf8PathBuf::from(
                    prompter
                        .input("ccache path", None)
                        .context("prompting for ccache path")?,
                ))
            } else {
                None
            }
        }
    };

    let versions = match &args.versions_dir {
        Some(p) => Some(p.clone()),
        None => {
            if prompter
                .confirm("Specify versions path?", false)
                .context("prompting for versions confirm")?
            {
                Some(Utf8PathBuf::from(
                    prompter
                        .input("Versions path", None)
                        .context("prompting for versions path")?,
                ))
            } else {
                None
            }
        }
    };

    Ok(PathsConfig {
        components,
        scratch,
        scratch_containers,
        ccache,
        versions,
    })
}

/// Components-path loop helper extracted from
/// [`config_init_paths`] to keep the parent function under 40
/// lines per CLAUDE.md §Function hygiene.
///
/// If `${cwd}/components` exists, offers it as a default first
/// entry. Then asks once whether to add more, and loops until the
/// operator declines.
fn prompt_components<P: Prompter>(prompter: &mut P, cwd: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut components: Vec<Utf8PathBuf> = Vec::new();
    let default_path = cwd.join("components");
    if default_path.exists() {
        let label = format!("Use '{default_path}' as components path?");
        if prompter
            .confirm(&label, true)
            .context("prompting for default components confirm")?
        {
            components.push(default_path);
        }
    }
    let confirm_label = if components.is_empty() {
        "Specify paths?"
    } else {
        "Specify additional paths?"
    };
    if prompter
        .confirm(confirm_label, false)
        .context("prompting for additional components confirm")?
    {
        loop {
            let p = prompter
                .input("Components path", None)
                .context("prompting for components path")?;
            components.push(Utf8PathBuf::from(p));
            if !prompter
                .confirm("Add another components path?", false)
                .context("prompting for add-another components confirm")?
            {
                break;
            }
        }
    }
    Ok(components)
}

/// Prompt-driven assembly of `Option<StorageConfig>`.
///
/// Top-level decline (`Confirm("Configure storage?")`) returns
/// `Ok(None)`. Otherwise the S3 + registry sub-blocks are each
/// optional. URL prompts are validated via [`validate_url`] after
/// the input arrives; on failure the function bails (no in-place
/// re-prompt — same trade-off as `init-vault`'s vault-address
/// prompt; see seq-003 plan §Commit 2 §Design constraints).
///
/// # Errors
///
/// Returns prompter IO failures and URL-validation failures.
pub(crate) fn config_init_storage<P: Prompter>(prompter: &mut P) -> Result<Option<StorageConfig>> {
    if !prompter
        .confirm("Configure storage?", false)
        .context("prompting for storage confirm")?
    {
        println!("skipping storage configuration");
        println!("this should be manually configured later if storage is used");
        return Ok(None);
    }

    let s3 = if prompter
        .confirm("Configure S3 storage for artifact upload?", false)
        .context("prompting for S3 confirm")?
    {
        let url = prompter
            .input("S3 storage URL", None)
            .context("prompting for S3 URL")?;
        validate_url(&url).map_err(|e| anyhow::anyhow!("S3 storage URL: {e}"))?;
        let artifacts_bucket = prompter
            .input("S3 artifacts bucket", None)
            .context("prompting for S3 artifacts bucket")?;
        let artifacts_loc = prompter
            .input("S3 artifacts location", None)
            .context("prompting for S3 artifacts location")?;
        let releases_bucket = prompter
            .input("S3 releases bucket", None)
            .context("prompting for S3 releases bucket")?;
        let releases_loc = prompter
            .input("S3 releases location", None)
            .context("prompting for S3 releases location")?;
        Some(S3StorageConfig {
            url,
            artifacts: S3LocationConfig {
                bucket: artifacts_bucket,
                loc: artifacts_loc,
            },
            releases: S3LocationConfig {
                bucket: releases_bucket,
                loc: releases_loc,
            },
        })
    } else {
        None
    };

    let registry = if prompter
        .confirm(
            "Configure registry storage for container image upload?",
            false,
        )
        .context("prompting for registry confirm")?
    {
        let url = prompter
            .input("Registry storage URL", None)
            .context("prompting for registry URL")?;
        validate_url(&url).map_err(|e| anyhow::anyhow!("registry storage URL: {e}"))?;
        Some(RegistryStorageConfig { url })
    } else {
        None
    };

    Ok(Some(StorageConfig { s3, registry }))
}

/// Prompt-driven assembly of `Option<SigningConfig>`.
///
/// Top-level decline returns `Ok(None)`. If accepted but both GPG
/// and transit are declined, prints a "no signing methods
/// specified" line and returns `Ok(None)` — mirrors Python lines
/// 219–221.
///
/// # Errors
///
/// Returns prompter IO failures.
pub(crate) fn config_init_signing<P: Prompter>(prompter: &mut P) -> Result<Option<SigningConfig>> {
    if !prompter
        .confirm("Configure artifact signing?", false)
        .context("prompting for signing confirm")?
    {
        println!("skipping signing configuration");
        println!("this should be manually configured later if signing is used");
        return Ok(None);
    }

    let gpg = if prompter
        .confirm("Specify package GPG signing secret name?", false)
        .context("prompting for GPG confirm")?
    {
        Some(
            prompter
                .input("GPG signing secret name", None)
                .context("prompting for GPG secret name")?,
        )
    } else {
        None
    };

    let transit = if prompter
        .confirm(
            "Specify container image transit signing secret name?",
            false,
        )
        .context("prompting for transit confirm")?
    {
        Some(
            prompter
                .input("Transit signing secret name", None)
                .context("prompting for transit secret name")?,
        )
    } else {
        None
    };

    if gpg.is_none() && transit.is_none() {
        println!("no signing methods specified, skipping signing configuration");
        return Ok(None);
    }

    Ok(Some(SigningConfig { gpg, transit }))
}

/// Prompt-driven assembly of the secrets-paths list.
///
/// If `args_secrets` is non-empty, return it verbatim (zero
/// prompts — the per-field bypass). Otherwise ask whether to
/// configure secrets at all, and loop on adding paths.
///
/// # Errors
///
/// Returns prompter IO failures.
pub(crate) fn config_init_secrets_paths<P: Prompter>(
    prompter: &mut P,
    args_secrets: &[Utf8PathBuf],
) -> Result<Vec<Utf8PathBuf>> {
    if !args_secrets.is_empty() {
        return Ok(args_secrets.to_vec());
    }

    if !prompter
        .confirm("Specify paths to secrets file(s)?", false)
        .context("prompting for secrets confirm")?
    {
        println!("skipping secrets files configuration");
        println!("these should be manually configured later");
        return Ok(Vec::new());
    }

    let mut secrets: Vec<Utf8PathBuf> = Vec::new();
    let first = prompter
        .input("Path to secrets file", None)
        .context("prompting for first secrets path")?;
    secrets.push(Utf8PathBuf::from(first));

    while prompter
        .confirm("Specify additional secrets files?", false)
        .context("prompting for additional secrets confirm")?
    {
        let next = prompter
            .input("Path to secrets file", None)
            .context("prompting for additional secrets path")?;
        secrets.push(Utf8PathBuf::from(next));
    }

    Ok(secrets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::cmds::config::prompts::{PromptAnswer, ScriptedPrompter};
    use clap::Parser;

    fn empty_args() -> InitArgs {
        InitArgs {
            for_systemd_install: false,
            for_containerized_run: false,
            components: Vec::new(),
            scratch: None,
            containers_scratch: None,
            ccache: None,
            versions_dir: None,
            vault: None,
            secrets: Vec::new(),
        }
    }

    fn fully_specified_args() -> InitArgs {
        InitArgs {
            for_systemd_install: false,
            for_containerized_run: false,
            components: vec!["/c1".into(), "/c2".into()],
            scratch: Some("/scratch".into()),
            containers_scratch: Some("/scratch-containers".into()),
            ccache: Some("/ccache".into()),
            versions_dir: Some("/versions".into()),
            vault: Some("/vault.yaml".into()),
            secrets: vec!["/sec1".into()],
        }
    }

    // ---------- clap parse ----------

    #[test]
    fn clap_accepts_init_with_no_flags() {
        let cli = Cli::try_parse_from(["cbsbuild", "config", "init"]).expect("clap parse");
        match cli.command {
            crate::cli::Command::Config(super::super::ConfigCommand::Init(args)) => {
                assert!(!args.for_systemd_install);
                assert!(!args.for_containerized_run);
                assert!(args.components.is_empty());
                assert!(args.scratch.is_none());
                assert!(args.containers_scratch.is_none());
                assert!(args.ccache.is_none());
                assert!(args.versions_dir.is_none());
                assert!(args.vault.is_none());
                assert!(args.secrets.is_empty());
            }
            other => panic!("expected ConfigCommand::Init, got {other:?}"),
        }
    }

    #[test]
    fn clap_accepts_versions_dir_flag() {
        let cli = Cli::try_parse_from([
            "cbsbuild",
            "config",
            "init",
            "--for-systemd-install",
            "--versions-dir",
            "/opt/v",
        ])
        .expect("clap parse");
        match cli.command {
            crate::cli::Command::Config(super::super::ConfigCommand::Init(args)) => {
                assert_eq!(args.versions_dir.as_deref(), Some(Utf8Path::new("/opt/v")));
                assert!(args.for_systemd_install);
            }
            other => panic!("expected ConfigCommand::Init, got {other:?}"),
        }
    }

    // ---------- config_init_paths ----------

    #[test]
    fn config_init_paths_all_flags_supplied_zero_prompts() {
        let cwd = Utf8PathBuf::from("/tmp");
        let args = fully_specified_args();
        let mut prompter = ScriptedPrompter::new([]);
        let paths = config_init_paths(&mut prompter, &cwd, &args).expect("paths");
        assert_eq!(
            paths.components,
            vec![Utf8PathBuf::from("/c1"), Utf8PathBuf::from("/c2")]
        );
        assert_eq!(paths.scratch, Utf8PathBuf::from("/scratch"));
        assert_eq!(
            paths.scratch_containers,
            Utf8PathBuf::from("/scratch-containers")
        );
        assert_eq!(paths.ccache.as_deref(), Some(Utf8Path::new("/ccache")));
        assert_eq!(paths.versions.as_deref(), Some(Utf8Path::new("/versions")));
        assert!(prompter.calls.is_empty());
    }

    #[test]
    fn config_init_paths_prompts_for_every_field() {
        // cwd has no "components" dir → step 1's default-confirm is skipped.
        let tmp = tempfile::tempdir().expect("tempdir");
        let cwd = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8");
        let args = empty_args();
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true), // specify components paths?
            PromptAnswer::Input("/etc/components".into()),
            PromptAnswer::Confirm(false), // add another? no
            PromptAnswer::Input("/scratch".into()),
            PromptAnswer::Input("/scratch-containers".into()),
            PromptAnswer::Confirm(true), // specify ccache?
            PromptAnswer::Input("/ccache".into()),
            PromptAnswer::Confirm(true), // specify versions?
            PromptAnswer::Input("/versions".into()),
        ]);
        let paths = config_init_paths(&mut prompter, &cwd, &args).expect("paths");
        assert_eq!(paths.components, vec![Utf8PathBuf::from("/etc/components")]);
        assert_eq!(paths.scratch, Utf8PathBuf::from("/scratch"));
        assert_eq!(
            paths.scratch_containers,
            Utf8PathBuf::from("/scratch-containers")
        );
        assert_eq!(paths.ccache.as_deref(), Some(Utf8Path::new("/ccache")));
        assert_eq!(paths.versions.as_deref(), Some(Utf8Path::new("/versions")));
    }

    #[test]
    fn config_init_paths_uses_default_components_when_cwd_has_one() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cwd = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8");
        std::fs::create_dir(cwd.join("components")).expect("mkdir");
        let args = empty_args();
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),  // use default components?
            PromptAnswer::Confirm(false), // additional paths? no
            PromptAnswer::Input("/scratch".into()),
            PromptAnswer::Input("/scratch-containers".into()),
            PromptAnswer::Confirm(false), // ccache? no
            PromptAnswer::Confirm(false), // versions? no
        ]);
        let paths = config_init_paths(&mut prompter, &cwd, &args).expect("paths");
        assert_eq!(paths.components, vec![cwd.join("components")]);
        assert_eq!(paths.ccache, None);
        assert_eq!(paths.versions, None);
    }

    // ---------- config_init_storage ----------

    #[test]
    fn config_init_storage_declined_returns_none() {
        let mut prompter = ScriptedPrompter::new([PromptAnswer::Confirm(false)]);
        let result = config_init_storage(&mut prompter).expect("storage");
        assert!(result.is_none());
    }

    #[test]
    fn config_init_storage_accepts_s3_and_registry() {
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true), // configure storage?
            PromptAnswer::Confirm(true), // S3?
            PromptAnswer::Input("https://s3.local".into()),
            PromptAnswer::Input("art-bucket".into()),
            PromptAnswer::Input("art-loc".into()),
            PromptAnswer::Input("rel-bucket".into()),
            PromptAnswer::Input("rel-loc".into()),
            PromptAnswer::Confirm(true), // registry?
            PromptAnswer::Input("https://registry.local".into()),
        ]);
        let storage = config_init_storage(&mut prompter)
            .expect("storage")
            .expect("Some(StorageConfig)");
        let s3 = storage.s3.expect("s3");
        assert_eq!(s3.url, "https://s3.local");
        assert_eq!(s3.artifacts.bucket, "art-bucket");
        assert_eq!(s3.artifacts.loc, "art-loc");
        assert_eq!(s3.releases.bucket, "rel-bucket");
        assert_eq!(s3.releases.loc, "rel-loc");
        let registry = storage.registry.expect("registry");
        assert_eq!(registry.url, "https://registry.local");
    }

    #[test]
    fn config_init_storage_invalid_s3_url_bails() {
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),
            PromptAnswer::Confirm(true),
            PromptAnswer::Input("s3.local".into()), // no scheme → reject
        ]);
        let Err(err) = config_init_storage(&mut prompter) else {
            panic!("expected URL-validation bail");
        };
        assert!(err.to_string().contains("S3 storage URL"));
    }

    // ---------- config_init_signing ----------

    #[test]
    fn config_init_signing_declined_returns_none() {
        let mut prompter = ScriptedPrompter::new([PromptAnswer::Confirm(false)]);
        assert!(
            config_init_signing(&mut prompter)
                .expect("signing")
                .is_none()
        );
    }

    #[test]
    fn config_init_signing_gpg_only() {
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true), // configure?
            PromptAnswer::Confirm(true), // GPG?
            PromptAnswer::Input("gpg-secret".into()),
            PromptAnswer::Confirm(false), // transit?
        ]);
        let signing = config_init_signing(&mut prompter)
            .expect("signing")
            .expect("Some");
        assert_eq!(signing.gpg.as_deref(), Some("gpg-secret"));
        assert_eq!(signing.transit, None);
    }

    #[test]
    fn config_init_signing_neither_method_returns_none() {
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),  // configure?
            PromptAnswer::Confirm(false), // GPG? no
            PromptAnswer::Confirm(false), // transit? no
        ]);
        assert!(
            config_init_signing(&mut prompter)
                .expect("signing")
                .is_none()
        );
    }

    // ---------- config_init_secrets_paths ----------

    #[test]
    fn config_init_secrets_paths_returns_args_verbatim_when_supplied() {
        let mut prompter = ScriptedPrompter::new([]);
        let result = config_init_secrets_paths(
            &mut prompter,
            &[Utf8PathBuf::from("/a"), Utf8PathBuf::from("/b")],
        )
        .expect("secrets");
        assert_eq!(
            result,
            vec![Utf8PathBuf::from("/a"), Utf8PathBuf::from("/b")]
        );
        assert!(prompter.calls.is_empty());
    }

    #[test]
    fn config_init_secrets_paths_decline_returns_empty() {
        let mut prompter = ScriptedPrompter::new([PromptAnswer::Confirm(false)]);
        let result = config_init_secrets_paths(&mut prompter, &[]).expect("secrets");
        assert!(result.is_empty());
    }

    #[test]
    fn config_init_secrets_paths_loop() {
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),         // configure?
            PromptAnswer::Input("/sec1".into()), // first
            PromptAnswer::Confirm(true),         // another?
            PromptAnswer::Input("/sec2".into()), // second
            PromptAnswer::Confirm(false),        // another? no
        ]);
        let result = config_init_secrets_paths(&mut prompter, &[]).expect("secrets");
        assert_eq!(
            result,
            vec![Utf8PathBuf::from("/sec1"), Utf8PathBuf::from("/sec2")]
        );
    }

    // ---------- normalise_yaml_suffix ----------

    #[test]
    fn normalise_yaml_suffix_yaml_passthrough() {
        let p = Utf8Path::new("/etc/cbs/cbs-build.config.yaml");
        assert_eq!(normalise_yaml_suffix(p), p);
    }

    #[test]
    fn normalise_yaml_suffix_rewrites_json() {
        let p = Utf8Path::new("/etc/cbs/cbs-build.config.json");
        assert_eq!(
            normalise_yaml_suffix(p),
            Utf8PathBuf::from("/etc/cbs/cbs-build.config.yaml"),
        );
    }

    // ---------- config_init (full flow) ----------

    /// `await_holding_lock` is allowed: `CWD_LOCK` is the right
    /// serialisation primitive because the process cwd is shared
    /// across all tokio tasks; an async-aware mutex would solve
    /// the same problem at higher cost. Matches the rationale on
    /// the `versions::tests` cwd-mutating tests.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn config_init_full_flow_writes_config() {
        // Full-flow test under tempdir + ScriptedPrompter — verifies
        // that the assembled Config lands on disk and a post-write
        // `cbscore::config::load` round-trip recovers the same
        // Config. Mutates process cwd via the shared CWD_LOCK so
        // it doesn't race against other cwd-mutating tests in the
        // bin (see `cmds::shared::CWD_LOCK`).
        let _guard = crate::cmds::shared::CWD_LOCK.lock().expect("cwd lock");
        let tmp = tempfile::tempdir().expect("tempdir");
        let cwd = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8");
        let prev_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&cwd).expect("set cwd");
        let config_path = cwd.join("cbs-build.config.yaml");
        let args = empty_args();
        let mut prompter = ScriptedPrompter::new([
            // paths
            PromptAnswer::Confirm(true), // specify paths
            PromptAnswer::Input("/components".into()),
            PromptAnswer::Confirm(false), // add another? no
            PromptAnswer::Input("/scratch".into()),
            PromptAnswer::Input("/scratch-containers".into()),
            PromptAnswer::Confirm(false), // ccache? no
            PromptAnswer::Confirm(false), // versions? no
            // storage
            PromptAnswer::Confirm(false), // configure storage? no
            // signing
            PromptAnswer::Confirm(false), // configure signing? no
            // secrets
            PromptAnswer::Confirm(false), // specify secrets? no
            // write-confirm (file doesn't exist yet → no overwrite-confirm)
            PromptAnswer::Confirm(true), // write?
        ]);
        let result = config_init(&mut prompter, &args, &config_path).await;
        std::env::set_current_dir(&prev_cwd).expect("restore cwd");
        result.expect("config_init");
        assert!(config_path.exists(), "config file should be written");
        let loaded = cbscore::config::load(&config_path).await.expect("load");
        assert_eq!(
            loaded.paths.components,
            vec![Utf8PathBuf::from("/components")]
        );
        assert_eq!(loaded.paths.scratch, Utf8PathBuf::from("/scratch"));
        assert_eq!(loaded.storage, None);
        assert_eq!(loaded.signing, None);
        assert!(loaded.secrets.is_empty());
        assert_eq!(loaded.vault, None);
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn config_init_write_declined_bails() {
        let _guard = crate::cmds::shared::CWD_LOCK.lock().expect("cwd lock");
        let tmp = tempfile::tempdir().expect("tempdir");
        let cwd = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8");
        let prev_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&cwd).expect("set cwd");
        let config_path = cwd.join("cbs-build.config.yaml");
        let args = fully_specified_args();
        let mut prompter = ScriptedPrompter::new([
            // Paths + secrets are bypassed by the fully-specified args
            // (every path field set, secrets non-empty). Storage and
            // signing have no per-field bypass — each prompts once at
            // the top level, then declines. Then the final write-confirm
            // is declined to exercise the step 10 bail path.
            PromptAnswer::Confirm(false), // storage? no
            PromptAnswer::Confirm(false), // signing? no
            PromptAnswer::Confirm(false), // write? no
        ]);
        let result = config_init(&mut prompter, &args, &config_path).await;
        std::env::set_current_dir(&prev_cwd).expect("restore cwd");
        let Err(err) = result else {
            panic!("expected write-declined bail");
        };
        assert!(
            err.to_string().contains("do not write config files"),
            "unexpected error: {err}",
        );
        assert!(
            !config_path.exists(),
            "config must not be written on decline",
        );
    }
}
