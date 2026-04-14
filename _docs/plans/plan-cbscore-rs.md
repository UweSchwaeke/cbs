# Implementation Plan: cbscore Rust Rewrite

> Extracted from [feature-cbscore-rs.md](feature-cbscore-rs.md) sections 7 and 8.
> For architecture, design principles, and technical design, see the parent document.

## Table of Contents

- [Implementation Phases](#implementation-phases)
  - [Phase 0: Scaffolding (S)](#phase-0-scaffolding-s)
  - [Phase 1: Errors + Logging (S)](#phase-1-errors--logging-s)
  - [Phase 2: Version Management + Core Components (M)](#phase-2-version-management--core-components-m)
  - [Phase 3: Configuration System (M)](#phase-3-configuration-system-m)
  - [Phase 4: Secret Models (L)](#phase-4-secret-models-l)
  - [Phase 5: Vault + Secure Args (M)](#phase-5-vault--secure-args-m)
  - [Phase 6: Async Command Executor + Secrets Manager (L)](#phase-6-async-command-executor--secrets-manager-l)
  - [Phase 7: External Tool Wrappers (XL)](#phase-7-external-tool-wrappers-xl--parallelizable)
  - [Phase 8: Releases + Builder Pipeline (XL)](#phase-8-releases--builder-pipeline-xl)
  - [Phase 9: Container Building + Runner (L)](#phase-9-container-building--runner-l)
  - [Phase 10: Python Shim Cleanup (M)](#phase-10-python-shim-cleanup-m)
- [Critical Path & Parallelization](#critical-path--parallelization)

---

## Implementation Phases

### Phase 0: Scaffolding (S)

#### Goal

Create the Cargo workspace, crate skeletons, and CLI scaffold so that `maturin develop` works and all existing Python code remains unchanged.

#### Public Interface

No public library interface in this phase (scaffolding only). The only externally visible artifact is the PyO3 `version()` function:

```rust
/// Returns the cbscore-lib crate version string.
#[pyfunction]
pub fn version() -> &'static str;
```

#### Deliverables

- Create Cargo workspace, all 3 crate skeletons
- Configure Maturin in `pyproject.toml`
- Expose a trivial `cbscore._cbscore.version()` via PyO3
- `cbsbuild/src/main.rs`: Clap command tree scaffold with `#[tokio::main]` — all subcommand stubs (`build`, `runner build`, `versions create`, `versions list`, `config init`, `config init-vault`, `advanced`) defined with their arg structs but returning `todo!()` or a "not yet implemented" error
- Verify `maturin develop` + `uv sync --all-packages` work together
- Verify existing Python code still works unchanged

#### Test Plan

- Unit: N/A (no library logic)
- Integration:
  - `python -c "from cbscore._cbscore import version; print(version())"`
  - Baseline smoke tests for all subcommands (capture and snapshot outputs to detect regressions):
    - `cbsbuild --help`
    - `cbsbuild build --help`
    - `cbsbuild runner build --help`
    - `cbsbuild versions --help`
    - `cbsbuild versions create --help`
    - `cbsbuild versions list --help`
    - `cbsbuild config --help`
    - `cbsbuild config init --help`
    - `cbsbuild config init-vault --help`
  - These help-output snapshots serve as regression anchors for all subsequent phases
- PyO3: `from cbscore._cbscore import version` returns a version string

### Phase 1: Errors + Logging (S)

#### Goal

Define the public error hierarchy and logging setup so all subsequent phases have a consistent error and logging foundation.

#### Public Interface

| Function / Type | Input | Output | Error | Example |
|-----------------|-------|--------|-------|---------|
| `CbsError` enum | N/A (type definition) | N/A | N/A | `CbsError::Config("file not found".into())` |
| `init_logging` | `verbose: bool` | `()` | N/A | `init_logging(false)` sets INFO; `init_logging(true)` sets DEBUG |

```rust
// cbscore-lib/src/types/errors.rs
#[derive(Debug, thiserror::Error)]
pub enum CbsError {
    #[error("config error: {0}")]          Config(String),
    #[error("version error: {0}")]         Version(String),
    #[error("malformed version: {0}")]     MalformedVersion(String),
    #[error("no such version: {0}")]       NoSuchVersion(String),
    #[error("builder error: {0}")]         Builder(String),
    #[error("runner error: {0}")]          Runner(String),
    #[error("vault error: {0}")]           Vault(String),
    #[error("secrets error: {0}")]         Secrets(String),
    #[error(transparent)]                  Other(anyhow::Error),
}

// No `#[from] anyhow::Error` — boundary functions must explicitly map internal
// errors to the correct `CbsError` variant using
// `.map_err(|e| CbsError::Builder(format!("{e:#}")))` or similar.
// This prevents accidental silent conversion.

// cbscore-lib/src/logging.rs
/// Initialize tracing subscriber. `verbose = true` sets DEBUG level.
pub fn init_logging(verbose: bool);
```

Python exception hierarchy (in `src/cbscore/_exceptions.py`, pure Python):

| Python Exception | Maps from `CbsError` variant | Python base class |
|------------------|------------------------------|-------------------|
| `CESError` | `Builder`, `Vault`, `Secrets`, `Other` | `Exception` |
| `ConfigError` | `Config` | `CESError` |
| `VersionError` | `Version` | `CESError` |
| `MalformedVersionError` | `MalformedVersion` | `CESError` |
| `NoSuchVersionError` | `NoSuchVersion` | `CESError` |
| `RunnerError` | `Runner` | `CESError` |
| `UnknownRepositoryError` | N/A (kept for backward compat) | `CESError` |

#### Internal Functions

- `map_error_to_pyerr(err: CbsError) -> PyErr` — converts a Rust `CbsError` into the corresponding Python exception using `GILOnceCell`-cached exception classes (in `cbscore-python/src/errors.rs`)

#### Deliverables

- `cbscore-lib/src/types/errors.rs`: `CbsError` enum (~8 variants, `thiserror`) — public API boundary errors only; internal modules use `anyhow::Result`
- `cbscore-lib/src/logging.rs`: `tracing` setup
- `src/cbscore/_exceptions.py`: Pure Python exception hierarchy
- PyO3 error mapping in `cbscore-python/src/errors.rs`
- Update `src/cbscore/errors.py` to re-export from `_exceptions.py`

#### Test Plan

- Unit: `CbsError` Display output matches expected strings (e.g., `CbsError::Config("bad file".into()).to_string()` == `"config error: bad file"`); each variant round-trips through `Display`
- Integration: existing `cbsd` imports still work
- PyO3: `from cbscore.errors import CESError; raise CESError("test")` succeeds; `from cbscore.errors import MalformedVersionError; str(MalformedVersionError("x"))` == `"malformed version: x"`

### Phase 2: Version Management + Core Components (M)

#### Goal

Implement version parsing/normalization utilities, the version descriptor type, core component loading, and the `versions create` CLI command.

#### Public Interface

**Version utilities** (`cbscore-lib/src/types/versions/utils.rs`):

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `parse_version` | `version: &str` | `ParsedVersion` | `anyhow::Error` | `"ces-v99.99.1-asd"` -> `ParsedVersion { prefix: Some("ces"), major: "99", minor: Some("99"), patch: Some("1"), suffix: Some("asd-qwe") }` |
| `normalize_version` | `version: &str` | `String` | `anyhow::Error` | `"ces-99.99.1-asd"` -> `"ces-v99.99.1-asd"` |
| `get_version_type` | `type_name: &str` | `VersionType` | `anyhow::Error` | `"release"` -> `VersionType::Release` |
| `parse_component_refs` | `components: &[String]` | `HashMap<String, String>` | `anyhow::Error` | `["ceph@v18.2.4"]` -> `{"ceph": "v18.2.4"}` |
| `get_major_version` | `version: &str` | `String` | `anyhow::Error` | `"ces-v18.2.4"` -> `"18.2"` |
| `get_minor_version` | `version: &str` | `Option<String>` | `anyhow::Error` | `"ces-v18.2.4"` -> `Some("18.2.4")` |

```rust
// cbscore-lib/src/types/versions/utils.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVersion {
    pub prefix: Option<String>,
    pub major: String,
    pub minor: Option<String>,
    pub patch: Option<String>,
    pub suffix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    Release,
    Dev,
    Test,
    Ci,
}

pub fn parse_version(version: &str) -> anyhow::Result<ParsedVersion>;
pub fn normalize_version(version: &str) -> anyhow::Result<String>;
pub fn get_version_type(type_name: &str) -> anyhow::Result<VersionType>;
pub fn get_version_type_desc(version_type: VersionType) -> &'static str;
pub fn parse_component_refs(components: &[String]) -> anyhow::Result<HashMap<String, String>>;
pub fn get_major_version(version: &str) -> anyhow::Result<String>;
pub fn get_minor_version(version: &str) -> anyhow::Result<Option<String>>;
```

Version utility functions return `anyhow::Result` (internal). The CLI handler and `version_create_helper` (the boundary) map to `CbsError::MalformedVersion` / `CbsError::Version`.

Test vectors from Python inline tests (33 cases for `parse_version`, 19 for `normalize_version`):

```text
# parse_version — valid
"ces-v99.99.1-asd-qwe" -> ("ces", "99", "99", "1", "asd-qwe")
"ces-v99.99.1"         -> ("ces", "99", "99", "1", None)
"ces-v99.99"           -> ("ces", "99", "99", None, None)
"v99.99.1-asd"         -> (None, "99", "99", "1", "asd")
"99.99.1"              -> (None, "99", "99", "1", None)
"99"                   -> (None, "99", None, None, None)

# parse_version — invalid (MalformedVersion)
"ces", "ces-", "ces-v", "-99.99.1-asd", "-99", "-v99",
"ces-99.", "ces-99.99.", "ces-v99.99.1-", "ces-v99.99.1.",
"ces-v99-asd", "ces-v99.asd", "ces-asd", "99.asd", "99-asd",
"ces-.99.99.1-asd"

# normalize_version — valid
"ces-99.99.1-asd" -> "ces-v99.99.1-asd"
"99.99.1"         -> "v99.99.1"
"v99.99"          -> "v99.99"

# normalize_version — invalid (MalformedVersion)
"ces-v99", "v99", "99", "ces-v", "ces-", "ces"
```

**Version descriptor** (`cbscore-lib/src/types/versions/desc.rs`):

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `VersionDescriptor::read` | `path: &Path` | `VersionDescriptor` | `anyhow::Error` | reads JSON file |
| `VersionDescriptor::write` | `&self, path: &Path` | `()` | `anyhow::Error` | writes JSON with indent=2 |

```rust
// cbscore-lib/src/types/versions/desc.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionSignedOffBy {
    pub user: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionImage {
    pub registry: String,
    pub name: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionComponent {
    pub name: String,
    pub repo: String,
    #[serde(rename = "ref")]
    pub r#ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDescriptor {
    pub version: String,
    pub title: String,
    pub signed_off_by: VersionSignedOffBy,
    pub image: VersionImage,
    pub components: Vec<VersionComponent>,
    pub distro: String,
    pub el_version: i32,
}

impl VersionDescriptor {
    pub fn read(path: &Path) -> anyhow::Result<Self>;
    pub fn write(&self, path: &Path) -> anyhow::Result<()>;
}
```

**Core components** (`cbscore-lib/src/types/core/component.rs`):

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `CoreComponent::load` | `path: &Path` | `CoreComponent` | `CbsError::Other` (anyhow) | loads `cbs.component.yaml` |
| `load_components` | `paths: &[PathBuf]` | `HashMap<String, CoreComponentLoc>` | N/A (logs and skips errors) | scans directories for `cbs.component.yaml` files |

```rust
// cbscore-lib/src/types/core/component.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreComponentContainersSection {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreComponentBuildRPMSection {
    pub build: String,
    #[serde(rename = "release-rpm")]
    pub release_rpm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreComponentBuildSection {
    pub rpm: Option<CoreComponentBuildRPMSection>,
    #[serde(rename = "get-version")]
    pub get_version: String,
    pub deps: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreComponent {
    pub name: String,
    pub repo: String,
    pub build: CoreComponentBuildSection,
    pub containers: CoreComponentContainersSection,
}

impl CoreComponent {
    pub fn load(path: &Path) -> anyhow::Result<Self>;
}

#[derive(Debug, Clone)]
pub struct CoreComponentLoc {
    pub path: PathBuf,
    pub comp: CoreComponent,
}

pub fn load_components(paths: &[PathBuf]) -> HashMap<String, CoreComponentLoc>;
```

**Version creation** (`cbscore-lib/src/versions/create.rs`):

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `version_create_helper` | `req: &VersionCreateRequest` | `VersionDescriptor` | `CbsError::MalformedVersion`, `CbsError::Version` | creates a version descriptor from component refs and metadata |

```rust
// cbscore-lib/src/versions/create.rs

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

pub fn version_create_helper(req: &VersionCreateRequest) -> Result<VersionDescriptor, CbsError>;
```

#### Internal Functions

- `_validate_version(v: &str) -> bool` — checks that a version has at least major.minor.patch components
- `_do_version_title(version: &str, version_type: VersionType) -> Result<String, CbsError>` — constructs a human-readable version title string (e.g., `"Release General Availability CES version 99.99.1 (ASD)"`)
- `create(...)` -> `Result<VersionDescriptor, CbsError>` — lower-level creation function that `version_create_helper` delegates to after loading components and validating inputs

#### Deliverables

- `cbscore-lib/src/types/versions/`: `VersionType`, `parse_version()`, `normalize_version()`, `get_version_type()`, `parse_component_refs()`, `VersionDescriptor` + sub-types with serde JSON
- `cbscore-lib/src/types/core/component.rs`: `CoreComponent`, `CoreComponentLoc`, `load_components()` with serde YAML
- `cbscore-lib/src/versions/create.rs`: `version_create_helper()`
- PyO3 bindings for all above — pure `#[pyclass]` types, `VersionType` as PyO3 enum with string conversion, `VersionDescriptor` with `__get_pydantic_core_schema__` for cbsd compatibility
- CLI handler: `cmds/versions.rs` — `handle_versions_create()` wiring the `versions create` subcommand to `version_create_helper()` (see [subcmd-versions-create.md](subcmd-versions-create.md))

#### Test Plan

- Unit: Port ~33 `parse_version` inline tests and ~19 `normalize_version` inline tests from Python `versions/utils.py` to Rust `#[test]`; `get_version_type("release")` == `VersionType::Release`; `get_version_type("unknown")` returns error; `parse_component_refs(["ceph@v18.2.4"])` == `{"ceph": "v18.2.4"}`; `parse_component_refs(["bad"])` returns error
- Integration: JSON round-trip for `VersionDescriptor` (serialize then deserialize, assert equality); `load_components()` against fixture dirs containing `cbs.component.yaml` files; `cbsbuild versions create` with fixtures; snapshot `cbsbuild versions create --help`
- PyO3: `from cbscore._cbscore import VersionType, parse_component_refs, VersionDescriptor`; verify `cbsdcore`, `cbc`, `cbsd` imports still work; re-run baseline subcommand help tests from Phase 0

### Phase 3: Configuration System (M)

#### Goal

Implement the full configuration model hierarchy with YAML serialization, `load`/`store` operations, and the interactive `config init` / `config init-vault` CLI commands.

#### Public Interface

**Config types** (`cbscore-lib/src/types/config.rs`):

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `Config::load` | `path: &Path` | `Config` | `CbsError::Config` | `Config::load(Path::new("cbscore.config.yaml"))` |
| `Config::store` | `&self, path: &Path` | `()` | `CbsError::Config` | writes YAML with kebab-case keys |
| `VaultConfig::load` | `path: &Path` | `VaultConfig` | `CbsError::Config` | `VaultConfig::load(Path::new("vault.yaml"))` |
| `VaultConfig::store` | `&self, path: &Path` | `()` | `CbsError::Config` | writes YAML vault config |

```rust
// cbscore-lib/src/types/config.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VaultUserPassConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VaultAppRoleConfig {
    pub role_id: String,
    pub secret_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VaultConfig {
    pub vault_addr: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_user: Option<VaultUserPassConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_approle: Option<VaultAppRoleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
}

impl VaultConfig {
    pub fn load(path: &Path) -> Result<Self, CbsError>;
    pub fn store(&self, path: &Path) -> Result<(), CbsError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PathsConfig {
    pub components: Vec<PathBuf>,
    pub scratch: PathBuf,
    pub scratch_containers: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ccache: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3LocationConfig {
    pub bucket: String,
    pub loc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3StorageConfig {
    pub url: String,
    pub artifacts: S3LocationConfig,
    pub releases: S3LocationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStorageConfig {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3: Option<S3StorageConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<RegistryStorageConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LoggingConfig {
    pub log_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub paths: PathsConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing: Option<SigningConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<PathBuf>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, CbsError>;
    pub fn store(&self, path: &Path) -> Result<(), CbsError>;
}
```

**CLI config init** (`cbsbuild/src/cmds/config.rs`):

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `handle_config_init` | `ConfigInitArgs` (Clap) | `()` (writes YAML to disk) | `CbsError::Config` | `cbsbuild config init --for-containerized-run` |
| `handle_config_init_vault` | `ConfigInitVaultArgs` (Clap) | `()` (writes YAML to disk) | `CbsError::Config` | `cbsbuild config init-vault --vault vault.yaml` |

```rust
// cbsbuild/src/cmds/config.rs

/// Options for non-interactive config init (from CLI flags).
pub struct ConfigInitOptions {
    pub components: Option<Vec<PathBuf>>,
    pub scratch: Option<PathBuf>,
    pub containers_scratch: Option<PathBuf>,
    pub ccache: Option<PathBuf>,
    pub secrets: Option<Vec<PathBuf>>,
    pub vault: Option<PathBuf>,
}

pub fn handle_config_init(config_path: &Path, opts: &ConfigInitOptions) -> Result<(), CbsError>;
pub fn handle_config_init_vault(vault_config_path: Option<&Path>) -> Result<Option<PathBuf>, CbsError>;
```

When `--for-containerized-run` or `--for-systemd-install` is passed, all paths are preset to fixed values (e.g., `/cbs/components`, `/cbs/scratch`, `/var/lib/containers`, `/cbs/ccache`, `/cbs/config/secrets.yaml`, `/cbs/config/vault.yaml`) and no interactive prompts are issued. Without these flags, the CLI uses `dialoguer` for interactive prompts (replacing Python's `click.confirm`/`click.prompt`).

#### Internal Functions

- `config_init_paths(cwd, opts) -> PathsConfig` — resolves component/scratch/ccache paths interactively or from CLI flags
- `config_init_storage() -> Option<StorageConfig>` — interactively prompts for S3 and registry storage settings
- `config_init_signing() -> Option<SigningConfig>` — interactively prompts for GPG and transit signing secret names
- `config_init_secrets_paths(paths) -> Vec<PathBuf>` — resolves secrets file paths interactively or from CLI flags

#### Deliverables

- `cbscore-lib/src/types/config.rs`: All config models with serde aliases, `Config::load()`, `Config::store()`
- PyO3 `PyConfig` wrapper with getters and `model_dump_json()`
- Python shim `config.py`
- CLI handlers: `cmds/config.rs` — `handle_config_init()` and `handle_config_init_vault()` wiring the `config init` and `config init-vault` subcommands (see [subcmd-config-init.md](subcmd-config-init.md) and [subcmd-config-init-vault.md](subcmd-config-init-vault.md))

#### Test Plan

- Unit: YAML round-trip for `Config` (load a fixture YAML, store to temp file, reload and assert equality); field alias tests (`scratch-containers` YAML key maps to `scratch_containers` field, `vault-addr` maps to `vault_addr`); `VaultConfig` round-trip with each auth variant (user-pass, approle, token); `Config::load` on nonexistent path returns `CbsError::Config`; `Config::load` on malformed YAML returns `CbsError::Config`
- Integration: `cbsbuild config init --for-containerized-run` generates valid YAML with preset paths (`/cbs/components`, `/cbs/scratch`, etc.); snapshot `cbsbuild config init --help` and `cbsbuild config init-vault --help`
- PyO3: `Config.load(path)` from Python matches original Pydantic-based `Config.load()`; `PyConfig` wrapper exposes getters for `paths`, `storage`, `signing`, `logging`, `secrets`, `vault`

### Phase 4: Secret Models (L)

#### Goal

Implement all 16 secret model structs, 4 discriminated union enums with custom deserialization, the `Secrets` container with file I/O and merge, and the URI matching + best-candidate-selection utilities used by secret resolution.

#### Public Interface

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `Secrets::load` | `path: &Path` (YAML or JSON) | `Secrets` | `anyhow::Error` | `Secrets::load("secrets.yaml")` loads 4 secret maps |
| `Secrets::store` | `&self`, `path: &Path` | `()` | `anyhow::Error` | `secrets.store("out.yaml")` writes YAML |
| `Secrets::merge` | `&mut self`, `other: Secrets` | `()` | -- | second secrets' entries override first's |
| `find_best_secret_candidate` | `secrets: &[&str]`, `uri: &str` | `Option<String>` | -- | `(["github.com", "github.com/ceph"], "github.com/ceph/ceph")` -> `Some("github.com/ceph")` |
| `matches_uri` | `pattern: &str`, `uri: &str` | `UriMatch` | `anyhow::Error` | `("github.com", "https://github.com/ceph")` -> `Partial { remainder: "ceph" }` |

```rust
// cbscore-lib/src/types/secrets/models.rs

/// 16 secret structs, grouped into 4 discriminated unions.
/// Each union uses custom `Deserialize` that inspects `creds` + `type` fields
/// via `serde_yml::Value` (preserves YAML line/column in errors).

// --- Git secrets (5 variants) ---
pub struct GitSSHSecret { pub ssh_key: String, pub username: String }
pub struct GitTokenSecret { pub token: String, pub username: String }
pub struct GitHTTPSSecret { pub username: String, pub password: String }
pub struct GitVaultSSHSecret { pub key: String, pub ssh_key: String, pub username: String }
pub struct GitVaultHTTPSSecret { pub key: String, pub username: String, pub password: String }

pub enum GitSecret {
    PlainSSH(GitSSHSecret),
    PlainToken(GitTokenSecret),
    PlainHTTPS(GitHTTPSSecret),
    VaultSSH(GitVaultSSHSecret),
    VaultHTTPS(GitVaultHTTPSSecret),
}

// --- Storage secrets (2 variants) ---
pub struct StoragePlainS3Secret { pub access_id: String, pub secret_id: String }
pub struct StorageVaultS3Secret { pub key: String, pub access_id: String, pub secret_id: String }

pub enum StorageSecret {
    PlainS3(StoragePlainS3Secret),
    VaultS3(StorageVaultS3Secret),
}

// --- Signing secrets (5 variants) ---
pub struct GPGPlainSecret {
    pub private_key: String, pub public_key: Option<String>,
    pub passphrase: Option<String>, pub email: String,
}
pub struct GPGVaultSingleSecret {
    pub key: String, pub private_key: String, pub public_key: Option<String>,
    pub passphrase: Option<String>, pub email: String,
}
pub struct GPGVaultPrivateKeySecret {
    pub key: String, pub private_key: String,
    pub passphrase: Option<String>, pub email: String,
}
pub struct GPGVaultPublicKeySecret {
    pub key: String, pub public_key: String, pub email: String,
}
pub struct VaultTransitSecret { pub key: String, pub mount: String }

pub enum SigningSecret {
    PlainGPG(GPGPlainSecret),
    VaultGPGSingle(GPGVaultSingleSecret),
    VaultGPGPrivateKey(GPGVaultPrivateKeySecret),
    VaultGPGPublicKey(GPGVaultPublicKeySecret),
    VaultTransit(VaultTransitSecret),
}

// --- Registry secrets (2 variants) ---
pub struct RegistryPlainSecret { pub username: String, pub password: String, pub address: String }
pub struct RegistryVaultSecret {
    pub key: String, pub username: String, pub password: String, pub address: String,
}

pub enum RegistrySecret {
    Plain(RegistryPlainSecret),
    Vault(RegistryVaultSecret),
}

// --- Container ---
pub struct Secrets {
    pub git: HashMap<String, GitSecret>,
    pub storage: HashMap<String, StorageSecret>,
    pub sign: HashMap<String, SigningSecret>,
    pub registry: HashMap<String, RegistrySecret>,
}

impl Secrets {
    pub fn load(path: &Path) -> anyhow::Result<Self>;
    pub fn store(&self, path: &Path) -> anyhow::Result<()>;
    pub fn merge(&mut self, other: Secrets);
}

// cbscore-lib/src/utils/uris.rs

/// Result of matching a URI pattern against a target URI.
pub enum UriMatch {
    NoMatch,
    Full,
    Partial { remainder: String },
}

pub fn matches_uri(pattern: &str, uri: &str) -> anyhow::Result<UriMatch>;

// cbscore-lib/src/secrets/utils.rs

pub fn find_best_secret_candidate(secrets: &[&str], uri: &str) -> Option<String>;
```

Discriminator logic (custom `Deserialize` for each union enum):
- `GitSecret`: inspect `creds` (`"plain"`/`"vault"`) + presence of `ssh-key`, `token`, or `username`+`password`
- `StorageSecret`: inspect `creds` + `type` (`"s3"`)
- `SigningSecret`: inspect `creds` + `type` (`"gpg-armor-key"`, `"gpg-single-key"`, `"gpg-pvt-key"`, `"gpg-pub-key"`, `"transit"`)
- `RegistrySecret`: inspect `creds` (`"plain"`/`"vault"`)

Field alias handling: fields with hyphens (e.g. `ssh-key`, `access-id`, `secret-id`, `private-key`, `public-key`) use `#[serde(rename = "ssh-key")]`. The `type` discriminator field uses `#[serde(rename = "type")]` on a `r#type` field (Rust keyword collision -- see feature-cbscore-rs.md section 5.4).

#### Internal Functions

- `deserialize_git_secret(Value) -> Result<GitSecret>` -- custom discriminator for git union
- `deserialize_storage_secret(Value) -> Result<StorageSecret>` -- custom discriminator for storage union
- `deserialize_signing_secret(Value) -> Result<SigningSecret>` -- custom discriminator for signing union
- `deserialize_registry_secret(Value) -> Result<RegistrySecret>` -- custom discriminator for registry union

#### Deliverables

- `cbscore-lib/src/types/secrets/models.rs`: All 16 secret struct types + 4 discriminated union enums with custom `Deserialize`
- `Secrets` container with `load()`, `store()`, `merge()`
- `cbscore-lib/src/secrets/utils.rs`: `find_best_secret_candidate()`
- `cbscore-lib/src/utils/uris.rs`: `matches_uri()`, `UriMatch`

#### Test Plan

- Unit: YAML round-trip for each of the 16 secret variants; discriminator selects correct variant for each `creds`+`type`/field combination; `Secrets::merge` overwrites keys from second into first; field alias round-trip (`ssh-key` -> `ssh_key` -> serialized back to `ssh-key`)
- Unit (uris): `matches_uri` against 8 test cases from Python `uris.py`:
  - `("https://github.com", "https://github.com")` -> `Full`
  - `("github.com", "https://github.com")` -> `Full`
  - `("github.com", "https://github.com/ceph")` -> `Partial { remainder: "ceph" }`
  - `("github.com", "https://github.com/ceph/ceph")` -> `Partial { remainder: "ceph/ceph" }`
  - `("foobar.com", "https://github.com/ceph/ceph")` -> `NoMatch`
  - `("harbor.foo.tld", "https://harbor.foo.tld")` -> `Full`
  - `("harbor.foo.tld/projects", "https://harbor.foo.tld")` -> `NoMatch`
  - `("harbor.foo.tld", "https://harbor.foo.tld/projects")` -> `Partial { remainder: "projects" }`
- Unit (secrets/utils): `find_best_secret_candidate` against 10 test cases from Python `utils.py`:
  - `([], "foo.bar.tld")` -> `None`
  - `(["foo.bar.tld"], "foo.bar.baz")` -> `None`
  - `(["foo.bar.tld", "foo.baz.tld"], "foo.bar.tld")` -> `Some("foo.bar.tld")`
  - `(["foo.bar.tld", "foo.baz.tld"], "foo.bar.tld/foobar")` -> `Some("foo.bar.tld")`
  - `(["foo.bar.tld/foobar", "foo.baz.tld"], "foo.bar.tld")` -> `None`
  - `(["foo.bar.tld/foobar", "foo.baz.tld"], "foo.bar.tld/foobar")` -> `Some("foo.bar.tld/foobar")`
  - `(["foo.bar.tld/foo", "foo.bar.tld/foo/bar"], "foo.bar.tld/foo")` -> `Some("foo.bar.tld/foo")`
  - `(["foo.bar.tld/foo", "foo.bar.tld/foo/bar", "foo.bar.tld/baz"], "foo.bar.tld/foo/bar")` -> `Some("foo.bar.tld/foo/bar")`
  - `(["foo.bar.tld/foo", "foo.bar.tld/bar"], "foo.bar.tld/foo/bar")` -> `Some("foo.bar.tld/foo")`
- Integration: load a real secrets YAML fixture with mixed vault/plain entries across all 4 categories

### Phase 5: Vault + Secure Args (M)

#### Goal

Implement the Vault client (AppRole/UserPass/Token auth via `vaultrs`) and the synchronous command execution layer with secure argument masking.

#### Public Interface

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `VaultClient::new` | `config: &VaultConfig` | `VaultClient` | `anyhow::Error` | from config with AppRole auth |
| `VaultClient::read_secret` | `&self`, `path: &str` | `HashMap<String, String>` | `anyhow::Error` | `read_secret("ces-kv/data/git")` -> `{"ssh-key": "...", "username": "..."}` |
| `VaultClient::check_connection` | `&self` | `()` | `anyhow::Error` | verifies vault reachable + auth valid |
| `run_cmd` | `args: &[CmdArg]`, `env: Option<&HashMap<String, String>>` | `CmdResult` | `anyhow::Error` | `run_cmd(&[Plain("git"), Plain("status")], None)` -> `CmdResult { exit_code: 0, .. }` |
| `sanitize_cmd` | `args: &[CmdArg]` | `Vec<String>` | -- | `[Plain("gpg"), Plain("--passphrase"), Plain("s3cret")]` -> `["gpg", "--passphrase", "****"]` |

```rust
// cbscore-lib/src/vault.rs

pub enum VaultAuth {
    AppRole { role_id: String, secret_id: String },
    UserPass { username: String, password: String },
    Token(String),
}

pub struct VaultClient {
    addr: String,
    auth: VaultAuth,
}

impl VaultClient {
    /// Build a VaultClient from deserialized VaultConfig.
    /// Fails if no auth method is configured.
    pub fn new(config: &VaultConfig) -> anyhow::Result<Self>;

    /// Read a KVv2 secret at the given path (mount: "ces-kv").
    pub async fn read_secret(&self, path: &str) -> anyhow::Result<HashMap<String, String>>;

    /// Verify that the Vault server is reachable and the credentials are valid.
    pub async fn check_connection(&self) -> anyhow::Result<()>;
}

// cbscore-lib/src/cmd.rs (partial -- sync only in this phase)

/// A command argument that may carry a secret value.
pub enum CmdArg {
    /// Non-sensitive argument.
    Plain(String),
    /// Sensitive argument: `display` shown in logs, `value` used at execution.
    Secure { display: String, value: String },
}

/// Result of running a command.
pub struct CmdResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Structured log events for step-level transparency.
pub enum CmdEvent {
    Started { cmd: Vec<String> },
    Stdout(String),
    Stderr(String),
    Finished { exit_code: i32 },
}

/// Sanitize a command argument list for logging.
/// Replaces SecureArg values with their display form, masks --passphrase/--pass values.
pub fn sanitize_cmd(args: &[CmdArg]) -> Vec<String>;

/// Execute a command synchronously. Returns CmdResult with exit code, stdout, stderr.
pub fn run_cmd(args: &[CmdArg], env: Option<&HashMap<String, String>>) -> anyhow::Result<CmdResult>;
```

#### Internal Functions

- `get_unsecured_cmd(args: &[CmdArg]) -> Vec<String>` -- expands all CmdArgs to their real values for subprocess execution

#### Deliverables

- `cbscore-lib/src/vault.rs`: `VaultClient` struct + `VaultAuth` enum via `vaultrs` crate (concrete implementation, no trait hierarchy)
- `cbscore-lib/src/cmd.rs` (partial): `CmdArg`, `CmdResult`, `CmdEvent`, `sanitize_cmd`, `run_cmd()` (sync)

#### Test Plan

- Unit: `CmdArg::Secure { display: "<CENSORED>", value: "s3cret" }` -- `sanitize_cmd` returns `"<CENSORED>"`; `sanitize_cmd` masks `--passphrase` followed by a value with `"****"`; `sanitize_cmd` masks inline `--passphrase=value` patterns
- Unit: `run_cmd(&[Plain("echo"), Plain("hello")], None)` returns `CmdResult { exit_code: 0, stdout: "hello\n", stderr: "" }`
- Unit: `VaultClient::new` fails with "no authentication method configured" when all auth fields are `None`
- Integration: Vault integration test with dev Vault container -- `VaultClient::new` with AppRole, `check_connection` succeeds, `read_secret` returns expected map; Token auth; UserPass auth; invalid credentials -> error

### Phase 6: Async Command Executor + Secrets Manager (L)

#### Goal

Complete the async command executor with streaming, timeout, and cancellation; implement `SecretsMgr` and all secret-category resolvers (git, storage, signing, registry) with RAII guards for SSH keys and GPG keyrings.

#### Public Interface

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `async_run_cmd` | `args: &[CmdArg]`, `opts: &CmdOpts` | `CmdResult` | `anyhow::Error` | `async_run_cmd(&[Plain("echo"), Plain("hi")], &default_opts)` -> `CmdResult { exit_code: 0, .. }` |
| `SecretsMgr::new` | `secrets: Secrets`, `vault_config: Option<&VaultConfig>` | `SecretsMgr` | `anyhow::Error` | constructs vault client + verifies connection |
| `SecretsMgr::git_url_for` | `&self`, `url: &str` | `GitUrlGuard` | `anyhow::Error` | `git_url_for("https://github.com/ceph/ceph")` -> guard with SSH or HTTPS URL |
| `SecretsMgr::s3_creds` (async) | `&self`, `url: &str` | `(String, String, String)` | `anyhow::Error` | `s3_creds("s3.example.com")` -> `("s3.example.com", "AKID...", "secret...")` |
| `SecretsMgr::gpg_signing_key` (async) | `&self`, `id: &str` | `GpgKeyringGuard` | `anyhow::Error` | guard yields `(keyring_path, passphrase, email)` |
| `SecretsMgr::transit` | `&self`, `id: &str` | `(String, String)` | `anyhow::Error` | `transit("cosign")` -> `("transit-mount", "cosign-key")` |
| `SecretsMgr::registry_creds` (async) | `&self`, `uri: &str` | `(String, String, String)` | `anyhow::Error` | `registry_creds("harbor.example.com/proj")` -> `("harbor.example.com", "user", "pass")` |
| `SecretsMgr::has_vault` | `&self` | `bool` | -- | `true` if vault client is configured |
| `SecretsMgr::has_s3_creds` | `&self`, `url: &str` | `bool` | -- | checks storage map for url |
| `SecretsMgr::has_gpg_signing_key` | `&self`, `id: &str` | `bool` | -- | `true` for GPGPlain/VaultSingle/VaultPrivateKey variants |
| `SecretsMgr::has_transit_key` | `&self`, `id: &str` | `bool` | -- | `true` for VaultTransit variant |
| `SecretsMgr::has_registry_creds` | `&self`, `id: &str` | `bool` | -- | checks registry map for id |

```rust
// cbscore-lib/src/cmd.rs (complete -- async added)

pub type CmdEventCallback = Box<dyn Fn(&CmdEvent) + Send + Sync>;

pub struct CmdOpts {
    pub cwd: Option<PathBuf>,
    pub timeout: Option<Duration>,
    pub event_cb: Option<CmdEventCallback>,
    pub env: Option<HashMap<String, String>>,
    pub reset_python_env: bool,
}

/// Execute a command asynchronously with tokio::process.
/// Streams stdout/stderr line-by-line through event_cb.
/// On timeout, kills the child process and returns an error.
pub async fn async_run_cmd(args: &[CmdArg], opts: &CmdOpts) -> anyhow::Result<CmdResult>;

// cbscore-lib/src/secrets/mgr.rs

pub struct SecretsMgr {
    secrets: Secrets,
    vault: Option<VaultClient>,
}

impl SecretsMgr {
    /// Construct a SecretsMgr: builds VaultClient if config provided,
    /// verifies vault connection, stores secrets.
    pub async fn new(
        secrets: Secrets,
        vault_config: Option<&VaultConfig>,
    ) -> anyhow::Result<Self>;

    /// Obtain a git URL with credentials for the given URL.
    /// Returns a RAII guard that cleans up SSH keys on drop.
    pub async fn git_url_for(&self, url: &str) -> anyhow::Result<GitUrlGuard>;

    /// Obtain S3 credentials for the given URL.
    /// Returns (host, access_id, secret_id).
    /// Vault-backed secret variants require async resolution via VaultClient::read_secret.
    pub async fn s3_creds(&self, url: &str) -> anyhow::Result<(String, String, String)>;

    /// Obtain a GPG signing keyring for the given signing key ID.
    /// Returns a RAII guard that erases the temp keyring on drop.
    /// Vault-backed secret variants require async resolution via VaultClient::read_secret.
    pub async fn gpg_signing_key(&self, id: &str) -> anyhow::Result<GpgKeyringGuard>;

    /// Obtain Vault Transit key info for the given ID.
    /// Returns (transit_mount, transit_key).
    pub fn transit(&self, id: &str) -> anyhow::Result<(String, String)>;

    /// Obtain registry credentials for the given URI.
    /// Returns (address, username, password).
    /// Vault-backed secret variants require async resolution via VaultClient::read_secret.
    pub async fn registry_creds(&self, uri: &str) -> anyhow::Result<(String, String, String)>;

    pub fn has_vault(&self) -> bool;
    pub fn has_s3_creds(&self, url: &str) -> bool;
    pub fn has_gpg_signing_key(&self, id: &str) -> bool;
    pub fn has_transit_key(&self, id: &str) -> bool;
    pub fn has_registry_creds(&self, id: &str) -> bool;
}

// cbscore-lib/src/secrets/git.rs

/// RAII guard for git URL credentials.
/// SSH variant: cleans up SSH key file and config entry on drop.
/// HTTPS/Token variant: holds the constructed URL (no cleanup needed).
pub struct GitUrlGuard { /* ... */ }

impl GitUrlGuard {
    /// Get the credential-bearing URL (SSH remote alias or HTTPS with embedded creds).
    pub fn url(&self) -> &str;

    /// Explicit async cleanup (preferred over Drop for error propagation).
    pub async fn cleanup(self) -> anyhow::Result<()>;
}

impl Drop for GitUrlGuard {
    /// Best-effort cleanup. Logs warnings on failure, never panics.
    fn drop(&mut self);
}

// cbscore-lib/src/secrets/signing.rs

// **Drop limitation**: The `Drop` impl uses sync `std::fs` operations (not `tokio::fs`)
// since `Drop` cannot be async. This means `Drop` may briefly block the tokio runtime
// during file cleanup. The explicit `async fn cleanup(self)` method is preferred --
// `Drop` is a safety net only. For `GpgKeyringGuard`, the `Drop` impl performs
// `std::fs::remove_dir_all` (no shredding); full secure cleanup requires calling
// `cleanup()` explicitly.

/// RAII guard for a temporary GPG keyring directory.
/// Imports the private key into a temp GNUPGHOME on creation,
/// erases (shred + rmdir) on drop.
pub struct GpgKeyringGuard { /* ... */ }

impl GpgKeyringGuard {
    /// Path to the temporary GNUPGHOME directory containing the imported key.
    pub fn keyring_path(&self) -> &Path;
    /// Passphrase for the private key, if any.
    pub fn passphrase(&self) -> Option<&str>;
    /// Email associated with the GPG key.
    pub fn email(&self) -> &str;
    /// Explicit async cleanup.
    pub async fn cleanup(self) -> anyhow::Result<()>;
}

impl Drop for GpgKeyringGuard {
    fn drop(&mut self);
}
```

**SecretsMgr construction**: `async fn SecretsMgr::new(secrets, vault_config) -> Result<SecretsMgr>` loads secrets from files, constructs the VaultClient, and verifies the vault connection in a single call. This matches the Python `__init__` behavior -- no caller ever wants a half-initialized SecretsMgr.

**Secret resolution logic** (maps from Python implementations):
- `git_url_for`: uses `find_best_secret_candidate` on git secrets map keys, then dispatches to SSH (creates temp key file + ssh config entry), HTTPS (embeds user:pass in URL), or Token (embeds user:token in URL). SSH and Vault-SSH variants read the actual key from Vault via `read_secret`.
- `s3_creds`: direct key lookup in storage map; plain returns fields directly, vault variant reads from Vault via `read_secret`.
- `gpg_signing_key`: looks up signing key by ID; for plain GPG, uses field directly; for vault variants, reads from Vault. Creates temp keyring dir, imports private key via `gpg --import --batch`.
- `transit`: looks up signing key by ID, asserts it is a `VaultTransitSecret`, returns `(mount, key)`.
- `registry_creds`: uses `find_best_secret_candidate` on registry secrets map keys; plain returns fields directly, vault variant reads from Vault via `read_secret`.

#### Internal Functions

- `storage_get_s3_creds(host, secrets, vault) -> Result<(String, String, String)>` -- resolves S3 credentials from storage secret map
- `registry_get_creds(uri, secrets, vault) -> Result<(String, String, String)>` -- resolves registry credentials from registry secret map
- `gpg_private_keyring(id, secrets, vault) -> Result<GpgKeyringGuard>` -- creates temp GPG keyring with imported private key
- `signing_transit(id, secrets) -> Result<(String, String)>` -- extracts transit mount and key from signing secrets
- `git_url_for_inner(url, secrets, vault) -> Result<GitUrlGuard>` -- dispatches to SSH/HTTPS/Token URL construction
- `ssh_git_url_for(url, entry, vault) -> Result<GitUrlGuard>` -- creates SSH key file, config entry, returns remote alias
- `https_git_url_for(url, entry, vault) -> Result<String>` -- constructs HTTPS URL with embedded credentials
- `token_git_url_for(url, entry) -> Result<String>` -- constructs token-based HTTPS URL
- `reset_python_env(env) -> HashMap<String, String>` -- removes Python virtualenv from PATH for subprocess isolation

#### Deliverables

- `cbscore-lib/src/cmd.rs` (complete): `async_run_cmd()` with tokio::process, streaming, timeout
- `cbscore-lib/src/secrets/mgr.rs`: `SecretsMgr` with all accessor methods
- `cbscore-lib/src/secrets/git.rs`: `GitUrlGuard`, SSH key RAII guard
- `cbscore-lib/src/secrets/signing.rs`: `GpgKeyringGuard`, GPG keyring RAII guard
- `cbscore-lib/src/secrets/storage.rs`: `storage_get_s3_creds`
- `cbscore-lib/src/secrets/registry.rs`: `registry_get_creds`

#### Test Plan

- Unit: `async_run_cmd(&[Plain("echo"), Plain("hello")], &default_opts)` returns `CmdResult { exit_code: 0, stdout: "hello\n", stderr: "" }`
- Unit: `async_run_cmd` with 1-second timeout on `sleep 60` kills the process and returns a timeout error
- Unit: `async_run_cmd` with `event_cb` receives `Started`, `Stdout("hello\n")`, `Finished { exit_code: 0 }` events in order
- Unit: `SecretsMgr::has_vault` returns `false` when no vault_config provided; `true` otherwise
- Unit: `SecretsMgr::transit("cosign")` with a `VaultTransitSecret { mount: "transit", key: "cosign-key" }` returns `("transit", "cosign-key")`
- Unit: `SecretsMgr::has_gpg_signing_key` returns `true` for GPGPlainSecret, GPGVaultSingleSecret, GPGVaultPrivateKeySecret; `false` for VaultTransitSecret and GPGVaultPublicKeySecret
- Integration: `SecretsMgr::new` with mock Vault verifies connection is checked during construction
- Integration: `GitUrlGuard` for SSH variant creates key file and config, cleanup removes them
- Integration: `GpgKeyringGuard` creates temp keyring dir, cleanup shreds and removes it

### Phase 7: External Tool Wrappers (XL -- parallelizable)

#### Goal

Implement async wrappers for all external tools (git, podman, buildah, S3, skopeo) and the container/image utility functions, organized as 4 independent parallel tracks.

Can split into 4 independent tracks:

#### 7a. Git -- `utils/git.rs`

All git async operations (clone, checkout, worktree, fetch, etc.)

##### Public Interface (7a)

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `run_git` | `args: &[CmdArg]`, `path: Option<&Path>` | `String` (stdout) | `anyhow::Error` | `run_git(&[Plain("status")], Some(repo_path))` |
| `get_git_user` | -- | `(String, String)` | `anyhow::Error` | `()` -> `("John Doe", "john@example.com")` |
| `get_git_repo_root` | -- | `PathBuf` | `anyhow::Error` | `()` -> `/home/user/repo` |
| `get_git_modified_paths` | `base_sha: &str`, `r#ref: &str`, `in_repo_path: Option<&str>`, `repo_path: Option<&Path>` | `(Vec<PathBuf>, Vec<PathBuf>)` | `anyhow::Error` | returns (modified, deleted) paths |
| `git_clone` | `repo: MaybeSecure`, `base_path: &Path`, `repo_name: &str` | `PathBuf` | `anyhow::Error` | clones mirror or updates existing; returns repo path |
| `git_checkout` | `repo_path: &Path`, `r#ref: &str`, `worktrees_base: &Path` | `PathBuf` | `anyhow::Error` | creates worktree; returns worktree path |
| `git_remove_worktree` | `repo_path: &Path`, `worktree_path: &Path` | `()` | `anyhow::Error` | force-removes a worktree |
| `git_fetch` | `remote: &str`, `from_ref: &str`, `to_branch: &str`, `repo_path: Option<&Path>` | `()` | `anyhow::Error` | fetches ref from remote to local branch |
| `git_pull` | `remote: MaybeSecure`, `from_branch: Option<&str>`, `to_branch: Option<&str>`, `repo_path: Option<&Path>` | `()` | `anyhow::Error` | pulls from remote |
| `git_cherry_pick` | `sha: &str`, `sha_end: Option<&str>`, `repo_path: Option<&Path>` | `()` | `anyhow::Error` | cherry-picks commit(s) |
| `git_apply` | `repo_path: &Path`, `patch_path: &Path` | `()` | `anyhow::Error` | applies patch file |
| `git_get_sha1` | `repo_path: &Path` | `String` | `anyhow::Error` | returns HEAD sha1 |
| `git_get_current_branch` | `repo_path: &Path` | `String` | `anyhow::Error` | returns current branch name |

```rust
// cbscore-lib/src/utils/git.rs

/// Type alias: a command argument that may carry a secret (e.g. a credential-bearing URL).
pub type MaybeSecure = CmdArg;

pub async fn run_git(args: &[CmdArg], path: Option<&Path>) -> anyhow::Result<String>;
pub async fn get_git_user() -> anyhow::Result<(String, String)>;
pub async fn get_git_repo_root() -> anyhow::Result<PathBuf>;
pub async fn get_git_modified_paths(
    base_sha: &str,
    r#ref: &str,
    in_repo_path: Option<&str>,
    repo_path: Option<&Path>,
) -> anyhow::Result<(Vec<PathBuf>, Vec<PathBuf>)>;
pub async fn git_clone(
    repo: MaybeSecure, base_path: &Path, repo_name: &str,
) -> anyhow::Result<PathBuf>;
pub async fn git_checkout(
    repo_path: &Path, r#ref: &str, worktrees_base: &Path,
) -> anyhow::Result<PathBuf>;
pub async fn git_remove_worktree(
    repo_path: &Path, worktree_path: &Path,
) -> anyhow::Result<()>;
pub async fn git_fetch(
    remote: &str, from_ref: &str, to_branch: &str, repo_path: Option<&Path>,
) -> anyhow::Result<()>;
pub async fn git_pull(
    remote: MaybeSecure, from_branch: Option<&str>,
    to_branch: Option<&str>, repo_path: Option<&Path>,
) -> anyhow::Result<()>;
pub async fn git_cherry_pick(
    sha: &str, sha_end: Option<&str>, repo_path: Option<&Path>,
) -> anyhow::Result<()>;
pub async fn git_apply(repo_path: &Path, patch_path: &Path) -> anyhow::Result<()>;
pub async fn git_get_sha1(repo_path: &Path) -> anyhow::Result<String>;
pub async fn git_get_current_branch(repo_path: &Path) -> anyhow::Result<String>;
```

##### Internal Functions (7a)

- `_clone(repo, dest_path)` -- raw `git clone --mirror --quiet`
- `_update(repo, repo_path)` -- `git remote set-url origin` + `git remote update`

#### 7b. Podman + Buildah -- `utils/podman.rs`, `utils/buildah.rs`

`BuildahContainer`, `podman_run`, `podman_stop`.

- `podman_run` must support a `persist_on_failure: bool` flag -- when set, the container is **not** removed after a failed run, allowing manual `podman exec` into the container for debugging
- `podman_run` must replicate the Python implementation's security and device options required for Buildah-in-Podman:
  - `--security-opt label=disable` (disable SELinux labeling)
  - `--security-opt seccomp=unconfined` (when `unconfined` flag is set)
  - `--device /dev/fuse:/dev/fuse:rw` (FUSE device for overlay mounts inside the container)

##### Public Interface (7b)

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `podman_run` | `opts: &PodmanRunOpts` | `CmdResult` | `anyhow::Error` | runs container with specified image, env, volumes |
| `podman_stop` | `name: Option<&str>`, `timeout: u32` | `()` | `anyhow::Error` | stops container by name or all |
| `BuildahContainer::new` | via `buildah_new_container` | `BuildahContainer` | `anyhow::Error` | `buildah from <distro>` + sets initial config |
| `BuildahContainer::set_config` | `&self`, author, annotations, labels, env | `()` | `anyhow::Error` | `buildah config --author ... --label ...` |
| `BuildahContainer::copy` | `&self`, `source: &Path`, `dest: &str` | `()` | `anyhow::Error` | `buildah copy <cid> <src> <dest>` |
| `BuildahContainer::run` | `&self`, `args: &[String]` | `()` | `anyhow::Error` | `buildah run --isolation chroot <cid> -- <args>` |
| `BuildahContainer::finish` | `&mut self`, `secrets: &SecretsMgr`, `sign_with_transit: Option<&str>` | `()` | `anyhow::Error` | commit, push to registry, optionally cosign sign |
| `buildah_new_container` | `desc: &VersionDescriptor` | `BuildahContainer` | `anyhow::Error` | creates container from distro base image |

```rust
// cbscore-lib/src/utils/podman.rs

pub struct PodmanRunOpts {
    pub image: String,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub volumes: Option<HashMap<String, String>>,
    pub devices: Option<HashMap<String, String>>,
    pub entrypoint: Option<String>,
    pub name: Option<String>,
    pub use_user_ns: bool,
    pub timeout: Option<Duration>,
    pub use_host_network: bool,
    pub unconfined: bool,
    pub replace_if_exists: bool,
    pub persist_on_failure: bool,
    pub event_cb: Option<CmdEventCallback>,
}

pub async fn podman_run(opts: &PodmanRunOpts) -> anyhow::Result<CmdResult>;
pub async fn podman_stop(name: Option<&str>, timeout: u32) -> anyhow::Result<()>;

// cbscore-lib/src/utils/buildah.rs

pub struct BuildahContainer {
    cid: String,
    version_desc: VersionDescriptor,
    is_committed: bool,
}

impl BuildahContainer {
    pub async fn set_config(
        &self,
        author: Option<&str>,
        annotations: Option<&HashMap<String, String>>,
        labels: Option<&HashMap<String, String>>,
        env: Option<&HashMap<String, String>>,
    ) -> anyhow::Result<()>;

    pub async fn copy(&self, source: &Path, dest: &str) -> anyhow::Result<()>;
    pub async fn run(&self, args: &[String]) -> anyhow::Result<()>;

    /// Commit container as image, push to registry, optionally sign with cosign Transit.
    pub async fn finish(
        &mut self, secrets: &SecretsMgr, sign_with_transit: Option<&str>,
    ) -> anyhow::Result<()>;
}

pub async fn buildah_new_container(
    desc: &VersionDescriptor,
) -> anyhow::Result<BuildahContainer>;
```

##### Internal Functions (7b)

- `_buildah_run(cmd, cid, args, with_args_divider, outcb) -> Result<CmdResult>` -- low-level buildah command executor

#### 7c. S3 -- `s3.rs`

`aws-sdk-s3` replacing `aioboto3` (upload, download, list).

##### Public Interface (7c)

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `s3_upload_str_obj` | `secrets: &SecretsMgr`, `url`, `dst_bucket`, `location`, `contents`, `content_type` | `()` | `anyhow::Error` | uploads string as S3 object |
| `s3_download_str_obj` | `secrets: &SecretsMgr`, `url`, `src_bucket`, `location`, `content_type: Option<&str>` | `Option<String>` | `anyhow::Error` | returns `None` if object not found |
| `s3_upload_json` | `secrets: &SecretsMgr`, `url`, `bucket`, `location`, `contents` | `()` | `anyhow::Error` | convenience wrapper: content_type = `"application/json"` |
| `s3_download_json` | `secrets: &SecretsMgr`, `url`, `bucket`, `location` | `Option<String>` | `anyhow::Error` | convenience wrapper: content_type = `"application/json"` |
| `s3_upload_files` | `secrets: &SecretsMgr`, `url`, `dst_bucket`, `file_locs: &[S3FileLocator]`, `public: bool` | `()` | `anyhow::Error` | uploads list of local files |
| `s3_list` | `secrets: &SecretsMgr`, `url`, `target_bucket`, `prefix: Option<&str>`, `prefix_as_directory: bool` | `S3ListResult` | `anyhow::Error` | paginated listing with CommonPrefixes support |

```rust
// cbscore-lib/src/s3.rs

pub struct S3FileLocator {
    pub src: PathBuf,
    pub dst: String,
    pub name: String,
}

pub struct S3ObjectEntry {
    pub key: String,
    pub size: i64,
    pub last_modified: DateTime<Utc>,
}

impl S3ObjectEntry {
    /// Extract the filename from the key (everything after the last '/').
    pub fn name(&self) -> &str;
}

pub struct S3ListResult {
    pub objects: Vec<S3ObjectEntry>,
    pub common_prefixes: Vec<String>,
}

pub async fn s3_upload_str_obj(
    secrets: &SecretsMgr, url: &str, dst_bucket: &str, location: &str,
    contents: &str, content_type: &str,
) -> anyhow::Result<()>;

pub async fn s3_download_str_obj(
    secrets: &SecretsMgr, url: &str, src_bucket: &str, location: &str,
    content_type: Option<&str>,
) -> anyhow::Result<Option<String>>;

pub async fn s3_upload_json(
    secrets: &SecretsMgr, url: &str, bucket: &str, location: &str,
    contents: &str,
) -> anyhow::Result<()>;

pub async fn s3_download_json(
    secrets: &SecretsMgr, url: &str, bucket: &str, location: &str,
) -> anyhow::Result<Option<String>>;

pub async fn s3_upload_files(
    secrets: &SecretsMgr, url: &str, dst_bucket: &str,
    file_locs: &[S3FileLocator], public: bool,
) -> anyhow::Result<()>;

pub async fn s3_list(
    secrets: &SecretsMgr, url: &str, target_bucket: &str,
    prefix: Option<&str>, prefix_as_directory: bool,
) -> anyhow::Result<S3ListResult>;
```

#### 7d. Skopeo + Images -- `images/skopeo.rs`, `images/signing.rs`, `images/sync.rs`, `images/desc.rs`

##### Public Interface (7d)

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `skopeo_get_tags` (async) | `img: &str` | `SkopeoTagListResult` | `anyhow::Error` | `skopeo_get_tags("harbor.example.com/proj/ceph")` -> tags list |
| `skopeo_copy` (async) | `src: &str`, `dst: &str`, `dst_registry: &str`, `secrets: &SecretsMgr`, `transit: &str` | `()` | `anyhow::Error` | copies image + optionally signs |
| `skopeo_inspect` (async) | `img: &str`, `secrets: &SecretsMgr`, `tls_verify: bool` | `String` (JSON) | `anyhow::Error` | returns raw JSON inspect output |
| `skopeo_image_exists` (async) | `img: &str`, `secrets: &SecretsMgr`, `tls_verify: bool` | `bool` | `anyhow::Error` | `true` if image exists in registry |
| `sign` (async) | `img: &str`, `secrets: &SecretsMgr`, `transit: &str` | `()` | `anyhow::Error` | async cosign sign via Vault Transit |
| `can_sign` | `registry: &str`, `secrets: &SecretsMgr`, `transit: &str` | `bool` | -- | checks vault + transit key + registry creds are available (pure check, no I/O) |
| `sync_image` (async) | `src`, `dst`, `dst_registry`, `secrets`, `transit`, `force: bool`, `dry_run: bool` | `()` | `anyhow::Error` | syncs image between registries |
| `get_image_desc` | `version: &str` | `ImageDescriptor` | `anyhow::Error` | loads image descriptor JSON matching version |
| `get_image_name` | `img: &str` | `String` | -- | `"harbor.example.com/proj:v1"` -> `"harbor.example.com/proj"` |
| `get_image_tag` | `img: &str` | `Option<String>` | -- | `"harbor.example.com/proj:v1"` -> `Some("v1")` |
| `get_container_image_base_uri` | `desc: &VersionDescriptor` | `String` | -- | -> `"registry.example.com/image-name"` |
| `get_container_image_base_uri_from_str` | `uri: &str` | `String` | `anyhow::Error` | `"registry/name:tag"` -> `"registry/name"` |
| `get_container_canonical_uri` | `desc: &VersionDescriptor`, `digest: Option<&str>` | `String` | -- | `(desc, None)` -> `"registry/name:tag"`; `(desc, Some("sha256:abc"))` -> `"registry/name@sha256:abc"` |

```rust
// cbscore-lib/src/images/skopeo.rs

pub struct SkopeoTagListResult {
    pub repository: String,
    pub tags: Vec<String>,
}

pub async fn skopeo_get_tags(img: &str) -> anyhow::Result<SkopeoTagListResult>;
pub async fn skopeo_copy(
    src: &str, dst: &str, dst_registry: &str,
    secrets: &SecretsMgr, transit: &str,
) -> anyhow::Result<()>;
pub async fn skopeo_inspect(
    img: &str, secrets: &SecretsMgr, tls_verify: bool,
) -> anyhow::Result<String>;
pub async fn skopeo_image_exists(
    img: &str, secrets: &SecretsMgr, tls_verify: bool,
) -> anyhow::Result<bool>;

// cbscore-lib/src/images/signing.rs

pub fn can_sign(registry: &str, secrets: &SecretsMgr, transit: &str) -> bool;
pub async fn sign(
    img: &str, secrets: &SecretsMgr, transit: &str,
) -> anyhow::Result<()>;

// cbscore-lib/src/images/sync.rs

#[allow(dead_code)] // TODO: evaluate if this function is still needed
pub async fn sync_image(
    src: &str, dst: &str, dst_registry: &str, secrets: &SecretsMgr,
    transit: &str, force: bool, dry_run: bool,
) -> anyhow::Result<()>;

// cbscore-lib/src/images/desc.rs

pub struct ImageLocations {
    pub src: String,
    pub dst: String,
}

pub struct ImageDescriptor {
    pub releases: Vec<String>,
    pub images: Vec<ImageLocations>,
}

pub async fn get_image_desc(version: &str) -> anyhow::Result<ImageDescriptor>;

// cbscore-lib/src/utils/containers.rs

pub fn get_container_image_base_uri(desc: &VersionDescriptor) -> String;
pub fn get_container_image_base_uri_from_str(uri: &str) -> anyhow::Result<String>;
pub fn get_container_canonical_uri(
    desc: &VersionDescriptor, digest: Option<&str>,
) -> String;

// cbscore-lib/src/images/mod.rs (or utils)

pub fn get_image_name(img: &str) -> String;
pub fn get_image_tag(img: &str) -> Option<String>;
```

##### Internal Functions (7d)

- `skopeo(args: &[CmdArg]) -> Result<CmdResult>` -- low-level skopeo command runner
- `_get_signing_params(registry, secrets, transit) -> Result<(String, String, String, String)>` -- extracts (username, password, transit_mount, transit_key) or errors

Also: `utils/containers.rs` (container URI helpers), `utils/paths.rs` (script path resolution)

#### Test Plan (all tracks)

- **7a Git**: create temp git repos with `git init --bare`; test `git_clone` creates mirror, second call updates; `git_checkout` creates worktree with correct ref; `git_remove_worktree` cleans up; `git_get_sha1` returns 40-hex-char string; `get_git_modified_paths` against known commits returns expected modified/deleted lists; `git_cherry_pick` applies commit to target branch
- **7b Podman + Buildah**: integration-only (requires podman/buildah on system); `podman_run` with `echo hello` returns exit code 0; `podman_stop` stops a running container; `BuildahContainer::set_config` does not error with valid annotations; `podman_run` with `persist_on_failure: true` does not remove container on non-zero exit
- **7c S3**: integration test with Ceph RGW (S3-compatible); `s3_upload_str_obj` + `s3_download_str_obj` round-trip; `s3_upload_json` + `s3_download_json` round-trip; `s3_download_str_obj` returns `None` for non-existent key; `s3_list` with prefix returns expected objects and common_prefixes; `s3_upload_files` uploads multiple files; `S3ObjectEntry::name()` extracts filename from key (e.g. `"path/to/file.rpm"` -> `"file.rpm"`)
- **7d Skopeo + Images**: integration-only for skopeo/cosign (requires tools on system); unit tests for pure functions: `get_image_name("harbor.example.com/proj:v1")` == `"harbor.example.com/proj"`; `get_image_tag("harbor.example.com/proj:v1")` == `Some("v1")`; `get_image_tag("harbor.example.com/proj")` == `None`; `get_container_image_base_uri_from_str("harbor.example.com/proj:v1")` == `"harbor.example.com/proj"`; `get_container_canonical_uri(desc, None)` == `"registry/name:tag"`; `get_container_canonical_uri(desc, Some("sha256:abc"))` == `"registry/name@sha256:abc"`; `SkopeoTagListResult` deserializes from JSON with `Repository`/`Tags` field names

### Phase 8: Releases + Builder Pipeline (XL)

#### Goal
Implement release S3 operations (check, upload, list) and the full `Builder` pipeline (prepare, build RPMs, sign, upload, produce `ReleaseDesc`).

#### Public Interface

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `check_release_exists` | `&SecretsMgr`, `url`, `bucket`, `bucket_loc`, `version` | `Option<ReleaseDesc>` | `anyhow::Error` | `"18.2.4"` with existing release -> `Some(ReleaseDesc{...})` |
| `release_desc_upload` | `&SecretsMgr`, `url`, `bucket`, `bucket_loc`, `version`, `&ReleaseBuildEntry` | `ReleaseDesc` | `anyhow::Error` | uploads `{bucket_loc}/18.2.4.json` to S3 |
| `release_upload_components` | `&SecretsMgr`, `url`, `bucket`, `bucket_loc`, `&HashMap<String, ReleaseComponent>` | `()` | `anyhow::Error` | parallel upload of per-component JSON descriptors |
| `check_released_components` | `&SecretsMgr`, `url`, `bucket`, `bucket_loc`, `&HashMap<String, String>` | `HashMap<String, ReleaseComponent>` | `anyhow::Error` | `{"ceph": "18.2.4-1.clyso"}` -> existing components in S3 |
| `list_releases` | `&SecretsMgr`, `url`, `bucket`, `bucket_loc` | `HashMap<String, ReleaseDesc>` | `anyhow::Error` | lists all `*.json` under `{bucket_loc}/` |
| `get_component_release_rpm` | `&CoreComponentLoc`, `el_version: i32` | `Option<String>` | `anyhow::Error` | runs release RPM script, returns RPM name |
| `Builder::new` | `desc: VersionDescriptor`, `config: &Config`, `flags: BuildFlags` | `Builder` | `CbsError` | constructs builder, loads components, initializes `SecretsMgr` |
| `Builder::run` | `&mut self` | `()` | `CbsError` | full pipeline: prepare -> check existing -> build RPMs -> sign -> upload -> container |
| `build_rpms` | `rpms_path`, `el_version`, `components_locs`, `components`, opts | `HashMap<String, ComponentBuild>` | `anyhow::Error` | parallel RPM build via `JoinSet` |
| `sign_rpms` | `&SecretsMgr`, `gpg_key_id: &str`, `&HashMap<String, ComponentBuild>` | `()` | `anyhow::Error` | parallel GPG signing of all RPMs per component |
| `s3_upload_rpms` | `&SecretsMgr`, `url`, `bucket`, `bucket_loc`, `&HashMap<String, ComponentBuild>`, `el_version` | `HashMap<String, S3ComponentLocation>` | `anyhow::Error` | parallel upload of RPMs + repodata to S3 |

```rust
// releases/s3.rs
pub async fn check_release_exists(
    secrets: &SecretsMgr,
    url: &str,
    bucket: &str,
    bucket_loc: &str,
    version: &str,
) -> anyhow::Result<Option<ReleaseDesc>>;

pub async fn release_desc_upload(
    secrets: &SecretsMgr,
    url: &str,
    bucket: &str,
    bucket_loc: &str,
    version: &str,
    release_build: &ReleaseBuildEntry,
) -> anyhow::Result<ReleaseDesc>;

pub async fn release_upload_components(
    secrets: &SecretsMgr,
    url: &str,
    bucket: &str,
    bucket_loc: &str,
    component_releases: &HashMap<String, ReleaseComponent>,
) -> anyhow::Result<()>;

pub async fn check_released_components(
    secrets: &SecretsMgr,
    url: &str,
    bucket: &str,
    bucket_loc: &str,
    components: &HashMap<String, String>,
) -> anyhow::Result<HashMap<String, ReleaseComponent>>;

pub async fn list_releases(
    secrets: &SecretsMgr,
    url: &str,
    bucket: &str,
    bucket_loc: &str,
) -> anyhow::Result<HashMap<String, ReleaseDesc>>;

// releases/utils.rs
pub async fn get_component_release_rpm(
    component_loc: &CoreComponentLoc,
    el_version: i32,
) -> anyhow::Result<Option<String>>;

// builder/build.rs
pub struct BuildFlags {
    pub skip_build: bool,
    pub force: bool,
    pub tls_verify: bool,
}

pub struct Builder {
    // private fields: desc, config, scratch_path, components,
    // storage_config, signing_config, secrets, ccache_path, flags
}

impl Builder {
    pub async fn new(
        desc: VersionDescriptor,
        config: &Config,
        flags: BuildFlags,
    ) -> Result<Self, CbsError>;

    /// The builder pipeline mutates internal state (tracking built components,
    /// scratch directories). Using `&mut self` makes mutation explicit.
    pub async fn run(&mut self) -> Result<(), CbsError>;
}

// builder/rpmbuild.rs
pub struct ComponentBuild {
    pub version: String,
    pub rpms_path: PathBuf,
}

pub async fn build_rpms(
    rpms_path: &Path,
    el_version: i32,
    components_locs: &HashMap<String, CoreComponentLoc>,
    components: &HashMap<String, BuildComponentInfo>,
    ccache_path: Option<&Path>,
    skip_build: bool,
) -> anyhow::Result<HashMap<String, ComponentBuild>>;

// builder/signing.rs
pub async fn sign_rpms(
    secrets: &SecretsMgr,
    sign_with_gpg: &str,
    components_rpms: &HashMap<String, ComponentBuild>,
) -> anyhow::Result<()>;

// builder/upload.rs
pub struct S3ComponentLocation {
    pub name: String,
    pub version: String,
    pub location: String,
}

pub async fn s3_upload_rpms(
    secrets: &SecretsMgr,
    url: &str,
    bucket: &str,
    bucket_loc: &str,
    components: &HashMap<String, ComponentBuild>,
    el_version: i32,
) -> anyhow::Result<HashMap<String, S3ComponentLocation>>;
```

#### Internal Functions
- `prepare_builder()` -- installs system dependencies (dnf, epel-release, cosign) inside the build container
- `prepare_components()` -- async context manager: clone repos, checkout refs, apply patches, yield `BuildComponentInfo`, cleanup on exit
- `_build_component()` -- run a single component's build script via `async_run_cmd`
- `_install_deps()` -- install build dependencies for all components sequentially
- `_sign_component_rpms()` -- sign all RPMs in a directory using `rpm --addsign`
- `_upload_component_rpms()` -- gather RPMs + repodata, upload to S3 for one component
- `_get_rpms()` -- collect `.rpm` files into `S3FileLocator` list
- `_get_repo()` -- run `createrepo` and collect repodata files for S3 upload
- `_get_patch_list()` -- find and order patches by priority for a given version
- `get_component_version()` -- run component's `get_version` script, return version string
- `cleanup_components()` -- remove git worktrees after build

#### Deliverables
- `releases/desc.rs`: Release descriptor types (already in `types/` from Phase 4 area, but S3 operations here)
- `releases/s3.rs`: `check_release_exists`, `release_desc_upload`, `release_upload_components`, `check_released_components`, `list_releases`
- `builder/`: `Builder.run()`, `prepare_builder()`, `prepare_components()`, `build_rpms()`, `sign_rpms()`, `s3_upload_rpms()`
- **State Checkpointing**: The builder checks for existing remote artifacts before starting each stage (matching the Python implementation -- no local scratch dir checks):
  - Container image already in registry? (`skopeo inspect`)
  - Release descriptor already in S3? (`s3_download_str_obj`)
  - Component builds already in S3? (per-component check)
- Parallel RPM builds via `tokio::task::JoinSet`
- CLI handler: `cmds/versions.rs` -- `handle_versions_list()` wiring the `versions list` subcommand to `list_releases()` (see [subcmd-versions-list.md](subcmd-versions-list.md))

#### Test Plan
- Unit: Release descriptor JSON round-trip; `BuildFlags` defaults; `S3ComponentLocation` construction
- Integration: `Builder::new()` with fixture config + version descriptor; `build_rpms()` with mock components; `check_release_exists()` / `list_releases()` against Ceph RGW (S3-compatible)
- PyO3: Not applicable (builder runs inside the container via CLI, not via Python bindings)
- CLI: Snapshot `cbsbuild versions list --help`

### Phase 9: Container Building + Runner (L)

#### Goal
Implement container image construction (PRE/PACKAGES/POST/CONFIG stages via Buildah) and the Podman-based runner that executes the full build pipeline inside a container.

#### Public Interface

| Function | Input | Output | Error | Example |
|----------|-------|--------|-------|---------|
| `ContainerBuilder::new` | `desc: VersionDescriptor`, `release_desc: ReleaseDesc`, `components: HashMap<String, CoreComponentLoc>` | `ContainerBuilder` | -- | constructs builder with no container yet |
| `ContainerBuilder::build` | `&mut self` | `()` | `anyhow::Error` | resolves components, creates buildah container, applies PRE/PACKAGES/POST/CONFIG |
| `ContainerBuilder::finish` | `&self`, `&SecretsMgr`, `sign_with_transit: Option<&str>` | `()` | `anyhow::Error` | commits, pushes, and optionally signs the image |
| `ComponentContainer::new` | `component_loc: &CoreComponentLoc`, `version: &str`, `vars: Option<&HashMap<String, String>>` | `ComponentContainer` | `anyhow::Error` | loads best-match `container.yaml` with variable substitution |
| `ContainerDescriptor::load` | `path: &Path`, `vars: Option<&HashMap<String, String>>` | `ContainerDescriptor` | `anyhow::Error` | `"container.yaml"` with `{version}` -> substituted + parsed YAML |
| `substitute_vars` | `template: &str`, `vars: &HashMap<String, String>` | `String` | `anyhow::Error` | `"v{version}-el{el}"` with `{"version":"18.2.4","el":"9"}` -> `"v18.2.4-el9"` |
| `runner` | `desc_file_path`, `cbscore_path`, `config`, `opts: RunnerOpts` | `()` | `CbsError` | launches Podman container with volume mounts, runs entrypoint |
| `gen_run_name` | `prefix: &str` | `String` | -- | `"ces_"` -> `"ces_abcdefghij"` (10 random lowercase chars) |
| `stop` | `name: Option<&str>`, `timeout: u32` | `()` | `anyhow::Error` | stops named container or all containers |

```rust
// containers/build.rs
pub struct ContainerBuilder {
    // private fields: version_desc, release_desc, components, container
}

impl ContainerBuilder {
    pub fn new(
        version_desc: VersionDescriptor,
        release_desc: ReleaseDesc,
        components: HashMap<String, CoreComponentLoc>,
    ) -> Self;

    pub async fn build(&mut self) -> anyhow::Result<()>;

    pub async fn finish(
        &self,
        secrets: &SecretsMgr,
        sign_with_transit: Option<&str>,
    ) -> anyhow::Result<()>;
}

// containers/component.rs
pub struct ComponentContainer {
    // private fields: version, component_loc, container_file_path, desc
}

impl ComponentContainer {
    pub fn new(
        component_loc: &CoreComponentLoc,
        version: &str,
        vars: Option<&HashMap<String, String>>,
    ) -> anyhow::Result<Self>;

    pub async fn apply_pre(&self, container: &BuildahContainer) -> anyhow::Result<()>;
    pub fn get_packages(&self, optional: bool) -> Vec<String>;
    pub async fn apply_post(&self, container: &BuildahContainer) -> anyhow::Result<()>;
    pub async fn apply_config(&self, container: &BuildahContainer) -> anyhow::Result<()>;
}

// containers/desc.rs
pub struct ContainerDescriptor {
    pub config: Option<ContainerConfig>,
    pub pre: ContainerPre,
    pub packages: ContainerPackages,
    pub post: Vec<ContainerScript>,
}

impl ContainerDescriptor {
    pub fn load(
        path: &Path,
        vars: Option<&HashMap<String, String>>,
    ) -> anyhow::Result<Self>;
}

pub fn substitute_vars(
    template: &str,
    vars: &HashMap<String, String>,
) -> anyhow::Result<String>;

// containers/repos.rs
pub enum ContainerRepo {
    File { name: String, source: String, dest: String },
    Url { name: String, source: String, dest: String },
    Copr { name: String, source: String },
}

impl ContainerRepo {
    pub async fn install(
        &self,
        container: &BuildahContainer,
        hint: &Path,
        root: &Path,
    ) -> anyhow::Result<()>;
}

// runner.rs
pub struct RunnerOpts {
    pub run_name: Option<String>,
    pub replace_run: bool,
    pub entrypoint_path: Option<PathBuf>,
    /// CLI parses `--timeout 14400` as seconds and converts to `Duration::from_secs(14400)`.
    pub timeout: Duration,
    pub log_file_path: Option<PathBuf>,
    pub log_out_cb: Option<CmdEventCallback>,
    pub skip_build: bool,
    pub force: bool,
    pub tls_verify: bool,
    pub cancel_token: CancellationToken,
}

pub async fn runner(
    desc_file_path: &Path,
    cbscore_path: &Path,
    config: &Config,
    opts: RunnerOpts,
) -> Result<(), CbsError>;

pub fn gen_run_name(prefix: &str) -> String;

pub async fn stop(name: Option<&str>, timeout: u32) -> anyhow::Result<()>;
```

#### Internal Functions
- `get_components()` -- resolves `ComponentContainer` for each version component, populating template variables from the release descriptor
- `apply_pre()` -- runs PRE scripts, imports RPM keys, installs PRE packages, installs repos
- `install_packages()` -- installs all required packages via `dnf install` in the buildah container
- `apply_post()` -- runs POST scripts
- `apply_config()` -- applies env vars, labels, and annotations via `buildah config`
- `_get_container_desc()` -- finds best-matching `container.yaml` by version specificity
- `_run_script()` -- copies a script into the container, executes it, removes it
- `_setup_components_dir()` -- creates temp directory aggregating all component paths
- `_cleanup_components_dir()` -- removes the temporary components directory
- `_log_callback()` -- produces an async callback that writes to a log file or delegates to a caller-provided callback

#### Deliverables
- `containers/build.rs`: `ContainerBuilder` with `build()`, `finish()`
- `containers/component.rs`: `ComponentContainer` with PRE/POST/CONFIG
- `containers/repos.rs`: `ContainerRepo` enum (File/URL/COPR variants) with `install()` method -- enum + match, not a trait hierarchy (same pattern as `VaultAuth`)
- `containers/desc.rs`: `ContainerDescriptor::load()` performs **template variable substitution** before YAML parsing
  - The raw YAML file content may contain `{key}` placeholders (Python `str.format()` syntax)
  - Before deserializing, all `{key}` patterns are replaced with values from an optional `HashMap<String, String>`
  - Implementation: single-pass parser (~20 lines, no external deps) -- iterate through the template, match `{key}` patterns, replace with values from the map, error on unresolved placeholders (matches Python's `KeyError` behavior, returned as `anyhow::Error` with context)
  - **Do not** use a multi-pass `str::replace` loop (re-substitution bug if a value contains `{another_key}`)
  - **Do not** use an external templating crate (no conditionals, loops, or format specs are used -- KISS)
  - 7 known template variables (constructed in `ContainerBuilder::get_components()`):
    | Variable | Source | Type |
    |----------|--------|------|
    | `version` | `ReleaseComponentVersion.version` | `String` |
    | `el` | `VersionDescriptor.el_version` | `i32` -> `.to_string()` |
    | `git_ref` | `ReleaseComponentVersion.version` | `String` |
    | `git_sha1` | `ReleaseComponentVersion.sha1` | `String` |
    | `git_repo_url` | `ReleaseComponentVersion.repo_url` | `String` |
    | `component_name` | `ReleaseComponentVersion.name` | `String` (currently unused in YAML files) |
    | `distro` | `VersionDescriptor.distro` | `String` |
  - Used by all Ceph `container.yaml` files (v17.2, v18.2, v19.2, v20.2) for env vars, labels, and RPM repo URLs
- `runner.rs`: `runner()`, `gen_run_name()`, `stop()`
- **Entrypoint verification**: Verify that `cbscore-entrypoint.sh` correctly installs the Rust-backed wheel inside the Podman container and that the `cbsbuild` binary is available on `PATH` for the recursive `cbsbuild runner build` call. This may require updating the entrypoint script to use `maturin` or `pip install` for the wheel instead of `uv tool install .`.
- **Critical**: PyO3 async binding for `runner()` using `pyo3-async-runtimes`
- CLI handlers: `cmds/builds.rs` -- `handle_build()` and `handle_runner_build()` wiring the `build` and `runner build` subcommands (see [subcmd-build.md](subcmd-build.md) and [subcmd-runner-build.md](subcmd-runner-build.md))

#### Test Plan
- Unit: `substitute_vars()` with known vars, unknown var error, empty vars pass-through, value containing `{braces}` not re-substituted; `gen_run_name()` prefix and length; `ContainerRepo` discriminator logic (file/url/copr)
- Integration: `ContainerDescriptor::load()` against real `container.yaml` fixtures; `runner()` with Podman (end-to-end); `ContainerBuilder::build()` with mock buildah
- PyO3: `from cbscore._cbscore import runner` async bridge via `pyo3-async-runtimes`; verify `cbsd` can call `runner()` from Python
- CLI: Snapshot `cbsbuild build --help` and `cbsbuild runner build --help`

### Phase 10: Python Shim Cleanup (M)

#### Goal
Replace all Python implementation files with thin re-export shims and remove unused Python dependencies, completing the migration to Rust.

#### Public Interface

No new public Rust interface in this phase. The work is removing Python code and verifying existing Rust interfaces are correctly re-exported.

#### Deliverables
- Replace all Python implementation files with thin re-export shims delegating to `_cbscore`
- `_exceptions.py` remains as the exception hierarchy definition (used by PyO3 error mapping)
- Remove now-unused Python dependencies (`aioboto3`, `aiofiles`, `hvac`, `click`)
- Verify all `cbsd`/`cbsdcore`/`cbc` tests pass
- Re-run all baseline subcommand help tests

Note: Full elimination of Python code (including `_exceptions.py` and shims) is out of scope for this plan.

#### Test Plan
- Unit: Not applicable (no new Rust code)
- Integration: All existing `cbsd`, `cbsdcore`, `cbc` test suites pass unchanged; `pyproject.toml` no longer lists removed dependencies
- PyO3: Every public symbol previously importable from `cbscore.*` is still importable and delegates to `_cbscore`; `from cbscore.errors import CESError`, `from cbscore.config import Config`, `from cbscore.runner import runner`, `from cbscore.versions.utils import parse_version` all work
- CLI: Re-run all baseline subcommand help snapshots from Phase 0; `cbsbuild --help`, `cbsbuild build --help`, `cbsbuild versions list --help` unchanged

---

## Critical Path & Parallelization

```
Phase 0 -> 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 (parallel tracks) -> 8 -> 9 -> 10
```

Each CLI handler is now implemented in the phase where its library dependencies are satisfied, rather than deferred to a separate CLI phase. This means `versions create` lands in Phase 2, `config init`/`config init-vault` land in Phase 3, `versions list` lands in Phase 8, and `build`/`runner build` land in Phase 9.

### Parallelization opportunities
- Phase 7 splits into 4 independent tracks (git, podman/buildah, S3, skopeo/images)
- Phase 8 depends on all 4 tracks of Phase 7 (builder uses git, S3, skopeo)
- Phase 9 depends on Phase 8 (ContainerBuilder consumes `ReleaseDesc` produced by `Builder`)
- Phase 10 depends on Phase 9 (all Rust interfaces must be complete before replacing Python)
