// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso
//! Config-file IO for the top-level [`Config`] struct.
//!
//! Load + atomic store on top of [`tokio::fs`], with format dispatch
//! by file extension (`.yaml` / `.yml` → YAML, anything else → JSON
//! per design 002 §Configuration & Secrets Subsystem §IO).
//!
//! `Config::load` and `Config::store` would be the natural Python
//! call-site shape, but the orphan rule prevents adding inherent
//! methods to a foreign-crate type — they live as free functions
//! here instead. Callers write `cbscore::config::load(path).await?`
//! / `cbscore::config::store(&cfg, path).await?`.

use camino::Utf8Path;
use cbscore_types::config::{
    Config, ConfigError, VaultConfig, VersionedConfig, VersionedVaultConfig,
};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

const TARGET_CONFIG: &str = "cbscore::config";

/// Wire format inferred from a config-file path's extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WireFormat {
    Yaml,
    Json,
}

impl WireFormat {
    fn from_path(path: &Utf8Path) -> Self {
        match path.extension() {
            Some("yaml" | "yml") => Self::Yaml,
            _ => Self::Json,
        }
    }
}

/// Load a cbscore [`Config`] from a YAML or JSON file at `path`.
///
/// Format is inferred from the file extension (`.yaml` / `.yml` →
/// YAML; anything else → JSON). The file's `schema-version` key
/// (kebab per design 002 §Wire-Format Versioning) is enforced via
/// [`VersionedConfig::from_value`].
///
/// # Errors
///
/// - [`ConfigError::NotFound`] when the file does not exist
///   ([`std::io::ErrorKind::NotFound`] mapped to a structured shape
///   so the CLI can surface the operator-facing
///   "create one with cbsbuild config init" hint).
/// - [`ConfigError::Io`] for any other IO failure (permission
///   denied, IO failure mid-read).
/// - [`ConfigError::MissingSchemaVersion`] when the file is missing
///   the kebab `schema-version` key.
/// - [`ConfigError::UnknownSchemaVersion`] when the file declares a
///   `schema-version` higher than this build supports.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
///
/// # async fn demo() -> Result<(), cbscore_types::config::ConfigError> {
/// let cfg = cbscore::config::load(
///     Utf8Path::new("/etc/cbs/cbs-build.config.yaml"),
/// )
/// .await?;
/// let _ = cfg;
/// # Ok(()) }
/// ```
pub async fn load(path: &Utf8Path) -> Result<Config, ConfigError> {
    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ConfigError::NotFound {
                path: path.to_owned(),
            });
        }
        Err(e) => return Err(ConfigError::Io { source: e }),
    };
    let value: serde_value::Value = match WireFormat::from_path(path) {
        WireFormat::Yaml => serde_saphyr::from_slice(&bytes).map_err(|e| ConfigError::Io {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
        })?,
        WireFormat::Json => serde_json::from_slice(&bytes).map_err(|e| ConfigError::Io {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
        })?,
    };
    let cfg = VersionedConfig::from_value(value, path)?.into_latest();
    tracing::debug!(
        target: TARGET_CONFIG,
        path = %path,
        format = ?WireFormat::from_path(path),
        "config loaded",
    );
    Ok(cfg)
}

/// Serialise `cfg` as YAML (with the kebab `schema-version: 1`
/// marker first) and write it atomically to `path`.
///
/// Behaviour:
///
/// 1. `tokio::fs::create_dir_all` on `path.parent()` — `cbsbuild
///    config init` writes to
///    `~/.config/cbsd/${deployment}/worker/cbscore.config.yaml` on
///    a fresh workstation, where the parent dir does not yet exist.
/// 2. Serialise via `serde_saphyr` through
///    [`VersionedConfig::new`]; the wire format starts with
///    `schema-version: 1` because the kebab tag is emitted first by
///    [`VersionedConfig`]'s `Serialize` impl.
/// 3. Open a sibling tempfile in the same parent dir, `sync_all` to
///    flush data + metadata to disk, `rename` to the final path —
///    matches the atomic-write pattern in
///    [`crate::secrets::utils::write_secure_file`] and is required
///    by design 002 line 498–507. Rename within the same dir is
///    atomic on Linux (POSIX `rename(2)` guarantee), so a concurrent
///    reader never observes a partially-written config file even if
///    `store` is interrupted (signal, panic, write error).
///
/// `store` writes YAML unconditionally — the design 002 line 498
/// reference notes Python also produces YAML.
///
/// # Errors
///
/// Returns [`ConfigError::Io`] on any IO failure during dir create,
/// serialisation, tempfile write, fsync, or rename.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
/// use cbscore_types::config::Config;
///
/// # async fn demo(cfg: &Config) -> Result<(), cbscore_types::config::ConfigError> {
/// cbscore::config::store(
///     cfg,
///     Utf8Path::new("/etc/cbs/cbs-build.config.yaml"),
/// )
/// .await?;
/// # Ok(()) }
/// ```
pub async fn store(cfg: &Config, path: &Utf8Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent()
        && !parent.as_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ConfigError::Io { source: e })?;
    }
    let yaml = serde_saphyr::to_string(&VersionedConfig::new(cfg.clone())).map_err(|e| {
        ConfigError::Io {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
        }
    })?;
    write_atomic(path, yaml.as_bytes()).await?;
    tracing::debug!(
        target: TARGET_CONFIG,
        path = %path,
        bytes = yaml.len(),
        "config stored",
    );
    Ok(())
}

/// Serialise `vault` as YAML (with the kebab `schema-version: 1`
/// marker first) and write it atomically to `path`.
///
/// Mirrors [`store`] for the [`VaultConfig`] file shape — the
/// `cbs-build.vault.yaml` companion file produced by `cbsbuild
/// config init-vault`. Same parent-directory creation, same
/// tempfile + fsync + rename atomic-write pattern.
///
/// The wire format starts with `schema-version: 1` because the
/// kebab tag is emitted first by [`VersionedVaultConfig`]'s
/// `Serialize` impl.
///
/// # Errors
///
/// Returns [`ConfigError::Io`] on any IO failure during dir create,
/// serialisation, tempfile write, fsync, or rename.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
/// use cbscore_types::config::VaultConfig;
///
/// # async fn demo(vault: &VaultConfig) -> Result<(), cbscore_types::config::ConfigError> {
/// cbscore::config::store_vault(
///     vault,
///     Utf8Path::new("/etc/cbs/cbs-build.vault.yaml"),
/// )
/// .await?;
/// # Ok(()) }
/// ```
pub async fn store_vault(vault: &VaultConfig, path: &Utf8Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent()
        && !parent.as_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ConfigError::Io { source: e })?;
    }
    let yaml = serde_saphyr::to_string(&VersionedVaultConfig::new(vault.clone())).map_err(|e| {
        ConfigError::Io {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
        }
    })?;
    write_atomic(path, yaml.as_bytes()).await?;
    tracing::debug!(
        target: TARGET_CONFIG,
        path = %path,
        bytes = yaml.len(),
        "vault config stored",
    );
    Ok(())
}

/// Atomic write helper: sibling tempfile + fsync + rename within the
/// same parent dir. Returns IO errors mapped into [`ConfigError`].
///
/// Mirrors [`crate::secrets::utils::write_secure_file`] except the
/// tempfile is created at default mode (the secrets variant pins to
/// 0600 because credentials must be owner-only; config files have
/// no such requirement and follow the umask).
async fn write_atomic(path: &Utf8Path, contents: &[u8]) -> Result<(), ConfigError> {
    let parent = path.parent().ok_or_else(|| ConfigError::Io {
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cannot derive parent dir of '{path}' for atomic write"),
        ),
    })?;
    let tmp_path = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name().unwrap_or("cbs-config"),
        std::process::id(),
    ));
    let mut tmp = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp_path)
        .await
        .map_err(|e| ConfigError::Io { source: e })?;
    tmp.write_all(contents)
        .await
        .map_err(|e| ConfigError::Io { source: e })?;
    tmp.sync_all()
        .await
        .map_err(|e| ConfigError::Io { source: e })?;
    drop(tmp);
    tokio::fs::rename(&tmp_path, path)
        .await
        .map_err(|e| ConfigError::Io { source: e })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::config::{Config, PathsConfig, VaultConfig};

    fn sample_vault() -> VaultConfig {
        VaultConfig {
            vault_addr: "https://vault.example.com".into(),
            auth_user: None,
            auth_approle: None,
            auth_token: Some("hvs.AAAA".into()),
        }
    }

    fn sample_config() -> Config {
        Config {
            paths: PathsConfig {
                components: vec![camino::Utf8PathBuf::from("/srv/components")],
                scratch: camino::Utf8PathBuf::from("/srv/scratch"),
                scratch_containers: camino::Utf8PathBuf::from("/srv/scratch-containers"),
                ccache: None,
                versions: None,
            },
            storage: None,
            signing: None,
            logging: None,
            secrets: Vec::new(),
            vault: None,
        }
    }

    #[tokio::test]
    async fn round_trip_yaml() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("cfg.yaml")).expect("utf8 path");
        let cfg = sample_config();
        store(&cfg, &path).await.expect("store");
        let loaded = load(&path).await.expect("load");
        assert_eq!(loaded, cfg);
    }

    #[tokio::test]
    async fn round_trip_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let yaml_path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("cfg.yaml")).expect("utf8 path");
        let cfg = sample_config();
        store(&cfg, &yaml_path).await.expect("store yaml");
        // Re-emit as JSON (`store` always writes YAML; we hand-roll
        // a JSON variant here to exercise the `load` dispatch path).
        let json_path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("cfg.json")).expect("utf8 path");
        let json = serde_json::to_string(&VersionedConfig::new(cfg.clone())).expect("json");
        tokio::fs::write(&json_path, json)
            .await
            .expect("write json");
        let loaded = load(&json_path).await.expect("load json");
        assert_eq!(loaded, cfg);
    }

    #[tokio::test]
    async fn load_missing_file_returns_not_found() {
        let path = camino::Utf8PathBuf::from("/nonexistent/cbs-build.config.yaml");
        let Err(err) = load(&path).await else {
            panic!("expected NotFound, got Ok");
        };
        assert!(matches!(err, ConfigError::NotFound { .. }));
    }

    #[tokio::test]
    async fn store_creates_parent_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("a/b/c/cbs-build.config.yaml"))
                .expect("utf8 path");
        assert!(!nested.parent().unwrap().exists());
        store(&sample_config(), &nested).await.expect("store");
        assert!(nested.exists());
        assert!(nested.parent().unwrap().exists());
    }

    #[tokio::test]
    async fn store_emits_schema_version_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("cfg.yaml")).expect("utf8 path");
        store(&sample_config(), &path).await.expect("store");
        let body = tokio::fs::read_to_string(&path).await.expect("read");
        assert!(
            body.starts_with("schema-version: 1\n"),
            "expected 'schema-version: 1' first, got: {}",
            body.lines().next().unwrap_or(""),
        );
    }

    #[tokio::test]
    async fn load_missing_schema_version_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("cfg.yaml")).expect("utf8 path");
        // Write a YAML body without the schema-version marker.
        tokio::fs::write(&path, b"paths: {}\n")
            .await
            .expect("write");
        let Err(err) = load(&path).await else {
            panic!("expected MissingSchemaVersion, got Ok");
        };
        assert!(matches!(err, ConfigError::MissingSchemaVersion { .. }));
    }

    #[tokio::test]
    async fn store_vault_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("cbs-build.vault.yaml"))
            .expect("utf8 path");
        let vault = sample_vault();
        store_vault(&vault, &path).await.expect("store_vault");
        let body = tokio::fs::read_to_string(&path).await.expect("read");
        assert!(
            body.starts_with("schema-version: 1\n"),
            "expected 'schema-version: 1' first, got: {}",
            body.lines().next().unwrap_or(""),
        );
        let raw: serde_value::Value = serde_saphyr::from_str(&body).expect("parse vault yaml");
        let parsed = VersionedVaultConfig::from_value(raw, &path)
            .expect("from_value")
            .into_latest();
        assert_eq!(parsed, vault);
    }

    #[tokio::test]
    async fn store_vault_creates_parent_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("a/b/c/cbs-build.vault.yaml"))
                .expect("utf8 path");
        assert!(!nested.parent().unwrap().exists());
        store_vault(&sample_vault(), &nested)
            .await
            .expect("store_vault");
        assert!(nested.exists());
        assert!(nested.parent().unwrap().exists());
    }

    #[tokio::test]
    async fn load_future_schema_version_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("cfg.yaml")).expect("utf8 path");
        tokio::fs::write(&path, b"schema-version: 99\npaths: {}\n")
            .await
            .expect("write");
        let Err(err) = load(&path).await else {
            panic!("expected UnknownSchemaVersion, got Ok");
        };
        assert!(matches!(
            err,
            ConfigError::UnknownSchemaVersion {
                found: 99,
                max_supported: 1,
                ..
            }
        ));
    }
}
