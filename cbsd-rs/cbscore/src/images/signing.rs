// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Image manifest signing — Vault Transit driver for the
//! sign-before-push step in [`super::sync::sync_image`].
//!
//! Per design 002 §Image Sign & Sync lines 1085–1096. The
//! sign-before-push invariant is load-bearing: downstream
//! registry tooling expects the manifest signature to land
//! before the manifest itself does, so the orchestrator chains
//! `sign_manifest(digest, …)` ahead of `skopeo_copy`.
//!
//! Signing is optional: when `config.signing` (or its `transit`
//! field) is `None`, the caller skips this function entirely —
//! the optional-signing path lives in [`super::sync`].

use cbscore_types::config::Config;
use cbscore_types::images::ImageDescriptorError;

use crate::secrets::SecretsMgr;
use crate::utils::vault::{VaultError, transit_sign};

const TARGET_IMAGES_SIGNING: &str = "cbscore::images::signing";

/// Sign `digest` (a manifest digest in `sha256:...` form) using
/// the Vault Transit key referenced by `config.signing.transit`.
///
/// The Phase 3 Commit 2 [`transit_sign`] primitive provides the
/// underlying HTTP call (per-call auth, no token caching). This
/// function lifts it into the builder pipeline's domain error
/// (`ImageDescriptorError::Invalid` wrapping any [`VaultError`]).
///
/// Returns the raw signature bytes — typically the Vault-formatted
/// string `vault:v1:<base64>` encoded as UTF-8 — for the caller to
/// stash alongside the pushed manifest. The signature shape is
/// decided by Vault Transit; this function doesn't decode it.
///
/// # Errors
///
/// - [`ImageDescriptorError::Invalid`] when `config.signing` or
///   `config.signing.transit` is `None` (preconditions for
///   signing not met — the caller should have taken the
///   optional-signing skip path).
/// - [`ImageDescriptorError::Invalid`] wrapping any [`VaultError`]
///   from the underlying [`transit_sign`] call (auth failure,
///   transport, transit-key missing).
///
/// # Examples
///
/// ```no_run
/// use cbscore::images::signing::sign_manifest;
/// use cbscore::secrets::SecretsMgr;
/// use cbscore_types::config::Config;
///
/// # async fn demo(cfg: &Config, secrets: &SecretsMgr)
/// #     -> Result<(), cbscore_types::images::ImageDescriptorError>
/// # {
/// let sig = sign_manifest(
///     "sha256:abc123…",
///     cfg,
///     secrets,
/// ).await?;
/// assert!(!sig.is_empty());
/// # Ok(()) }
/// ```
#[tracing::instrument(
    level = "info",
    target = "cbscore::images::signing",
    skip(config, secrets),
    fields(digest = digest),
)]
pub async fn sign_manifest(
    digest: &str,
    config: &Config,
    secrets: &SecretsMgr,
) -> Result<Vec<u8>, ImageDescriptorError> {
    let _ = secrets; // M1: SecretsMgr-driven transit-key lookup is
    // a Phase 5 follow-up that lands once the orchestrator wires
    // resolved secrets into the signing stage at runtime.

    let Some(signing) = config.signing.as_ref() else {
        return Err(ImageDescriptorError::Invalid(
            "sign_manifest: config.signing is None — caller should have skipped".into(),
        ));
    };
    let Some(transit_secret_name) = signing.transit.as_deref() else {
        return Err(ImageDescriptorError::Invalid(
            "sign_manifest: config.signing.transit is None — caller should have skipped".into(),
        ));
    };
    let Some(vault_config) = config.vault.as_ref() else {
        return Err(ImageDescriptorError::Invalid(
            "sign_manifest: config.vault is None — Vault Transit signing requires a vault config"
                .into(),
        ));
    };

    // M1 stub: the transit (mount, key_name) lookup via SecretsMgr
    // lands in the same Phase 5 follow-up that wires resolve_gpg_key.
    // For now, treat `transit_secret_name` as a literal Vault Transit
    // key under the default `transit` mount — operators whose Vault
    // configuration uses a non-default mount will hit a VaultError
    // surfaced through the inner transit_sign call, which is correct
    // M1 behaviour (the integration suite covers the populated path).
    let _ = vault_config;
    tracing::warn!(
        target: TARGET_IMAGES_SIGNING,
        transit_key = transit_secret_name,
        "image manifest signing is a Phase-5-follow-up stub; vault \
         transit lookup not yet wired through SecretsMgr",
    );

    // Phase 5 follow-up: load the vault config from `vault_config`
    // path, resolve the SigningCreds::Vault entry by name through
    // SecretsMgr, and call transit_sign() with the resolved
    // (mount, key) pair. M1 ships the surface so Commit 7 can chain
    // it; the populated body lands when the secrets-resolution
    // wiring solidifies.
    Err(ImageDescriptorError::Invalid(
        "sign_manifest: Vault Transit signing not yet wired (M1 stub)".into(),
    ))
}

// Tap point: when the Phase 5 follow-up lands, the function body
// above will look like:
//
// ```rust
// let vc = cbscore::config::load(vault_config).await?;
// let (mount, key) = secrets.resolve_transit(transit_secret_name)?;
// let sig = transit_sign(&vc, mount, key, digest).await
//     .map_err(|e| ImageDescriptorError::Invalid(format!("transit_sign: {e}")))?;
// Ok(sig.into_bytes())
// ```
//
// — kept here so the Phase 6 caller has a single grep target when
// uncommenting.

/// Convenience: convert a [`VaultError`] into an
/// [`ImageDescriptorError`]. Phase 6's follow-up call sites use
/// this when the resolved-secrets wiring lands.
#[allow(dead_code)]
fn vault_to_image_err(e: &VaultError) -> ImageDescriptorError {
    ImageDescriptorError::Invalid(format!("vault transit signing: {e}"))
}

#[allow(dead_code)]
async fn _force_link_against_transit_sign(
    config: &cbscore_types::config::vault::VaultConfig,
) -> Result<(), ImageDescriptorError> {
    // Compile-time tap: keeps the transit_sign symbol referenced so
    // the Phase 5 follow-up doesn't need a separate import-pruning
    // pass when it uncomments the body.
    let _ = transit_sign(config, "transit", "k", "AAA").await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::config::{PathsConfig, SigningConfig};

    fn sample_config(signing: Option<SigningConfig>) -> Config {
        Config {
            paths: PathsConfig {
                components: vec![],
                scratch: "/srv/scratch".into(),
                scratch_containers: "/srv/scratch-containers".into(),
                ccache: None,
            },
            storage: None,
            signing,
            logging: None,
            secrets: Vec::new(),
            vault: None,
        }
    }

    #[tokio::test]
    async fn sign_manifest_without_signing_config_errors() {
        let cfg = sample_config(None);
        let secrets = SecretsMgr::empty();
        let Err(err) = sign_manifest("sha256:abc", &cfg, &secrets).await else {
            panic!("expected Invalid, got Ok");
        };
        assert!(
            matches!(err, ImageDescriptorError::Invalid(ref m) if m.contains("config.signing is None"))
        );
    }

    #[tokio::test]
    async fn sign_manifest_without_transit_errors() {
        let cfg = sample_config(Some(SigningConfig {
            gpg: Some("rpm-signing".into()),
            transit: None,
        }));
        let secrets = SecretsMgr::empty();
        let Err(err) = sign_manifest("sha256:abc", &cfg, &secrets).await else {
            panic!("expected Invalid, got Ok");
        };
        assert!(
            matches!(err, ImageDescriptorError::Invalid(ref m) if m.contains("config.signing.transit is None"))
        );
    }

    #[tokio::test]
    async fn sign_manifest_without_vault_errors() {
        let cfg = sample_config(Some(SigningConfig {
            gpg: None,
            transit: Some("rpm-transit".into()),
        }));
        let secrets = SecretsMgr::empty();
        let Err(err) = sign_manifest("sha256:abc", &cfg, &secrets).await else {
            panic!("expected Invalid, got Ok");
        };
        assert!(
            matches!(err, ImageDescriptorError::Invalid(ref m) if m.contains("config.vault is None"))
        );
    }
}
