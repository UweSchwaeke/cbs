// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild versions …` — version-descriptor lifecycle.
//!
//! - `versions create <VERSION>` — resolve component refs via
//!   `git ls-remote`, build a `VersionDescriptor`, write to
//!   `<root>/<type>/<VERSION>.json` where `<root>` is resolved by
//!   [`cbscore::versions::resolve_root`] from the CLI flag
//!   `--versions-dir`, the config field `paths.versions`, or the
//!   `<git-root>/_versions` Python-parity fallback (design 004).
//! - `versions list [--path DIR]` — list known releases from S3
//!   via [`cbscore::releases::s3::check_released_components`].
//! - `versions show <descriptor.json>` — pretty-print a descriptor.
//! - `versions validate <descriptor.json>` — exit 0 / non-zero on
//!   validation.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use cbscore::config;
use cbscore::core::component::load_components;
use cbscore::releases::s3::check_released_components;
use cbscore::versions::create::{VersionCreateInput, version_create_helper};
use cbscore::versions::desc::{read_descriptor, write_descriptor};
use cbscore::versions::utils::get_version_type;
use cbscore_types::versions::desc::VersionSignedOffBy;
use clap::{Args, Subcommand};

const DEFAULT_REGISTRY: &str = "quay.io";
const DEFAULT_IMAGE_NAME: &str = "ces-builder";
const DEFAULT_DISTRO: &str = "centos";
const DEFAULT_EL_VERSION: u32 = 9;

/// `cbsbuild versions …` subcommand enum.
///
/// `Create` boxes its argument struct because it carries
/// substantially more fields than the other variants — clippy's
/// `large_enum_variant` would otherwise complain that
/// `VersionsCommand`'s size is dominated by one arm.
#[derive(Debug, Subcommand)]
pub(crate) enum VersionsCommand {
    /// Create a version descriptor for `VERSION`.
    Create(Box<CreateArgs>),
    /// List released versions from S3.
    List(ListArgs),
    /// Pretty-print a descriptor file.
    Show(ShowArgs),
    /// Validate a descriptor file and exit 0 / 1.
    Validate(ShowArgs),
}

/// `cbsbuild versions create` arguments.
#[derive(Debug, Args)]
pub(crate) struct CreateArgs {
    /// Version string to create the descriptor for.
    pub version: String,
    /// Component refs in `component@ref` form. Repeatable.
    #[arg(short = 'c', long = "component", value_name = "COMPONENT@REF")]
    pub components: Vec<String>,
    /// Release type. Defaults to `dev`.
    #[arg(long = "type", default_value = "dev")]
    pub version_type: String,
    /// Optional title — falls back to a generated string when
    /// absent.
    #[arg(long = "title")]
    pub title: Option<String>,
    /// Optional override for the descriptor's `distro` field.
    #[arg(long = "distro")]
    pub distro: Option<String>,
    /// Operator user identity for `signed-off-by.user`.
    #[arg(long = "user", default_value = "ops")]
    pub user: String,
    /// Operator email for `signed-off-by.email`.
    #[arg(long = "email", default_value = "ops@cbs.invalid")]
    pub email: String,
    /// Builder image registry hostname.
    #[arg(long = "registry", default_value = DEFAULT_REGISTRY)]
    pub registry: String,
    /// Builder image name.
    #[arg(long = "image-name", default_value = DEFAULT_IMAGE_NAME)]
    pub image_name: String,
    /// Optional builder image tag. Falls back to `VERSION` when
    /// absent.
    #[arg(long = "image-tag")]
    pub image_tag: Option<String>,
    /// Override the descriptor store root for this invocation.
    /// Precedence: this flag, then `Config.paths.versions` in
    /// cbs-build.config.yaml, then `<git-root>/_versions` if invoked
    /// inside a git checkout.
    #[arg(long = "versions-dir", value_name = "PATH")]
    pub versions_dir: Option<Utf8PathBuf>,
}

/// `cbsbuild versions list` arguments.
#[derive(Debug, Args)]
pub(crate) struct ListArgs {
    /// Optional override for the S3 prefix to enumerate.
    #[arg(long = "path")]
    pub path: Option<String>,
}

/// `cbsbuild versions show` / `validate` arguments.
#[derive(Debug, Args)]
pub(crate) struct ShowArgs {
    /// Path to the descriptor JSON.
    pub descriptor: Utf8PathBuf,
}

/// `cbsbuild versions …` handler.
pub(crate) async fn handle(cmd: VersionsCommand, config_path: &Utf8Path) -> Result<()> {
    match cmd {
        VersionsCommand::Create(args) => handle_create(*args, config_path).await,
        VersionsCommand::List(args) => handle_list(args, config_path).await,
        VersionsCommand::Show(args) => handle_show(args).await,
        VersionsCommand::Validate(args) => handle_validate(args).await,
    }
}

async fn handle_create(args: CreateArgs, config_path: &Utf8Path) -> Result<()> {
    let cfg = config::load(config_path)
        .await
        .with_context(|| format!("loading config at '{config_path}'"))?;

    let component_refs = parse_component_refs(&args.components)?;
    if component_refs.is_empty() {
        bail!(
            "cbsbuild versions create: at least one --component COMPONENT@REF \
             is required"
        );
    }

    let components_root = cfg.paths.components.first().ok_or_else(|| {
        anyhow::anyhow!("config.paths.components is empty; cannot resolve component repos")
    })?;
    let core_components = load_components(components_root)
        .await
        .with_context(|| format!("loading components from '{components_root}'"))?;

    let mut component_repos: HashMap<String, String> = HashMap::new();
    for (name, _ref) in &component_refs {
        let comp = core_components.get(name).ok_or_else(|| {
            anyhow::anyhow!("component '{name}' not present in components dir '{components_root}'")
        })?;
        component_repos.insert(name.clone(), comp.repo.clone());
    }

    let version_type = get_version_type(&args.version_type)
        .with_context(|| format!("invalid --type '{}'", args.version_type))?;

    let input = VersionCreateInput {
        version: args.version.clone(),
        version_type,
        component_refs,
        component_repos,
        signed_off_by: VersionSignedOffBy {
            user: args.user,
            email: args.email,
        },
        registry: args.registry,
        image_name: args.image_name,
        image_tag: args.image_tag,
        distro: args.distro.unwrap_or_else(|| DEFAULT_DISTRO.to_string()),
        el_version: DEFAULT_EL_VERSION,
    };

    let desc = version_create_helper(&input)
        .await
        .with_context(|| format!("creating descriptor for '{}'", args.version))?;

    let dst = write_resolved_descriptor(
        args.versions_dir.as_deref(),
        &cfg,
        version_type,
        &args.version,
        &desc,
    )
    .await?;
    println!("{dst}");
    Ok(())
}

/// Resolve the descriptor-store root via
/// [`cbscore::versions::resolve_root`], compute the descriptor's
/// on-disk path via
/// [`cbscore_types::versions::desc::descriptor_path`], refuse to
/// overwrite an existing file, and write `desc` as JSON. Returns the
/// path written, so the caller can echo it.
///
/// Extracted from `handle_create` so the four plan-mandated
/// integration tests for `versions create`'s write-path behaviour
/// (precedence ladder + EEXIST + OQ5 error text) can drive it
/// without the surrounding component-loading / git-ls-remote /
/// `version_create_helper` machinery.
async fn write_resolved_descriptor(
    cli_versions_dir: Option<&Utf8Path>,
    cfg: &cbscore_types::config::Config,
    version_type: cbscore_types::versions::VersionType,
    version: &str,
    desc: &cbscore_types::versions::desc::VersionDescriptor,
) -> Result<Utf8PathBuf> {
    let root = cbscore::versions::resolve_root(cli_versions_dir, cfg)
        .await
        .context("resolving descriptor store root")?;
    let dst = cbscore_types::versions::desc::descriptor_path(&root, version_type, version);
    if dst.exists() {
        return Err(cbscore_types::versions::VersionError::AlreadyExists { path: dst }.into());
    }
    write_descriptor(desc, &dst)
        .await
        .with_context(|| format!("writing descriptor to '{dst}'"))?;
    Ok(dst)
}

async fn handle_list(args: ListArgs, config_path: &Utf8Path) -> Result<()> {
    let cfg = config::load(config_path)
        .await
        .with_context(|| format!("loading config at '{config_path}'"))?;
    let storage = cfg
        .storage
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("config.storage is None; cannot list releases"))?;
    let s3 = storage
        .s3
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("config.storage.s3 is None; cannot list releases"))?;

    let prefix = args.path.as_deref().unwrap_or(s3.releases.loc.as_str());
    let prefix = if prefix.ends_with('/') {
        prefix.to_string()
    } else {
        format!("{prefix}/")
    };
    let keys = check_released_components(&s3.releases.bucket, &prefix)
        .await
        .with_context(|| format!("listing releases in s3://{}/{}", s3.releases.bucket, prefix))?;

    for key in keys {
        if let Some(name) = key.strip_suffix(".json") {
            let basename = name.rsplit_once('/').map_or(name, |(_, b)| b);
            println!("{basename}");
        }
    }
    Ok(())
}

async fn handle_show(args: ShowArgs) -> Result<()> {
    let desc = read_descriptor(&args.descriptor)
        .await
        .with_context(|| format!("reading descriptor at '{}'", args.descriptor))?;
    let body = serde_json::to_string_pretty(&desc).context("serialising descriptor for display")?;
    println!("{body}");
    Ok(())
}

async fn handle_validate(args: ShowArgs) -> Result<()> {
    let _desc = read_descriptor(&args.descriptor)
        .await
        .with_context(|| format!("validating descriptor at '{}'", args.descriptor))?;
    println!("ok: {}", args.descriptor);
    Ok(())
}

fn parse_component_refs(raw: &[String]) -> Result<Vec<(String, String)>> {
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let (name, r) = entry
            .split_once('@')
            .ok_or_else(|| anyhow::anyhow!("--component expects COMPONENT@REF, got '{entry}'"))?;
        if name.is_empty() || r.is_empty() {
            bail!("--component COMPONENT@REF: empty component or ref in '{entry}'");
        }
        out.push((name.to_string(), r.to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::config::{Config, PathsConfig};
    use cbscore_types::versions::{
        VersionError, VersionType,
        desc::{VersionComponent, VersionDescriptor, VersionImage, VersionSignedOffBy},
    };
    use std::process::Command;
    use std::sync::Mutex;

    #[test]
    fn parse_component_refs_basic() {
        let v = parse_component_refs(&["ceph@main".into(), "ubuntu@v22.04".into()]).unwrap();
        assert_eq!(
            v,
            vec![
                ("ceph".into(), "main".into()),
                ("ubuntu".into(), "v22.04".into()),
            ],
        );
    }

    #[test]
    fn parse_component_refs_rejects_missing_at() {
        assert!(parse_component_refs(&["ceph".into()]).is_err());
    }

    #[test]
    fn parse_component_refs_rejects_empty_parts() {
        assert!(parse_component_refs(&["@main".into()]).is_err());
        assert!(parse_component_refs(&["ceph@".into()]).is_err());
    }

    // ----- write_resolved_descriptor: plan-mandated integration tests -----

    /// Shared mutex for tests that mutate the process cwd; matches the
    /// pattern in `cbscore::versions::resolve::tests` for the same
    /// reason (tokio's multi-threaded test runtime can interleave cwd
    /// mutations across `#[tokio::test]` tasks).
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    fn stub_config(override_versions: Option<&str>) -> Config {
        Config {
            paths: PathsConfig {
                components: vec!["/c".into()],
                scratch: "/s".into(),
                scratch_containers: "/s/c".into(),
                ccache: None,
                versions: override_versions.map(Utf8PathBuf::from),
            },
            storage: None,
            signing: None,
            logging: None,
            secrets: vec![],
            vault: None,
        }
    }

    /// Builds a minimal `VersionDescriptor` that round-trips through
    /// `write_descriptor` / `read_descriptor`; the field values are
    /// content-irrelevant to the tests below — only the on-disk path
    /// is asserted.
    fn stub_descriptor(version: &str) -> VersionDescriptor {
        VersionDescriptor {
            version: version.into(),
            title: format!("Release {version}"),
            signed_off_by: VersionSignedOffBy {
                user: "ops".into(),
                email: "ops@cbs.invalid".into(),
            },
            image: VersionImage {
                registry: "quay.io".into(),
                name: "ces-builder".into(),
                tag: version.into(),
            },
            components: vec![VersionComponent {
                name: "ceph".into(),
                repo: "https://github.com/ceph/ceph.git".into(),
                ref_: "abcdef1234".into(),
            }],
            distro: "centos".into(),
            el_version: 9,
        }
    }

    /// `--versions-dir <PATH>` wins: descriptor lands under
    /// `<PATH>/<type>/<VERSION>.json` even when `paths.versions` and
    /// the git fallback would point elsewhere.
    #[tokio::test]
    async fn cli_versions_dir_wins_and_writes_under_it() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = Utf8PathBuf::try_from(dir.path().to_owned()).unwrap();
        let cfg = stub_config(Some("/should-be-ignored"));
        let desc = stub_descriptor("19.2.3-dev1");

        let written = write_resolved_descriptor(
            Some(dir_path.as_path()),
            &cfg,
            VersionType::Dev,
            "19.2.3-dev1",
            &desc,
        )
        .await
        .expect("write");

        let expected = dir_path
            .canonicalize_utf8()
            .unwrap()
            .join("dev")
            .join("19.2.3-dev1.json");
        assert_eq!(written, expected);
        assert!(
            written.exists(),
            "descriptor file should exist at {written}"
        );
    }

    /// Config-field path wins when `--versions-dir` is unset.
    #[tokio::test]
    async fn config_versions_path_wins_when_cli_unset() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = Utf8PathBuf::try_from(dir.path().to_owned()).unwrap();
        let cfg = stub_config(Some(dir_path.as_str()));
        let desc = stub_descriptor("19.2.3");

        let written = write_resolved_descriptor(None, &cfg, VersionType::Release, "19.2.3", &desc)
            .await
            .expect("write");

        let expected = dir_path
            .canonicalize_utf8()
            .unwrap()
            .join("release")
            .join("19.2.3.json");
        assert_eq!(written, expected);
        assert!(written.exists());
    }

    /// With both overrides unset, the resolver falls back to the
    /// current git checkout — descriptor lands under
    /// `<git-root>/_versions/<type>/<VERSION>.json`, byte-identical
    /// to the Python implementation.
    ///
    /// `await_holding_lock` is allowed: `CWD_LOCK` is the right
    /// serialisation primitive because the process cwd is shared
    /// across all tokio tasks; an async-aware mutex would solve the
    /// same problem at higher cost.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn fallback_lands_under_git_root_versions_subdir() {
        let repo = tempfile::tempdir().unwrap();
        let status = Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(repo.path())
            .status()
            .expect("git init");
        assert!(status.success(), "git init failed");

        let _guard = CWD_LOCK.lock().expect("cwd lock");
        let prev_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(repo.path()).expect("cd repo");
        let cfg = stub_config(None);
        let desc = stub_descriptor("19.2.3-dev1");
        let written =
            write_resolved_descriptor(None, &cfg, VersionType::Dev, "19.2.3-dev1", &desc).await;
        std::env::set_current_dir(&prev_cwd).expect("restore cwd");
        let written = written.expect("write");

        let expected = Utf8PathBuf::try_from(repo.path().canonicalize().expect("canon"))
            .unwrap()
            .join("_versions")
            .join("dev")
            .join("19.2.3-dev1.json");
        assert_eq!(written, expected);
        assert!(written.exists());
    }

    /// With both overrides unset and the cwd outside any git
    /// checkout, the command surfaces `VersionError::NoDescriptorRoot`
    /// (rendered by the caller as the OQ5 four-line operator-
    /// actionable text). The test asserts the typed error rather than
    /// the rendered text — the rendering is already snapshot-tested
    /// in `cbscore::versions::resolve::tests`.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn no_overrides_outside_git_returns_no_descriptor_root() {
        let outside = tempfile::tempdir().unwrap();

        let _guard = CWD_LOCK.lock().expect("cwd lock");
        let prev_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(outside.path()).expect("cd outside");
        let cfg = stub_config(None);
        let desc = stub_descriptor("19.2.3");
        let result = write_resolved_descriptor(None, &cfg, VersionType::Dev, "19.2.3", &desc).await;
        std::env::set_current_dir(&prev_cwd).expect("restore cwd");

        let err = result.expect_err("must fail");
        let typed = err
            .downcast_ref::<VersionError>()
            .expect("expected VersionError");
        assert!(
            matches!(typed, VersionError::NoDescriptorRoot { .. }),
            "expected NoDescriptorRoot, got {typed:?}",
        );
    }

    /// Re-running `versions create` against an existing descriptor
    /// file refuses to overwrite (Python EEXIST parity); surfaces
    /// `VersionError::AlreadyExists`.
    #[tokio::test]
    async fn refuses_to_overwrite_existing_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = Utf8PathBuf::try_from(dir.path().to_owned()).unwrap();
        let cfg = stub_config(None);
        let desc = stub_descriptor("19.2.3-dev1");

        // First call writes successfully.
        write_resolved_descriptor(
            Some(dir_path.as_path()),
            &cfg,
            VersionType::Dev,
            "19.2.3-dev1",
            &desc,
        )
        .await
        .expect("first write");

        // Second call refuses.
        let err = write_resolved_descriptor(
            Some(dir_path.as_path()),
            &cfg,
            VersionType::Dev,
            "19.2.3-dev1",
            &desc,
        )
        .await
        .expect_err("must refuse");
        let typed = err
            .downcast_ref::<VersionError>()
            .expect("expected VersionError");
        assert!(
            matches!(typed, VersionError::AlreadyExists { .. }),
            "expected AlreadyExists, got {typed:?}",
        );
    }
}
