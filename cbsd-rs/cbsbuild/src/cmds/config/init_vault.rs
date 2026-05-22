// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild config init-vault` — interactive flow that walks the
//! operator through producing a `cbs-build.vault.yaml` file.
//!
//! Mirrors Python `cmd_config_init_vault` /
//! `config_init_vault` (`cbscore/src/cbscore/cmds/config.py`
//! lines 28–105) step-by-step:
//!
//! 0. Short-circuit when `--vault PATH` is supplied AND the file
//!    already exists — return that path unchanged, no prompts.
//! 1. Top-level `Confirm("Configure vault authentication?")` —
//!    `false` returns `Ok(None)` with no further prompts.
//! 2. Prompt for the vault config path (default
//!    `${cwd}/cbs-build.vault.yaml`).
//! 3. If that path exists, ask whether to overwrite — `false`
//!    returns `Ok(Some(path))` without writing.
//! 4. Prompt for the vault address; validated via
//!    [`crate::cmds::config::prompts::validate_url`].
//! 5. Try user/pass auth — if accepted, prompt username + password.
//! 6. Else try `AppRole` auth — prompt role-id + secret-id.
//! 7. Else fall back to a token prompt. Empty token bails (the
//!    Python flow exits with `errno.EINVAL` at this point; the
//!    Rust port surfaces the error message and exits via the
//!    `EXIT_UNRECOVERABLE` mapper. Tightening the exit code to
//!    `EXIT_INVAL` is a follow-up — the message is operator-clear
//!    either way).
//!
//! The flow is generic over [`Prompter`] so unit tests drive it
//! with a [`super::prompts::ScriptedPrompter`] rather than spawning
//! a TTY. Production calls construct a
//! [`super::prompts::DialoguerPrompter`].

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::config::{VaultAppRoleConfig, VaultConfig, VaultUserPassConfig};
use clap::Args;

use super::prompts::{DialoguerPrompter, Prompter, validate_url};

/// `cbsbuild config init-vault` arguments.
///
/// Matches the Python `cmd_config_init_vault` option set: a single
/// optional `--vault PATH` that names a pre-existing vault config
/// file. When supplied AND the file already exists, the flow
/// short-circuits and the path is echoed back unchanged.
#[derive(Debug, Args)]
pub(crate) struct InitVaultArgs {
    /// Path to an existing `cbs-build.vault.yaml`, or where one
    /// should be written. When supplied AND the file already
    /// exists, no prompts fire and the path is echoed back
    /// unchanged. Without this flag, the operator is prompted for
    /// a path (default `${cwd}/cbs-build.vault.yaml`).
    #[arg(long = "vault")]
    pub vault_config_path: Option<Utf8PathBuf>,
}

/// `cbsbuild config init-vault` top-level handler.
///
/// Constructs a [`DialoguerPrompter`], resolves the current
/// working directory, dispatches to [`config_init_vault`], and
/// prints the resulting path (or a "not configured" note) to
/// stdout for operator visibility.
pub(crate) async fn handle_init_vault(args: InitVaultArgs) -> Result<()> {
    let cwd_std = std::env::current_dir().context("resolving current working directory")?;
    let cwd = Utf8PathBuf::from_path_buf(cwd_std).map_err(|p| {
        anyhow::anyhow!(
            "current working directory '{}' is not valid UTF-8",
            p.display()
        )
    })?;

    let mut prompter = DialoguerPrompter;
    let result = config_init_vault(&mut prompter, &cwd, args.vault_config_path.as_deref()).await?;

    match result {
        Some(path) => println!("{path}"),
        None => println!("vault configuration not initialized"),
    }
    Ok(())
}

/// Prompter-driven core of `cbsbuild config init-vault`.
///
/// Returns:
///
/// - `Ok(Some(path))` when a vault config file was either
///   discovered (step 0 short-circuit), declined-to-overwrite
///   (step 3), or successfully written (steps 4–7).
/// - `Ok(None)` when the operator declined the top-level
///   "Configure vault authentication?" prompt (step 1).
/// - `Err(_)` for IO failures, URL validation failures, or empty
///   token (step 7).
///
/// # Errors
///
/// - URL validation failure on the Vault address (step 4) bails
///   with a context message including the parser diagnostic.
/// - Empty token on the token fallback (step 7) bails — the
///   Python flow uses `sys.exit(errno.EINVAL)` here; the Rust
///   port surfaces the message but maps to the default
///   `EXIT_UNRECOVERABLE` exit code (see module docs).
/// - Underlying `Prompter` IO failures propagate as
///   [`super::prompts::PromptError`].
/// - [`cbscore_types::config::ConfigError::Io`] from the vault
///   file write propagates with a context wrap naming the path.
pub(crate) async fn config_init_vault<P: Prompter>(
    prompter: &mut P,
    cwd: &Utf8Path,
    vault_config_path: Option<&Utf8Path>,
) -> Result<Option<Utf8PathBuf>> {
    // Step 0: pre-existing file short-circuit.
    if let Some(path) = vault_config_path
        && path.exists()
    {
        return Ok(Some(path.to_owned()));
    }

    // Step 1: top-level confirm.
    if !prompter
        .confirm("Configure vault authentication?", false)
        .context("prompting for vault-authentication confirm")?
    {
        return Ok(None);
    }

    // Step 2: vault config path with cwd-derived default.
    let default_vault_path = cwd.join("cbs-build.vault.yaml");
    let path_str = prompter
        .input("Vault config path", Some(default_vault_path.as_str()))
        .context("prompting for vault config path")?;
    let path = Utf8PathBuf::from(path_str);

    // Step 3: overwrite confirm if the path exists.
    if path.exists() {
        let overwrite_prompt = format!("Vault config path '{path}' already exists. Overwrite?");
        let overwrite = prompter
            .confirm(&overwrite_prompt, false)
            .context("prompting for overwrite confirm")?;
        if !overwrite {
            return Ok(Some(path));
        }
    }

    // Step 4: vault address + URL validation.
    let vault_addr = prompter
        .input("Vault address (incl. scheme, e.g. https://...)", None)
        .context("prompting for vault address")?;
    validate_url(&vault_addr).map_err(|e| anyhow::anyhow!("vault address: {e}"))?;

    // Steps 5–7: auth method selection.
    let (auth_user, auth_approle, auth_token) = prompt_auth(prompter)?;

    let vault = VaultConfig {
        vault_addr,
        auth_user,
        auth_approle,
        auth_token,
    };

    cbscore::config::store_vault(&vault, &path)
        .await
        .with_context(|| format!("writing vault config to '{path}'"))?;

    Ok(Some(path))
}

/// Auth-method ladder: user/pass → `AppRole` → token fallback.
///
/// Returns the three `Option`s in the order the [`VaultConfig`]
/// struct lays them out so the caller can paste them in directly.
/// Exactly one of the three is `Some` on a successful return; the
/// other two are `None`.
fn prompt_auth<P: Prompter>(
    prompter: &mut P,
) -> Result<(
    Option<VaultUserPassConfig>,
    Option<VaultAppRoleConfig>,
    Option<String>,
)> {
    // Step 5: user/pass.
    if prompter
        .confirm("Specify user/pass auth for vault?", false)
        .context("prompting for user/pass auth confirm")?
    {
        let username = prompter
            .input("Username", None)
            .context("prompting for vault username")?;
        let password = prompter
            .password("Password")
            .context("prompting for vault password")?;
        return Ok((Some(VaultUserPassConfig { username, password }), None, None));
    }

    // Step 6: `AppRole`.
    if prompter
        .confirm("Specify AppRole auth for vault?", false)
        .context("prompting for AppRole auth confirm")?
    {
        let role_id = prompter
            .input("Role ID", None)
            .context("prompting for AppRole role-id")?;
        let secret_id = prompter
            .password("Secret ID")
            .context("prompting for AppRole secret-id")?;
        return Ok((None, Some(VaultAppRoleConfig { role_id, secret_id }), None));
    }

    // Step 7: token fallback. Empty token bails.
    let token = prompter
        .password("Vault token")
        .context("prompting for vault token")?;
    if token.is_empty() {
        bail!("cbsbuild config init-vault: vault token cannot be empty");
    }
    Ok((None, None, Some(token)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::cmds::config::prompts::{PromptAnswer, ScriptedPrompter};
    use clap::Parser;

    fn make_cwd() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 cwd");
        (dir, path)
    }

    /// Assert the on-disk vault YAML carries the expected fields.
    ///
    /// We assert against the YAML body string directly (substring
    /// match) rather than round-tripping through
    /// `VersionedVaultConfig::from_value` — the latter would require
    /// adding `serde-saphyr` + `serde-value` as `cbsbuild`
    /// dev-dependencies just for the test path. The store-side
    /// wire-format correctness is already covered by the
    /// `cbscore::config::store_vault` round-trip tests in
    /// `cbscore/src/config.rs`; here we only need to verify that
    /// the right fields landed on disk in the right shape.
    async fn assert_vault_yaml(path: &Utf8Path, expectations: &[&str]) {
        let body = tokio::fs::read_to_string(path).await.expect("read vault");
        assert!(
            body.starts_with("schema-version: 1\n"),
            "expected schema-version marker first, got: {}",
            body.lines().next().unwrap_or(""),
        );
        for needle in expectations {
            assert!(
                body.contains(needle),
                "expected vault yaml to contain '{needle}', got body:\n{body}",
            );
        }
    }

    // -- Step 0 short-circuit -------------------------------------------------

    #[tokio::test]
    async fn pre_existing_vault_short_circuits() {
        let (_tmp, cwd) = make_cwd();
        let existing = cwd.join("existing.vault.yaml");
        tokio::fs::write(&existing, b"placeholder\n")
            .await
            .expect("write placeholder");

        let mut prompter = ScriptedPrompter::new([]);
        let result = config_init_vault(&mut prompter, &cwd, Some(&existing))
            .await
            .expect("short-circuit");
        assert_eq!(result, Some(existing.clone()));
        assert!(prompter.calls.is_empty(), "expected zero prompts");
        // The placeholder was not overwritten.
        let body = tokio::fs::read_to_string(&existing).await.expect("read");
        assert_eq!(body, "placeholder\n");
    }

    #[tokio::test]
    async fn vault_path_supplied_but_missing_falls_through_to_interactive() {
        // Step 0 short-circuits only when `vault_config_path` is
        // `Some(p)` AND `p.exists()`. When the path is supplied
        // but does not yet exist, control falls through to Step 1.
        // The test pins that behaviour — Step 1's confirm prompt
        // fires and a decline returns `Ok(None)`.
        let (_tmp, cwd) = make_cwd();
        let missing = cwd.join("not-yet-created.vault.yaml");
        assert!(!missing.exists(), "test setup: file must not exist");
        let mut prompter = ScriptedPrompter::new([PromptAnswer::Confirm(false)]);
        let result = config_init_vault(&mut prompter, &cwd, Some(&missing))
            .await
            .expect("fall-through");
        assert_eq!(result, None);
        assert_eq!(prompter.calls.len(), 1, "Step 1 confirm should fire");
    }

    // -- Step 1 decline -------------------------------------------------------

    #[tokio::test]
    async fn top_level_decline_returns_none() {
        let (_tmp, cwd) = make_cwd();
        let mut prompter = ScriptedPrompter::new([PromptAnswer::Confirm(false)]);
        let result = config_init_vault(&mut prompter, &cwd, None)
            .await
            .expect("decline");
        assert_eq!(result, None);
        assert_eq!(prompter.calls.len(), 1, "expected exactly one prompt");
    }

    // -- Step 5 user/pass path ------------------------------------------------

    #[tokio::test]
    async fn user_pass_path_writes_vault_file() {
        let (_tmp, cwd) = make_cwd();
        let target = cwd.join("vault.yaml");
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),                       // step 1
            PromptAnswer::Input(target.to_string()),           // step 2
            PromptAnswer::Input("https://vault.local".into()), // step 4
            PromptAnswer::Confirm(true),                       // step 5 user/pass yes
            PromptAnswer::Input("deploy".into()),              // username
            PromptAnswer::Password("hunter2".into()),          // password
        ]);
        let result = config_init_vault(&mut prompter, &cwd, None)
            .await
            .expect("user/pass path");
        assert_eq!(result, Some(target.clone()));
        assert_vault_yaml(
            &target,
            &[
                "vault-addr: https://vault.local",
                "auth-user:",
                "username: deploy",
                "password: hunter2",
            ],
        )
        .await;
        let body = tokio::fs::read_to_string(&target).await.expect("read");
        assert!(!body.contains("auth-approle"), "should not write approle");
        assert!(!body.contains("auth-token"), "should not write token");
    }

    // -- Step 6 `AppRole` path --------------------------------------------------

    #[tokio::test]
    async fn approle_path_writes_vault_file() {
        let (_tmp, cwd) = make_cwd();
        let target = cwd.join("vault.yaml");
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),
            PromptAnswer::Input(target.to_string()),
            PromptAnswer::Input("https://vault.local".into()),
            PromptAnswer::Confirm(false), // user/pass no
            PromptAnswer::Confirm(true),  // `AppRole` yes
            PromptAnswer::Input("role-id".into()),
            PromptAnswer::Password("secret-id".into()),
        ]);
        let result = config_init_vault(&mut prompter, &cwd, None)
            .await
            .expect("approle path");
        assert_eq!(result, Some(target.clone()));
        assert_vault_yaml(
            &target,
            &[
                "vault-addr: https://vault.local",
                "auth-approle:",
                "role-id: role-id",
                "secret-id: secret-id",
            ],
        )
        .await;
        let body = tokio::fs::read_to_string(&target).await.expect("read");
        assert!(!body.contains("auth-user"), "should not write user/pass");
        assert!(!body.contains("auth-token"), "should not write token");
    }

    // -- Step 7 token fallback ------------------------------------------------

    #[tokio::test]
    async fn token_fallback_writes_vault_file() {
        let (_tmp, cwd) = make_cwd();
        let target = cwd.join("vault.yaml");
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),
            PromptAnswer::Input(target.to_string()),
            PromptAnswer::Input("https://vault.local".into()),
            PromptAnswer::Confirm(false), // user/pass no
            PromptAnswer::Confirm(false), // `AppRole` no
            PromptAnswer::Password("hvs.AAAA".into()),
        ]);
        let result = config_init_vault(&mut prompter, &cwd, None)
            .await
            .expect("token fallback");
        assert_eq!(result, Some(target.clone()));
        assert_vault_yaml(
            &target,
            &["vault-addr: https://vault.local", "auth-token: hvs.AAAA"],
        )
        .await;
        let body = tokio::fs::read_to_string(&target).await.expect("read");
        assert!(!body.contains("auth-user"), "should not write user/pass");
        assert!(!body.contains("auth-approle"), "should not write approle");
    }

    #[tokio::test]
    async fn empty_token_bails() {
        let (_tmp, cwd) = make_cwd();
        let target = cwd.join("vault.yaml");
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),
            PromptAnswer::Input(target.to_string()),
            PromptAnswer::Input("https://vault.local".into()),
            PromptAnswer::Confirm(false),
            PromptAnswer::Confirm(false),
            PromptAnswer::Password(String::new()), // empty token
        ]);
        let Err(err) = config_init_vault(&mut prompter, &cwd, None).await else {
            panic!("expected empty-token bail");
        };
        assert!(
            err.to_string().contains("vault token cannot be empty"),
            "unexpected error: {err}",
        );
        assert!(!target.exists(), "vault file must not be written on bail",);
    }

    // -- Step 3 overwrite-declined path --------------------------------------

    #[tokio::test]
    async fn overwrite_declined_returns_path_without_writing() {
        let (_tmp, cwd) = make_cwd();
        let target = cwd.join("vault.yaml");
        tokio::fs::write(&target, b"placeholder\n")
            .await
            .expect("seed existing target");
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),             // step 1
            PromptAnswer::Input(target.to_string()), // step 2
            PromptAnswer::Confirm(false),            // step 3 do not overwrite
        ]);
        let result = config_init_vault(&mut prompter, &cwd, None)
            .await
            .expect("overwrite-declined");
        assert_eq!(result, Some(target.clone()));
        // The placeholder is intact — no write happened.
        let body = tokio::fs::read_to_string(&target).await.expect("read");
        assert_eq!(body, "placeholder\n");
    }

    // -- URL validation failure ----------------------------------------------

    #[tokio::test]
    async fn invalid_vault_address_bails() {
        let (_tmp, cwd) = make_cwd();
        let target = cwd.join("vault.yaml");
        let mut prompter = ScriptedPrompter::new([
            PromptAnswer::Confirm(true),
            PromptAnswer::Input(target.to_string()),
            PromptAnswer::Input("vault.local".into()), // no scheme → reject
        ]);
        let Err(err) = config_init_vault(&mut prompter, &cwd, None).await else {
            panic!("expected URL-validation bail");
        };
        assert!(
            err.to_string().contains("vault address"),
            "unexpected error: {err}",
        );
        assert!(!target.exists(), "vault file must not be written on bail",);
    }

    // -- Clap parse -----------------------------------------------------------

    #[test]
    fn clap_accepts_vault_flag() {
        let cli = Cli::try_parse_from([
            "cbsbuild",
            "config",
            "init-vault",
            "--vault",
            "/path/to/vault.yaml",
        ])
        .expect("clap parse");
        match cli.command {
            crate::cli::Command::Config(super::super::ConfigCommand::InitVault(args)) => {
                assert_eq!(
                    args.vault_config_path.as_deref(),
                    Some(Utf8Path::new("/path/to/vault.yaml")),
                );
            }
            other => panic!("expected ConfigCommand::InitVault, got {other:?}"),
        }
    }

    #[test]
    fn clap_accepts_init_vault_without_flag() {
        let cli =
            Cli::try_parse_from(["cbsbuild", "config", "init-vault"]).expect("clap parse no flag");
        match cli.command {
            crate::cli::Command::Config(super::super::ConfigCommand::InitVault(args)) => {
                assert!(args.vault_config_path.is_none());
            }
            other => panic!("expected ConfigCommand::InitVault, got {other:?}"),
        }
    }
}
