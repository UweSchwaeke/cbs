// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Async wrappers around the `buildah` CLI.
//!
//! Independent of [`crate::utils::podman`] — Phase 4's runner
//! orchestrates them. Phase 5 Commit 4's `containers::build` is the
//! primary consumer.

use cbscore_types::utils::buildah::BuildahError;

use crate::utils::subprocess::{CmdArg, RunOpts, RunOutcome, async_run_cmd};

// ---------------------------------------------------------------------
// buildah from
// ---------------------------------------------------------------------

/// Build the argv for `buildah from <image>`.
#[must_use]
pub fn buildah_from_argv(image: &str) -> Vec<String> {
    vec!["buildah".into(), "from".into(), image.to_owned()]
}

/// `buildah from` — create a new working container from `image`.
/// Returns the container ID (stdout's first non-empty line).
///
/// # Errors
///
/// Returns [`BuildahError::Failed`] on non-zero exit.
pub async fn buildah_from(image: &str) -> Result<String, BuildahError> {
    let argv = buildah_from_argv(image);
    let cmd: Vec<CmdArg> = argv.into_iter().map(CmdArg::Plain).collect();
    let outcome = run_and_classify("buildah from", &cmd, RunOpts::default()).await?;
    Ok(outcome
        .stdout
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_owned())
}

// ---------------------------------------------------------------------
// buildah commit
// ---------------------------------------------------------------------

/// Build the argv for `buildah commit <cid> <image>`.
#[must_use]
pub fn buildah_commit_argv(cid: &str, image: &str) -> Vec<String> {
    vec![
        "buildah".into(),
        "commit".into(),
        cid.to_owned(),
        image.to_owned(),
    ]
}

/// `buildah commit` — turn a working container into a final image.
///
/// # Errors
///
/// Returns [`BuildahError::Failed`] on non-zero exit.
pub async fn buildah_commit(cid: &str, image: &str) -> Result<(), BuildahError> {
    let argv = buildah_commit_argv(cid, image);
    let cmd: Vec<CmdArg> = argv.into_iter().map(CmdArg::Plain).collect();
    let _outcome = run_and_classify("buildah commit", &cmd, RunOpts::default()).await?;
    Ok(())
}

// ---------------------------------------------------------------------
// buildah unmount
// ---------------------------------------------------------------------

/// Build the argv for `buildah unmount <cid>`.
#[must_use]
pub fn buildah_unmount_argv(cid: &str) -> Vec<String> {
    vec!["buildah".into(), "unmount".into(), cid.to_owned()]
}

/// `buildah unmount` — unmount a working container's filesystem.
/// Already-unmounted containers produce a recoverable error
/// (`BuildahError::Failed` with the stderr from buildah).
///
/// # Errors
///
/// Returns [`BuildahError::Failed`] on non-zero exit. Callers that
/// want to ignore already-unmounted containers should inspect the
/// stderr message.
pub async fn buildah_unmount(cid: &str) -> Result<(), BuildahError> {
    let argv = buildah_unmount_argv(cid);
    let cmd: Vec<CmdArg> = argv.into_iter().map(CmdArg::Plain).collect();
    let _outcome = run_and_classify("buildah unmount", &cmd, RunOpts::default()).await?;
    Ok(())
}

// ---------------------------------------------------------------------
// buildah rm
// ---------------------------------------------------------------------

/// Build the argv for `buildah rm <cid>`.
#[must_use]
pub fn buildah_rm_argv(cid: &str) -> Vec<String> {
    vec!["buildah".into(), "rm".into(), cid.to_owned()]
}

/// `buildah rm` — remove a working container.
///
/// # Errors
///
/// Returns [`BuildahError::Failed`] on non-zero exit.
pub async fn buildah_rm(cid: &str) -> Result<(), BuildahError> {
    let argv = buildah_rm_argv(cid);
    let cmd: Vec<CmdArg> = argv.into_iter().map(CmdArg::Plain).collect();
    let _outcome = run_and_classify("buildah rm", &cmd, RunOpts::default()).await?;
    Ok(())
}

// ---------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------

async fn run_and_classify(
    op: &str,
    cmd: &[CmdArg],
    opts: RunOpts<'_>,
) -> Result<RunOutcome, BuildahError> {
    let outcome = async_run_cmd(cmd, opts)
        .await
        .map_err(|e| BuildahError::Failed {
            retcode: -1,
            stderr: format!("{op}: {e}"),
        })?;
    if outcome.rc == 0 {
        Ok(outcome)
    } else {
        Err(BuildahError::Failed {
            retcode: outcome.rc,
            stderr: outcome.stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_argv() {
        assert_eq!(
            buildah_from_argv("registry.fedoraproject.org/fedora:39"),
            vec!["buildah", "from", "registry.fedoraproject.org/fedora:39"],
        );
    }

    #[test]
    fn commit_argv() {
        assert_eq!(
            buildah_commit_argv("ces-build-abc123", "myreg/img:tag"),
            vec!["buildah", "commit", "ces-build-abc123", "myreg/img:tag"],
        );
    }

    #[test]
    fn unmount_argv() {
        assert_eq!(
            buildah_unmount_argv("ces-build-abc123"),
            vec!["buildah", "unmount", "ces-build-abc123"],
        );
    }

    #[test]
    fn rm_argv() {
        assert_eq!(
            buildah_rm_argv("ces-build-abc123"),
            vec!["buildah", "rm", "ces-build-abc123"],
        );
    }
}
