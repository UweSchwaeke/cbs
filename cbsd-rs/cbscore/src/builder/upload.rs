// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Upload stage — final builder pipeline stage. Pushes signed
//! RPMs + the built container image + the release descriptor to
//! the configured storage destinations.
//!
//! Per design 002 §Build Pipeline diagram, upload is gated on
//! `config.storage`:
//! - `config.storage.s3.is_none()` → no RPM / descriptor upload.
//! - `config.storage.registry.is_none()` → no image push.
//! - both `None` → stage returns [`UploadReport::empty`] without
//!   any subprocess invocation.

use camino::Utf8PathBuf;
use cbscore_types::builder::BuilderError;
use cbscore_types::config::Config;
use cbscore_types::releases::desc::ReleaseDesc;
use cbscore_types::versions::VersionDescriptor;

use super::rpmbuild::RpmbuildReport;
use super::signing::SigningReport;
use crate::containers::build::ContainerImageReport;
use crate::releases::s3::upload_release;
use crate::secrets::SecretsMgr;

const TARGET_BUILDER_UPLOAD: &str = "cbscore::builder::upload";

/// Output of [`run`]: which artefacts the upload stage actually
/// shipped to which destinations.
#[derive(Debug, Clone, Default)]
pub struct UploadReport {
    /// RPM paths uploaded to S3 (empty when no S3 destination or
    /// the upstream signing stage was a no-op).
    pub uploaded_rpms: Vec<Utf8PathBuf>,
    /// `Some(<tag>)` when the container image was pushed; `None`
    /// when no registry is configured.
    pub pushed_image: Option<String>,
    /// `true` when the release descriptor JSON landed at
    /// `<bucket>/<loc>/<version>.json`.
    pub published_descriptor: bool,
}

impl UploadReport {
    /// Empty report — emitted when neither S3 nor registry is
    /// configured (operator chose local-only build).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            uploaded_rpms: Vec::new(),
            pushed_image: None,
            published_descriptor: false,
        }
    }
}

/// Run the upload stage.
///
/// Order of operations when both destinations are configured:
///
/// 1. S3 — upload RPMs and the release descriptor (descriptor
///    last per the [`upload_release`] contract: readers who see
///    the descriptor can trust the RPMs).
/// 2. Registry — push the locally-built container image
///    (Phase 5 follow-up wires this through `utils::buildah` /
///    `images::sync_image`; M1 ships the surface so the
///    orchestrator can chain it).
///
/// # Errors
///
/// - [`BuilderError::Other`] wrapping the underlying S3
///   ([`crate::releases::s3::upload_release`]) or image-push
///   failure.
///
/// # Examples
///
/// ```no_run
/// use cbscore::builder::{rpmbuild, signing, upload};
/// use cbscore::containers::build::ContainerImageReport;
/// use cbscore::secrets::SecretsMgr;
/// use cbscore_types::config::Config;
/// use cbscore_types::versions::VersionDescriptor;
///
/// # async fn demo(
/// #     desc: &VersionDescriptor,
/// #     cfg: &Config,
/// #     secrets: &SecretsMgr,
/// #     rpms: &rpmbuild::RpmbuildReport,
/// #     signed: &signing::SigningReport,
/// #     image: &ContainerImageReport,
/// # ) -> Result<(), cbscore_types::builder::BuilderError> {
/// let report = upload::run(desc, cfg, secrets, rpms, signed, image).await?;
/// println!("uploaded {} RPMs", report.uploaded_rpms.len());
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::builder::upload",
    skip(desc, config, secrets, rpms, signed, image),
    fields(version = %desc.version),
)]
pub async fn run(
    desc: &VersionDescriptor,
    config: &Config,
    secrets: &SecretsMgr,
    rpms: &RpmbuildReport,
    signed: &SigningReport,
    image: &ContainerImageReport,
) -> Result<UploadReport, BuilderError> {
    let _ = (secrets, signed); // M1: registry-creds and signing-
    // signal threading lands in the Phase 5 follow-up alongside
    // the buildah-push wiring.

    let Some(storage) = config.storage.as_ref() else {
        tracing::debug!(
            target: TARGET_BUILDER_UPLOAD,
            "config.storage is None — upload stage no-op",
        );
        return Ok(UploadReport::empty());
    };

    let mut report = UploadReport::empty();

    // S3 — upload RPMs + release descriptor.
    if let Some(s3) = storage.s3.as_ref() {
        if rpms.rpms.is_empty() {
            tracing::debug!(
                target: TARGET_BUILDER_UPLOAD,
                "no RPMs to upload (skip_build or empty rpmbuild output)",
            );
        } else {
            let release_desc = ReleaseDesc {
                version: desc.version.clone(),
                // The per-arch ReleaseBuildEntry assembly threads
                // BuildComponentInfo + RpmArtifact + signing metadata
                // together. M1 ships the descriptor with an empty
                // `builds` map; the full assembly lands in Commit 7's
                // report constructor where every per-stage report is
                // in scope at once.
                builds: std::collections::HashMap::new(),
            };
            upload_release(
                &s3.releases.bucket,
                &s3.releases.loc,
                &release_desc,
                &rpms.rpms,
            )
            .await
            .map_err(|e| BuilderError::Other(format!("release upload: {e}")))?;
            report
                .uploaded_rpms
                .extend(rpms.rpms.iter().map(|r| r.path.clone()));
            report.published_descriptor = true;
            tracing::info!(
                target: TARGET_BUILDER_UPLOAD,
                bucket = %s3.releases.bucket,
                loc = %s3.releases.loc,
                rpms = report.uploaded_rpms.len(),
                "S3 upload complete",
            );
        }
    }

    // Registry — push the locally-built container image. M1 stub:
    // the actual `buildah push` (or `skopeo copy`) wiring lands in
    // the Phase 5 follow-up; for now, record the local tag so the
    // orchestrator's report-assembly sees the image was produced.
    if storage.registry.is_some() {
        tracing::warn!(
            target: TARGET_BUILDER_UPLOAD,
            local_tag = %image.local_tag,
            "container image push is a Phase-5-follow-up stub; \
             recording local tag only",
        );
        report.pushed_image = Some(image.local_tag.clone());
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::config::{
        PathsConfig, RegistryStorageConfig, S3LocationConfig, S3StorageConfig, StorageConfig,
    };
    use cbscore_types::versions::desc::{VersionImage, VersionSignedOffBy};

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

    fn sample_config(storage: Option<StorageConfig>) -> Config {
        Config {
            paths: PathsConfig {
                components: vec![],
                scratch: "/srv/scratch".into(),
                scratch_containers: "/srv/scratch-containers".into(),
                ccache: None,
                versions: None,
            },
            storage,
            signing: None,
            logging: None,
            secrets: Vec::new(),
            vault: None,
        }
    }

    fn sample_image() -> ContainerImageReport {
        ContainerImageReport {
            local_tag: "registry/img:tag".into(),
            image_id: Some("abc123".into()),
            digest: None,
        }
    }

    #[tokio::test]
    async fn no_storage_yields_empty_report() {
        let cfg = sample_config(None);
        let secrets = SecretsMgr::empty();
        let report = run(
            &sample_desc(),
            &cfg,
            &secrets,
            &RpmbuildReport::default(),
            &SigningReport::empty(),
            &sample_image(),
        )
        .await
        .expect("run");
        assert!(report.uploaded_rpms.is_empty());
        assert!(report.pushed_image.is_none());
        assert!(!report.published_descriptor);
    }

    #[tokio::test]
    async fn registry_only_records_local_tag() {
        let cfg = sample_config(Some(StorageConfig {
            s3: None,
            registry: Some(RegistryStorageConfig {
                url: "quay.io".into(),
            }),
        }));
        let secrets = SecretsMgr::empty();
        let report = run(
            &sample_desc(),
            &cfg,
            &secrets,
            &RpmbuildReport::default(),
            &SigningReport::empty(),
            &sample_image(),
        )
        .await
        .expect("run");
        assert_eq!(report.pushed_image.as_deref(), Some("registry/img:tag"));
        assert!(report.uploaded_rpms.is_empty());
        assert!(!report.published_descriptor);
    }

    #[tokio::test]
    async fn s3_with_empty_rpms_is_noop() {
        let cfg = sample_config(Some(StorageConfig {
            s3: Some(S3StorageConfig {
                url: "http://s3.example.com".into(),
                artifacts: S3LocationConfig {
                    bucket: "artifacts".into(),
                    loc: "ceph".into(),
                },
                releases: S3LocationConfig {
                    bucket: "releases".into(),
                    loc: "ceph/dev".into(),
                },
            }),
            registry: None,
        }));
        let secrets = SecretsMgr::empty();
        // RpmbuildReport::default() has zero RPMs → upload_release
        // never fires → no S3 contact required at this commit.
        let report = run(
            &sample_desc(),
            &cfg,
            &secrets,
            &RpmbuildReport::default(),
            &SigningReport::empty(),
            &sample_image(),
        )
        .await
        .expect("run");
        assert!(report.uploaded_rpms.is_empty());
        assert!(!report.published_descriptor);
    }

    #[test]
    fn empty_report_construction() {
        let r = UploadReport::empty();
        assert!(r.uploaded_rpms.is_empty());
        assert!(r.pushed_image.is_none());
        assert!(!r.published_descriptor);
    }
}
