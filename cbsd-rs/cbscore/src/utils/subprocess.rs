// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Async subprocess driver with secret redaction.
//!
//! This module is the foundation every later subsystem wrapper builds
//! on. Every `podman`, `buildah`, `skopeo`, `git`, … invocation goes
//! through [`async_run_cmd`].
//!
//! # Secret redaction
//!
//! Two complementary mechanisms keep secrets out of trace lines and
//! error messages:
//!
//! - The [`SecureArg`] trait — caller wraps a credential in
//!   [`Password`], [`PasswordArg`], or [`SecureUrl`] and pushes the
//!   resulting [`CmdArg::Secure`] onto the argv slice. [`CmdArg`]'s
//!   `Debug` impl emits the [`SecureArg::redacted`] form, so any
//!   `tracing::debug!(cmd = ?…)` line that captures the argv prints
//!   `"****"` (or a templated equivalent) in place of plaintext.
//!   Plaintext is only ever exposed by [`SecureArg::plaintext`],
//!   which the driver calls exactly once when passing the argument
//!   to [`tokio::process::Command`].
//!
//! - [`sanitize_cmd`] — a runtime walker that handles after-the-fact
//!   redaction of `--passphrase` / `--password` flag patterns, for
//!   call sites that didn't (or couldn't) use the typed-`CmdArg` path.
//!   Matches the two-token (`--passphrase value`) and one-token
//!   (`--passphrase=value`) forms.
//!
//! # Timeout / cancellation
//!
//! [`async_run_cmd`] owns its [`RunOpts::timeout`]. The caller never
//! observes a future-drop cancellation as [`CommandError::Timeout`];
//! the variant is reserved for the internal-timeout path. On timeout,
//! the driver calls `Child::start_kill()`, reaps the child via
//! `Child::wait().await`, and returns
//! `Err(CommandError::Timeout { after })` with whatever stdout/stderr
//! was captured up to the kill.
//!
//! Outer cancellation (e.g. a `tokio::select!` branch dropping the
//! `async_run_cmd` future) triggers an internal RAII guard that
//! signals a sibling task to kill the child. The kill is best-effort
//! — the future may not be polled to completion, so no reap is
//! performed by the guard itself.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use camino::Utf8Path;
use cbscore_types::logger::TARGET_UTILS_SUBPROCESS;
use cbscore_types::utils::subprocess::CommandError;
use regex::Regex;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;

// ---------------------------------------------------------------------
// SecureArg + concrete impls
// ---------------------------------------------------------------------

/// A command-line argument whose plaintext form must not leak into
/// trace lines or error messages.
///
/// Implementors expose [`plaintext`](Self::plaintext) for the spawn
/// site and [`redacted`](Self::redacted) for tracing. The default
/// [`redacted`](Self::redacted) impl returns `"****"`; types that
/// carry useful non-secret context (e.g. [`PasswordArg`]'s flag name)
/// override it.
pub trait SecureArg: Send + Sync {
    /// Rendered cleartext — only used when actually spawning the
    /// child process.
    fn plaintext(&self) -> Cow<'_, str>;

    /// Rendered redacted form — used for tracing and error messages.
    /// Defaults to `"****"`.
    fn redacted(&self) -> Cow<'_, str> {
        Cow::Borrowed("****")
    }
}

/// A single CLI argument — either plain text or a wrapped secret.
///
/// The `Debug` impl delegates to [`SecureArg::redacted`] for the
/// [`Secure`](Self::Secure) variant, so a
/// `tracing::debug!(cmd = ?args)` line never prints plaintext
/// credentials.
pub enum CmdArg {
    /// Plain-text argument; safe to log verbatim.
    Plain(String),
    /// Secret-bearing argument; logs as its redacted form.
    Secure(Box<dyn SecureArg>),
}

impl fmt::Debug for CmdArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plain(s) => fmt::Debug::fmt(s, f),
            Self::Secure(a) => f.write_str(&a.redacted()),
        }
    }
}

impl From<&str> for CmdArg {
    fn from(s: &str) -> Self {
        Self::Plain(s.to_owned())
    }
}

impl From<String> for CmdArg {
    fn from(s: String) -> Self {
        Self::Plain(s)
    }
}

/// Wraps a plaintext password. [`SecureArg::redacted`] returns
/// `"****"`.
///
/// # Examples
///
/// ```
/// use cbscore::utils::subprocess::{Password, SecureArg};
///
/// let p = Password::new("hunter2");
/// assert_eq!(&*p.plaintext(), "hunter2");
/// assert_eq!(&*p.redacted(), "****");
/// ```
pub struct Password(String);

impl Password {
    /// Construct from a plaintext string.
    #[must_use]
    pub fn new(p: impl Into<String>) -> Self {
        Self(p.into())
    }
}

impl SecureArg for Password {
    fn plaintext(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.0)
    }
}

/// `--flag=password`-style argument. Redacts to `--flag=****`,
/// preserving the flag name for operator-visible diagnostics.
///
/// # Examples
///
/// ```
/// use cbscore::utils::subprocess::{PasswordArg, SecureArg};
///
/// let a = PasswordArg::new("--passphrase", "hunter2");
/// assert_eq!(&*a.plaintext(), "--passphrase=hunter2");
/// assert_eq!(&*a.redacted(), "--passphrase=****");
/// ```
pub struct PasswordArg {
    flag: String,
    password: String,
}

impl PasswordArg {
    /// Construct from a flag name (e.g. `--passphrase`) and a
    /// plaintext password.
    #[must_use]
    pub fn new(flag: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            flag: flag.into(),
            password: password.into(),
        }
    }
}

impl SecureArg for PasswordArg {
    fn plaintext(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}={}", self.flag, self.password))
    }
    fn redacted(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}=****", self.flag))
    }
}

/// URL with embedded `user:password@host` credentials. Redacts to
/// `scheme://user:****@host/path`, preserving the username for
/// operator-visible debugging.
///
/// # Examples
///
/// ```
/// use cbscore::utils::subprocess::{SecureArg, SecureUrl};
///
/// let u = SecureUrl::new("https", "alice", "hunter2", "git.example.com", "/repo.git");
/// assert_eq!(
///     &*u.plaintext(),
///     "https://alice:hunter2@git.example.com/repo.git",
/// );
/// assert_eq!(
///     &*u.redacted(),
///     "https://alice:****@git.example.com/repo.git",
/// );
/// ```
pub struct SecureUrl {
    scheme: String,
    user: String,
    password: String,
    host: String,
    path: String,
}

impl SecureUrl {
    /// Construct from URL components.
    #[must_use]
    pub fn new(
        scheme: impl Into<String>,
        user: impl Into<String>,
        password: impl Into<String>,
        host: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            scheme: scheme.into(),
            user: user.into(),
            password: password.into(),
            host: host.into(),
            path: path.into(),
        }
    }
}

impl SecureArg for SecureUrl {
    fn plaintext(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}://{}:{}@{}{}",
            self.scheme, self.user, self.password, self.host, self.path
        ))
    }
    fn redacted(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}://{}:****@{}{}",
            self.scheme, self.user, self.host, self.path
        ))
    }
}

// ---------------------------------------------------------------------
// Runtime sanitiser (catches `--passphrase` / `--password` outside the
// typed `CmdArg::Secure` path)
// ---------------------------------------------------------------------

/// Redact `--pass[phrase|word]` flag arguments after the fact.
///
/// Handles both single-token (`--passphrase=foo`) and two-token
/// (`--passphrase foo`) forms; case-insensitive flag match. Use
/// `CmdArg::Secure` whenever possible to redact at construction time;
/// `sanitize_cmd` is a runtime fallback for command-line slices that
/// were already converted to plain strings.
///
/// # Examples
///
/// ```
/// use cbscore::utils::subprocess::sanitize_cmd;
///
/// let argv = vec![
///     "gpg".to_string(),
///     "--passphrase=hunter2".to_string(),
///     "--sign".to_string(),
/// ];
/// assert_eq!(
///     sanitize_cmd(&argv),
///     vec![
///         "gpg".to_string(),
///         "--passphrase=****".to_string(),
///         "--sign".to_string(),
///     ],
/// );
///
/// let argv = vec![
///     "gpg".to_string(),
///     "--passphrase".to_string(),
///     "hunter2".to_string(),
/// ];
/// assert_eq!(
///     sanitize_cmd(&argv),
///     vec![
///         "gpg".to_string(),
///         "--passphrase".to_string(),
///         "****".to_string(),
///     ],
/// );
/// ```
// `unwrap()` on the static regex literal cannot panic at runtime — the
// pattern is fixed and compile-tested by the doctest above. No `# Panics`
// doc warranted.
#[must_use]
#[allow(clippy::missing_panics_doc)]
pub fn sanitize_cmd(args: &[String]) -> Vec<String> {
    static SINGLE_TOKEN: OnceLock<Regex> = OnceLock::new();
    let re = SINGLE_TOKEN.get_or_init(|| Regex::new(r"^--(?i:pass(?:phrase|word)?)=").unwrap());

    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(m) = re.find(a) {
            // single-token form: --passphrase=<value>
            out.push(format!("{}=****", &a[..m.end() - 1]));
            i += 1;
            continue;
        }
        let lower = a.to_lowercase();
        if matches!(
            lower.as_str(),
            "--passphrase" | "--password" | "--pass" | "-p"
        ) {
            out.push(a.clone());
            if i + 1 < args.len() {
                out.push("****".to_owned());
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        out.push(a.clone());
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------
// async_run_cmd
// ---------------------------------------------------------------------

/// Per-line callback invoked by [`async_run_cmd`] for each stdout
/// line after the child exits.
pub type OutCb = Box<dyn FnMut(&str) + Send>;

/// Driver options for [`async_run_cmd`].
#[derive(Default)]
pub struct RunOpts<'a> {
    /// Internal per-call timeout. `None` disables the timer; the
    /// child runs until completion or outer cancellation.
    pub timeout: Option<Duration>,
    /// Working directory for the child. Inherits the parent's cwd
    /// when `None`.
    pub cwd: Option<&'a Utf8Path>,
    /// Extra environment variables. Layered on top of the parent's
    /// environment; entries here win on key collision.
    pub extra_env: Option<&'a HashMap<String, String>>,
    /// Optional per-line callback invoked on the child's stdout
    /// after exit. When `Some`, stdout is forwarded to the callback
    /// AND included in [`RunOutcome::stdout`] for the caller's
    /// inspection.
    pub out_cb: Option<OutCb>,
}

/// Captured outcome of a successful (in the lifecycle sense)
/// subprocess invocation.
///
/// Non-zero `rc` is **not** a [`CommandError`] — callers interpret
/// `rc` per their own domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    /// Process exit code. `-1` when the child was terminated by a
    /// signal (no exit code available).
    pub rc: i32,
    /// Captured stdout (lines joined by `\n`).
    pub stdout: String,
    /// Captured stderr (lines joined by `\n`).
    pub stderr: String,
}

/// Drive a subprocess to completion.
///
/// `cmd[0]` is the program name; the rest are arguments. Both plain
/// and secret-bearing arguments are accepted; secrets are sent to the
/// child verbatim and redacted in trace lines.
///
/// # Errors
///
/// Returns [`CommandError::Spawn`] if `tokio::process::Command::spawn`
/// fails (binary not found, permission denied, …);
/// [`CommandError::Io`] on pipe / wait IO failures;
/// [`CommandError::Timeout`] if the internal timeout fires before the
/// child exits.
///
/// # Examples
///
/// ```no_run
/// use cbscore::utils::subprocess::{async_run_cmd, CmdArg, RunOpts};
///
/// # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
/// let outcome = async_run_cmd(
///     &[CmdArg::from("echo"), CmdArg::from("hello")],
///     RunOpts::default(),
/// ).await?;
/// assert_eq!(outcome.rc, 0);
/// # Ok(()) }
/// ```
// The `expect("piped stdout/stderr")` calls cannot panic — both pipes
// are unconditionally configured with `Stdio::piped()` above the spawn,
// so `Child::stdout/stderr` are guaranteed `Some(_)`. No `# Panics` doc
// warranted.
//
// `too_many_lines`: the body sequences spawn → kill-guard install →
// timeout/wait → drain → outcome assembly as one logical unit; splitting
// it would obscure the SIGTERM/SIGKILL kill-path ordering.
//
// `items_after_statements`: the local `KillGuard` newtype + its `Drop`
// impl are intentionally scoped to this single use site rather than
// hoisted to module level.
#[allow(
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::items_after_statements
)]
pub async fn async_run_cmd(
    cmd: &[CmdArg],
    mut opts: RunOpts<'_>,
) -> Result<RunOutcome, CommandError> {
    let Some(CmdArg::Plain(program)) = cmd.first() else {
        return Err(CommandError::Spawn {
            source: io::Error::new(
                io::ErrorKind::InvalidInput,
                "first CmdArg must be CmdArg::Plain(program)",
            ),
        });
    };

    let mut command = Command::new(program);
    for a in &cmd[1..] {
        match a {
            CmdArg::Plain(s) => {
                command.arg(s);
            }
            CmdArg::Secure(sa) => {
                command.arg(sa.plaintext().as_ref());
            }
        }
    }
    if let Some(cwd) = opts.cwd {
        command.current_dir(cwd);
    }
    if let Some(env) = opts.extra_env {
        for (k, v) in env {
            command.env(k, v);
        }
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    // Tracing emits the redacted command line.
    let traced: Vec<String> = cmd
        .iter()
        .map(|a| match a {
            CmdArg::Plain(s) => s.clone(),
            CmdArg::Secure(sa) => sa.redacted().into_owned(),
        })
        .collect();
    tracing::debug!(
        target: TARGET_UTILS_SUBPROCESS,
        cmd = ?traced,
        "spawning subprocess",
    );

    let mut child = command
        .spawn()
        .map_err(|e| CommandError::Spawn { source: e })?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let stdout_task = tokio::spawn(drain_pipe(stdout));
    let stderr_task = tokio::spawn(drain_pipe(stderr));

    // RAII guard: on drop, signal the wait task to kill the child.
    // Sending an explicit () is unnecessary — when the sender drops
    // (whether normally, via timeout, or via outer cancellation), the
    // wait task observes the channel close and triggers its kill arm.
    let (kill_tx, kill_rx) = oneshot::channel::<()>();

    let wait_task = tokio::spawn(async move {
        tokio::select! {
            biased;
            _ = kill_rx => {
                let _ = child.start_kill();
                child.wait().await
            }
            res = child.wait() => res,
        }
    });

    // Guard holds the sender; dropping it triggers the kill branch.
    struct KillGuard(Option<oneshot::Sender<()>>);
    impl Drop for KillGuard {
        fn drop(&mut self) {
            // Dropping the sender closes the channel; the wait task's
            // `_ = kill_rx` arm sees the close and runs `start_kill`.
            drop(self.0.take());
        }
    }
    let mut guard = KillGuard(Some(kill_tx));

    tokio::pin!(wait_task);

    let waited = match opts.timeout {
        Some(t) => {
            tokio::select! {
                joined = &mut wait_task => joined,
                () = tokio::time::sleep(t) => {
                    // Internal timeout fired. Defuse the guard
                    // (kill explicitly) and drain the wait task so
                    // the child is fully reaped before we return.
                    drop(guard.0.take());
                    let _ = (&mut wait_task).await;
                    let stdout_lines = stdout_task.await.unwrap_or_default();
                    let stderr_lines = stderr_task.await.unwrap_or_default();
                    if let Some(cb) = opts.out_cb.as_mut() {
                        for line in &stdout_lines {
                            cb(line);
                        }
                    }
                    drop(stdout_lines);
                    drop(stderr_lines);
                    return Err(CommandError::Timeout { after: t });
                }
            }
        }
        None => (&mut wait_task).await,
    };

    // Defuse the guard — wait_task has already completed.
    let _ = guard.0.take();

    let exit_status = waited
        .map_err(|join_err| CommandError::Io {
            source: io::Error::other(join_err),
        })?
        .map_err(|e| CommandError::Io { source: e })?;

    let stdout_lines = stdout_task.await.map_err(|e| CommandError::Io {
        source: io::Error::other(e),
    })?;
    let stderr_lines = stderr_task.await.map_err(|e| CommandError::Io {
        source: io::Error::other(e),
    })?;

    if let Some(cb) = opts.out_cb.as_mut() {
        for line in &stdout_lines {
            cb(line);
        }
    }

    Ok(RunOutcome {
        rc: exit_status.code().unwrap_or(-1),
        stdout: stdout_lines.join("\n"),
        stderr: stderr_lines.join("\n"),
    })
}

async fn drain_pipe<R: AsyncRead + Unpin>(reader: R) -> Vec<String> {
    let mut acc = Vec::new();
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        acc.push(line);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn password_redacts() {
        let p = Password::new("hunter2");
        assert_eq!(&*p.redacted(), "****");
        assert_eq!(&*p.plaintext(), "hunter2");
    }

    #[test]
    fn password_arg_keeps_flag_name() {
        let a = PasswordArg::new("--passphrase", "hunter2");
        assert_eq!(&*a.redacted(), "--passphrase=****");
        assert_eq!(&*a.plaintext(), "--passphrase=hunter2");
    }

    #[test]
    fn secure_url_keeps_username() {
        let u = SecureUrl::new("https", "alice", "hunter2", "git.example.com", "/repo.git");
        assert_eq!(
            &*u.redacted(),
            "https://alice:****@git.example.com/repo.git",
        );
    }

    #[test]
    fn cmdarg_debug_emits_redacted() {
        let arg = CmdArg::Secure(Box::new(Password::new("hunter2")));
        assert_eq!(format!("{arg:?}"), "****");
    }

    #[test]
    fn sanitize_cmd_one_token_form() {
        let argv = vec![
            "gpg".to_string(),
            "--passphrase=hunter2".to_string(),
            "--sign".to_string(),
        ];
        assert_eq!(
            sanitize_cmd(&argv),
            vec!["gpg", "--passphrase=****", "--sign"],
        );
    }

    #[test]
    fn sanitize_cmd_two_token_form() {
        let argv = vec![
            "gpg".to_string(),
            "--passphrase".to_string(),
            "hunter2".to_string(),
        ];
        assert_eq!(sanitize_cmd(&argv), vec!["gpg", "--passphrase", "****"],);
    }

    #[test]
    fn sanitize_cmd_password_variant() {
        let argv = vec!["tool".to_string(), "--password=secret".to_string()];
        assert_eq!(sanitize_cmd(&argv), vec!["tool", "--password=****"]);
    }

    #[tokio::test]
    async fn run_echo_succeeds() {
        let outcome = async_run_cmd(
            &[CmdArg::from("echo"), CmdArg::from("hello world")],
            RunOpts::default(),
        )
        .await
        .expect("echo runs");
        assert_eq!(outcome.rc, 0);
        assert_eq!(outcome.stdout, "hello world");
    }

    #[tokio::test]
    async fn run_nonzero_exit_is_ok() {
        // `false` exits non-zero — still Ok(rc != 0), not an error.
        let outcome = async_run_cmd(&[CmdArg::from("false")], RunOpts::default())
            .await
            .expect("false runs");
        assert_ne!(outcome.rc, 0);
    }

    #[tokio::test]
    async fn run_captures_stderr() {
        let outcome = async_run_cmd(
            &[
                CmdArg::from("sh"),
                CmdArg::from("-c"),
                CmdArg::from("echo out; echo err >&2"),
            ],
            RunOpts::default(),
        )
        .await
        .expect("sh runs");
        assert_eq!(outcome.stdout, "out");
        assert_eq!(outcome.stderr, "err");
    }

    #[tokio::test]
    async fn run_timeout_fires() {
        let result = async_run_cmd(
            &[CmdArg::from("sleep"), CmdArg::from("5")],
            RunOpts {
                timeout: Some(Duration::from_millis(100)),
                ..Default::default()
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(CommandError::Timeout { after }) if after == Duration::from_millis(100)
        ));
    }

    #[tokio::test]
    async fn run_out_cb_called_per_line() {
        use std::sync::{Arc, Mutex};

        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let lines_cb = Arc::clone(&lines);
        let outcome = async_run_cmd(
            &[
                CmdArg::from("sh"),
                CmdArg::from("-c"),
                CmdArg::from("printf 'one\\ntwo\\nthree\\n'"),
            ],
            RunOpts {
                out_cb: Some(Box::new(move |line| {
                    lines_cb.lock().unwrap().push(line.to_owned());
                })),
                ..Default::default()
            },
        )
        .await
        .expect("sh runs");
        assert_eq!(outcome.rc, 0);
        assert_eq!(*lines.lock().unwrap(), vec!["one", "two", "three"]);
    }

    #[tokio::test]
    async fn spawn_failure_returns_typed_error() {
        let result = async_run_cmd(
            &[CmdArg::from("/this/binary/does/not/exist/anywhere")],
            RunOpts::default(),
        )
        .await;
        assert!(matches!(result, Err(CommandError::Spawn { .. })));
    }
}
