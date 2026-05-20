// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! M1 acceptance gate — end-to-end Rust-only smoke build.
//!
//! Runs `cbsbuild build` against a real component end-to-end,
//! asserts exit 0 and a non-empty RPM artefact set. No Python-side
//! comparison — the M1 gate is "the Rust port can produce RPMs"
//! per design 002 §Migration Strategy lines 1269–1281.
//!
//! Gated on three env vars so the test is `#[ignore]`-able in
//! environments without the required sidecars:
//!
//! - `CBSCORE_TEST_SMOKE=1` — opt-in flag. Without it, the test
//!   bails early at `#[ignore]` semantics: every assertion is
//!   skipped and `cargo test` reports the run as passed-but-
//!   skipped via a `println!` to stdout (visible under
//!   `-- --nocapture`).
//! - `CBSCORE_TEST_CONFIG` — path to a `cbs-build.config.yaml`
//!   pointing at a real components/ tree + scratch directory.
//! - `CBSCORE_TEST_DESCRIPTOR` — path to a version descriptor
//!   JSON the smoke build will execute against.
//!
//! The test expects a working podman daemon on PATH (the runner
//! spawns the builder container).

use std::path::PathBuf;
use std::process::Command;

/// `cargo run --bin cbsbuild` plus an env-passthrough harness —
/// resolves the binary path via Cargo's `CARGO_BIN_EXE_cbsbuild`
/// so the test always runs the freshly-built binary, not a stale
/// one on PATH.
fn cbsbuild_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cbsbuild"))
}

#[test]
fn m1_smoke_build() {
    if std::env::var("CBSCORE_TEST_SMOKE").is_err() {
        eprintln!(
            "m1_smoke_build: CBSCORE_TEST_SMOKE not set — skipping. \
             Set CBSCORE_TEST_SMOKE=1 + CBSCORE_TEST_CONFIG + \
             CBSCORE_TEST_DESCRIPTOR to enable.",
        );
        return;
    }
    let Ok(config) = std::env::var("CBSCORE_TEST_CONFIG") else {
        panic!("m1_smoke_build: CBSCORE_TEST_CONFIG not set");
    };
    let Ok(descriptor) = std::env::var("CBSCORE_TEST_DESCRIPTOR") else {
        panic!("m1_smoke_build: CBSCORE_TEST_DESCRIPTOR not set");
    };

    let bin = cbsbuild_bin();
    eprintln!(
        "m1_smoke_build: invoking {} build --config {} {}",
        bin.display(),
        config,
        descriptor,
    );

    let status = Command::new(&bin)
        .arg("--config")
        .arg(&config)
        .arg("build")
        .arg(&descriptor)
        .status()
        .expect("cbsbuild build: failed to spawn");

    assert!(
        status.success(),
        "cbsbuild build exited with status {status:?}",
    );

    // The scratch directory's `rpms/<component>/<version>` subtree
    // should contain at least one .rpm after a successful build.
    // The exact path is per-component; the assertion is "ANY .rpm
    // file lives under the scratch root", which is the minimum the
    // M1 gate requires (the test isn't asserting on count or
    // basename — just "RPMs were produced").
    let scratch = scratch_root_from_config(&config);
    let rpms = collect_rpms(&scratch);
    assert!(
        !rpms.is_empty(),
        "m1_smoke_build: no RPMs found under scratch root '{}'",
        scratch.display(),
    );

    eprintln!(
        "m1_smoke_build: {} RPMs produced (first: {})",
        rpms.len(),
        rpms[0].display(),
    );
}

/// Parse the `paths.scratch` field out of the config YAML without
/// pulling cbscore-types into the test crate's dep graph.
fn scratch_root_from_config(config_path: &str) -> PathBuf {
    let body = std::fs::read_to_string(config_path)
        .unwrap_or_else(|e| panic!("read config '{config_path}': {e}"));
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("scratch:") {
            return PathBuf::from(rest.trim());
        }
    }
    panic!("m1_smoke_build: config '{config_path}' has no `scratch:` key");
}

/// Recursively walk `root` and collect every `.rpm` file.
fn collect_rpms(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            walk(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("rpm"))
        {
            out.push(path);
        }
    }
}
