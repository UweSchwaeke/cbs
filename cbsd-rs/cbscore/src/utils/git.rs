// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Async wrappers around the `git` CLI (>= 2.23 — relies on
//! `git switch` and `git branch --show-current` per design 002
//! §Capability Mapping).
//!
//! Lift-out invariant per design 001: this module depends only on
//! [`crate::utils::subprocess`] + the [`errors::GitError`] type (which
//! itself depends only on `cbscore_types::utils::subprocess` and
//! `thiserror`). A future move to a `cbscommon-rs` crate is a
//! mechanical edit, not a rewrite.

pub mod errors;

pub use errors::GitError;

use std::collections::HashMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::utils::subprocess::{CmdArg, RunOpts, RunOutcome, async_run_cmd};

// ---------------------------------------------------------------------
// command-construction helpers
// ---------------------------------------------------------------------

/// Build the argv for `git ls-remote <url>`. `url` is a [`CmdArg`] so
/// auth-bearing URLs can stay wrapped in [`CmdArg::Secure`] for
/// trace-line redaction.
#[must_use]
pub fn git_ls_remote_cmd(url: CmdArg) -> Vec<CmdArg> {
    vec![CmdArg::from("git"), CmdArg::from("ls-remote"), url]
}

/// Build the argv for `git clone <url> <dest>`.
#[must_use]
pub fn git_clone_cmd(url: CmdArg, dest: &Utf8Path) -> Vec<CmdArg> {
    vec![
        CmdArg::from("git"),
        CmdArg::from("clone"),
        url,
        CmdArg::Plain(dest.as_str().to_owned()),
    ]
}

/// Build the argv for `git -C <repo> fetch [--tags] [--prune]`.
#[must_use]
pub fn git_fetch_cmd(repo_path: &Utf8Path) -> Vec<CmdArg> {
    vec![
        CmdArg::from("git"),
        CmdArg::from("-C"),
        CmdArg::Plain(repo_path.as_str().to_owned()),
        CmdArg::from("fetch"),
        CmdArg::from("--tags"),
        CmdArg::from("--prune"),
    ]
}

/// Build the argv for `git -C <repo> describe --always [--tags]`.
#[must_use]
pub fn git_describe_cmd(repo_path: &Utf8Path) -> Vec<CmdArg> {
    vec![
        CmdArg::from("git"),
        CmdArg::from("-C"),
        CmdArg::Plain(repo_path.as_str().to_owned()),
        CmdArg::from("describe"),
        CmdArg::from("--always"),
        CmdArg::from("--tags"),
    ]
}

/// Build the argv for `git -C <repo> switch [--detach] <ref>`.
#[must_use]
pub fn git_switch_cmd(repo_path: &Utf8Path, ref_: &str, detach: bool) -> Vec<CmdArg> {
    let mut cmd = vec![
        CmdArg::from("git"),
        CmdArg::from("-C"),
        CmdArg::Plain(repo_path.as_str().to_owned()),
        CmdArg::from("switch"),
    ];
    if detach {
        cmd.push(CmdArg::from("--detach"));
    }
    cmd.push(CmdArg::Plain(ref_.to_owned()));
    cmd
}

/// Build the argv for `git -C <repo> branch --show-current`.
#[must_use]
pub fn git_branch_show_current_cmd(repo_path: &Utf8Path) -> Vec<CmdArg> {
    vec![
        CmdArg::from("git"),
        CmdArg::from("-C"),
        CmdArg::Plain(repo_path.as_str().to_owned()),
        CmdArg::from("branch"),
        CmdArg::from("--show-current"),
    ]
}

/// Build the argv for `git -C <repo> rev-parse <ref>`.
#[must_use]
pub fn git_rev_parse_cmd(repo_path: &Utf8Path, ref_: &str) -> Vec<CmdArg> {
    vec![
        CmdArg::from("git"),
        CmdArg::from("-C"),
        CmdArg::Plain(repo_path.as_str().to_owned()),
        CmdArg::from("rev-parse"),
        CmdArg::Plain(ref_.to_owned()),
    ]
}

/// Build the argv for `git rev-parse --show-toplevel`. Invoked from
/// the current working directory.
#[must_use]
pub fn repo_root_cmd() -> Vec<CmdArg> {
    vec![
        CmdArg::from("git"),
        CmdArg::from("rev-parse"),
        CmdArg::from("--show-toplevel"),
    ]
}

// ---------------------------------------------------------------------
// async wrappers
// ---------------------------------------------------------------------

/// `git ls-remote <url>` — list refs on the remote.
///
/// Returns a `HashMap` of `ref → SHA`. Each output line has the shape
/// `<sha>\t<ref>` — the function parses each line into the map.
///
/// # Errors
///
/// Returns [`GitError::Failed`] on non-zero exit; [`GitError::Command`]
/// on subprocess driver failures (spawn / IO / timeout).
pub async fn git_ls_remote(url: CmdArg) -> Result<HashMap<String, String>, GitError> {
    let cmd = git_ls_remote_cmd(url);
    let outcome = run_and_classify(&cmd, RunOpts::default()).await?;
    let mut refs = HashMap::new();
    for line in outcome.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((sha, ref_name)) = line.split_once('\t') {
            refs.insert(ref_name.to_owned(), sha.to_owned());
        }
    }
    Ok(refs)
}

/// `git clone <url> <dest>`.
///
/// # Errors
///
/// Returns [`GitError::Failed`] on non-zero exit.
pub async fn git_clone(url: CmdArg, dest: &Utf8Path) -> Result<(), GitError> {
    let cmd = git_clone_cmd(url, dest);
    run_and_classify(&cmd, RunOpts::default()).await?;
    Ok(())
}

/// `git -C <repo> fetch --tags --prune`.
///
/// # Errors
///
/// Returns [`GitError::Failed`] on non-zero exit.
pub async fn git_fetch(repo_path: &Utf8Path) -> Result<(), GitError> {
    let cmd = git_fetch_cmd(repo_path);
    run_and_classify(&cmd, RunOpts::default()).await?;
    Ok(())
}

/// `git -C <repo> describe --always --tags` — return the describe
/// output (commit / nearest tag).
///
/// # Errors
///
/// Returns [`GitError::Failed`] on non-zero exit.
pub async fn git_describe(repo_path: &Utf8Path) -> Result<String, GitError> {
    let cmd = git_describe_cmd(repo_path);
    let outcome = run_and_classify(&cmd, RunOpts::default()).await?;
    Ok(outcome.stdout.trim().to_owned())
}

/// `git -C <repo> switch [--detach] <ref>`.
///
/// # Errors
///
/// Returns [`GitError::Failed`] on non-zero exit.
pub async fn git_switch(repo_path: &Utf8Path, ref_: &str, detach: bool) -> Result<(), GitError> {
    let cmd = git_switch_cmd(repo_path, ref_, detach);
    run_and_classify(&cmd, RunOpts::default()).await?;
    Ok(())
}

/// `git -C <repo> branch --show-current` — return the current branch
/// name. Returns `None` when in a detached HEAD state (`git`'s output
/// is empty).
///
/// # Errors
///
/// Returns [`GitError::Failed`] on non-zero exit.
pub async fn git_branch_show_current(repo_path: &Utf8Path) -> Result<Option<String>, GitError> {
    let cmd = git_branch_show_current_cmd(repo_path);
    let outcome = run_and_classify(&cmd, RunOpts::default()).await?;
    let name = outcome.stdout.trim();
    if name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(name.to_owned()))
    }
}

/// `git -C <repo> rev-parse <ref>` — resolve a ref to its SHA.
///
/// # Errors
///
/// Returns [`GitError::Failed`] on non-zero exit.
pub async fn git_rev_parse(repo_path: &Utf8Path, ref_: &str) -> Result<String, GitError> {
    let cmd = git_rev_parse_cmd(repo_path, ref_);
    let outcome = run_and_classify(&cmd, RunOpts::default()).await?;
    Ok(outcome.stdout.trim().to_owned())
}

/// `git rev-parse --show-toplevel` from the current working directory.
/// Returns the absolute path of the git checkout's top-level
/// directory.
///
/// Used by [`crate::versions::resolve_root`] as the third-precedence
/// fallback when neither `--versions-dir` nor `Config.paths.versions`
/// is set; `NotInRepo` distinguishes the operator-actionable
/// "outside a git checkout" case from other rev-parse failures.
///
/// # Errors
///
/// Returns [`GitError::NotInRepo`] when the cwd is not inside a git
/// checkout (rev-parse exits non-zero with a "not a git repository"
/// stderr); [`GitError::Failed`] for any other non-zero exit;
/// [`GitError::Command`] on subprocess driver failures.
pub async fn repo_root() -> Result<Utf8PathBuf, GitError> {
    let cmd = repo_root_cmd();
    let outcome = async_run_cmd(&cmd, RunOpts::default()).await?;
    if outcome.rc == 0 {
        let trimmed = outcome.stdout.trim();
        return Ok(Utf8PathBuf::from(trimmed));
    }
    // Non-zero exit: distinguish "not in a repo" from real failure.
    if outcome
        .stderr
        .to_lowercase()
        .contains("not a git repository")
    {
        return Err(GitError::NotInRepo);
    }
    Err(GitError::Failed {
        retcode: outcome.rc,
        stderr: outcome.stderr,
    })
}

// ---------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------

async fn run_and_classify(cmd: &[CmdArg], opts: RunOpts<'_>) -> Result<RunOutcome, GitError> {
    let outcome = async_run_cmd(cmd, opts).await?;
    if outcome.rc == 0 {
        Ok(outcome)
    } else {
        Err(GitError::Failed {
            retcode: outcome.rc,
            stderr: outcome.stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::subprocess::SecureUrl;

    fn debug_args(cmd: &[CmdArg]) -> Vec<String> {
        cmd.iter().map(|a| format!("{a:?}")).collect()
    }

    #[test]
    fn ls_remote_cmd_plain() {
        let cmd = git_ls_remote_cmd(CmdArg::from("https://github.com/ceph/ceph.git"));
        let args = debug_args(&cmd);
        assert_eq!(
            args,
            vec![
                "\"git\"",
                "\"ls-remote\"",
                "\"https://github.com/ceph/ceph.git\"",
            ],
        );
    }

    #[test]
    fn clone_cmd_redacts_token_url() {
        let url = CmdArg::Secure(Box::new(SecureUrl::new(
            "https",
            "token-user",
            "ghp_secrettoken",
            "github.com",
            "/ceph/ceph.git",
        )));
        let dest = Utf8Path::new("/tmp/repos/ceph");
        let cmd = git_clone_cmd(url, dest);
        let args = debug_args(&cmd);
        assert_eq!(args[0], "\"git\"");
        assert_eq!(args[1], "\"clone\"");
        // Secure URL emits its redacted form via Debug.
        assert_eq!(args[2], "https://token-user:****@github.com/ceph/ceph.git",);
        assert_eq!(args[3], "\"/tmp/repos/ceph\"");
        assert!(args.iter().all(|a| !a.contains("ghp_secrettoken")));
    }

    #[test]
    fn fetch_cmd() {
        let cmd = git_fetch_cmd(Utf8Path::new("/repos/ceph"));
        assert_eq!(
            debug_args(&cmd),
            vec![
                "\"git\"",
                "\"-C\"",
                "\"/repos/ceph\"",
                "\"fetch\"",
                "\"--tags\"",
                "\"--prune\"",
            ],
        );
    }

    #[test]
    fn describe_cmd() {
        let cmd = git_describe_cmd(Utf8Path::new("/repos/ceph"));
        assert_eq!(
            debug_args(&cmd),
            vec![
                "\"git\"",
                "\"-C\"",
                "\"/repos/ceph\"",
                "\"describe\"",
                "\"--always\"",
                "\"--tags\"",
            ],
        );
    }

    #[test]
    fn switch_cmd_with_detach() {
        let cmd = git_switch_cmd(Utf8Path::new("/repos/ceph"), "v19.2.3", true);
        assert_eq!(
            debug_args(&cmd),
            vec![
                "\"git\"",
                "\"-C\"",
                "\"/repos/ceph\"",
                "\"switch\"",
                "\"--detach\"",
                "\"v19.2.3\"",
            ],
        );
    }

    #[test]
    fn switch_cmd_without_detach() {
        let cmd = git_switch_cmd(Utf8Path::new("/repos/ceph"), "main", false);
        assert_eq!(
            debug_args(&cmd),
            vec![
                "\"git\"",
                "\"-C\"",
                "\"/repos/ceph\"",
                "\"switch\"",
                "\"main\"",
            ],
        );
    }

    #[test]
    fn branch_show_current_cmd() {
        let cmd = git_branch_show_current_cmd(Utf8Path::new("/repos/ceph"));
        assert_eq!(
            debug_args(&cmd),
            vec![
                "\"git\"",
                "\"-C\"",
                "\"/repos/ceph\"",
                "\"branch\"",
                "\"--show-current\"",
            ],
        );
    }

    #[test]
    fn rev_parse_cmd() {
        let cmd = git_rev_parse_cmd(Utf8Path::new("/repos/ceph"), "HEAD");
        assert_eq!(
            debug_args(&cmd),
            vec![
                "\"git\"",
                "\"-C\"",
                "\"/repos/ceph\"",
                "\"rev-parse\"",
                "\"HEAD\"",
            ],
        );
    }

    #[test]
    fn repo_root_cmd_is_constant() {
        assert_eq!(
            debug_args(&repo_root_cmd()),
            vec!["\"git\"", "\"rev-parse\"", "\"--show-toplevel\""],
        );
    }
}
