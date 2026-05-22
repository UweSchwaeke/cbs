// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Cross-subcommand helpers — config + secrets loading, small
//! tracing helpers. Kept in a single module so the
//! "load config + secrets" boilerplate doesn't fan out across
//! every handler.

use anyhow::{Context, Result};
use camino::Utf8Path;
use cbscore::config;
use cbscore::secrets::SecretsMgr;
use cbscore_types::config::Config;

const TARGET_CBSBUILD_SHARED: &str = "cbsbuild::shared";

/// Load [`Config`] from `config_path`, then assemble a
/// [`SecretsMgr`] from `cfg.secrets`'s file list.
///
/// Vault-ref resolution (`SecretsMgr::resolve_vault_refs`) is
/// **not** invoked here — M1 ships with the optional-secrets
/// path; deployments that need vault-ref resolution wire it via
/// the secrets stage at runtime once the Phase 5 follow-up lands.
pub(crate) async fn load_config_and_secrets(
    config_path: &Utf8Path,
) -> Result<(Config, SecretsMgr)> {
    let cfg = config::load(config_path)
        .await
        .with_context(|| format!("loading config at '{config_path}'"))?;
    let secrets = SecretsMgr::load_files(&cfg.secrets)
        .await
        .context("loading secrets files")?;
    tracing::debug!(
        target: TARGET_CBSBUILD_SHARED,
        config = %config_path,
        secrets_files = cfg.secrets.len(),
        "config + secrets loaded",
    );
    Ok((cfg, secrets))
}

/// Emit a one-line "wrote report to <path>" trace at INFO so the
/// in-container `runner build` flow tells operators where the
/// artefact landed.
pub(crate) fn dump_yaml_path(path: &Utf8Path) {
    tracing::info!(
        target: TARGET_CBSBUILD_SHARED,
        path = %path,
        "report written",
    );
}

/// Test-only shared mutex for tests that mutate the process cwd.
///
/// `std::env::set_current_dir` mutates process-global state, so
/// tests that call it must serialise against every other such
/// test in the binary — including across module boundaries.
/// Tokio's multi-threaded test runtime would otherwise interleave
/// `set_current_dir` calls across `#[tokio::test]` tasks and
/// produce flaky failures.
///
/// All cwd-mutating tests in `cbsbuild` acquire this lock for the
/// duration of the cwd dance:
///
/// ```ignore
/// let _guard = crate::cmds::shared::CWD_LOCK.lock().expect("cwd lock");
/// std::env::set_current_dir(&tmp).expect("cd tmp");
/// // ...
/// ```
#[cfg(test)]
pub(crate) static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
