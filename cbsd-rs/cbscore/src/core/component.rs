// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbs.component.yaml` loader — walks a directory tree, parses
//! every component file, and returns the typed component map.
//!
//! Used by the Phase 5 builder pipeline (via the orchestrator
//! in Commit 7) and by the Phase 6 `cbsbuild build` CLI.
//!
//! Walk semantics (per design 002 §Core Components):
//!
//! - Follow symlinks so operators can structure their
//!   `components/` tree with shared sub-trees.
//! - Warn-and-continue on symlink cycle detection — surfaced by
//!   `walkdir` as `loop_ancestor.is_some()`. A stray cycle does
//!   not block component loading.
//! - Per-file parse failures are logged at WARN with the path as
//!   a structured field but do **not** cascade — they're returned
//!   as the last-seen error variant only if **no** components
//!   loaded successfully.
//! - Component-name comparison is case-sensitive
//!   (`HashMap<String, CoreComponent>` byte-equality).
//! - Duplicate `name:` field across two files raises
//!   [`ComponentError::DuplicateComponentName`].

use std::collections::HashMap;

use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::core::component::{ComponentError, CoreComponent, VersionedCoreComponent};
use walkdir::WalkDir;

const TARGET_CORE_COMPONENT: &str = "cbscore::core::component";

/// Filename the walker looks for in every directory under `root`.
const COMPONENT_FILENAME: &str = "cbs.component.yaml";

/// Walk `root` recursively, parse every `cbs.component.yaml` file,
/// and return the typed component map keyed by `CoreComponent.name`.
///
/// # Errors
///
/// - [`ComponentError::Walk`] when the directory walk itself fails
///   for a reason other than a symlink cycle (permission denied,
///   non-cycle IO error). Cycle errors are warn-and-skipped.
/// - [`ComponentError::DuplicateComponentName`] when two component
///   files share the same `name:` field.
/// - Per-file parse failures
///   ([`ComponentError::Yaml`] / [`ComponentError::MissingSchemaVersion`]
///   / [`ComponentError::UnknownSchemaVersion`]) are warn-and-skipped
///   unless **no** component loaded successfully — then the last
///   such error is returned. This matches Python's
///   `try / except continue` behaviour.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
///
/// # async fn demo() -> Result<(), cbscore_types::core::component::ComponentError> {
/// let components =
///     cbscore::core::component::load_components(Utf8Path::new("/srv/components"))
///         .await?;
/// for (name, comp) in &components {
///     println!("{name}: repo = {}", comp.repo);
/// }
/// # Ok(()) }
/// ```
pub async fn load_components(
    root: &Utf8Path,
) -> Result<HashMap<String, CoreComponent>, ComponentError> {
    let root = root.to_owned();
    tokio::task::spawn_blocking(move || load_components_blocking(&root))
        .await
        .map_err(|e| ComponentError::Walk {
            source: std::io::Error::other(format!("join error: {e}")),
        })?
}

/// Synchronous loader — invoked via [`tokio::task::spawn_blocking`]
/// because `walkdir` is a blocking iterator and `serde_saphyr`
/// reads from a `&[u8]` synchronously.
fn load_components_blocking(
    root: &Utf8Path,
) -> Result<HashMap<String, CoreComponent>, ComponentError> {
    let mut out: HashMap<String, CoreComponent> = HashMap::new();
    let mut source_paths: HashMap<String, Utf8PathBuf> = HashMap::new();
    let mut last_per_file_err: Option<ComponentError> = None;
    let mut loaded_any = false;

    for entry in WalkDir::new(root.as_std_path()).follow_links(true) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) if err.loop_ancestor().is_some() => {
                tracing::warn!(
                    target: TARGET_CORE_COMPONENT,
                    path = %err.path().unwrap_or(root.as_std_path()).display(),
                    loop_ancestor = %err.loop_ancestor().unwrap().display(),
                    "skipping symlink cycle during component walk",
                );
                continue;
            }
            Err(err) => {
                return Err(ComponentError::Walk { source: err.into() });
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name() != COMPONENT_FILENAME {
            continue;
        }
        let path = match Utf8PathBuf::from_path_buf(entry.path().to_owned()) {
            Ok(p) => p,
            Err(p) => {
                tracing::warn!(
                    target: TARGET_CORE_COMPONENT,
                    path = %p.display(),
                    "skipping non-UTF8 component path",
                );
                continue;
            }
        };
        match parse_component(&path) {
            Ok(comp) => {
                if let Some(prior) = source_paths.get(&comp.name) {
                    return Err(ComponentError::DuplicateComponentName {
                        name: comp.name.clone(),
                        first: prior.clone(),
                        second: path,
                    });
                }
                source_paths.insert(comp.name.clone(), path);
                out.insert(comp.name.clone(), comp);
                loaded_any = true;
            }
            Err(err) => {
                tracing::warn!(
                    target: TARGET_CORE_COMPONENT,
                    path = %path,
                    "component file parse failed: {}", err,
                );
                last_per_file_err = Some(err);
            }
        }
    }

    if !loaded_any && let Some(err) = last_per_file_err {
        return Err(err);
    }
    Ok(out)
}

/// Parse a single `cbs.component.yaml` file at `path`. Maps the
/// `cbscore-types` schema-version dispatch errors and the
/// `serde_saphyr` parse errors into the matching
/// [`ComponentError`] variants.
fn parse_component(path: &Utf8Path) -> Result<CoreComponent, ComponentError> {
    let bytes = std::fs::read(path.as_std_path()).map_err(|e| ComponentError::Yaml {
        path: path.to_owned(),
        message: format!("read failed: {e}"),
    })?;
    let value: serde_value::Value =
        serde_saphyr::from_slice(&bytes).map_err(|e| ComponentError::Yaml {
            path: path.to_owned(),
            message: e.to_string(),
        })?;
    let versioned = VersionedCoreComponent::from_value(value, path)?;
    Ok(versioned.into_latest())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Utf8Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent.as_std_path()).expect("create parent");
        }
        fs::write(path.as_std_path(), contents).expect("write");
    }

    fn good_component_yaml(name: &str, repo: &str) -> String {
        format!(
            "schema-version: 1\n\
             name: {name}\n\
             repo: {repo}\n\
             build:\n  \
             get-version: git describe\n  \
             deps: \"\"\n\
             containers:\n  \
             path: containers/{name}.yaml\n",
        )
    }

    #[tokio::test]
    async fn empty_tree_returns_empty_map() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8Path::from_path(tmp.path()).expect("utf8");
        let map = load_components(root).await.expect("load");
        assert!(map.is_empty());
    }

    #[tokio::test]
    async fn loads_one_component() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        write(
            &root.join("ceph").join(COMPONENT_FILENAME),
            &good_component_yaml("ceph", "https://github.com/ceph/ceph"),
        );
        let map = load_components(&root).await.expect("load");
        assert_eq!(map.len(), 1);
        let comp = map.get("ceph").expect("ceph entry");
        assert_eq!(comp.name, "ceph");
        assert_eq!(comp.repo, "https://github.com/ceph/ceph");
    }

    #[tokio::test]
    async fn duplicate_name_errors() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        write(
            &root.join("ceph-a").join(COMPONENT_FILENAME),
            &good_component_yaml("ceph", "https://example.com/a"),
        );
        write(
            &root.join("ceph-b").join(COMPONENT_FILENAME),
            &good_component_yaml("ceph", "https://example.com/b"),
        );
        let Err(err) = load_components(&root).await else {
            panic!("expected DuplicateComponentName, got Ok");
        };
        assert!(matches!(
            err,
            ComponentError::DuplicateComponentName { name, .. } if name == "ceph"
        ));
    }

    #[tokio::test]
    async fn one_bad_one_good_yields_one_loaded() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        write(
            &root.join("ceph").join(COMPONENT_FILENAME),
            &good_component_yaml("ceph", "https://example.com/ceph"),
        );
        write(
            &root.join("broken").join(COMPONENT_FILENAME),
            "not: valid: yaml: at all\n",
        );
        let map = load_components(&root).await.expect("partial-ok");
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("ceph"));
    }

    #[tokio::test]
    async fn all_bad_propagates_last_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        // Both files fail (no schema-version marker).
        write(
            &root.join("a").join(COMPONENT_FILENAME),
            "name: a\nrepo: r\n",
        );
        write(
            &root.join("b").join(COMPONENT_FILENAME),
            "name: b\nrepo: r\n",
        );
        let Err(err) = load_components(&root).await else {
            panic!("expected per-file error, got Ok");
        };
        assert!(matches!(err, ComponentError::MissingSchemaVersion { .. }));
    }

    #[tokio::test]
    async fn case_sensitive_name_means_distinct_components() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).expect("utf8");
        write(
            &root.join("a").join(COMPONENT_FILENAME),
            &good_component_yaml("ceph", "https://example.com/lower"),
        );
        write(
            &root.join("b").join(COMPONENT_FILENAME),
            &good_component_yaml("Ceph", "https://example.com/upper"),
        );
        let map = load_components(&root).await.expect("load");
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("ceph"));
        assert!(map.contains_key("Ceph"));
    }
}
