// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Async wrappers around the `podman` CLI.
//!
//! Pure subprocess drivers — every call goes through
//! [`async_run_cmd`](crate::utils::subprocess::async_run_cmd) and
//! returns either an [`Ok`] payload (with the captured exit code,
//! stdout, and stderr surfaced via the per-function shape) or
//! [`PodmanError`] when the wrapped invocation exits non-zero.
//!
//! Phase 4's runner consumes [`podman_run`] for container spawn and
//! [`podman_stop`] for cleanup; Phase 5's container build pipeline
//! consumes [`podman_pull`] and [`podman_image_inspect`].

use std::collections::HashMap;
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::utils::podman::PodmanError;

use crate::utils::subprocess::{CmdArg, RunOpts, RunOutcome, async_run_cmd};

// ---------------------------------------------------------------------
// podman run
// ---------------------------------------------------------------------

/// Configuration for [`podman_run`].
///
/// The four `bool` flags mirror independent podman CLI toggles
/// (`--replace`, `--userns keep-id`, `--network host`,
/// `--security-opt seccomp=unconfined`); collapsing them into a bitflag
/// or enum would obscure the one-to-one mapping to the underlying CLI.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct PodmanRunOpts<'a> {
    /// Container image reference (registry path + tag).
    pub image: String,
    /// Optional `--cidfile` path. The runner writes the container ID
    /// here so cleanup paths can recover it after a kill.
    pub cidfile: Option<&'a Utf8Path>,
    /// Optional `--name` for the container.
    pub name: Option<String>,
    /// Optional `--entrypoint` override.
    pub entrypoint: Option<String>,
    /// Environment passed via `--env KEY=VAL`.
    pub env: HashMap<String, String>,
    /// Bind mounts: source-host-path → in-container-path. Translated
    /// to `--volume host:container`.
    pub volumes: HashMap<Utf8PathBuf, String>,
    /// Devices: source-host-path → in-container-path. Translated to
    /// `--device host:container`.
    pub devices: HashMap<Utf8PathBuf, String>,
    /// When `true`, emit `--replace` so the run overwrites any
    /// container with the same `name`.
    pub replace_if_exists: bool,
    /// When `true`, emit `--userns keep-id` so the container's user
    /// namespace mirrors the host.
    pub use_user_ns: bool,
    /// When `true`, emit `--network host`.
    pub use_host_network: bool,
    /// When `true`, emit `--security-opt seccomp=unconfined`.
    pub unconfined: bool,
    /// Internal timeout (passed both to podman via `--timeout` and to
    /// the subprocess driver's [`RunOpts::timeout`]).
    pub timeout: Option<Duration>,
    /// Trailing positional args appended after the image.
    pub args: Vec<String>,
}

/// Build the argv for `podman run` from a [`PodmanRunOpts`]. Exposed
/// for command-construction tests.
#[must_use]
pub fn podman_run_argv(opts: &PodmanRunOpts<'_>) -> Vec<String> {
    let mut argv: Vec<String> = vec![
        "podman".into(),
        "run".into(),
        "--security-opt".into(),
        "label=disable".into(),
    ];

    if let Some(cidfile) = opts.cidfile {
        argv.push("--cidfile".into());
        argv.push(cidfile.as_str().to_owned());
    }
    argv.push("--attach".into());
    argv.push("stdout".into());
    argv.push("--attach".into());
    argv.push("stderr".into());
    if let Some(name) = &opts.name {
        argv.push("--name".into());
        argv.push(name.clone());
    }
    if opts.use_user_ns {
        argv.push("--userns".into());
        argv.push("keep-id".into());
    }
    if let Some(t) = opts.timeout {
        argv.push("--timeout".into());
        argv.push(t.as_secs().to_string());
    }
    if opts.unconfined {
        argv.push("--security-opt".into());
        argv.push("seccomp=unconfined".into());
    }
    if opts.replace_if_exists {
        argv.push("--replace".into());
    }
    // Sort env / volumes / devices by key for deterministic argv;
    // matters for tests and for trace-line stability.
    let mut env: Vec<_> = opts.env.iter().collect();
    env.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in env {
        argv.push("--env".into());
        argv.push(format!("{k}={v}"));
    }
    let mut vols: Vec<_> = opts.volumes.iter().collect();
    vols.sort_by(|a, b| a.0.cmp(b.0));
    for (src, dst) in vols {
        argv.push("--volume".into());
        argv.push(format!("{src}:{dst}"));
    }
    let mut devs: Vec<_> = opts.devices.iter().collect();
    devs.sort_by(|a, b| a.0.cmp(b.0));
    for (src, dst) in devs {
        argv.push("--device".into());
        argv.push(format!("{src}:{dst}"));
    }
    if opts.use_host_network {
        argv.push("--network".into());
        argv.push("host".into());
    }
    if let Some(ep) = &opts.entrypoint {
        argv.push("--entrypoint".into());
        argv.push(ep.clone());
    }
    argv.push(opts.image.clone());
    argv.extend(opts.args.iter().cloned());

    argv
}

/// Run a podman container with the supplied options.
///
/// # Errors
///
/// Returns [`PodmanError::Failed`] if podman exits with a non-zero
/// status (carries the exit code + captured stderr).
///
/// # Examples
///
/// ```no_run
/// use cbscore::utils::podman::{podman_run, PodmanRunOpts};
///
/// # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
/// let outcome = podman_run(PodmanRunOpts {
///     image: "alpine:latest".into(),
///     args: vec!["echo".into(), "hello".into()],
///     ..Default::default()
/// }).await?;
/// assert_eq!(outcome.rc, 0);
/// # Ok(()) }
/// ```
pub async fn podman_run(opts: PodmanRunOpts<'_>) -> Result<RunOutcome, PodmanError> {
    let argv = podman_run_argv(&opts);
    let cmd: Vec<CmdArg> = argv.into_iter().map(CmdArg::Plain).collect();
    let run_opts = RunOpts {
        timeout: opts.timeout,
        ..Default::default()
    };
    run_and_classify("podman run", &cmd, run_opts).await
}

// ---------------------------------------------------------------------
// podman stop
// ---------------------------------------------------------------------

/// Build the argv for `podman stop`. Exposed for command-construction
/// tests.
#[must_use]
pub fn podman_stop_argv(name: Option<&str>, timeout: Duration) -> Vec<String> {
    let target = name.unwrap_or("--all").to_owned();
    vec![
        "podman".into(),
        "stop".into(),
        "--time".into(),
        timeout.as_secs().to_string(),
        target,
    ]
}

/// Stop a single container by `name`, or every running container when
/// `name` is `None` (the `--all` form).
///
/// Mirrors Python `cbscore.utils.podman.podman_stop`'s optional-name
/// behaviour 1:1 — Phase 4 Commit 2's `runner::stop(None, …)`
/// delegates directly without routing through a second helper.
///
/// # Errors
///
/// Returns [`PodmanError::Failed`] on non-zero exit.
pub async fn podman_stop(name: Option<&str>, timeout: Duration) -> Result<(), PodmanError> {
    let argv = podman_stop_argv(name, timeout);
    let cmd: Vec<CmdArg> = argv.into_iter().map(CmdArg::Plain).collect();
    let _outcome = run_and_classify("podman stop", &cmd, RunOpts::default()).await?;
    Ok(())
}

// ---------------------------------------------------------------------
// podman pull
// ---------------------------------------------------------------------

/// Build the argv for `podman pull`.
#[must_use]
pub fn podman_pull_argv(image_ref: &str) -> Vec<String> {
    vec!["podman".into(), "pull".into(), image_ref.to_owned()]
}

/// Pull an image from a registry.
///
/// # Errors
///
/// Returns [`PodmanError::Failed`] on non-zero exit.
pub async fn podman_pull(image_ref: &str) -> Result<(), PodmanError> {
    let argv = podman_pull_argv(image_ref);
    let cmd: Vec<CmdArg> = argv.into_iter().map(CmdArg::Plain).collect();
    let _outcome = run_and_classify("podman pull", &cmd, RunOpts::default()).await?;
    Ok(())
}

// ---------------------------------------------------------------------
// podman image inspect
// ---------------------------------------------------------------------

/// Build the argv for `podman image inspect`.
#[must_use]
pub fn podman_image_inspect_argv(image_ref: &str) -> Vec<String> {
    vec![
        "podman".into(),
        "image".into(),
        "inspect".into(),
        image_ref.to_owned(),
    ]
}

/// Minimal image metadata returned by [`podman_image_inspect`].
///
/// The full inspect payload is JSON with dozens of fields; this struct
/// surfaces only what cbscore-rs callers consume. Extend as later
/// consumers need more fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMeta {
    /// Image ID (the sha256 from `podman image inspect`).
    pub id: String,
    /// Captured stdout JSON for downstream callers needing more
    /// detail than [`id`](Self::id) alone.
    pub raw: String,
}

/// Inspect an image and return its ID + the raw JSON payload.
///
/// # Errors
///
/// Returns [`PodmanError::Failed`] on non-zero exit (image not found,
/// etc.).
pub async fn podman_image_inspect(image_ref: &str) -> Result<ImageMeta, PodmanError> {
    let argv = podman_image_inspect_argv(image_ref);
    let cmd: Vec<CmdArg> = argv.into_iter().map(CmdArg::Plain).collect();
    let outcome = run_and_classify("podman image inspect", &cmd, RunOpts::default()).await?;
    // Best-effort ID extraction; callers needing structured access use
    // `outcome.stdout` directly via `raw`.
    let id = outcome
        .stdout
        .split_once("\"Id\":")
        .and_then(|(_, rest)| rest.split('"').nth(1))
        .unwrap_or("")
        .to_owned();
    Ok(ImageMeta {
        id,
        raw: outcome.stdout,
    })
}

// ---------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------

async fn run_and_classify(
    op: &str,
    cmd: &[CmdArg],
    opts: RunOpts<'_>,
) -> Result<RunOutcome, PodmanError> {
    let outcome = async_run_cmd(cmd, opts)
        .await
        .map_err(|e| PodmanError::Failed {
            retcode: -1,
            stderr: format!("{op}: {e}"),
        })?;
    if outcome.rc == 0 {
        Ok(outcome)
    } else {
        Err(PodmanError::Failed {
            retcode: outcome.rc,
            stderr: outcome.stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_argv_minimal() {
        let opts = PodmanRunOpts {
            image: "alpine:latest".into(),
            ..Default::default()
        };
        let argv = podman_run_argv(&opts);
        assert_eq!(
            argv,
            vec![
                "podman",
                "run",
                "--security-opt",
                "label=disable",
                "--attach",
                "stdout",
                "--attach",
                "stderr",
                "alpine:latest",
            ],
        );
    }

    #[test]
    fn run_argv_full() {
        let tmpdir = tempfile::tempdir().unwrap();
        let cidfile = camino::Utf8PathBuf::from_path_buf(tmpdir.path().join("cid")).unwrap();
        let mut env = HashMap::new();
        env.insert("FOO".to_owned(), "bar".to_owned());
        let mut volumes = HashMap::new();
        volumes.insert(
            camino::Utf8PathBuf::from("/host/a"),
            "/container/a".to_owned(),
        );
        let opts = PodmanRunOpts {
            image: "myreg/img:tag".into(),
            cidfile: Some(cidfile.as_path()),
            name: Some("test-c".into()),
            entrypoint: Some("/bin/sh".into()),
            env,
            volumes,
            replace_if_exists: true,
            use_host_network: true,
            args: vec!["-c".into(), "echo hi".into()],
            ..Default::default()
        };
        let argv = podman_run_argv(&opts);
        assert!(argv.contains(&"--cidfile".to_owned()));
        assert!(argv.contains(&cidfile.as_str().to_owned()));
        assert!(argv.contains(&"--name".to_owned()));
        assert!(argv.contains(&"test-c".to_owned()));
        assert!(argv.contains(&"--replace".to_owned()));
        assert!(argv.contains(&"--network".to_owned()));
        assert!(argv.contains(&"host".to_owned()));
        assert!(argv.contains(&"--entrypoint".to_owned()));
        assert!(argv.contains(&"/bin/sh".to_owned()));
        assert!(argv.contains(&"--env".to_owned()));
        assert!(argv.contains(&"FOO=bar".to_owned()));
        assert!(argv.contains(&"--volume".to_owned()));
        assert!(argv.contains(&"/host/a:/container/a".to_owned()));
        // image + trailing args are at the end, in order
        let img_idx = argv.iter().position(|a| a == "myreg/img:tag").unwrap();
        assert_eq!(argv[img_idx + 1], "-c");
        assert_eq!(argv[img_idx + 2], "echo hi");
    }

    #[test]
    fn stop_argv_named() {
        assert_eq!(
            podman_stop_argv(Some("ces_abcdef0123"), Duration::from_secs(1)),
            vec!["podman", "stop", "--time", "1", "ces_abcdef0123"],
        );
    }

    #[test]
    fn stop_argv_all() {
        assert_eq!(
            podman_stop_argv(None, Duration::from_secs(1)),
            vec!["podman", "stop", "--time", "1", "--all"],
        );
    }

    #[test]
    fn pull_argv() {
        assert_eq!(
            podman_pull_argv("quay.io/cbs/img:tag"),
            vec!["podman", "pull", "quay.io/cbs/img:tag"],
        );
    }

    #[test]
    fn image_inspect_argv() {
        assert_eq!(
            podman_image_inspect_argv("quay.io/cbs/img:tag"),
            vec!["podman", "image", "inspect", "quay.io/cbs/img:tag"],
        );
    }
}
