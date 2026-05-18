// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Skopeo driver — `skopeo copy` and `skopeo image exists` wrappers.
//!
//! Per design 002 §Image Sign & Sync §Skopeo driver, this module
//! preserves the per-side TLS-verify and credential surfaces of the
//! underlying `skopeo copy` CLI rather than collapsing into a single
//! abstraction. [`SkopeoOpts`] carries
//! `{src,dst}_tls_verify: bool` and `{src,dst}_creds: Option<RegistryCreds>`
//! pairs; the registry-creds args are passed through
//! [`SecureSkopeoCreds`] so the `user:password` payload redacts to
//! `user:****` in trace lines (CLAUDE.md Correctness Invariant 5).

use std::borrow::Cow;

use cbscore_types::images::ImageDescriptorError;
use cbscore_types::utils::secrets::RegistryCreds;

use crate::utils::subprocess::{CmdArg, RunOpts, SecureArg, async_run_cmd};

/// A source / destination image reference for skopeo (e.g.
/// `docker://quay.io/cbs/img:tag` or `containers-storage:localhost/img`).
pub type ImageRef = String;

/// Per-side TLS / credential configuration for `skopeo copy` and
/// `skopeo image exists`.
#[derive(Default)]
pub struct SkopeoOpts {
    /// Whether to verify the source registry's TLS certificate.
    /// Defaults to `false`; callers must opt in to TLS verification.
    pub src_tls_verify: bool,
    /// Whether to verify the destination registry's TLS certificate.
    pub dst_tls_verify: bool,
    /// Source-side registry credentials, if authentication is needed.
    pub src_creds: Option<RegistryCreds>,
    /// Destination-side registry credentials, if authentication is needed.
    pub dst_creds: Option<RegistryCreds>,
}

/// [`SecureArg`] impl for skopeo credentials — keeps the username
/// visible while redacting the password in trace lines.
///
/// Wraps the `user:password` payload of `--src-creds` / `--dest-creds`.
/// [`SecureArg::plaintext`] returns the cleartext `user:password`
/// string passed to skopeo; [`SecureArg::redacted`] returns
/// `"<user>:****"`.
pub struct SecureSkopeoCreds {
    username: String,
    password: String,
}

impl SecureSkopeoCreds {
    /// Construct from username + password.
    #[must_use]
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }

    /// Construct from a [`RegistryCreds`] entry. Plain and Vault
    /// variants both carry username + password; either resolves to
    /// the same skopeo argument.
    #[must_use]
    pub fn from_registry_creds(creds: &RegistryCreds) -> Self {
        let (u, p) = match creds {
            RegistryCreds::Plain {
                username, password, ..
            }
            | RegistryCreds::Vault {
                username, password, ..
            } => (username.clone(), password.clone()),
        };
        Self {
            username: u,
            password: p,
        }
    }
}

impl SecureArg for SecureSkopeoCreds {
    fn plaintext(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}:{}", self.username, self.password))
    }
    fn redacted(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}:****", self.username))
    }
}

// ---------------------------------------------------------------------
// skopeo copy
// ---------------------------------------------------------------------

/// Build the command line for `skopeo copy`. Returns a Vec<CmdArg>
/// so credential args remain wrapped in [`CmdArg::Secure`] and redact
/// in trace lines.
#[must_use]
fn skopeo_copy_cmd(src: &str, dst: &str, opts: &SkopeoOpts) -> Vec<CmdArg> {
    let mut cmd: Vec<CmdArg> = vec![CmdArg::from("skopeo"), CmdArg::from("copy")];
    cmd.push(CmdArg::Plain(format!(
        "--src-tls-verify={}",
        opts.src_tls_verify
    )));
    cmd.push(CmdArg::Plain(format!(
        "--dest-tls-verify={}",
        opts.dst_tls_verify
    )));
    if let Some(c) = &opts.src_creds {
        cmd.push(CmdArg::from("--src-creds"));
        cmd.push(CmdArg::Secure(Box::new(
            SecureSkopeoCreds::from_registry_creds(c),
        )));
    }
    if let Some(c) = &opts.dst_creds {
        cmd.push(CmdArg::from("--dest-creds"));
        cmd.push(CmdArg::Secure(Box::new(
            SecureSkopeoCreds::from_registry_creds(c),
        )));
    }
    cmd.push(CmdArg::Plain(src.to_owned()));
    cmd.push(CmdArg::Plain(dst.to_owned()));
    cmd
}

/// Copy an image from `src` to `dst` via `skopeo copy`.
///
/// # Errors
///
/// Returns [`ImageDescriptorError::Invalid`] on subprocess failure or
/// non-zero exit, carrying the stderr payload.
///
/// # Examples
///
/// ```no_run
/// use cbscore::images::skopeo::{skopeo_copy, SkopeoOpts};
///
/// # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
/// skopeo_copy(
///     "docker://quay.io/cbs/img:dev",
///     "docker://quay.io/cbs/img:prod",
///     &SkopeoOpts { src_tls_verify: true, dst_tls_verify: true, ..Default::default() },
/// ).await?;
/// # Ok(()) }
/// ```
pub async fn skopeo_copy(
    src: &str,
    dst: &str,
    opts: &SkopeoOpts,
) -> Result<(), ImageDescriptorError> {
    let cmd = skopeo_copy_cmd(src, dst, opts);
    let outcome = async_run_cmd(&cmd, RunOpts::default())
        .await
        .map_err(|e| ImageDescriptorError::Invalid(format!("skopeo copy: {e}")))?;
    if outcome.rc != 0 {
        return Err(ImageDescriptorError::Invalid(format!(
            "skopeo copy {src} -> {dst} exited with code {}: {}",
            outcome.rc, outcome.stderr,
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------
// skopeo image exists (via `skopeo inspect`)
// ---------------------------------------------------------------------

/// Build the command line for `skopeo inspect <src>`. Used to probe
/// whether an image exists at the registry.
#[must_use]
fn skopeo_inspect_cmd(src: &str, opts: &SkopeoOpts) -> Vec<CmdArg> {
    let mut cmd: Vec<CmdArg> = vec![CmdArg::from("skopeo"), CmdArg::from("inspect")];
    cmd.push(CmdArg::Plain(format!(
        "--tls-verify={}",
        opts.src_tls_verify
    )));
    if let Some(c) = &opts.src_creds {
        cmd.push(CmdArg::from("--creds"));
        cmd.push(CmdArg::Secure(Box::new(
            SecureSkopeoCreds::from_registry_creds(c),
        )));
    }
    cmd.push(CmdArg::Plain(src.to_owned()));
    cmd
}

/// Check whether an image exists at `src`.
///
/// Distinguishes "image absent" from "registry unreachable": a
/// non-zero exit whose stderr names the missing image returns
/// `Ok(false)`; any other failure (network error, auth error,
/// subprocess failure) returns `Err`.
///
/// # Errors
///
/// Returns [`ImageDescriptorError::Invalid`] on subprocess failure or
/// a non-zero exit whose stderr does not indicate a missing image.
pub async fn skopeo_image_exists(
    src: &str,
    opts: &SkopeoOpts,
) -> Result<bool, ImageDescriptorError> {
    let cmd = skopeo_inspect_cmd(src, opts);
    let outcome = async_run_cmd(&cmd, RunOpts::default())
        .await
        .map_err(|e| ImageDescriptorError::Invalid(format!("skopeo inspect: {e}")))?;
    if outcome.rc == 0 {
        return Ok(true);
    }
    // skopeo emits `manifest unknown` (Docker v2) or `name unknown`
    // when the image is absent. Anything else is a real error.
    let stderr_lc = outcome.stderr.to_lowercase();
    if stderr_lc.contains("manifest unknown") || stderr_lc.contains("name unknown") {
        return Ok(false);
    }
    Err(ImageDescriptorError::Invalid(format!(
        "skopeo inspect {src} exited with code {}: {}",
        outcome.rc, outcome.stderr,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn debug_args(cmd: &[CmdArg]) -> Vec<String> {
        cmd.iter().map(|a| format!("{a:?}")).collect()
    }

    #[test]
    fn copy_cmd_minimal() {
        let cmd = skopeo_copy_cmd(
            "docker://quay.io/cbs/img:dev",
            "docker://quay.io/cbs/img:prod",
            &SkopeoOpts::default(),
        );
        let args = debug_args(&cmd);
        assert_eq!(args[0], "\"skopeo\"");
        assert_eq!(args[1], "\"copy\"");
        assert_eq!(args[2], "\"--src-tls-verify=false\"");
        assert_eq!(args[3], "\"--dest-tls-verify=false\"");
        assert_eq!(args[4], "\"docker://quay.io/cbs/img:dev\"");
        assert_eq!(args[5], "\"docker://quay.io/cbs/img:prod\"");
    }

    #[test]
    fn copy_cmd_with_tls_and_creds_redacts() {
        let creds = RegistryCreds::Plain {
            username: "alice".into(),
            password: "hunter2".into(),
            address: "quay.io".into(),
        };
        let cmd = skopeo_copy_cmd(
            "docker://quay.io/cbs/img:dev",
            "docker://quay.io/cbs/img:prod",
            &SkopeoOpts {
                src_tls_verify: true,
                dst_tls_verify: true,
                src_creds: Some(creds),
                dst_creds: None,
            },
        );
        let args = debug_args(&cmd);
        assert_eq!(args[2], "\"--src-tls-verify=true\"");
        assert_eq!(args[3], "\"--dest-tls-verify=true\"");
        assert_eq!(args[4], "\"--src-creds\"");
        // Secure variant: emits redacted form, not "user:password".
        assert_eq!(args[5], "alice:****");
        assert!(args.iter().all(|a| !a.contains("hunter2")));
    }

    #[test]
    fn inspect_cmd_with_creds_redacts() {
        let creds = RegistryCreds::Plain {
            username: "alice".into(),
            password: "hunter2".into(),
            address: "quay.io".into(),
        };
        let cmd = skopeo_inspect_cmd(
            "docker://quay.io/cbs/img:dev",
            &SkopeoOpts {
                src_tls_verify: true,
                src_creds: Some(creds),
                ..Default::default()
            },
        );
        let args = debug_args(&cmd);
        assert_eq!(args[0], "\"skopeo\"");
        assert_eq!(args[1], "\"inspect\"");
        assert_eq!(args[2], "\"--tls-verify=true\"");
        assert_eq!(args[3], "\"--creds\"");
        assert_eq!(args[4], "alice:****");
        assert!(args.iter().all(|a| !a.contains("hunter2")));
    }

    #[test]
    fn secure_skopeo_creds_from_vault() {
        let creds = RegistryCreds::Vault {
            key: "registry/quay".into(),
            username: "bob".into(),
            password: "swordfish".into(),
            address: "quay.io".into(),
        };
        let secure = SecureSkopeoCreds::from_registry_creds(&creds);
        assert_eq!(&*secure.plaintext(), "bob:swordfish");
        assert_eq!(&*secure.redacted(), "bob:****");
    }
}
