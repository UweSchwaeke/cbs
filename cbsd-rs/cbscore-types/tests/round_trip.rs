// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! M0 round-trip acceptance corpus.
//!
//! For every wire format named in design 002 §Wire-Format Versioning,
//! this test:
//!
//! 1. Reads a hand-crafted fixture (`tests/fixtures/<format>/`).
//! 2. Parses it through the appropriate `Versioned*` wrapper.
//! 3. Calls `into_latest()` to obtain the typed inner value.
//! 4. Re-serialises via the wrapper.
//! 5. Re-parses the serialised text.
//! 6. Asserts file-shape value-equality with step 2's parse.
//!
//! Plus negative tests for missing / unknown schema-version markers
//! and untagged git-secret entries.

use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::builder::VersionedBuildArtifactReport;
use cbscore_types::config::{ConfigError, VersionedConfig, VersionedVaultConfig};
use cbscore_types::containers::VersionedContainerDescriptor;
use cbscore_types::core::component::VersionedCoreComponent;
use cbscore_types::images::VersionedImageDescriptor;
use cbscore_types::releases::{VersionedReleaseComponent, VersionedReleaseDesc};
use cbscore_types::utils::secrets::VersionedSecrets;
use cbscore_types::versions::{VersionError, VersionedVersionDescriptor};

/// Absolute path of `tests/fixtures/<sub>/<file>` from the workspace root.
fn fixture(sub: &str, file: &str) -> Utf8PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Utf8PathBuf::from(format!("{manifest_dir}/tests/fixtures/{sub}/{file}"))
}

fn read_fixture(sub: &str, file: &str) -> (Utf8PathBuf, String) {
    let path = fixture(sub, file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    (path, text)
}

fn parse_yaml(text: &str) -> serde_value::Value {
    serde_saphyr::from_str(text).expect("parse YAML")
}

fn parse_json(text: &str) -> serde_value::Value {
    serde_json::from_str(text).expect("parse JSON")
}

// ---------------------------------------------------------------------
// Positive round-trip tests (one per wire format)
// ---------------------------------------------------------------------

#[test]
fn config_yaml_round_trip() {
    let (path, text) = read_fixture("config", "minimal.yaml");
    let value = parse_yaml(&text);

    let versioned = VersionedConfig::from_value(value.clone(), &path).unwrap();
    let cfg = versioned.into_latest();

    let yaml2 = serde_saphyr::to_string(&VersionedConfig::new(cfg)).unwrap();
    let value2 = parse_yaml(&yaml2);
    assert_eq!(value, value2, "file-shape round-trip mismatch for config");
}

#[test]
fn vault_yaml_round_trip() {
    let (path, text) = read_fixture("vault", "minimal.yaml");
    let value = parse_yaml(&text);

    let versioned = VersionedVaultConfig::from_value(value.clone(), &path).unwrap();
    let vc = versioned.into_latest();

    let yaml2 = serde_saphyr::to_string(&VersionedVaultConfig::new(vc)).unwrap();
    let value2 = parse_yaml(&yaml2);
    assert_eq!(value, value2, "file-shape round-trip mismatch for vault");
}

#[test]
fn secrets_yaml_round_trip() {
    let (path, text) = read_fixture("secrets", "four_families.yaml");
    let value = parse_yaml(&text);

    let versioned = VersionedSecrets::from_value(value.clone(), &path).unwrap();
    let s = versioned.into_latest();

    let yaml2 = serde_saphyr::to_string(&VersionedSecrets::new(s)).unwrap();
    let value2 = parse_yaml(&yaml2);
    assert_eq!(value, value2, "file-shape round-trip mismatch for secrets");
}

#[test]
fn core_component_yaml_round_trip() {
    let (path, text) = read_fixture("core_component", "minimal.yaml");
    let value = parse_yaml(&text);

    let versioned = VersionedCoreComponent::from_value(value.clone(), &path).unwrap();
    let c = versioned.into_latest();

    let yaml2 = serde_saphyr::to_string(&VersionedCoreComponent::new(c)).unwrap();
    let value2 = parse_yaml(&yaml2);
    assert_eq!(
        value, value2,
        "file-shape round-trip mismatch for core_component"
    );
}

#[test]
fn version_descriptor_json_round_trip() {
    let (path, text) = read_fixture("version_descriptor", "minimal.json");
    let value = parse_json(&text);

    let versioned = VersionedVersionDescriptor::from_value(value.clone(), &path).unwrap();
    let d = versioned.into_latest();

    let json2 = serde_json::to_string(&VersionedVersionDescriptor::new(d)).unwrap();
    let value2 = parse_json(&json2);
    assert_eq!(
        value, value2,
        "file-shape round-trip mismatch for version_descriptor"
    );
}

#[test]
fn container_descriptor_json_round_trip() {
    let (path, text) = read_fixture("container_descriptor", "minimal.json");
    let value = parse_json(&text);

    let versioned = VersionedContainerDescriptor::from_value(value.clone(), &path).unwrap();
    let d = versioned.into_latest();

    let json2 = serde_json::to_string(&VersionedContainerDescriptor::new(d)).unwrap();
    let value2 = parse_json(&json2);
    assert_eq!(
        value, value2,
        "file-shape round-trip mismatch for container_descriptor"
    );
}

#[test]
fn image_descriptor_json_round_trip() {
    let (path, text) = read_fixture("image_descriptor", "minimal.json");
    let value = parse_json(&text);

    let versioned = VersionedImageDescriptor::from_value(value.clone(), &path).unwrap();
    let d = versioned.into_latest();

    let json2 = serde_json::to_string(&VersionedImageDescriptor::new(d)).unwrap();
    let value2 = parse_json(&json2);
    assert_eq!(
        value, value2,
        "file-shape round-trip mismatch for image_descriptor"
    );
}

#[test]
fn release_desc_json_round_trip() {
    let (path, text) = read_fixture("release_desc", "minimal.json");
    let value = parse_json(&text);

    let versioned = VersionedReleaseDesc::from_value(value.clone(), &path).unwrap();
    let d = versioned.into_latest();

    let json2 = serde_json::to_string(&VersionedReleaseDesc::new(d)).unwrap();
    let value2 = parse_json(&json2);
    assert_eq!(
        value, value2,
        "file-shape round-trip mismatch for release_desc"
    );
}

#[test]
fn release_component_json_round_trip() {
    let (path, text) = read_fixture("release_component", "minimal.json");
    let value = parse_json(&text);

    let versioned = VersionedReleaseComponent::from_value(value.clone(), &path).unwrap();
    let c = versioned.into_latest();

    let json2 = serde_json::to_string(&VersionedReleaseComponent::new(c)).unwrap();
    let value2 = parse_json(&json2);
    assert_eq!(
        value, value2,
        "file-shape round-trip mismatch for release_component"
    );
}

#[test]
fn build_artifact_report_json_round_trip() {
    let (path, text) = read_fixture("build_artifact_report", "minimal.json");
    let value = parse_json(&text);

    let versioned = VersionedBuildArtifactReport::from_value(value.clone(), &path).unwrap();
    let r = versioned.into_latest();

    let json2 = serde_json::to_string(&VersionedBuildArtifactReport::new(r)).unwrap();
    let value2 = parse_json(&json2);
    assert_eq!(
        value, value2,
        "file-shape round-trip mismatch for build_artifact_report"
    );
}

// ---------------------------------------------------------------------
// Negative tests — schema-version marker validation
// ---------------------------------------------------------------------

#[test]
fn config_missing_schema_version_errors() {
    // Same fixture without the schema-version line:
    let yaml = "paths:\n  components: [/x]\n  scratch: /s\n  scratch-containers: /sc\n";
    let value = parse_yaml(yaml);
    let path = Utf8Path::new("/test/no-marker.yaml");
    let err = VersionedConfig::from_value(value, path).unwrap_err();
    assert!(
        matches!(err, ConfigError::MissingSchemaVersion { path: ref p } if p == path),
        "expected MissingSchemaVersion, got: {err:?}",
    );
}

#[test]
fn config_unknown_schema_version_errors() {
    let yaml =
        "schema-version: 99\npaths:\n  components: []\n  scratch: /s\n  scratch-containers: /sc\n";
    let value = parse_yaml(yaml);
    let path = Utf8Path::new("/test/too-new.yaml");
    let err = VersionedConfig::from_value(value, path).unwrap_err();
    match err {
        ConfigError::UnknownSchemaVersion {
            path: ref p,
            found,
            max_supported,
        } => {
            assert_eq!(p, path);
            assert_eq!(found, 99);
            assert_eq!(max_supported, 1);
        }
        other => panic!("expected UnknownSchemaVersion, got: {other:?}"),
    }
}

#[test]
fn version_descriptor_missing_schema_version_errors() {
    let json = r#"{"version":"19.2.3","title":"t","signed_off_by":{"user":"u","email":"e"},"image":{"registry":"r","name":"n","tag":"t"},"components":[],"distro":"c","el_version":9}"#;
    let value = parse_json(json);
    let path = Utf8Path::new("/test/no-marker.json");
    let err = VersionedVersionDescriptor::from_value(value, path).unwrap_err();
    assert!(
        matches!(err, VersionError::MissingSchemaVersion { path: ref p } if p == path),
        "expected MissingSchemaVersion, got: {err:?}",
    );
}

#[test]
fn version_descriptor_unknown_schema_version_errors() {
    let json = r#"{"schema_version":99,"version":"19.2.3","title":"t","signed_off_by":{"user":"u","email":"e"},"image":{"registry":"r","name":"n","tag":"t"},"components":[],"distro":"c","el_version":9}"#;
    let value = parse_json(json);
    let path = Utf8Path::new("/test/too-new.json");
    let err = VersionedVersionDescriptor::from_value(value, path).unwrap_err();
    match err {
        VersionError::UnknownSchemaVersion {
            path: ref p,
            found,
            max_supported,
        } => {
            assert_eq!(p, path);
            assert_eq!(found, 99);
            assert_eq!(max_supported, 1);
        }
        other => panic!("expected UnknownSchemaVersion, got: {other:?}"),
    }
}

#[test]
fn secrets_git_untagged_entry_errors() {
    // A git entry without the inner `type:` discriminator must fail
    // (wire-format break documented in design 002 §Git secrets).
    let yaml = r#"schema-version: 1
git:
  ceph-mirror:
    creds: plain
    username: git
    ssh-key: fake
"#;
    let value = parse_yaml(yaml);
    let path = Utf8Path::new("/test/untagged-git.yaml");
    // `VersionedSecrets` doesn't derive `Debug` (credential redaction
    // safety), so `unwrap_err()` would not compile — match instead.
    let Err(err) = VersionedSecrets::from_value(value, path) else {
        panic!("expected error on untagged git entry, got Ok");
    };
    // The error is surfaced as ConfigError::Io (the inner-deserialize
    // failure path); the underlying message names the missing tag.
    let msg = err.to_string();
    assert!(
        msg.contains("type") || msg.contains("git") || msg.contains("variant"),
        "expected serde error mentioning missing tag, got: {msg}",
    );
}
