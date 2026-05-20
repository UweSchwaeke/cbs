// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Release publish driver — uploads RPMs + the release descriptor
//! JSON to S3 using Phase 3 Commit 1's [`utils::s3`](crate::utils::s3)
//! primitives.

use cbscore_types::releases::ReleaseError;
use cbscore_types::releases::desc::ReleaseDesc;

use super::utils::release_desc_key;
use crate::builder::rpmbuild::RpmArtifact;
use crate::utils::s3::{S3Error, release_desc_upload, s3_upload_rpms};

const TARGET_RELEASES_S3: &str = "cbscore::releases::s3";

/// Upload `rpms` to `bucket/<loc>/<version>/` and the serialised
/// `desc` to `bucket/<loc>/<version>.json` per design 002 §S3
/// operations.
///
/// Upload order: RPMs first (multi-file), descriptor last so a
/// reader who sees the descriptor file can trust the RPMs exist.
///
/// # Errors
///
/// - [`ReleaseError::Invalid`] wrapping any underlying
///   [`S3Error`] from the S3 PUT operations. The Phase 3 Commit 1
///   primitives provide retry-via-aws-sdk on transient errors;
///   only non-retryable failures bubble up here.
///
/// # Examples
///
/// ```no_run
/// use cbscore::releases::s3::upload_release;
/// use cbscore::builder::rpmbuild::RpmArtifact;
/// use cbscore_types::releases::desc::ReleaseDesc;
///
/// # async fn demo(desc: &ReleaseDesc, rpms: &[RpmArtifact])
/// #     -> Result<(), cbscore_types::releases::ReleaseError>
/// # {
/// upload_release("releases-bucket", "ceph/dev", desc, rpms).await?;
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::releases::s3",
    skip(desc, rpms),
    fields(version = %desc.version, rpm_count = rpms.len()),
)]
pub async fn upload_release(
    bucket: &str,
    loc: &str,
    desc: &ReleaseDesc,
    rpms: &[RpmArtifact],
) -> Result<(), ReleaseError> {
    // 1. Upload every RPM. The s3_upload_rpms helper takes a slice
    //    of Utf8PathBufs; project the RpmArtifact list onto that
    //    shape.
    let paths: Vec<camino::Utf8PathBuf> = rpms.iter().map(|a| a.path.clone()).collect();
    if !paths.is_empty() {
        let key_prefix = format!("{}/{}", loc.trim_end_matches('/'), desc.version);
        s3_upload_rpms(bucket, &key_prefix, &paths)
            .await
            .map_err(|e| wrap_s3(&e))?;
        tracing::info!(
            target: TARGET_RELEASES_S3,
            bucket,
            prefix = %key_prefix,
            count = paths.len(),
            "RPMs uploaded",
        );
    }

    // 2. Upload the release descriptor JSON. The key shape
    //    matches Python: <loc>/<version>.json.
    let desc_json = serde_json::to_vec_pretty(desc).map_err(|e| {
        ReleaseError::Invalid(format!("serialise ReleaseDesc for '{}': {e}", desc.version))
    })?;
    let desc_key = release_desc_key(loc, &desc.version);
    release_desc_upload(bucket, &desc_key, desc_json)
        .await
        .map_err(|e| wrap_s3(&e))?;
    tracing::info!(
        target: TARGET_RELEASES_S3,
        bucket,
        key = %desc_key,
        "release descriptor uploaded",
    );

    Ok(())
}

/// Map an [`S3Error`] from Phase 3's primitives into a
/// [`ReleaseError::Invalid`]. The source chain terminates here;
/// the cbscore-types `ReleaseError` enum is intentionally lossy
/// (no `#[from] S3Error` because that would pull aws-sdk-s3 into
/// cbscore-types' dependency graph per design 001).
fn wrap_s3(e: &S3Error) -> ReleaseError {
    ReleaseError::Invalid(format!("S3: {e}"))
}

// Re-export so the symbol is referenced when callers look at the
// module's public surface without picking up a separate utils
// import.
pub use super::utils::rpm_key as derive_rpm_key;

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::releases::desc::ReleaseDesc;
    use std::collections::HashMap;

    fn sample_desc(version: &str) -> ReleaseDesc {
        ReleaseDesc {
            version: version.into(),
            builds: HashMap::new(),
        }
    }

    #[test]
    fn rpm_key_helper_is_reexported() {
        let k = derive_rpm_key("ceph/dev", "19.2.3", camino::Utf8Path::new("/tmp/x.rpm"));
        assert_eq!(k, "ceph/dev/19.2.3/x.rpm");
    }

    #[test]
    fn serialise_release_desc_round_trips() {
        let d = sample_desc("19.2.3");
        let bytes = serde_json::to_vec_pretty(&d).unwrap();
        let parsed: ReleaseDesc = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed, d);
    }

    // upload_release end-to-end exercise requires a live S3 endpoint
    // (MinIO / LocalStack); deferred to the env-driven integration
    // suite in Phase 6 per the plan.
}
