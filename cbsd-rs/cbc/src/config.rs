// Copyright (C) 2026  Clyso
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug)]
pub struct Config {
    pub host: String,
    pub token: SecretString,
}

/// On-disk JSON shape. Kept separate from [`Config`] so the in-memory holder
/// can wrap `token` in `SecretString` — which deliberately implements neither
/// `Serialize` nor `Deserialize` — while the file format stays unchanged.
#[derive(Serialize, Deserialize)]
struct ConfigFile {
    host: String,
    token: String,
}

impl Config {
    /// Load configuration from the given path, or from the default location.
    ///
    /// Resolution order:
    /// 1. Explicit `path` argument.
    /// 2. `dirs::config_dir()/cbc/config.json`.
    pub fn load(path: Option<&Path>) -> Result<Self, Error> {
        let p = match path {
            Some(p) => p.to_path_buf(),
            None => Self::default_path()
                .ok_or_else(|| Error::Config("cannot determine config directory".into()))?,
        };

        let contents = std::fs::read_to_string(&p)
            .map_err(|e| Error::Config(format!("cannot read {}: {e}", p.display())))?;

        let file: ConfigFile = serde_json::from_str(&contents)
            .map_err(|e| Error::Config(format!("invalid config at {}: {e}", p.display())))?;
        Ok(Self {
            host: file.host,
            token: SecretString::from(file.token),
        })
    }

    /// Persist this configuration to disk as JSON.
    ///
    /// Creates parent directories if needed and restricts file permissions to
    /// 0600 on Unix.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::Config(format!("cannot create directory {}: {e}", parent.display()))
            })?;
        }

        let file = ConfigFile {
            host: self.host.clone(),
            token: self.token.expose_secret().to_string(),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| Error::Config(format!("cannot serialize config: {e}")))?;

        std::fs::write(path, &json)
            .map_err(|e| Error::Config(format!("cannot write {}: {e}", path.display())))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(
                |e| Error::Config(format!("cannot set permissions on {}: {e}", path.display())),
            )?;
        }

        Ok(())
    }

    /// Return the default config file path: `$XDG_CONFIG_HOME/cbc/config.json`
    /// (or platform equivalent via `dirs::config_dir`).
    ///
    /// Returns `None` when the platform config directory cannot be determined.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("cbc").join("config.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The credential holder must never gain a `Serialize` impl: that would
    // defeat the `SecretString` wrap by letting the raw token be written out.
    // The on-disk shape is `ConfigFile`, not `Config`.
    static_assertions::assert_not_impl_any!(Config: Serialize);

    #[test]
    fn token_is_redacted_in_debug() {
        let cfg = Config {
            host: "https://example.invalid".to_string(),
            token: SecretString::from("super-secret-token"),
        };
        let rendered = format!("{cfg:?}");
        assert!(
            !rendered.contains("super-secret-token"),
            "token must not appear in Debug output: {rendered}"
        );
    }

    #[test]
    fn save_then_load_round_trips_and_preserves_json_format() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("config.json");

        let cfg = Config {
            host: "https://cbs.example".to_string(),
            token: SecretString::from("cbsk_round_trip_value"),
        };
        cfg.save(&path).expect("save config");

        // On-disk format is unchanged: top-level plain `host` and `token`
        // string fields, with the token written out in the clear (the
        // intentional expose-on-save).
        let raw = std::fs::read_to_string(&path).expect("read raw config");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("on-disk JSON parses");
        assert_eq!(parsed["host"], "https://cbs.example");
        assert_eq!(parsed["token"], "cbsk_round_trip_value");

        // Round-trip: load wraps the token back into a SecretString.
        let loaded = Config::load(Some(&path)).expect("load config");
        assert_eq!(loaded.host, "https://cbs.example");
        assert_eq!(loaded.token.expose_secret(), "cbsk_round_trip_value");
    }
}
