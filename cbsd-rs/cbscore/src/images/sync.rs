// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Image sync — orchestrates `skopeo copy` from a source registry
//! to a destination registry, optionally signing the manifest
//! along the way (Phase 5 Commit 5 lands the sign step).
//!
//! Per design 002 §Image Sign & Sync lines 1098–1104. The
//! sign-before-push invariant ("sign before push, not after"
//! — matches the Python implementation and is a downstream-tooling
//! precondition) takes effect once Commit 5 lands
//! [`super::signing::sign_manifest`]; until then, `sync_image`
//! skips the sign step regardless of `config.signing`'s setting,
//! matching the "no signing configured" path.

use cbscore_types::config::Config;
use cbscore_types::images::ImageDescriptorError;
use cbscore_types::utils::secrets::RegistryCreds;

use crate::images::skopeo::{SkopeoOpts, skopeo_copy};
use crate::secrets::SecretsMgr;

const TARGET_IMAGES_SYNC: &str = "cbscore::images::sync";

/// Source / destination image reference for [`sync_image`].
///
/// Carries the `docker://` (or other-protocol) URL plus the
/// per-side registry credentials needed to authenticate the
/// `skopeo copy --src-creds` / `--dest-creds` flags.
///
/// Does not derive `Debug` because [`RegistryCreds`] deliberately
/// does not (secret-redaction invariant per CLAUDE.md Correctness
/// Invariant 5); the credentials must never reach a tracing field
/// or panic message verbatim.
#[derive(Clone)]
pub struct ImageRef {
    /// Full image reference (`docker://registry/image:tag`).
    pub url: String,
    /// Optional registry credentials.
    pub creds: Option<RegistryCreds>,
}

/// Copy `src.url` → `dst.url` via `skopeo copy`, applying the
/// per-side TLS / creds configuration from each [`ImageRef`].
///
/// When `config.signing.is_some()`, Commit 5 will chain a
/// `super::signing::sign_manifest` call between
/// `inspect(src) -> get digest` and `skopeo copy`. Until that
/// commit lands, `sync_image` always takes the plain-copy path
/// regardless of `config.signing`'s state, matching the
/// "no signing configured" semantics.
///
/// # Errors
///
/// Returns [`ImageDescriptorError::Invalid`] wrapping any underlying
/// `skopeo` failure.
///
/// # Examples
///
/// ```no_run
/// use cbscore::images::sync::{sync_image, ImageRef};
/// use cbscore::secrets::SecretsMgr;
/// use cbscore_types::config::Config;
///
/// # async fn demo(cfg: &Config, secrets: &SecretsMgr)
/// #     -> Result<(), cbscore_types::images::ImageDescriptorError>
/// # {
/// sync_image(
///     &ImageRef {
///         url: "docker://staging.example.com/ceph:dev".into(),
///         creds: None,
///     },
///     &ImageRef {
///         url: "docker://prod.example.com/ceph:prod".into(),
///         creds: None,
///     },
///     cfg,
///     secrets,
/// ).await?;
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::images::sync",
    skip(src, dst, _config, _secrets),
    fields(src_url = %src.url, dst_url = %dst.url),
)]
pub async fn sync_image(
    src: &ImageRef,
    dst: &ImageRef,
    _config: &Config,
    _secrets: &SecretsMgr,
) -> Result<(), ImageDescriptorError> {
    tracing::info!(
        target: TARGET_IMAGES_SYNC,
        src = %src.url,
        dst = %dst.url,
        "sync_image: skopeo copy (sign step lands in Commit 5)",
    );
    let opts = SkopeoOpts {
        src_tls_verify: true,
        dst_tls_verify: true,
        src_creds: src.creds.clone(),
        dst_creds: dst.creds.clone(),
    };
    skopeo_copy(&src.url, &dst.url, &opts).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_ref_carries_creds() {
        let r = ImageRef {
            url: "docker://r/i:t".into(),
            creds: Some(RegistryCreds::Plain {
                username: "u".into(),
                password: "p".into(),
                address: "r".into(),
            }),
        };
        assert!(r.creds.is_some());
        assert_eq!(r.url, "docker://r/i:t");
    }

    #[test]
    fn image_ref_no_creds() {
        let r = ImageRef {
            url: "docker://public/img:tag".into(),
            creds: None,
        };
        assert!(r.creds.is_none());
    }
}
