// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Secrets value surface — top-level [`Secrets`] container plus the
//! four credential families ([`GitCreds`], [`StorageCreds`],
//! [`SigningCreds`], [`RegistryCreds`]).
//!
//! Phase 3 Commit 3 adds the IO-bearing `SecretsMgr` in
//! `cbscore::secrets`; this module owns only the zero-IO value side.
//!
//! # Secret redaction
//!
//! Per CLAUDE.md §Correctness Invariants item 5, every credential leaf
//! type in this module deliberately does **not** derive [`Debug`]. A
//! code path that tries to `dbg!()` or `{:?}`-print a credential value
//! gets a compile error rather than silently leaking the secret to a
//! log line. Equality testing still works via [`PartialEq`] — use
//! `assert!(a == b)` (not [`assert_eq!`], which requires [`Debug`])
//! when round-tripping credentials in tests.

pub mod errors;
pub mod versioned;

pub use errors::SecretsError;
pub use versioned::VersionedSecrets;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level secrets container.
///
/// Four `HashMap<String, *Creds>` fields keyed by operator-chosen
/// names; [`Default::default`] gives empty maps so a `secrets.yaml`
/// that omits some families parses cleanly.
///
/// # Examples
///
/// Construct an empty container and round-trip it through serde_json:
///
/// ```
/// use cbscore_types::utils::secrets::Secrets;
///
/// let s = Secrets::default();
/// let json = serde_json::to_string(&s).unwrap();
/// let parsed: Secrets = serde_json::from_str(&json).unwrap();
/// assert!(parsed == s);
/// ```
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Secrets {
    /// Git credentials keyed by operator-chosen name.
    #[serde(default)]
    pub git: HashMap<String, GitCreds>,
    /// Storage (S3) credentials keyed by operator-chosen name.
    #[serde(default)]
    pub storage: HashMap<String, StorageCreds>,
    /// Signing credentials keyed by operator-chosen name.
    #[serde(default)]
    pub signing: HashMap<String, SigningCreds>,
    /// Registry credentials keyed by operator-chosen name.
    #[serde(default)]
    pub registry: HashMap<String, RegistryCreds>,
}

// ---------------------------------------------------------------------
// Git family
// ---------------------------------------------------------------------

/// Git credentials — either plain on-disk or referenced through Vault.
///
/// The outer `creds:` tag selects between [`GitPlainCreds`] and
/// [`GitVaultCreds`]; the inner `type:` tag selects the auth shape.
///
/// # Examples
///
/// ```
/// use cbscore_types::utils::secrets::{GitCreds, GitPlainCreds};
///
/// let g = GitCreds::Plain(GitPlainCreds::Ssh {
///     username: "git".into(),
///     ssh_key: "-----BEGIN OPENSSH PRIVATE KEY-----\n...\n".into(),
/// });
/// let json = serde_json::to_string(&g).unwrap();
/// let parsed: GitCreds = serde_json::from_str(&json).unwrap();
/// assert!(parsed == g);
/// ```
///
/// An entry that omits the `type:` discriminator is a serde error:
///
/// ```
/// use cbscore_types::utils::secrets::GitCreds;
///
/// let bad = r#"{"creds":"plain","username":"git","ssh-key":"..."}"#;
/// assert!(serde_json::from_str::<GitCreds>(bad).is_err());
/// ```
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "creds", rename_all = "lowercase")]
pub enum GitCreds {
    /// On-disk plain-text credentials.
    Plain(GitPlainCreds),
    /// Vault-referenced credentials; resolved at build time.
    Vault(GitVaultCreds),
}

/// Plain-text git credentials.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GitPlainCreds {
    /// SSH-key authentication.
    Ssh {
        /// Git username.
        username: String,
        /// PEM-encoded SSH private key. Wire key: `ssh-key`.
        #[serde(rename = "ssh-key")]
        ssh_key: String,
    },
    /// Personal-access-token authentication over HTTPS.
    Token {
        /// Git username.
        username: String,
        /// HTTPS personal access token.
        token: String,
    },
    /// HTTPS password authentication.
    Https {
        /// Git username.
        username: String,
        /// HTTPS password.
        password: String,
    },
}

/// Vault-referenced git credentials. `key` is the Vault keyref.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GitVaultCreds {
    /// SSH-key authentication, key payload stored in Vault.
    Ssh {
        /// Git username.
        username: String,
        /// PEM-encoded SSH private key. Wire key: `ssh-key`.
        #[serde(rename = "ssh-key")]
        ssh_key: String,
        /// Vault keyref.
        key: String,
    },
    /// HTTPS password authentication, password payload stored in Vault.
    Https {
        /// Git username.
        username: String,
        /// HTTPS password.
        password: String,
        /// Vault keyref.
        key: String,
    },
}

// ---------------------------------------------------------------------
// Storage family
// ---------------------------------------------------------------------

/// S3 storage credentials — plain on-disk or Vault-referenced.
///
/// # Examples
///
/// ```
/// use cbscore_types::utils::secrets::{StorageCreds, StoragePlainCreds};
///
/// let s = StorageCreds::Plain(StoragePlainCreds::S3 {
///     access_id: "AKIA...".into(),
///     secret_id: "wJalrXUt...".into(),
/// });
/// let json = serde_json::to_string(&s).unwrap();
/// let parsed: StorageCreds = serde_json::from_str(&json).unwrap();
/// assert!(parsed == s);
/// ```
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "creds", rename_all = "lowercase")]
pub enum StorageCreds {
    /// On-disk plain-text S3 credentials.
    Plain(StoragePlainCreds),
    /// Vault-referenced S3 credentials.
    Vault(StorageVaultCreds),
}

/// Plain-text storage credentials.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StoragePlainCreds {
    /// S3 access-key / secret-key pair.
    S3 {
        /// S3 access key. Wire key: `access-id`.
        #[serde(rename = "access-id")]
        access_id: String,
        /// S3 secret key. Wire key: `secret-id`.
        #[serde(rename = "secret-id")]
        secret_id: String,
    },
}

/// Vault-referenced storage credentials. `key` is the Vault keyref.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StorageVaultCreds {
    /// S3 access-key / secret-key pair, key payload stored in Vault.
    S3 {
        /// Vault keyref.
        key: String,
        /// S3 access key. Wire key: `access-id`.
        #[serde(rename = "access-id")]
        access_id: String,
        /// S3 secret key. Wire key: `secret-id`.
        #[serde(rename = "secret-id")]
        secret_id: String,
    },
}

// ---------------------------------------------------------------------
// Signing family
// ---------------------------------------------------------------------

/// Signing credentials — plain on-disk or Vault-referenced.
///
/// Five leaf variants split across one plain ([`SigningPlainCreds`]) and
/// four Vault shapes ([`SigningVaultCreds`]) per design 002 §Signing
/// secrets.
///
/// # Examples
///
/// ```
/// use cbscore_types::utils::secrets::{SigningCreds, SigningVaultCreds};
///
/// let s = SigningCreds::Vault(SigningVaultCreds::Transit {
///     key: "vault-key".into(),
///     mount: "transit/".into(),
/// });
/// let json = serde_json::to_string(&s).unwrap();
/// let parsed: SigningCreds = serde_json::from_str(&json).unwrap();
/// assert!(parsed == s);
/// ```
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "creds", rename_all = "lowercase")]
pub enum SigningCreds {
    /// On-disk plain-text signing credentials.
    Plain(SigningPlainCreds),
    /// Vault-referenced signing credentials.
    Vault(SigningVaultCreds),
}

/// Plain-text signing credentials — GPG armor key on disk.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SigningPlainCreds {
    /// GPG armor key in plain text. Wire tag: `gpg-armor-key`.
    GpgArmorKey {
        /// GPG private key (armored PEM).
        #[serde(rename = "private-key")]
        private_key: String,
        /// Optional public key (armored PEM).
        #[serde(
            rename = "public-key",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        public_key: Option<String>,
        /// Optional passphrase for the private key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
        /// Signing email identity.
        email: String,
    },
}

/// Vault-referenced signing credentials. `key` is the Vault keyref.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SigningVaultCreds {
    /// GPG single key (private + optional public) stored in Vault.
    /// Wire tag: `gpg-single-key`.
    GpgSingleKey {
        /// Vault keyref.
        key: String,
        /// GPG private key (armored PEM).
        #[serde(rename = "private-key")]
        private_key: String,
        /// Optional public key (armored PEM).
        #[serde(
            rename = "public-key",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        public_key: Option<String>,
        /// Optional passphrase for the private key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
        /// Signing email identity.
        email: String,
    },
    /// GPG private key only, stored in Vault. Wire tag: `gpg-pvt-key`.
    GpgPvtKey {
        /// Vault keyref.
        key: String,
        /// GPG private key (armored PEM).
        #[serde(rename = "private-key")]
        private_key: String,
        /// Optional passphrase for the private key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
        /// Signing email identity.
        email: String,
    },
    /// GPG public key only, stored in Vault. Wire tag: `gpg-pub-key`.
    GpgPubKey {
        /// Vault keyref.
        key: String,
        /// GPG public key (armored PEM).
        #[serde(rename = "public-key")]
        public_key: String,
        /// Signing email identity.
        email: String,
    },
    /// Vault Transit signing. Wire tag: `transit`.
    Transit {
        /// Vault Transit key name.
        key: String,
        /// Vault Transit mount path.
        mount: String,
    },
}

// ---------------------------------------------------------------------
// Registry family
// ---------------------------------------------------------------------

/// Container-registry credentials — plain on-disk or Vault-referenced.
///
/// Single leaf shape per outer `creds:` value (no inner `type:` tag).
///
/// # Examples
///
/// ```
/// use cbscore_types::utils::secrets::RegistryCreds;
///
/// let r = RegistryCreds::Plain {
///     username: "deploy".into(),
///     password: "hunter2".into(),
///     address: "quay.io".into(),
/// };
/// let json = serde_json::to_string(&r).unwrap();
/// let parsed: RegistryCreds = serde_json::from_str(&json).unwrap();
/// assert!(parsed == r);
/// ```
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "creds", rename_all = "lowercase")]
pub enum RegistryCreds {
    /// Plain-text registry credentials.
    Plain {
        /// Registry username.
        username: String,
        /// Registry password.
        password: String,
        /// Registry hostname.
        address: String,
    },
    /// Vault-referenced registry credentials. `key` is the Vault keyref.
    Vault {
        /// Vault keyref.
        key: String,
        /// Registry username.
        username: String,
        /// Registry password.
        password: String,
        /// Registry hostname.
        address: String,
    },
}
