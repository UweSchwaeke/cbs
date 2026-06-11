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
    /// Creates parent directories if needed and writes atomically: the JSON is
    /// written to a sibling temp file created with mode `0600` (Unix), then
    /// renamed over the target. Rename within a directory is atomic on POSIX,
    /// so a concurrent reader never observes a partially written or
    /// world-readable config — only the previous contents or the new ones,
    /// always mode `0600`. The previous implementation wrote the file and
    /// then `chmod`-ed it, leaving a window in which the token was
    /// world-readable under a typical `0o022` umask (audit-rem D8 / F11).
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
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

        // A unique name per call (pid + counter) keeps `create_new(true)` from
        // colliding with a concurrent save in the same directory.
        let tmp_path = temp_path_for(path);

        let mut open_opts = std::fs::OpenOptions::new();
        open_opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open_opts.mode(0o600);
        }
        let mut tmp = open_opts.open(&tmp_path).map_err(|e| {
            Error::Config(format!(
                "cannot create temp file {}: {e}",
                tmp_path.display()
            ))
        })?;

        // From here on, every error path must clean up the temp file. Compute
        // the write outcome, then funnel both it and the rename through the
        // shared cleanup below.
        let write_result = {
            use std::io::Write as _;
            tmp.write_all(json.as_bytes())
                .map_err(|e| Error::Config(format!("cannot write {}: {e}", tmp_path.display())))
        };
        // Close the handle before renaming (required on Windows, harmless on
        // Unix).
        drop(tmp);

        let result = write_result.and_then(|()| {
            std::fs::rename(&tmp_path, path).map_err(|e| {
                Error::Config(format!(
                    "cannot replace {} with {}: {e}",
                    path.display(),
                    tmp_path.display()
                ))
            })
        });

        if result.is_err() {
            // Best-effort cleanup; the caller still sees the original error.
            match std::fs::remove_file(&tmp_path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => eprintln!(
                    "warning: failed to clean up temp file {}: {e}",
                    tmp_path.display()
                ),
            }
        }

        result
    }

    /// Return the default config file path: `$XDG_CONFIG_HOME/cbc/config.json`
    /// (or platform equivalent via `dirs::config_dir`).
    ///
    /// Returns `None` when the platform config directory cannot be determined.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("cbc").join("config.json"))
    }
}

/// Build a unique sibling temp path for `path`
/// (e.g. `…/config.json.<pid>.<n>.tmp`) for the atomic write-then-rename in
/// [`Config::save`]. The per-process counter makes concurrent saves choose
/// distinct names, so `create_new(true)` does not collide.
fn temp_path_for(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();

    let mut name = path
        .file_name()
        .map(|f| f.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("config.json"));
    name.push(format!(".{pid}.{n}.tmp"));

    match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
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

    #[cfg(unix)]
    #[test]
    fn save_writes_file_with_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("config.json");

        let cfg = Config {
            host: "https://cbs.example".to_string(),
            token: SecretString::from("cbsk_mode"),
        };
        cfg.save(&path).expect("save config");

        let mode = std::fs::metadata(&path)
            .expect("stat config")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "config file mode is {mode:o}, expected 600");
    }

    // The window the previous write-then-chmod implementation left open (file
    // visible at 0644 before the chmod) must not exist: a reader watching the
    // target never sees anything but mode 0600.
    #[cfg(unix)]
    #[test]
    fn save_never_exposes_permissive_mode_to_concurrent_readers() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("config.json");

        let cfg = Config {
            host: "https://cbs.example".to_string(),
            token: SecretString::from("cbsk_atomic"),
        };
        // Establish the target so the reader always sees a named file.
        cfg.save(&path).expect("initial save");

        let stop = Arc::new(AtomicBool::new(false));
        let reader_stop = Arc::clone(&stop);
        let reader_path = path.clone();
        let reader = std::thread::spawn(move || {
            while !reader_stop.load(Ordering::Relaxed) {
                if let Ok(meta) = std::fs::metadata(&reader_path) {
                    let mode = meta.permissions().mode() & 0o777;
                    assert_eq!(mode, 0o600, "reader observed mode {mode:o}");
                }
            }
        });

        for _ in 0..200 {
            cfg.save(&path).expect("concurrent save");
        }
        stop.store(true, Ordering::Relaxed);
        reader
            .join()
            .expect("reader observed a permissive (non-0600) config mode");
    }

    #[test]
    fn save_cleans_up_temp_file_when_rename_fails() {
        let dir = tempfile::tempdir().expect("create tempdir");
        // Make the target a directory so the final rename(file -> dir) fails
        // after the temp file has been created and written.
        let path = dir.path().join("config.json");
        std::fs::create_dir(&path).expect("create directory at target path");

        let cfg = Config {
            host: "https://cbs.example".to_string(),
            token: SecretString::from("cbsk_cleanup"),
        };
        let err = cfg
            .save(&path)
            .expect_err("saving onto a directory target must fail");
        assert!(matches!(err, Error::Config(_)));

        // The temp file must not survive the failed save.
        let leftover_tmp: Vec<String> = std::fs::read_dir(dir.path())
            .expect("read tempdir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(
            leftover_tmp.is_empty(),
            "temp file(s) left behind after failed save: {leftover_tmp:?}"
        );
    }
}
