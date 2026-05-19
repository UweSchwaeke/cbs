// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Secrets manager — loads, merges, and resolves the per-family
//! credential maps consumed by the build pipeline.

use camino::Utf8Path;
use cbscore_types::config::vault::VaultConfig;
use cbscore_types::utils::secrets::{
    GitCreds, GitPlainCreds, GitVaultCreds, RegistryCreds, Secrets, SecretsError, SigningCreds,
    SigningPlainCreds, SigningVaultCreds, StorageCreds, StoragePlainCreds, StorageVaultCreds,
    VersionedSecrets,
};

use super::models;
use super::utils::write_secure_file;
use crate::utils::vault::{VaultError, kv_read};

const TARGET_SECRETS_MGR: &str = "cbscore::secrets::mgr";

const VAULT_DEFAULT_MOUNT: &str = "ces-kv";

/// Manager for the per-family credential maps.
///
/// Wraps a single [`Secrets`] payload built up incrementally from
/// one or more files (via [`SecretsMgr::load_files`] /
/// [`SecretsMgr::merge`]) and resolved against Vault in-place (via
/// [`SecretsMgr::resolve_vault_refs`]) before being dumped to the
/// runner-mounted secrets tempfile (via
/// [`SecretsMgr::dump_to_runner`]).
///
/// The internal `Secrets` is intentionally **not** publicly exposed
/// — direct mutation would let callers bypass the in-place vault-ref
/// resolution contract.
#[derive(Default)]
pub struct SecretsMgr {
    inner: Secrets,
}

impl SecretsMgr {
    /// Construct an empty manager with all four family `HashMap`s
    /// initialised to empty.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Borrow the underlying [`Secrets`] for read-only access — used
    /// by callers that need to inspect resolved credentials before
    /// dump.
    #[must_use]
    pub const fn secrets(&self) -> &Secrets {
        &self.inner
    }

    /// Load and merge a list of secrets YAML files into a fresh
    /// manager.
    ///
    /// An empty `paths` slice returns an empty manager — not an
    /// error. Deployments that mint `secrets.yaml` from Vault refs at
    /// request time start with zero pre-populated files; the manager
    /// is built incrementally and the empty-paths case is a normal
    /// startup path. The caller is responsible for ensuring the
    /// manager is non-empty before [`Self::dump_to_runner`] if the
    /// build pipeline requires credentials.
    ///
    /// Files are loaded and merged in the order given. Per-family
    /// keys overlap with `dict.update()` semantics: the value from
    /// the later file overwrites the earlier file's entry.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError::Manager`] if any file fails to read
    /// or parse. The wrapped message names the offending path.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use camino::Utf8PathBuf;
    /// use cbscore::secrets::SecretsMgr;
    ///
    /// # async fn demo() -> Result<(), cbscore_types::utils::secrets::SecretsError> {
    /// let mgr = SecretsMgr::load_files(&[
    ///     Utf8PathBuf::from("/etc/cbs/secrets.yaml"),
    ///     Utf8PathBuf::from("/etc/cbs/overrides.yaml"),
    /// ])
    /// .await?;
    /// let _ = mgr;
    /// # Ok(()) }
    /// ```
    pub async fn load_files(paths: &[camino::Utf8PathBuf]) -> Result<Self, SecretsError> {
        let mut mgr = Self::empty();
        for p in paths {
            let s = models::load_secrets_file(p).await?;
            mgr.merge(s);
        }
        tracing::debug!(
            target: TARGET_SECRETS_MGR,
            file_count = paths.len(),
            git = mgr.inner.git.len(),
            storage = mgr.inner.storage.len(),
            signing = mgr.inner.signing.len(),
            registry = mgr.inner.registry.len(),
            "secrets manager built",
        );
        Ok(mgr)
    }

    /// Merge `other` into `self` — per-family `HashMap.extend()`
    /// semantics (later wins on key conflict, matching Python
    /// `dict.update()`).
    pub fn merge(&mut self, other: Secrets) {
        self.inner.git.extend(other.git);
        self.inner.storage.extend(other.storage);
        self.inner.signing.extend(other.signing);
        self.inner.registry.extend(other.registry);
    }

    /// Walk every Vault-side entry across all four families
    /// (`GitCreds::Vault`, `StorageCreds::Vault`,
    /// `SigningCreds::Vault`, `RegistryCreds::Vault`) and replace
    /// them with their `*Plain*` counterparts in place, fetching the
    /// payload from Vault via [`kv_read`].
    ///
    /// The mount used for every lookup is `ces-kv` (matching
    /// Python's hardcoded mount — operators that need a different
    /// mount today provision a `ces-kv` alias).
    ///
    /// # Retry safety
    ///
    /// Resolution is idempotent: already-resolved `*Plain*` variants
    /// are skipped on subsequent calls, and the remaining
    /// `*Vault*` entries are retried. On `Err` mid-resolution
    /// (e.g. the 4th of 5 entries fails) the first 3 entries are
    /// already resolved and the 4th + 5th remain as `*Vault*` —
    /// the caller may invoke `resolve_vault_refs` again.
    ///
    /// **Caller contract:** do not call [`Self::dump_to_runner`]
    /// after an `Err` until a subsequent `resolve_vault_refs`
    /// returns `Ok`; otherwise the dumped YAML carries unresolved
    /// vault refs that the in-container build cannot dereference.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError::Manager`] wrapping the first Vault
    /// error encountered, with a message identifying the offending
    /// family + keyref.
    pub async fn resolve_vault_refs(&mut self, config: &VaultConfig) -> Result<(), SecretsError> {
        for (name, entry) in &mut self.inner.git {
            if let GitCreds::Vault(v) = entry {
                let plain = resolve_git_vault(config, v)
                    .await
                    .map_err(|e| SecretsError::Manager(format!("git entry '{name}': {e}")))?;
                *entry = GitCreds::Plain(plain);
            }
        }
        for (name, entry) in &mut self.inner.storage {
            if let StorageCreds::Vault(v) = entry {
                let plain = resolve_storage_vault(config, v)
                    .await
                    .map_err(|e| SecretsError::Manager(format!("storage entry '{name}': {e}")))?;
                *entry = StorageCreds::Plain(plain);
            }
        }
        for (name, entry) in &mut self.inner.signing {
            if let SigningCreds::Vault(v) = entry {
                let plain = resolve_signing_vault(config, v)
                    .await
                    .map_err(|e| SecretsError::Manager(format!("signing entry '{name}': {e}")))?;
                *entry = SigningCreds::Plain(plain);
            }
        }
        for (name, entry) in &mut self.inner.registry {
            if let RegistryCreds::Vault { key, address, .. } = entry {
                let resolved = resolve_registry_vault(config, key, address)
                    .await
                    .map_err(|e| SecretsError::Manager(format!("registry entry '{name}': {e}")))?;
                *entry = resolved;
            }
        }
        Ok(())
    }

    /// Serialise the merged + resolved secrets payload to YAML and
    /// write it atomically to `path` at mode 0600. The runner
    /// (Phase 4) mounts this file at
    /// `/runner/cbs-build.secrets.yaml` inside the builder
    /// container.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError::Manager`] on serialisation failure or
    /// any IO failure during the tempfile create / write / fsync /
    /// rename sequence in [`write_secure_file`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use camino::Utf8Path;
    /// use cbscore::secrets::SecretsMgr;
    ///
    /// # async fn demo(mgr: &SecretsMgr) -> Result<(), cbscore_types::utils::secrets::SecretsError> {
    /// mgr.dump_to_runner(Utf8Path::new("/run/secrets/cbs-build.secrets.yaml"))
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn dump_to_runner(&self, path: &Utf8Path) -> Result<(), SecretsError> {
        let yaml = serde_saphyr::to_string(&VersionedSecrets::new(self.inner.clone()))
            .map_err(|e| SecretsError::Manager(format!("serialise secrets to YAML: {e}")))?;
        write_secure_file(path, yaml.as_bytes()).await?;
        tracing::debug!(
            target: TARGET_SECRETS_MGR,
            path = %path,
            bytes = yaml.len(),
            "secrets dumped to runner mount",
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Per-family Vault → Plain resolution
// ---------------------------------------------------------------------

/// Decode a Vault payload field, returning `Manager` on missing key.
fn require_field(
    map: &std::collections::HashMap<String, String>,
    key: &str,
) -> Result<String, VaultError> {
    map.get(key)
        .cloned()
        .ok_or_else(|| VaultError::BadResponse {
            message: format!("vault payload missing '{key}' field"),
        })
}

async fn resolve_git_vault(
    config: &VaultConfig,
    creds: &GitVaultCreds,
) -> Result<GitPlainCreds, VaultError> {
    match creds {
        GitVaultCreds::Ssh {
            username,
            ssh_key,
            key,
        } => {
            // The `ssh_key` field in the wire format is the in-vault
            // field name for the PEM payload; the actual key payload
            // lives in Vault under the operator-chosen `key` keyref.
            let payload = kv_read(config, VAULT_DEFAULT_MOUNT, key).await?;
            let resolved_key = require_field(&payload, ssh_key)?;
            Ok(GitPlainCreds::Ssh {
                username: username.clone(),
                ssh_key: resolved_key,
            })
        }
        GitVaultCreds::Https {
            username,
            password,
            key,
        } => {
            let payload = kv_read(config, VAULT_DEFAULT_MOUNT, key).await?;
            let resolved_pw = require_field(&payload, password)?;
            Ok(GitPlainCreds::Https {
                username: username.clone(),
                password: resolved_pw,
            })
        }
    }
}

async fn resolve_storage_vault(
    config: &VaultConfig,
    creds: &StorageVaultCreds,
) -> Result<StoragePlainCreds, VaultError> {
    let StorageVaultCreds::S3 {
        key,
        access_id,
        secret_id,
    } = creds;
    let payload = kv_read(config, VAULT_DEFAULT_MOUNT, key).await?;
    Ok(StoragePlainCreds::S3 {
        access_id: require_field(&payload, access_id)?,
        secret_id: require_field(&payload, secret_id)?,
    })
}

async fn resolve_signing_vault(
    config: &VaultConfig,
    creds: &SigningVaultCreds,
) -> Result<SigningPlainCreds, VaultError> {
    match creds {
        SigningVaultCreds::GpgSingleKey {
            key,
            private_key,
            public_key,
            passphrase,
            email,
        } => {
            let payload = kv_read(config, VAULT_DEFAULT_MOUNT, key).await?;
            Ok(SigningPlainCreds::GpgArmorKey {
                private_key: require_field(&payload, private_key)?,
                public_key: public_key
                    .as_deref()
                    .map(|f| require_field(&payload, f))
                    .transpose()?,
                passphrase: passphrase
                    .as_deref()
                    .map(|f| require_field(&payload, f))
                    .transpose()?,
                email: email.clone(),
            })
        }
        SigningVaultCreds::GpgPvtKey {
            key,
            private_key,
            passphrase,
            email,
        } => {
            let payload = kv_read(config, VAULT_DEFAULT_MOUNT, key).await?;
            Ok(SigningPlainCreds::GpgArmorKey {
                private_key: require_field(&payload, private_key)?,
                public_key: None,
                passphrase: passphrase
                    .as_deref()
                    .map(|f| require_field(&payload, f))
                    .transpose()?,
                email: email.clone(),
            })
        }
        SigningVaultCreds::GpgPubKey { .. } | SigningVaultCreds::Transit { .. } => {
            // Pub-only and Transit don't have a plain-text projection
            // — the builder consumes them as-is via their dedicated
            // path. Return BadResponse so the caller skips the
            // in-place rewrite for these shapes.
            Err(VaultError::BadResponse {
                message: "signing creds variant has no plain-text projection".into(),
            })
        }
    }
}

async fn resolve_registry_vault(
    config: &VaultConfig,
    key: &str,
    address: &str,
) -> Result<RegistryCreds, VaultError> {
    let payload = kv_read(config, VAULT_DEFAULT_MOUNT, key).await?;
    Ok(RegistryCreds::Plain {
        username: require_field(&payload, "username")?,
        password: require_field(&payload, "password")?,
        address: address.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_secrets() -> Secrets {
        let mut git = HashMap::new();
        git.insert(
            "ceph-mirror".into(),
            GitCreds::Plain(GitPlainCreds::Ssh {
                username: "git".into(),
                ssh_key: "FAKE-KEY".into(),
            }),
        );
        Secrets {
            git,
            ..Secrets::default()
        }
    }

    #[tokio::test]
    async fn empty_paths_returns_empty_manager() {
        let mgr = SecretsMgr::load_files(&[])
            .await
            .expect("empty paths is not an error");
        assert!(mgr.inner.git.is_empty());
        assert!(mgr.inner.storage.is_empty());
        assert!(mgr.inner.signing.is_empty());
        assert!(mgr.inner.registry.is_empty());
    }

    #[test]
    fn merge_extends_per_family_maps() {
        let mut mgr = SecretsMgr::empty();
        mgr.merge(sample_secrets());
        assert_eq!(mgr.inner.git.len(), 1);

        // Second merge with disjoint key → both entries present.
        let mut second = Secrets::default();
        second.git.insert(
            "ubuntu-mirror".into(),
            GitCreds::Plain(GitPlainCreds::Token {
                username: "git".into(),
                token: "FAKE".into(),
            }),
        );
        mgr.merge(second);
        assert_eq!(mgr.inner.git.len(), 2);
    }

    #[test]
    fn merge_overwrites_on_key_conflict() {
        let mut mgr = SecretsMgr::empty();
        mgr.merge(sample_secrets());
        // Overwrite "ceph-mirror" with a Token variant.
        let mut second = Secrets::default();
        second.git.insert(
            "ceph-mirror".into(),
            GitCreds::Plain(GitPlainCreds::Token {
                username: "git".into(),
                token: "NEW".into(),
            }),
        );
        mgr.merge(second);
        assert_eq!(mgr.inner.git.len(), 1);
        let entry = mgr.inner.git.get("ceph-mirror").expect("entry");
        assert!(matches!(
            entry,
            GitCreds::Plain(GitPlainCreds::Token { .. })
        ));
    }

    #[tokio::test]
    async fn dump_to_runner_round_trips() {
        let mut mgr = SecretsMgr::empty();
        mgr.merge(sample_secrets());
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("secrets.yaml")).expect("utf8 path");
        mgr.dump_to_runner(&path).await.expect("dump");
        // Reload via models::load_secrets_file (the same path
        // load_files uses internally).
        let loaded = models::load_secrets_file(&path).await.expect("load");
        assert_eq!(loaded.git.len(), 1);
        assert!(matches!(
            loaded.git.get("ceph-mirror").unwrap(),
            GitCreds::Plain(GitPlainCreds::Ssh { .. })
        ));
    }
}
