// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! GPG-side helpers — keyring setup, `rpm --addsign` argv
//! construction, and the [`GpgKey`] handle the [`super::run`]
//! signing loop consumes.
//!
//! Pinned under `builder/signing/` (not the shared `utils/gpg.rs`
//! location) because GPG is a builder-pipeline concern;
//! `images::signing` re-imports the helpers from here. This keeps
//! the design 001 §Lift-out invariants safe — `utils/` stays clean
//! of cbscore-internal dependencies, so the future `cbscommon-rs`
//! lift-out path for `utils/` is unaffected.
//!
//! Passphrase redaction: any `rpm --addsign` invocation carries
//! the passphrase as a
//! [`PasswordArg`](crate::utils::subprocess::PasswordArg), so the
//! Phase 2 Commit 1 redacted-tracing contract is in force — traced
//! command lines emit `****`, never the plaintext passphrase
//! (CLAUDE.md Correctness Invariant 5).

use camino::Utf8Path;
use cbscore_types::builder::BuilderError;

use crate::secrets::SecretsMgr;
use crate::utils::subprocess::{CmdArg, PasswordArg};

/// Resolved GPG signing key handle — what [`super::run`]'s
/// signing loop needs to fire `rpm --addsign` against.
///
/// `keyring_path` is currently `None` for the M1 stub; Phase 5
/// follow-up populates it with a host-side tempfile written via
/// [`crate::secrets::signing::write_gpg_keys`] (Phase 3 Commit 3).
/// The follow-up plumbs the `SecretsMgr` GPG-key lookup through this
/// helper so the keyring is materialised lazily under the signing
/// stage's tempdir.
#[derive(Debug, Clone)]
pub struct GpgKey {
    /// Optional keyring path passed via `rpm --define
    /// "_gpg_path …"` to the rpmbuild subprocess.
    pub keyring_path: Option<camino::Utf8PathBuf>,
    /// Signing identity email — required by `rpm --addsign`.
    pub email: String,
    /// Optional passphrase. Wrapped in [`PasswordArg`] at argv
    /// construction so it never traces verbatim.
    pub passphrase: Option<String>,
}

/// Resolve the operator-chosen GPG signing-secret name to a
/// [`GpgKey`] handle.
///
/// M1 stub: the M1 milestone covers the optional-signing path
/// (when `config.signing.gpg` is `None`, the stage is a no-op);
/// when it is `Some`, this resolver returns a placeholder
/// [`GpgKey`] carrying an empty `keyring_path`. The Phase 5
/// follow-up wires the [`SecretsMgr`] GPG-secret resolver here.
///
/// # Errors
///
/// Returns [`BuilderError::Other`] if the operator-chosen secret
/// name is missing from the resolved secrets store. The M1 stub
/// always succeeds.
pub fn resolve_gpg_key(_secrets: &SecretsMgr, name: &str) -> Result<GpgKey, BuilderError> {
    // M1: SecretsMgr's typed GPG resolver lands in a Phase 5
    // follow-up. For now, return a placeholder so the signing
    // stage's flow is exercisable; the actual `rpm --addsign`
    // invocation will fail at the subprocess layer until the
    // keyring path is real, which is correct M1 behaviour
    // (signing is optional and the integration suite covers the
    // populated path).
    tracing::warn!(
        target: "cbscore::builder::signing::gpg",
        secret_name = name,
        "GPG key resolution is a Phase-5-follow-up stub; signing \
         will fail at the subprocess layer until the resolver lands",
    );
    Ok(GpgKey {
        keyring_path: None,
        email: format!("{name}@cbs.invalid"),
        passphrase: None,
    })
}

/// Build the `rpm --addsign` argv for `rpm_path` with `key`.
///
/// The resulting `Vec<CmdArg>` carries the passphrase wrapped in
/// [`PasswordArg`] (when set) so traced command lines redact to
/// `--define=_gpg_sign_cmd_extra_args=****`.
#[must_use]
pub fn rpm_addsign_argv(rpm_path: &Utf8Path, key: &GpgKey) -> Vec<CmdArg> {
    let mut argv: Vec<CmdArg> = vec![CmdArg::from("rpm"), CmdArg::from("--addsign")];
    if let Some(kp) = key.keyring_path.as_deref() {
        argv.push(CmdArg::from("--define"));
        argv.push(CmdArg::Plain(format!("_gpg_path {kp}")));
    }
    argv.push(CmdArg::from("--define"));
    argv.push(CmdArg::Plain(format!("_gpg_name {}", key.email)));
    if let Some(pp) = key.passphrase.as_deref() {
        argv.push(CmdArg::from("--define"));
        argv.push(CmdArg::Secure(Box::new(PasswordArg::new(
            "_gpg_sign_cmd_extra_args --pinentry-mode loopback --passphrase",
            pp,
        ))));
    }
    argv.push(CmdArg::Plain(rpm_path.as_str().to_owned()));
    argv
}

#[cfg(test)]
mod tests {
    use super::*;
    // SecureArg trait method calls (`.plaintext()`, `.redacted()`)
    // on the boxed dyn inside CmdArg::Secure are dispatched via
    // dyn-trait so the trait import is implied — no explicit
    // `use crate::utils::subprocess::SecureArg;` is needed here.

    #[test]
    fn resolve_gpg_key_returns_stub() {
        let secrets = SecretsMgr::empty();
        let key = resolve_gpg_key(&secrets, "rpm-signing").expect("stub");
        assert!(key.keyring_path.is_none());
        assert_eq!(key.email, "rpm-signing@cbs.invalid");
    }

    #[test]
    fn rpm_addsign_argv_minimal() {
        let key = GpgKey {
            keyring_path: None,
            email: "ops@example.com".into(),
            passphrase: None,
        };
        let argv = rpm_addsign_argv(camino::Utf8Path::new("/tmp/x.rpm"), &key);
        // Render as plain strings for the no-secret variants.
        let rendered: Vec<String> = argv
            .iter()
            .map(|a| match a {
                CmdArg::Plain(s) => s.clone(),
                CmdArg::Secure(s) => s.plaintext().into_owned(),
            })
            .collect();
        assert_eq!(rendered[0], "rpm");
        assert_eq!(rendered[1], "--addsign");
        assert!(rendered.iter().any(|s| s == "_gpg_name ops@example.com"));
        assert_eq!(rendered.last().unwrap(), "/tmp/x.rpm");
    }

    #[test]
    fn rpm_addsign_argv_includes_keyring_and_passphrase() {
        let key = GpgKey {
            keyring_path: Some("/tmp/keyring.gpg".into()),
            email: "ops@example.com".into(),
            passphrase: Some("hunter2".into()),
        };
        let argv = rpm_addsign_argv(camino::Utf8Path::new("/tmp/x.rpm"), &key);
        let plain: Vec<String> = argv
            .iter()
            .map(|a| match a {
                CmdArg::Plain(s) => s.clone(),
                CmdArg::Secure(s) => s.plaintext().into_owned(),
            })
            .collect();
        let redacted: Vec<String> = argv
            .iter()
            .map(|a| match a {
                CmdArg::Plain(s) => s.clone(),
                CmdArg::Secure(s) => s.redacted().into_owned(),
            })
            .collect();
        assert!(plain.iter().any(|s| s == "_gpg_path /tmp/keyring.gpg"));
        assert!(plain.iter().any(|s| s.contains("hunter2")));
        // The redacted view must never carry the plaintext passphrase.
        for s in &redacted {
            assert!(!s.contains("hunter2"), "leaked passphrase: {s}");
        }
    }
}
