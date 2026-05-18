// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Storage-destination subset of cbs-build.config.yaml.

use serde::{Deserialize, Serialize};

/// An S3 bucket + key-prefix location.
///
/// # Examples
///
/// ```
/// use cbscore_types::config::S3LocationConfig;
///
/// let loc = S3LocationConfig {
///     bucket: "my-bucket".into(),
///     loc: "releases/v1".into(),
/// };
/// let json = serde_json::to_string(&loc).unwrap();
/// let parsed: S3LocationConfig = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, loc);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct S3LocationConfig {
    /// S3 bucket name.
    pub bucket: String,
    /// Key prefix within the bucket.
    pub loc: String,
}

/// S3 storage configuration — endpoint URL + artifact + release locations.
///
/// # Examples
///
/// ```
/// use cbscore_types::config::{S3LocationConfig, S3StorageConfig};
///
/// let s = S3StorageConfig {
///     url: "https://s3.example.com".into(),
///     artifacts: S3LocationConfig {
///         bucket: "artifacts".into(),
///         loc: "ceph".into(),
///     },
///     releases: S3LocationConfig {
///         bucket: "releases".into(),
///         loc: "ceph".into(),
///     },
/// };
/// let json = serde_json::to_string(&s).unwrap();
/// let parsed: S3StorageConfig = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, s);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct S3StorageConfig {
    /// S3 endpoint URL.
    pub url: String,
    /// Bucket + prefix for build artifacts (intermediate / per-build).
    pub artifacts: S3LocationConfig,
    /// Bucket + prefix for published releases.
    pub releases: S3LocationConfig,
}

/// Container-registry storage configuration.
///
/// `url` is the registry hostname (e.g. `quay.io`). Currently ignored
/// by the builder — the image's destination registry is taken from the
/// version descriptor itself; this stub mirrors the Python pydantic
/// model so cbscore-rs accepts existing config files unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RegistryStorageConfig {
    /// Registry hostname.
    pub url: String,
}

/// Storage destinations for release artifacts and container images.
///
/// Both fields are optional; an operator who only publishes to S3
/// leaves `registry` as `None`, and vice versa.
///
/// # Examples
///
/// ```
/// use cbscore_types::config::{RegistryStorageConfig, StorageConfig};
///
/// let s = StorageConfig {
///     s3: None,
///     registry: Some(RegistryStorageConfig { url: "quay.io".into() }),
/// };
/// let json = serde_json::to_string(&s).unwrap();
/// let parsed: StorageConfig = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, s);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StorageConfig {
    /// Optional S3 storage block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3: Option<S3StorageConfig>,
    /// Optional container-registry block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<RegistryStorageConfig>,
}
