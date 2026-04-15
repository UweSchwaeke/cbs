# Subcommand: `cbsbuild versions create`

## Description

`cbsbuild versions create` generates a **version descriptor** JSON file. A version descriptor is the primary input artifact for the build pipeline — it declares what to build (which components at which git refs), the target container image (registry, name, tag), the base distribution, and who signed off the build.

The command is used both interactively by developers (via the CLI) and programmatically by the CBS daemon (`cbsd`) via the `version_create_helper()` library function.

### What it does

1. **Reads the current git user** (name + email) from git config — used as the sign-off author
2. **Parses component refs** — each `--component NAME@VERSION` is split into a name→ref map
3. **Loads component definitions** — reads `cbs.component.yaml` files from `--components-path` directories (or `./components/` by default) to validate that each requested component exists and to look up its default git repo URI
4. **Applies URI overrides** — any `-o COMPONENT=URI` flags replace the default repo URI for that component
5. **Validates the version string** — must match `[prefix-]vM.m.p[-suffix]` (major, minor, patch all required)
6. **Generates a version title** — human-readable title derived from the version string, prefix, suffix, and version type (e.g., "Release Development CES version 24.11.0 (GA 1)")
7. **Assembles the `VersionDescriptor`** — all the above plus distro, EL version, image coordinates
8. **Writes the JSON file** — to `<output-dir>/<type>/<version>.json` (fails if it already exists)
9. **Checks for image descriptor** — warns if no matching image descriptor exists in the `desc/` directory (non-fatal)

### CLI signature

```
cbsbuild versions create VERSION [OPTIONS]

Arguments:
  VERSION                       Version string (format: [prefix-]vM.m.p[-suffix])

Options:
  -t, --type TYPE               Version type [default: dev] (release, dev, test, ci)
  -c, --component NAME@VERSION  Component ref (required, repeatable)
  --components-path PATH        Directory holding component definitions (repeatable)
  -o, --override-component-uri COMPONENT=URI
                                Override component git URI (repeatable)
  --distro NAME                 Base distribution [default: rockylinux:9]
  --el-version VERSION          EL version number [default: 9]
  --registry URL                Container registry [default: harbor.clyso.com]
  --image-name NAME             Container image name [default: ces/ceph/ceph]
  --image-tag TAG               Container image tag [default: VERSION string]
  --output-dir PATH             Output directory for version descriptors
                                [default: from config or <repo>/_versions]
```

Inherits from parent `cbsbuild`:
```
  -d, --debug                   Enable debug output
  -c, --config PATH             Path to configuration file [default: cbs-build.config.yaml]
```

### Output

A JSON file written to `<output-dir>/<version-type>/<version>.json`:

```json
{
  "version": "ces-v24.11.0-ga.1",
  "title": "Release Development CES version 24.11.0 (GA 1)",
  "signed_off_by": {
    "user": "Jane Developer",
    "email": "jane@clyso.com"
  },
  "image": {
    "registry": "harbor.clyso.com",
    "name": "ces/ceph/ceph",
    "tag": "ces-v24.11.0-ga.1"
  },
  "components": [
    {
      "name": "ceph",
      "repo": "https://github.com/ceph/ceph.git",
      "ref": "ces-v24.11.0-ga.1"
    }
  ],
  "distro": "rockylinux:9",
  "el_version": 9
}
```

---

## Sequence Diagram

```mermaid
sequenceDiagram
    actor User
    participant CLI as cbsbuild versions create
    participant Git as git utils
    participant Comp as load_components()
    participant Create as version_create_helper()
    participant FS as Filesystem
    participant ImgDesc as get_image_desc()

    User->>CLI: cbsbuild versions create VERSION [options]

    CLI->>Git: get_git_user()
    Git-->>CLI: (user_name, user_email)

    CLI->>CLI: parse_component_refs(--component args)
    Note right of CLI: Split "NAME@VERSION" → {name: ref}

    CLI->>Create: version_create_helper(version, type, refs, paths, ...)

    Create->>Create: Resolve components_paths (default: ./components/)
    Create->>Comp: load_components(paths)
    Comp->>FS: Read cbs.component.yaml files
    Comp-->>Create: {name: CoreComponentLoc}

    Create->>Create: Apply URI overrides (-o flags)
    Create->>Create: Validate version type
    Create->>Create: Validate version format (M.m.p required)
    Create->>Create: Generate version title
    Create->>Create: Assemble VersionDescriptor

    Create-->>CLI: VersionDescriptor

    CLI->>CLI: Print version + title

    CLI->>Git: get_git_repo_root()
    Git-->>CLI: repo_path

    CLI->>CLI: Resolve output path
    Note right of CLI: <output-dir>/<type>/<version>.json<br/>output-dir from: --output-dir flag,<br/>config field, or <repo>/_versions/

    alt Version file already exists
        CLI->>User: "version for <version> already exists" (error)
    end

    CLI->>FS: Create parent directories
    CLI->>CLI: Serialize descriptor to JSON (indent=2)
    CLI->>User: Print JSON preview
    CLI->>FS: Write descriptor JSON file
    CLI->>User: "-> written to <path>"

    CLI->>ImgDesc: get_image_desc(version)
    alt Image descriptor found
        Note right of CLI: Silent (no output)
    else Image descriptor missing
        CLI->>User: "image descriptor for version '<version>' missing" (warning)
    else Error checking
        CLI->>User: "error obtaining image descriptor: <err>" (warning)
    end
```

---

## Rust Implementation Plan

> For domain types, see the Unified Class Diagram in [feature-cbscore-rs.md §3.4](feature-cbscore-rs.md).

### Crate: `cbsbuild` (CLI binary)

**File**: `rust/cbsbuild/src/cmds/versions.rs`

### Clap structure

```rust
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum VersionsCmd {
    /// Create a new version descriptor file.
    Create(VersionsCreateArgs),
    /// List known release versions from S3.
    List(VersionsListArgs),
}

#[derive(Args)]
pub struct VersionsCreateArgs {
    /// Version string (format: [prefix-]vM.m.p[-suffix])
    version: String,

    /// Version type
    #[arg(short = 't', long = "type", default_value = "dev")]
    version_type: String,

    /// Component refs (NAME@VERSION, repeatable, required)
    #[arg(short = 'c', long = "component", required = true)]
    components: Vec<String>,

    /// Path to directory holding component definitions (repeatable)
    #[arg(long = "components-path")]
    components_paths: Vec<PathBuf>,

    /// Override component git URI (COMPONENT=URI, repeatable)
    #[arg(short = 'o', long = "override-component-uri")]
    component_uri_overrides: Vec<String>,

    /// Base distribution
    #[arg(long, default_value = "rockylinux:9")]
    distro: String,

    /// EL version number
    #[arg(long = "el-version", default_value_t = 9)]
    el_version: i32,

    /// Container registry
    #[arg(long, default_value = "harbor.clyso.com")]
    registry: String,

    /// Container image name
    #[arg(long = "image-name", default_value = "ces/ceph/ceph")]
    image_name: String,

    /// Container image tag (defaults to VERSION)
    #[arg(long = "image-tag")]
    image_tag: Option<String>,

    /// Output directory for version descriptors
    #[arg(long = "output-dir")]
    output_dir: Option<PathBuf>,
}
```

### Output directory resolution

The output directory is resolved from three sources (first wins):

1. `--output-dir` CLI flag
2. `versions_dir` field in the config file (new field added in this rewrite)
3. `<git-repo-root>/_versions/` (hardcoded fallback)

```rust
/// Resolve the output directory for version descriptors.
async fn resolve_output_dir(
    cli_output_dir: Option<&Path>,
    config_versions_dir: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if let Some(dir) = cli_output_dir {
        return Ok(dir.to_path_buf());
    }
    if let Some(dir) = config_versions_dir {
        return Ok(dir.to_path_buf());
    }
    let repo_root = get_git_repo_root().await?;
    Ok(repo_root.join("_versions"))
}
```

The config model (`Config` in `cbscore-lib::types::config`) gains an optional field (resolves the Python FIXME at `cmds/versions.py:88` — "make this configurable". This enables deployments where the git repo root is not available, e.g., cbsd workers):

```rust
pub struct Config {
    // ... existing fields ...
    /// Directory for storing version descriptors.
    #[serde(rename = "versions-dir")]
    pub versions_dir: Option<PathBuf>,
}
```

### Implementation functions

Split into focused helpers following the orchestrator pattern:

```rust
/// Fetch the git user (name, email) for sign-off.
async fn get_sign_off() -> anyhow::Result<VersionSignedOffBy> {
    let git_user = get_git_user().await
        .map_err(|e| anyhow::anyhow!("error obtaining git user info: {e}"))?;
    Ok(VersionSignedOffBy { user: git_user.name, email: git_user.email })
}

/// Determine the output file path and check it doesn't already exist.
fn resolve_descriptor_path(
    output_dir: &Path,
    version_type: &str,
    version: &str,
) -> anyhow::Result<PathBuf> {
    let path = output_dir
        .join(version_type)
        .join(format!("{version}.json"));
    if path.exists() {
        anyhow::bail!("version for {version} already exists");
    }
    Ok(path)
}

/// Write the descriptor JSON and print confirmation.
fn write_descriptor(
    desc: &VersionDescriptor,
    path: &Path,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Serde serializes fields in declaration order by default.
    // The VersionDescriptor struct field order must match the Python
    // output (version, title, signed_off_by, image, components,
    // distro, el_version) for consistent, human-reviewable JSON.
    let json = serde_json::to_string_pretty(desc)?;
    println!("{json}");
    desc.write(path)?;
    println!("-> written to {}", path.display());
    Ok(())
}

/// Check for a matching image descriptor (non-fatal warning).
async fn check_image_descriptor(version: &str) {
    match get_image_desc(version).await {
        Ok(_) => {}
        Err(CbsError::NoSuchVersion(_)) => {
            eprintln!("image descriptor for version '{version}' missing");
        }
        Err(e) => {
            eprintln!("error obtaining image descriptor for '{version}': {e}");
        }
    }
}
```

### Command handler

```rust
/// Build a VersionDescriptor from CLI args.
async fn create_descriptor(
    args: &VersionsCreateArgs,
) -> anyhow::Result<VersionDescriptor> {
    let req = VersionCreateRequest {
        version: args.version.clone(),
        version_type_name: args.version_type.clone(),
        component_refs: parse_component_refs(&args.components)?,
        components_paths: args.components_paths.clone(),
        component_uri_overrides: parse_uri_overrides(&args.component_uri_overrides)?,
        distro: args.distro.clone(),
        el_version: args.el_version,
        registry: args.registry.clone(),
        image_name: args.image_name.clone(),
        image_tag: args.image_tag.clone(),
        sign_off: get_sign_off().await?,
    };
    version_create_helper(&req)
        .map_err(Into::into)
}

/// Write descriptor to disk and check for image descriptor.
async fn write_and_verify(
    desc: &VersionDescriptor,
    output_dir: &Path,
    version_type: &str,
) -> anyhow::Result<()> {
    let path = resolve_descriptor_path(output_dir, version_type, &desc.version)?;
    write_descriptor(desc, &path)?;
    check_image_descriptor(&desc.version).await;
    Ok(())
}

/// Handle the `cbsbuild versions create` command.
pub async fn handle_versions_create(
    config: Option<&Config>,
    args: VersionsCreateArgs,
) -> anyhow::Result<()> {
    let desc = create_descriptor(&args).await?;
    println!("version: {}", desc.version);
    println!("version title: {}", desc.title);

    let config_dir = config.and_then(|c| c.versions_dir.as_deref());
    let output_dir = resolve_output_dir(args.output_dir.as_deref(), config_dir).await?;

    write_and_verify(&desc, &output_dir, &args.version_type).await
}
```

### Library function: `version_create_helper`

Located in `cbscore-lib/src/versions/create.rs`. This is the **shared** function called by both the CLI and `cbsd`'s worker. It is **not async** (all logic is pure computation + file reads).

```rust
/// Create a VersionDescriptor from the provided parameters.
///
/// This is the primary entry point for both CLI and daemon usage.
/// It validates inputs, loads components, applies overrides, and
/// assembles the descriptor.
///
/// All inputs needed to create a version descriptor.
/// Used by both the CLI handler and the cbsd daemon.
/// `component_refs` and `component_uri_overrides` are pre-parsed maps
/// (name → ref/uri). The CLI parses raw "NAME@VERSION" / "COMPONENT=URI"
/// strings before constructing this.
pub struct VersionCreateRequest {
    pub version: String,
    pub version_type_name: String,
    pub component_refs: HashMap<String, String>,
    pub components_paths: Vec<PathBuf>,
    pub component_uri_overrides: HashMap<String, String>,
    pub distro: String,
    pub el_version: i32,
    pub registry: String,
    pub image_name: String,
    pub image_tag: Option<String>,
    pub sign_off: VersionSignedOffBy,
}

pub fn version_create_helper(req: &VersionCreateRequest) -> Result<VersionDescriptor, CbsError> { ... }
```

Internally splits into:
- `resolve_components_paths()` — default to `./components/` if empty
- `validate_and_create()` — calls `create()` which validates version, builds title, assembles descriptor

### Dependencies

Implemented in Phase 2. See [plan-cbscore-rs.md](plan-cbscore-rs.md).

### Error handling

Error handling follows [feature-cbscore-rs.md §5.2](feature-cbscore-rs.md): `anyhow::Result` internally, `CbsError` at module boundaries.

### Tests

- **Unit**: `parse_component_refs()` — valid and malformed inputs (port from Python inline tests)
- **Unit**: `parse_version()` — ~30 test cases (port from Python's `versions/utils.py` inline tests)
- **Unit**: `normalize_version()` — ~18 test cases (port from Python)
- **Unit**: `version_create_helper()` — with fixture component dirs, verify JSON output structure
- **Unit**: Version title generation — various prefix/suffix combinations
- **Unit**: URI override parsing — valid `COMPONENT=URI` and malformed inputs
- **Unit**: Output path resolution — CLI flag > config field > git root fallback
- **Unit**: Reject duplicate version file (path already exists)
- **Integration**: `cbsbuild versions create` with a temp git repo and fixture components dir
- **Snapshot**: `cbsbuild versions create --help` output matches baseline
