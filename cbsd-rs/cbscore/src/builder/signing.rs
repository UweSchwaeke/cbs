// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! RPM signing stage — third of the four-stage builder pipeline.
//!
//! Per-RPM GPG signing via `rpm --addsign`. The cbscore wrapper
//! supplies the passphrase via Phase 2 Commit 1's
//! [`PasswordArg`](crate::utils::subprocess::PasswordArg), so any
//! traced subprocess line emits the redacted form
//! (`--define=_gpg_sign_cmd_extra_args=****`) — never the
//! plaintext passphrase per CLAUDE.md Correctness Invariant 5.
//!
//! Signing is optional: when `config.signing` is `None`, or when
//! its `gpg` field is `None`, [`run`] returns
//! [`SigningReport::empty`] without invoking any subprocess. Per
//! design 002 line 1094–1096 (recent Python commit d2e8a91
//! "cbscore: make signing optional").
//!
//! The GPG keyring setup (writing the resolved key payload to a
//! mode-0600 tempfile + invoking `gpg --import`) lives in
//! [`gpg`] alongside the rest of the GPG-specific subprocess
//! helpers.

pub mod gpg;

use camino::Utf8PathBuf;
use cbscore_types::builder::BuilderError;
use cbscore_types::config::Config;

use super::rpmbuild::{RpmArtifact, RpmbuildReport};
use crate::secrets::SecretsMgr;

const TARGET_BUILDER_SIGNING: &str = "cbscore::builder::signing";

/// Output of [`run`]: which RPMs were signed (or all of them, when
/// signing was a no-op).
#[derive(Debug, Clone, Default)]
pub struct SigningReport {
    /// Paths of RPMs that were signed in this stage. Empty when
    /// signing was disabled or skipped.
    pub signed: Vec<Utf8PathBuf>,
    /// `true` when signing was disabled (config.signing.gpg was
    /// None) — downstream stages know this is intentional.
    pub skipped: bool,
}

impl SigningReport {
    /// An empty signing report — emitted when signing is disabled
    /// via `config.signing.gpg = None` or when `rpms.rpms` is empty
    /// (`skip_build` short-circuited rpmbuild).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            signed: Vec::new(),
            skipped: true,
        }
    }
}

/// Run the signing stage over every RPM produced by the rpmbuild
/// stage.
///
/// Order of operations (when signing is enabled):
///
/// 1. Resolve the GPG signing creds via the secrets manager. M1
///    leaves this stubbed (resolution lands in a Phase 6 follow-up
///    once cbsbuild's CLI is providing real secrets at runtime); the
///    stage currently surfaces `SigningReport::empty()` whenever
///    `config.signing.gpg.is_none()`.
/// 2. For each `RpmArtifact` in `rpms.rpms`, invoke `rpm --addsign`
///    with the resolved keyring path + email + (optional)
///    passphrase wrapped in `PasswordArg` so the traced subprocess
///    line carries `--passphrase=****`.
/// 3. On any per-RPM failure, short-circuit and propagate the error
///    (matches Python's per-component-then-fail-fast pattern).
///
/// # Errors
///
/// - [`BuilderError::Other`] wrapping the subprocess driver's error
///   on `rpm --addsign` failure.
///
/// # Examples
///
/// ```no_run
/// use cbscore::builder::{signing, rpmbuild};
/// use cbscore::secrets::SecretsMgr;
/// use cbscore_types::config::Config;
/// use cbscore_types::versions::VersionDescriptor;
///
/// # async fn demo(
/// #     desc: &VersionDescriptor,
/// #     cfg: &Config,
/// #     secrets: &SecretsMgr,
/// #     rpms: &rpmbuild::RpmbuildReport,
/// # ) -> Result<(), cbscore_types::builder::BuilderError> {
/// let report = signing::run(desc, cfg, secrets, rpms).await?;
/// println!("signed {} RPMs (skipped={})", report.signed.len(), report.skipped);
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::builder::signing",
    skip(desc, config, secrets, rpms),
    fields(version = %desc.version, rpm_count = rpms.rpms.len()),
)]
pub async fn run(
    desc: &cbscore_types::versions::VersionDescriptor,
    config: &Config,
    secrets: &SecretsMgr,
    rpms: &RpmbuildReport,
) -> Result<SigningReport, BuilderError> {
    let _ = (desc, secrets); // M1: hook for the secrets resolution
    // wiring that the Phase 6 CLI surface delivers.

    let Some(signing) = config.signing.as_ref() else {
        tracing::debug!(
            target: TARGET_BUILDER_SIGNING,
            "config.signing is None — signing stage no-op",
        );
        return Ok(SigningReport::empty());
    };
    let Some(gpg_secret_name) = signing.gpg.as_deref() else {
        tracing::debug!(
            target: TARGET_BUILDER_SIGNING,
            "config.signing.gpg is None — signing stage no-op",
        );
        return Ok(SigningReport::empty());
    };
    if rpms.rpms.is_empty() {
        tracing::debug!(
            target: TARGET_BUILDER_SIGNING,
            "no RPMs to sign (rpmbuild stage produced none or skip_build was set)",
        );
        return Ok(SigningReport::empty());
    }

    // Resolve the GPG signing key via the helper module. M1 keeps
    // the helper minimal: looks up the SigningCreds entry by name
    // and returns a placeholder `GpgKey` carrying the email +
    // optional passphrase. Phase 5 follow-up extends it with the
    // full keyring-tempfile setup once SecretsMgr has the
    // signing-key resolver wired.
    let gpg_key = gpg::resolve_gpg_key(secrets, gpg_secret_name)?;

    let mut signed: Vec<Utf8PathBuf> = Vec::with_capacity(rpms.rpms.len());
    for rpm in &rpms.rpms {
        sign_one(rpm, &gpg_key).await?;
        signed.push(rpm.path.clone());
    }

    tracing::info!(
        target: TARGET_BUILDER_SIGNING,
        signed = signed.len(),
        "signing stage complete",
    );
    Ok(SigningReport {
        signed,
        skipped: false,
    })
}

/// Sign a single [`RpmArtifact`] via `rpm --addsign`.
async fn sign_one(rpm: &RpmArtifact, key: &gpg::GpgKey) -> Result<(), BuilderError> {
    let argv = gpg::rpm_addsign_argv(&rpm.path, key);
    let outcome = crate::utils::subprocess::async_run_cmd(
        &argv,
        crate::utils::subprocess::RunOpts::default(),
    )
    .await
    .map_err(|e| BuilderError::Other(format!("rpm --addsign {}: {e}", rpm.path)))?;
    if outcome.rc != 0 {
        return Err(BuilderError::Other(format!(
            "rpm --addsign {} exited with code {} ({})",
            rpm.path,
            outcome.rc,
            outcome.stderr.trim(),
        )));
    }
    tracing::debug!(
        target: TARGET_BUILDER_SIGNING,
        rpm = %rpm.path,
        "signed",
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::config::{PathsConfig, SigningConfig};
    use cbscore_types::releases::desc::ArchType;
    use cbscore_types::versions::VersionDescriptor;
    use cbscore_types::versions::desc::{VersionImage, VersionSignedOffBy};

    fn sample_config(signing: Option<SigningConfig>) -> Config {
        Config {
            paths: PathsConfig {
                components: vec![],
                scratch: "/srv/scratch".into(),
                scratch_containers: "/srv/scratch-containers".into(),
                ccache: None,
                versions: None,
            },
            storage: None,
            signing,
            logging: None,
            secrets: Vec::new(),
            vault: None,
        }
    }

    fn sample_desc() -> VersionDescriptor {
        VersionDescriptor {
            version: "19.2.3".into(),
            title: "t".into(),
            signed_off_by: VersionSignedOffBy {
                user: "u".into(),
                email: "e".into(),
            },
            image: VersionImage {
                registry: "r".into(),
                name: "n".into(),
                tag: "t".into(),
            },
            components: Vec::new(),
            distro: "centos".into(),
            el_version: 9,
        }
    }

    fn sample_rpm(path: &str, component: &str) -> RpmArtifact {
        RpmArtifact {
            path: path.into(),
            component: component.into(),
            arch: ArchType::X86_64,
            is_srpm: false,
        }
    }

    #[tokio::test]
    async fn run_no_signing_config_returns_empty() {
        let cfg = sample_config(None);
        let desc = sample_desc();
        let secrets = SecretsMgr::empty();
        let report = run(&desc, &cfg, &secrets, &RpmbuildReport::default())
            .await
            .expect("run");
        assert!(report.skipped);
        assert!(report.signed.is_empty());
    }

    #[tokio::test]
    async fn run_signing_without_gpg_returns_empty() {
        let cfg = sample_config(Some(SigningConfig {
            gpg: None,
            transit: Some("vault-transit-key".into()),
        }));
        let desc = sample_desc();
        let secrets = SecretsMgr::empty();
        let report = run(&desc, &cfg, &secrets, &RpmbuildReport::default())
            .await
            .expect("run");
        assert!(report.skipped);
    }

    #[tokio::test]
    async fn run_empty_rpms_returns_empty() {
        let cfg = sample_config(Some(SigningConfig {
            gpg: Some("rpm-signing".into()),
            transit: None,
        }));
        let desc = sample_desc();
        let secrets = SecretsMgr::empty();
        let report = run(&desc, &cfg, &secrets, &RpmbuildReport::default())
            .await
            .expect("run");
        assert!(report.skipped);
    }

    #[test]
    fn empty_report_is_skipped() {
        let r = SigningReport::empty();
        assert!(r.skipped);
        assert!(r.signed.is_empty());
    }

    #[test]
    fn sample_rpm_construction() {
        let r = sample_rpm("/tmp/ceph-1.0.x86_64.rpm", "ceph");
        assert_eq!(r.path.as_str(), "/tmp/ceph-1.0.x86_64.rpm");
        assert_eq!(r.component, "ceph");
        assert!(!r.is_srpm);
    }
}
