// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild versions …` — version-descriptor lifecycle.
//!
//! - `versions create <VERSION>` — resolve component refs via
//!   `git ls-remote`, build a `VersionDescriptor`, write to
//!   `<git-root>/_versions/<type>/<VERSION>.json` (Python-parity
//!   hardcoded path; seq-004 makes it configurable).
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

    // Python-parity output path: <cwd>/_versions/<type>/<VERSION>.json.
    // seq-004 (post-M1) swaps the hardcoded shape for a configurable
    // resolver via Config.paths.versions + --versions-dir.
    let cwd = camino::Utf8PathBuf::from_path_buf(
        std::env::current_dir().context("current_dir() failed")?,
    )
    .map_err(|p| anyhow::anyhow!("non-UTF8 cwd: {}", p.display()))?;
    let dst = cwd
        .join("_versions")
        .join(&args.version_type)
        .join(format!("{}.json", args.version));

    write_descriptor(&desc, &dst)
        .await
        .with_context(|| format!("writing descriptor to '{dst}'"))?;
    println!("{dst}");
    Ok(())
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
}
