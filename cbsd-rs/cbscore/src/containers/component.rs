// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Per-container-component build helpers.
//!
//! Phase 5 Commit 4 lands the [`ComponentContainer`] shape and the
//! [`load_container_descriptor`] helper that the Phase 5 Commit 7
//! orchestrator uses to fan out across containers declared by a
//! descriptor. The full `apply_pre` / `install_packages` /
//! `apply_post` sub-stage chain (Python
//! `cbscore.containers.component`'s ~320 lines) wires into
//! [`super::build::build_image`] in a follow-up fixup once the
//! runner-side `RpmbuildReport` → build-context plumbing is solid.

use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::containers::ContainerError;
use cbscore_types::containers::desc::ContainerDescriptor;
use cbscore_types::core::component::CoreComponent;

/// A single container slated for production by the
/// [`super::build::build_image`] driver.
///
/// Carries the parsed [`ContainerDescriptor`] alongside the
/// host-side path it loaded from (used by the build-context
/// assembler to resolve relative `file://` repo entries).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentContainer {
    /// Component name (matches the descriptor's `name:` field).
    pub component: String,
    /// Host-side path to the loaded container descriptor YAML.
    pub descriptor_path: Utf8PathBuf,
    /// Parsed container descriptor.
    pub descriptor: ContainerDescriptor,
}

/// Load and parse the container descriptor referenced by
/// `core_component.containers.path` (relative to `component_dir`).
///
/// # Errors
///
/// Returns [`ContainerError::Io`] on file IO failure;
/// [`ContainerError::Invalid`] wrapping the YAML parser message on
/// parse failure.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
/// use cbscore::containers::component::load_container_descriptor;
/// use cbscore_types::core::component::CoreComponent;
///
/// # async fn demo(comp: &CoreComponent) -> Result<(), cbscore_types::containers::ContainerError> {
/// let cc = load_container_descriptor(
///     Utf8Path::new("/srv/components/ceph"),
///     comp,
/// )
/// .await?;
/// let _ = cc;
/// # Ok(()) }
/// ```
pub async fn load_container_descriptor(
    component_dir: &Utf8Path,
    core_component: &CoreComponent,
) -> Result<ComponentContainer, ContainerError> {
    let descriptor_path = component_dir.join(&core_component.containers.path);
    let bytes = tokio::fs::read(&descriptor_path)
        .await
        .map_err(|e| ContainerError::Io { source: e })?;
    let descriptor: ContainerDescriptor = serde_saphyr::from_slice(&bytes).map_err(|e| {
        ContainerError::Invalid(format!(
            "container descriptor at '{descriptor_path}': YAML parse: {e}",
        ))
    })?;
    Ok(ComponentContainer {
        component: core_component.name.clone(),
        descriptor_path,
        descriptor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::core::component::{
        CoreComponent, CoreComponentBuildSection, CoreComponentContainersSection,
    };
    use std::fs;

    fn sample_core(name: &str) -> CoreComponent {
        CoreComponent {
            name: name.to_owned(),
            repo: "https://example.com/x.git".into(),
            build: CoreComponentBuildSection {
                rpm: None,
                get_version: "git describe".into(),
                deps: String::new(),
            },
            containers: CoreComponentContainersSection {
                path: Utf8PathBuf::from("containers/x.yaml"),
            },
        }
    }

    #[tokio::test]
    async fn load_minimal_descriptor() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let comp_dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        let descriptor_path = comp_dir.join("containers").join("x.yaml");
        fs::create_dir_all(descriptor_path.parent().unwrap().as_std_path()).expect("create parent");
        fs::write(
            descriptor_path.as_std_path(),
            "pre:\n  keys: []\n  packages: []\n  repos: []\n  scripts: []\n\
             packages:\n  required: []\n  optional: []\n\
             post: []\n",
        )
        .expect("write");

        let cc = load_container_descriptor(&comp_dir, &sample_core("x"))
            .await
            .expect("load");
        assert_eq!(cc.component, "x");
        assert_eq!(cc.descriptor_path, descriptor_path);
    }

    #[tokio::test]
    async fn load_missing_file_errors_io() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let comp_dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        let Err(err) = load_container_descriptor(&comp_dir, &sample_core("x")).await else {
            panic!("expected Io error, got Ok");
        };
        assert!(matches!(err, ContainerError::Io { .. }));
    }

    #[tokio::test]
    async fn load_malformed_yaml_errors_invalid() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let comp_dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        let descriptor_path = comp_dir.join("containers").join("x.yaml");
        fs::create_dir_all(descriptor_path.parent().unwrap().as_std_path()).expect("create parent");
        fs::write(descriptor_path.as_std_path(), "not: valid: yaml: at all\n").expect("write");
        let Err(err) = load_container_descriptor(&comp_dir, &sample_core("x")).await else {
            panic!("expected Invalid error, got Ok");
        };
        assert!(matches!(err, ContainerError::Invalid(_)));
    }
}
