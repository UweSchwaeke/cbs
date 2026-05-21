// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Build-artifact report assembly — gathers the per-stage outputs
//! into a single [`BuildArtifactReport`] for the in-container
//! `cbsbuild runner build` to write to
//! `/runner/<name>.report.json`.
//!
//! The Phase 1 Commit 4 wire-format type lives at
//! [`cbscore_types::builder::report::BuildArtifactReport`] (carries
//! `schema_version: 1` per Phase 1 Commit 5); this file holds the
//! constructor that glues per-stage reports together — no IO, no
//! subprocess, just struct → struct projection.

use cbscore_types::builder::report::{
    BuildArtifactReport, ComponentReport, ContainerImageReport as ReportContainerImage,
    ReleaseDescriptorReport,
};
use cbscore_types::config::Config;
use cbscore_types::versions::VersionDescriptor;

use super::prepare::PrepareReport;
use super::rpmbuild::RpmbuildReport;
use super::signing::SigningReport;
use super::upload::UploadReport;
use crate::containers::build::ContainerImageReport;

/// Current `schema_version` written into the build report.
const REPORT_SCHEMA_VERSION: u64 = 1;

/// Build a [`BuildArtifactReport`] from the four per-stage reports.
///
/// `skipped` is `true` when the build was a no-op
/// (`opts.skip_build` short-circuited rpmbuild, so signing +
/// upload also became no-ops). Downstream readers can
/// distinguish a real build from a metadata-only entry by this
/// flag.
///
/// # Examples
///
/// ```
/// use cbscore::builder::{prepare, report, rpmbuild, signing, upload};
/// use cbscore::containers::build::ContainerImageReport;
/// use cbscore_types::config::Config;
/// use cbscore_types::versions::VersionDescriptor;
/// use cbscore_types::versions::desc::{VersionImage, VersionSignedOffBy};
/// use cbscore_types::config::PathsConfig;
///
/// let desc = VersionDescriptor {
///     version: "19.2.3".into(),
///     title: "t".into(),
///     signed_off_by: VersionSignedOffBy { user: "u".into(), email: "e".into() },
///     image: VersionImage { registry: "r".into(), name: "n".into(), tag: "t".into() },
///     components: vec![],
///     distro: "centos".into(),
///     el_version: 9,
/// };
/// let cfg = Config {
///     paths: PathsConfig {
///         components: vec![],
///         scratch: "/srv/scratch".into(),
///         scratch_containers: "/srv/sc".into(),
///         ccache: None,
///         versions: None,
///     },
///     storage: None,
///     signing: None,
///     logging: None,
///     secrets: vec![],
///     vault: None,
/// };
/// let image = ContainerImageReport {
///     local_tag: "r/n:t".into(),
///     image_id: None,
///     digest: None,
/// };
/// let r = report::assemble(
///     &desc,
///     &cfg,
///     &prepare::PrepareReport::default(),
///     &rpmbuild::RpmbuildReport::default(),
///     &image,
///     &signing::SigningReport::empty(),
///     &upload::UploadReport::empty(),
/// );
/// assert_eq!(r.version, "19.2.3");
/// assert_eq!(r.schema_version, 1);
/// assert!(r.skipped); // no RPMs, no upload → skipped semantics
/// ```
#[must_use]
pub fn assemble(
    desc: &VersionDescriptor,
    config: &Config,
    prep: &PrepareReport,
    rpms: &RpmbuildReport,
    image: &ContainerImageReport,
    _signed: &SigningReport,
    upload: &UploadReport,
) -> BuildArtifactReport {
    // `skipped` follows the same rule as Python: if the rpmbuild
    // stage produced zero artefacts and the upload stage didn't
    // ship anything, the report is metadata-only.
    let skipped = rpms.rpms.is_empty() && upload.uploaded_rpms.is_empty();

    // Project the cbscore-side ContainerImageReport (local build
    // metadata: local_tag, image_id, digest) onto the report-side
    // shape (registry-qualified name + tag + pushed flag).
    let container_image = if image.local_tag.is_empty() {
        None
    } else {
        let (name, tag) = split_image_tag(&image.local_tag, desc);
        Some(ReportContainerImage {
            name,
            tag,
            pushed: upload.pushed_image.is_some(),
        })
    };

    // Project upload metadata onto the report's release-descriptor
    // location field. Only populated when the upload stage actually
    // published the descriptor to S3.
    let release_descriptor = config
        .storage
        .as_ref()
        .and_then(|s| s.s3.as_ref())
        .filter(|_| upload.published_descriptor)
        .map(|s3| ReleaseDescriptorReport {
            s3_path: super::super::releases::utils::release_desc_key(
                &s3.releases.loc,
                &desc.version,
            ),
            bucket: s3.releases.bucket.clone(),
        });

    // Per-component projection — one ComponentReport per entry in
    // the prepare report. The S3 path is the per-component prefix
    // when upload was configured; None otherwise.
    let s3_prefix = config
        .storage
        .as_ref()
        .and_then(|s| s.s3.as_ref())
        .filter(|_| upload.published_descriptor)
        .map(|s3| {
            super::super::releases::utils::release_rpm_prefix(&s3.releases.loc, &desc.version)
        });

    let mut components: Vec<ComponentReport> = desc
        .components
        .iter()
        .filter_map(|vc| {
            let info = prep.components.get(&vc.name)?;
            Some(ComponentReport {
                name: info.name.clone(),
                version: info.base_ref.clone(),
                sha1: info.sha1.clone(),
                repo_url: info.repo_url.clone(),
                rpms_s3_path: s3_prefix.as_deref().map(str::to_owned),
            })
        })
        .collect();
    // Keep component order stable (descriptor order, which is
    // already enforced by iterating desc.components).
    components.sort_by(|a, b| a.name.cmp(&b.name));

    BuildArtifactReport {
        schema_version: REPORT_SCHEMA_VERSION,
        version: desc.version.clone(),
        skipped,
        container_image,
        release_descriptor,
        components,
    }
}

/// Split a `<registry>/<name>:<tag>` into `(name, tag)`, falling
/// back to the descriptor's image block when the local tag doesn't
/// split cleanly.
fn split_image_tag(local_tag: &str, desc: &VersionDescriptor) -> (String, String) {
    if let Some((name, tag)) = local_tag.rsplit_once(':') {
        (name.to_owned(), tag.to_owned())
    } else {
        (
            format!("{}/{}", desc.image.registry, desc.image.name),
            desc.image.tag.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::config::{PathsConfig, S3LocationConfig, S3StorageConfig, StorageConfig};
    use cbscore_types::versions::desc::{VersionComponent, VersionImage, VersionSignedOffBy};

    fn desc() -> VersionDescriptor {
        VersionDescriptor {
            version: "19.2.3".into(),
            title: "t".into(),
            signed_off_by: VersionSignedOffBy {
                user: "u".into(),
                email: "e".into(),
            },
            image: VersionImage {
                registry: "quay.io".into(),
                name: "ceph".into(),
                tag: "v19.2.3".into(),
            },
            components: vec![VersionComponent {
                name: "ceph".into(),
                repo: "https://example.com/ceph.git".into(),
                ref_: "v19.2.3".into(),
            }],
            distro: "centos".into(),
            el_version: 9,
        }
    }

    fn cfg_with_s3() -> Config {
        Config {
            paths: PathsConfig {
                components: vec![],
                scratch: "/srv/scratch".into(),
                scratch_containers: "/srv/sc".into(),
                ccache: None,
                versions: None,
            },
            storage: Some(StorageConfig {
                s3: Some(S3StorageConfig {
                    url: "http://s3.example.com".into(),
                    artifacts: S3LocationConfig {
                        bucket: "art".into(),
                        loc: "ceph".into(),
                    },
                    releases: S3LocationConfig {
                        bucket: "releases".into(),
                        loc: "ceph/dev".into(),
                    },
                }),
                registry: None,
            }),
            signing: None,
            logging: None,
            secrets: Vec::new(),
            vault: None,
        }
    }

    fn local_image() -> ContainerImageReport {
        ContainerImageReport {
            local_tag: "quay.io/ceph:v19.2.3".into(),
            image_id: Some("abc".into()),
            digest: None,
        }
    }

    #[test]
    fn assemble_minimum() {
        let r = assemble(
            &desc(),
            &cfg_with_s3(),
            &PrepareReport::default(),
            &RpmbuildReport::default(),
            &local_image(),
            &SigningReport::empty(),
            &UploadReport::empty(),
        );
        assert_eq!(r.schema_version, 1);
        assert_eq!(r.version, "19.2.3");
        assert!(r.skipped);
        assert!(r.release_descriptor.is_none());
        assert!(r.components.is_empty());
    }

    #[test]
    fn assemble_with_uploaded_descriptor_populates_location() {
        let mut upload = UploadReport::empty();
        upload.published_descriptor = true;
        upload.uploaded_rpms = vec!["/tmp/ceph.rpm".into()];
        let r = assemble(
            &desc(),
            &cfg_with_s3(),
            &PrepareReport::default(),
            &RpmbuildReport::default(),
            &local_image(),
            &SigningReport::empty(),
            &upload,
        );
        let rd = r.release_descriptor.expect("descriptor populated");
        assert_eq!(rd.bucket, "releases");
        assert_eq!(rd.s3_path, "ceph/dev/19.2.3.json");
        assert!(!r.skipped); // upload shipped something
    }

    #[test]
    fn assemble_container_image_from_local_tag() {
        let r = assemble(
            &desc(),
            &cfg_with_s3(),
            &PrepareReport::default(),
            &RpmbuildReport::default(),
            &local_image(),
            &SigningReport::empty(),
            &UploadReport::empty(),
        );
        let ci = r.container_image.expect("image present");
        assert_eq!(ci.name, "quay.io/ceph");
        assert_eq!(ci.tag, "v19.2.3");
        assert!(!ci.pushed);
    }

    #[test]
    fn assemble_components_projected_from_prep() {
        let mut prep = PrepareReport::default();
        prep.components.insert(
            "ceph".into(),
            super::super::prepare::BuildComponentInfo {
                name: "ceph".into(),
                repo_path: "/srv/scratch/ceph".into(),
                repo_url: "https://example.com/ceph.git".into(),
                base_ref: "v19.2.3".into(),
                sha1: "deadbeef".into(),
                patches: Vec::new(),
            },
        );
        let r = assemble(
            &desc(),
            &cfg_with_s3(),
            &prep,
            &RpmbuildReport::default(),
            &local_image(),
            &SigningReport::empty(),
            &UploadReport::empty(),
        );
        assert_eq!(r.components.len(), 1);
        let c = &r.components[0];
        assert_eq!(c.name, "ceph");
        assert_eq!(c.sha1, "deadbeef");
        assert_eq!(c.repo_url, "https://example.com/ceph.git");
    }

    #[test]
    fn split_image_tag_basic() {
        let (name, tag) = split_image_tag("quay.io/ceph:v19.2.3", &desc());
        assert_eq!(name, "quay.io/ceph");
        assert_eq!(tag, "v19.2.3");
    }

    #[test]
    fn split_image_tag_no_colon_falls_back_to_desc() {
        let (name, tag) = split_image_tag("quay.io/ceph", &desc());
        assert_eq!(name, "quay.io/ceph");
        assert_eq!(tag, "v19.2.3");
    }
}
