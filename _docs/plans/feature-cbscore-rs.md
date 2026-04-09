# Plan: Rewrite cbscore from Python to Rust

## Context

`cbscore` (~280KB, ~9,800 lines of Python across 55 files) is the core build library of CBS. It handles Ceph RPM building, container image creation, S3 artifact management, and Vault secrets. Three Python packages depend on it: `cbsd` (heavily — runner, config, versions, components), `cbsdcore` (lightly — VersionType, CESError), and `cbc` (lightly — version utilities, errors). The CLI `cbsbuild` is also part of cbscore.

The rewrite targets: **Rust 2024 edition, Clap CLI, Tokio async, Maturin + PyO3 for Python interop**.

---

## 1. Cargo Workspace Structure

Place the Rust workspace inside `cbscore/`:

```
cbscore/
├── Cargo.toml                      # Workspace root
├── pyproject.toml                   # Maturin build config (replaces uv_build)
├── rust/
│   ├── cbscore-types/               # Pure domain types — NO async, NO I/O
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── errors.rs            # CbsError enum hierarchy (thiserror)
│   │       ├── config.rs            # Config, PathsConfig, StorageConfig, etc. (serde)
│   │       ├── versions.rs          # mod declarations for versions submodules
│   │       ├── versions/
│   │       │   ├── desc.rs          # VersionDescriptor, VersionImage, etc.
│   │       │   ├── errors.rs        # VersionError variants
│   │       │   └── utils.rs         # VersionType, parse_version, parse_component_refs
│   │       ├── core.rs              # mod declarations for core submodules
│   │       ├── core/
│   │       │   └── component.rs     # CoreComponent, CoreComponentLoc, load_components
│   │       ├── secrets.rs           # mod declarations for secrets submodules
│   │       ├── secrets/
│   │       │   └── models.rs        # All 16 secret types + 4 discriminated unions
│   │       ├── releases.rs          # mod declarations for releases submodules
│   │       ├── releases/
│   │       │   └── desc.rs          # ArchType, BuildType, ReleaseDesc, etc.
│   │       ├── containers.rs        # mod declarations for containers submodules
│   │       ├── containers/
│   │       │   └── desc.rs          # ContainerDescriptor, repos, scripts
│   │       ├── images.rs            # mod declarations for images submodules
│   │       └── images/
│   │           ├── desc.rs          # ImageDescriptor
│   │           └── errors.rs        # SkopeoError, ImageNotFoundError, etc.
│   │
│   ├── cbscore-lib/                 # Async library — subprocess, S3, Vault, builder, runner
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── logging.rs           # tracing-based logging
│   │       ├── cmd.rs               # CmdArg, async_run_cmd, run_cmd, SecureArg types
│   │       ├── runner.rs            # runner(), gen_run_name(), stop()
│   │       ├── vault.rs             # Vault trait + backends (reqwest)
│   │       ├── s3.rs                # S3 operations (aws-sdk-s3)
│   │       ├── secrets.rs           # mod declarations for secrets submodules
│   │       ├── secrets/
│   │       │   ├── mgr.rs           # SecretsMgr
│   │       │   ├── git.rs           # SSH key setup, git_url_for
│   │       │   ├── storage.rs       # S3 credential resolution
│   │       │   ├── signing.rs       # GPG keyring creation
│   │       │   ├── registry.rs      # Registry credential resolution
│   │       │   └── utils.rs         # find_best_secret_candidate
│   │       ├── utils.rs             # mod declarations for utils submodules
│   │       ├── utils/
│   │       │   ├── git.rs           # run_git, git_clone, git_checkout, etc.
│   │       │   ├── podman.rs        # podman_run, podman_stop
│   │       │   ├── buildah.rs       # BuildahContainer, buildah_new_container
│   │       │   ├── containers.rs    # get_container_canonical_uri
│   │       │   ├── paths.rs         # Script path resolution
│   │       │   └── uris.rs          # matches_uri
│   │       ├── builder.rs           # mod declarations for builder submodules
│   │       ├── builder/
│   │       │   ├── build.rs         # Builder struct with run()
│   │       │   ├── prepare.rs       # prepare_builder, prepare_components
│   │       │   ├── rpmbuild.rs      # build_rpms, ComponentBuild
│   │       │   ├── signing.rs       # sign_rpms (GPG)
│   │       │   └── upload.rs        # s3_upload_rpms
│   │       ├── containers.rs        # mod declarations for containers submodules
│   │       ├── containers/
│   │       │   ├── build.rs         # ContainerBuilder
│   │       │   ├── component.rs     # ComponentContainer
│   │       │   └── repos.rs         # File/URL/COPR repository impls
│   │       ├── images.rs            # mod declarations for images submodules
│   │       ├── images/
│   │       │   ├── skopeo.rs        # skopeo_get_tags, _copy, _inspect, _image_exists
│   │       │   ├── signing.rs       # cosign signing
│   │       │   └── sync.rs          # Image sync
│   │       ├── releases.rs          # mod declarations for releases submodules
│   │       ├── releases/
│   │       │   ├── s3.rs            # check_release_exists, release_desc_upload, etc.
│   │       │   └── utils.rs         # get_component_release_rpm
│   │       ├── versions.rs          # mod declarations for versions submodules
│   │       └── versions/
│   │           └── create.rs        # version_create_helper
│   │
│   ├── cbscore-python/              # PyO3 bindings — thin wrapper
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs               # #[pymodule] _cbscore
│   │       ├── errors.rs            # Rust error → Python exception mapping
│   │       ├── config.rs            # PyConfig wrapper
│   │       ├── versions.rs          # PyVersionDescriptor, version funcs
│   │       ├── runner.rs            # Async runner bridge
│   │       ├── core.rs              # load_components wrapper
│   │       └── logging.rs           # set_debug_logging wrapper
│   │
│   └── cbsbuild/                    # CLI binary
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs              # #[tokio::main], Clap root
│           └── cmds/
│               ├── mod.rs
│               ├── builds.rs        # build, runner build
│               ├── versions.rs      # versions create, versions list
│               ├── config.rs        # config init, config init-vault (dialoguer)
│               └── advanced.rs      # Placeholder
│
└── src/cbscore/                     # Python package (transitional shims → removed in Phase 11)
    ├── __init__.py                  # Re-exports from _cbscore
    ├── _exceptions.py               # Python exception hierarchy (used by PyO3 error mapping)
    ├── errors.py                    # Re-exports from _exceptions
    ├── config.py                    # Re-exports from _cbscore
    ├── logger.py                    # Re-exports from _cbscore
    ├── runner.py                    # Async bridge wrapping Rust runner
    ├── __main__.py                  # CLI entry (transitional, replaced by cbsbuild binary)
    ├── versions/
    │   ├── __init__.py
    │   ├── utils.py                 # Re-exports from _cbscore
    │   ├── desc.py                  # Re-exports from _cbscore
    │   ├── create.py                # Re-exports from _cbscore
    │   └── errors.py                # Re-exports from _exceptions
    └── core/
        ├── __init__.py
        └── component.py             # Re-exports from _cbscore
```

### Crate dependency graph

```
cbscore-types  (serde, thiserror, regex, strum — zero async)
    ↑
cbscore-lib    (cbscore-types, tokio, aws-sdk-s3, vaultrs, tracing)
    ↑
    ├── cbsbuild        (cbscore-lib, cbscore-types, clap, dialoguer, anyhow)
    └── cbscore-python  (cbscore-lib, cbscore-types, pyo3, pyo3-async-runtimes, pyo3-log)
```

### Root `Cargo.toml`

```toml
[workspace]
members = ["rust/*"]
resolver = "3"

[workspace.package]
edition = "2024"
version = "2.0.0"
license = "GPL-3.0-or-later"

[workspace.dependencies]
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
dirs = "6"
regex = "1"
strum = { version = "0.26", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
dialoguer = "0.11"
anyhow = "1"
pyo3 = { version = "0.23", features = ["extension-module"] }
pyo3-async-runtimes = { version = "0.23", features = ["tokio-runtime"] }
pyo3-log = "0.12"
aws-config = "1"
aws-sdk-s3 = "1"
reqwest = { version = "0.12", features = ["json"] }
vaultrs = "0.7"
rand = "0.9"
tempfile = "3"
```

---

## 2. Key Design Decisions

### Documentation

All public functions, structs, enums, traits, and methods must have `///` doc comments. This is enforced via `#![warn(missing_docs)]` at the crate level for all four crates. Doc comments should describe:
- **What** the item does (not how — the code shows that)
- **Parameters** and return values for non-obvious signatures
- **Errors** — which error variants can be returned
- **Panics** — if the function can panic, document when

Private functions should have doc comments when the intent is not self-evident from the name and signature.

### Function size

Functions must be short and focused — each function does **one thing**. As a guideline, a function body should not exceed ~20-30 lines. When a function grows beyond that, extract logical steps into well-named helper functions. This applies equally to implementation code and CLI command handlers.

Patterns to follow:
- **Orchestrator + helpers**: A top-level function calls a sequence of small helpers, each handling one step
- **Early returns**: Use guard clauses and `?` to keep the happy path flat
- **Named steps**: Extract conditional blocks (e.g., prompt sequences, validation) into their own functions with descriptive names

This keeps code readable, testable, and aligned with the Single Responsibility Principle.

### Error hierarchy

Single top-level `CbsError` enum with `#[from]` conversions for each domain error. Maps to Python exception hierarchy via `_exceptions.py` (pure Python) + Rust `From<CbsError> for PyErr` using `GILOnceCell`-cached exception classes.

```rust
// cbscore-types/src/errors.rs
#[derive(Debug, Error)]
pub enum CbsError {
    #[error(transparent)] Config(#[from] ConfigError),
    #[error(transparent)] Version(#[from] VersionError),
    #[error(transparent)] Builder(#[from] BuilderError),
    #[error(transparent)] Runner(#[from] RunnerError),
    #[error(transparent)] Container(#[from] ContainerError),
    #[error(transparent)] Release(#[from] ReleaseError),
    #[error(transparent)] Secrets(#[from] SecretsError),
    #[error(transparent)] Command(#[from] CommandError),
    #[error(transparent)] S3(#[from] S3Error),
    #[error(transparent)] Vault(#[from] VaultError),
    #[error(transparent)] Image(#[from] ImageError),
    #[error(transparent)] Git(#[from] GitError),
    #[error("malformed version: {0}")] MalformedVersion(String),
    #[error("no such version: {0}")] NoSuchVersion(String),
    #[error("unknown repository: {0}")] UnknownRepository(String),
}
```

### Config models (serde)

Use `#[serde(alias = "...")]` for Python field aliases (e.g., `scratch-containers`). `Config::load()` and `Config::store()` use synchronous `std::fs` in `cbscore-types` since config I/O is simple file read/write.

### Secret discriminated unions

Custom `Deserialize` implementations. Deserialize to `serde_json::Value` first, inspect `creds` and `type` fields, then deserialize to the correct variant. This mirrors the Python discriminator functions exactly.

### Async command executor (foundation)

```rust
// cbscore-lib/src/cmd.rs
pub enum CmdArg {
    Plain(String),
    Secure { display: String, value: String },
}

pub struct CmdOpts<'a> {
    pub cwd: Option<&'a Path>,
    pub timeout: Option<Duration>,
    pub output_cb: Option<Box<dyn Fn(&str) + Send + Sync>>,
    pub env: Option<HashMap<String, String>>,
    pub reset_python_env: bool,
}

pub async fn async_run_cmd(args: &[CmdArg], opts: CmdOpts<'_>) -> Result<CmdResult, CommandError>;
pub fn run_cmd(args: &[CmdArg], env: Option<&HashMap<String, String>>) -> Result<CmdResult, CommandError>;
```

Uses `tokio::process::Command` with `BufReader` on stdout/stderr for streaming. Timeout via `tokio::time::timeout` with `child.kill()` on expiry.

### Vault client

Use the `vaultrs` crate for full Vault client support. Although only 3 endpoints are currently needed (AppRole login, UserPass login, KVv2 read), using the established crate provides better API coverage for future needs and avoids maintaining a custom HTTP client.

### S3 client

Replace `aioboto3` with `aws-sdk-s3`. Explicit `Credentials::new()` from `SecretsMgr` (no env-based credential loading).

### Python context managers → Rust RAII guards

`gpg_signing_key()` → `GpgKeyringGuard` with `Drop` that erases the temp keyring.
`git_url_for()` → `GitUrlGuard` with `Drop` that cleans up SSH key/config.

---

## 3. PyO3 Binding Strategy

### Module structure

The native extension is `cbscore._cbscore` (flat module). Python shim files in `src/cbscore/` re-export with the correct submodule paths. This avoids `sys.modules` hacks.

### Exception hierarchy

Defined in pure Python (`_exceptions.py`) with real inheritance. Rust errors map to these via cached `GILOnceCell<Py<PyAny>>` references:

```rust
fn map_error_to_pyerr(err: CbsError) -> PyErr {
    Python::with_gil(|py| {
        let (cls_name, msg) = match &err {
            CbsError::MalformedVersion(m) => ("MalformedVersionError", m.clone()),
            CbsError::Config(e) => ("ConfigError", e.to_string()),
            CbsError::Version(e) => ("VersionError", e.to_string()),
            CbsError::Runner(e) => ("RunnerError", e.to_string()),
            _ => ("CESError", err.to_string()),
        };
        let cls = py.import("cbscore._exceptions").unwrap().getattr(cls_name).unwrap();
        PyErr::from_value(cls.call1((msg,)).unwrap())
    })
}
```

### Types consumed as Pydantic fields

`VersionDescriptor` is embedded in `cbsd`'s `WorkerBuildEntry(pydantic.BaseModel)` and serialized via `model_dump_json()`. Since the goal is to eliminate all Python code eventually, all types must be pure `#[pyclass]` in Rust.

**Strategy**: Implement `__get_pydantic_core_schema__` on `#[pyclass]` types that are used as Pydantic fields. This tells Pydantic how to validate and serialize the Rust-backed type without requiring Python inheritance from `BaseModel`.

```rust
#[pymethods]
impl PyVersionDescriptor {
    #[classmethod]
    fn __get_pydantic_core_schema__(
        _cls: &Bound<'_, PyType>,
        _source_type: &Bound<'_, PyAny>,
        _handler: &Bound<'_, PyAny>,
    ) -> PyResult<PyObject> {
        // Return a pydantic_core schema:
        // - validate from JSON: call cls.model_validate_json()
        // - validate from dict: construct from fields
        // - serialize: call .model_dump_json()
    }
}
```

This applies to `VersionDescriptor` (used in `WorkerBuildEntry`). All other types (`Config`, `CoreComponentLoc`, etc.) use plain `#[pyclass]` with getters since they are not embedded in Pydantic models.

### Async runner bridge

The `runner()` function is the critical async boundary. `cbsd` calls it via `await runner(...)` from its own asyncio event loop.

Two approaches were evaluated for the log streaming callback:

**Option 1: Sync callback bridge**
| Pros | Cons |
|------|------|
| Simple implementation (~10 lines) | Blocks tokio thread while Python callback runs |
| No extra crate dependency | GIL contention via `run_coroutine_threadsafe` |
| Easy to debug | Not truly non-blocking |
| Works with any event loop impl | Throttles build output if callback is slow |

**Option 2: Full async bridge (pyo3-async-runtimes)**
| Pros | Cons |
|------|------|
| True non-blocking end-to-end | Must match pyo3 version exactly |
| No tokio thread blocking | Complex tokio ↔ asyncio interaction |
| Better throughput at high log volume | Risk of deadlock with unclear loop ownership |
| Future-proof for complex async | cbsd's per-thread event loop needs verification |

**Decision: Option 2 (full async bridge)**. Non-blocking is required. Use `pyo3-async-runtimes` with tokio feature for true async end-to-end. The cbsd per-thread event loop compatibility must be validated early in Phase 9.

Regardless of callback approach, the runner itself uses `pyo3-async-runtimes::tokio::future_into_py` to return a Python awaitable:

```rust
// cbscore-python/src/runner.rs
#[pyfunction]
fn rust_runner<'py>(py: Python<'py>, /* params */) -> PyResult<Bound<'py, PyAny>> {
    future_into_py(py, async move {
        cbscore_lib::runner::runner(/* ... */).await
            .map_err(|e| map_error_to_pyerr(e.into()))
    })
}
```

### CLI binary installation

The `cbsbuild` binary is available via two paths:
- **Dev workflow**: Maturin bundles the binary in the Python wheel. Available after `uv sync` / `maturin develop`.
- **Production/standalone**: Built separately via `cargo build --release -p cbsbuild`. Decoupled from Python, deployable independently.

### Maturin pyproject.toml

```toml
[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[project]
name = "cbscore"
version = "2.0.0"
requires-python = ">=3.13"
dependencies = ["pydantic>=2.11.6"]

[tool.maturin]
manifest-path = "rust/cbscore-python/Cargo.toml"
module-name = "cbscore._cbscore"
python-source = "src"
features = ["pyo3/extension-module"]
# Include cbsbuild binary in the wheel
bindings = "pyo3"
```

---

## 4. Implementation Phases

### Phase 0: Scaffolding (S)

- Create Cargo workspace, all 4 crate skeletons
- Configure Maturin in `pyproject.toml`
- Expose a trivial `cbscore._cbscore.version()` via PyO3
- Verify `maturin develop` + `uv sync --all-packages` work together
- Verify existing Python code still works unchanged

**Test**:
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

### Phase 1: Errors + Logging (S)

- `cbscore-types/src/errors.rs`: Full `thiserror` hierarchy (~22 error types)
- `cbscore-lib/src/logging.rs`: `tracing` setup
- `src/cbscore/_exceptions.py`: Pure Python exception hierarchy
- PyO3 error mapping in `cbscore-python/src/errors.rs`
- Update `src/cbscore/errors.py` to re-export from `_exceptions.py`

**Test**: Unit tests for error types; `from cbscore.errors import CESError; raise CESError("test")` from Python; existing `cbsd` imports still work.

### Phase 2: Version Management + Core Components (M)

- `cbscore-types/src/versions/`: `VersionType`, `parse_version()`, `normalize_version()`, `get_version_type()`, `parse_component_refs()`, `VersionDescriptor` + sub-types with serde JSON
- `cbscore-types/src/core/component.rs`: `CoreComponent`, `CoreComponentLoc`, `load_components()` with serde YAML
- `cbscore-lib/src/versions/create.rs`: `version_create_helper()`
- PyO3 bindings for all above — pure `#[pyclass]` types, `VersionType` as PyO3 enum with string conversion, `VersionDescriptor` with `__get_pydantic_core_schema__` for cbsd compatibility

**Test**:
- Port ~30 inline tests from Python `versions/utils.py` to Rust `#[test]`
- JSON round-trip for VersionDescriptor
- `load_components()` against fixture dirs
- PyO3: `from cbscore._cbscore import VersionType, parse_component_refs, VersionDescriptor`
- Verify `cbsdcore`, `cbc`, `cbsd` imports still work
- Re-run baseline subcommand help tests from Phase 0

### Phase 3: Configuration System (M)

- `cbscore-types/src/config.rs`: All config models with serde aliases, `Config::load()`, `Config::store()`
- PyO3 `PyConfig` wrapper with getters and `model_dump_json()`
- Python shim `config.py`

**Test**: YAML round-trip; field alias tests; `Config.load(path)` from Python matches original.

### Phase 4: Secret Models (L)

- `cbscore-types/src/secrets/models.rs`: All 16 secret struct types + 4 discriminated union enums with custom `Deserialize`
- `Secrets` container with `load()`, `store()`, `merge()`
- `cbscore-lib/src/secrets/utils.rs`: `find_best_secret_candidate()`
- `cbscore-types/src/utils/uris.rs` (if needed for matching logic)

**Test**: YAML round-trip for each secret variant; discriminator logic tests; merge tests.

### Phase 5: Vault + Secure Args (M)

- `cbscore-lib/src/vault.rs`: `Vault` trait, AppRole/UserPass/Token backends via `vaultrs` crate
- `cbscore-lib/src/cmd.rs` (partial): `CmdArg`, `SecureArg`, sanitize, `run_cmd()` (sync)

**Test**: SecureArg display masking; sanitize_cmd behavior; Vault integration test (dev Vault container).

### Phase 6: Async Command Executor + Secrets Manager (L)

- `cbscore-lib/src/cmd.rs` (complete): `async_run_cmd()` with tokio::process, streaming, timeout
- `cbscore-lib/src/secrets/mgr.rs`: `SecretsMgr` with `git_url_for()`, `s3_creds()`, `gpg_signing_key()`, `transit()`, `registry_creds()`
- `cbscore-lib/src/secrets/git.rs`: SSH key RAII guard
- `cbscore-lib/src/secrets/signing.rs`: GPG keyring RAII guard
- `cbscore-lib/src/secrets/storage.rs`, `registry.rs`

**Test**: `async_run_cmd("echo", "hello")`; timeout + kill; SecretsMgr with mock Vault.

### Phase 7: External Tool Wrappers (XL — parallelizable)

Can split into 4 independent tracks:

**7a. Git** — `utils/git.rs`: all git async operations (clone, checkout, worktree, fetch, etc.)
**7b. Podman + Buildah** — `utils/podman.rs`, `utils/buildah.rs`: `BuildahContainer`, `podman_run`, `podman_stop`
- `podman_run` must support a `persist_on_failure: bool` flag — when set, the container is **not** removed after a failed run, allowing manual `podman exec` into the container for debugging
- `podman_run` must replicate the Python implementation's security and device options required for Buildah-in-Podman:
  - `--security-opt label=disable` (disable SELinux labeling)
  - `--security-opt seccomp=unconfined` (when `unconfined` flag is set)
  - `--device /dev/fuse:/dev/fuse:rw` (FUSE device for overlay mounts inside the container)
**7c. S3** — `s3.rs`: `aws-sdk-s3` replacing `aioboto3` (upload, download, list)
**7d. Skopeo + Images** — `images/skopeo.rs`, `images/signing.rs`, `images/sync.rs`, `images/desc.rs`

Also: `utils/containers.rs`, `utils/paths.rs`

**Test**: Git with temp repos; S3 with Ceph RGW (S3-compatible); Podman/Buildah/Skopeo as integration-only.

### Phase 8: Releases + Builder Pipeline (XL)

- `releases/desc.rs`: Release descriptor types (already in cbscore-types from Phase 4 area, but S3 operations here)
- `releases/s3.rs`: `check_release_exists`, `release_desc_upload`, `release_upload_components`, `check_released_components`, `list_releases`
- `builder/`: `Builder.run()`, `prepare_builder()`, `prepare_components()`, `build_rpms()`, `sign_rpms()`, `s3_upload_rpms()`
- **State Checkpointing**: The builder pipeline must check for existing artifacts (in scratch dir and S3) before starting each stage, allowing resume-on-failure. This follows the KISS approach — no external state store, just check if the output of a stage already exists before running it. Stages to checkpoint:
  - Component source checkout (scratch dir exists with correct SHA?)
  - RPM build (RPMs already in scratch/rpms/?)
  - RPM signing (signed RPMs present?)
  - S3 upload (artifacts already in S3 bucket?)
  - Container image (already in registry? — this already exists in the Python code via `skopeo_image_exists`)
- Parallel RPM builds via `tokio::task::JoinSet`

**Test**: Release descriptor JSON round-trip; builder integration tests.

### Phase 9: Container Building + Runner (L)

- `containers/build.rs`: `ContainerBuilder` with `build()`, `finish()`
- `containers/component.rs`: `ComponentContainer` with PRE/POST/CONFIG
- `containers/repos.rs`: File/URL/COPR repository types
- `runner.rs`: `runner()`, `gen_run_name()`, `stop()`
- **Entrypoint verification**: Verify that `cbscore-entrypoint.sh` correctly installs the Rust-backed wheel inside the Podman container and that the `cbsbuild` binary is available on `PATH` for the recursive `cbsbuild runner build` call. This may require updating the entrypoint script to use `maturin` or `pip install` for the wheel instead of `uv tool install .`.
- **Critical**: PyO3 async binding for `runner()` using `pyo3-async-runtimes`

**Test**: Container descriptor YAML loading; runner integration test with Podman.

### Phase 10: CLI with Clap (M)

- `cbsbuild/src/main.rs`: Clap command tree with `#[tokio::main]`
- `cmds/builds.rs`: `build`, `runner build`
- `cmds/versions.rs`: `versions create`, `versions list`
- `cmds/config.rs`: `config init`, `config init-vault`
- `cmds/advanced.rs`: empty placeholder

Each subcommand has a detailed plan with description, sequence diagram, class diagram, and implementation specifics. See [Subcommand Detail Plans](#subcommand-detail-plans) below.

**Test**: CLI help output snapshots; `cbsbuild versions create` with fixtures; re-run all baseline tests.

### Phase 11: Python Shim Cleanup (M)

- Replace all Python implementation files with thin re-export shims delegating to `_cbscore`
- `_exceptions.py` remains as the exception hierarchy definition (used by PyO3 error mapping)
- Remove now-unused Python dependencies (`aioboto3`, `aiofiles`, `hvac`, `click`)
- Verify all `cbsd`/`cbsdcore`/`cbc` tests pass
- Re-run all baseline subcommand help tests

Note: Full elimination of Python code (including `_exceptions.py` and shims) is out of scope for this plan.

---

## 5. Critical Path

```
Phase 0 → 1 → 2 → 3 → 4 → 5 → 6 → 7 (parallel tracks) → 8 → 9 → 10 → 11
                                                                  ↗
                                    Phase 10 (versions+config cmds can start after Phase 3)
```

### Parallelization opportunities
- Phase 7 splits into 4 independent tracks (git, podman/buildah, S3, skopeo/images)
- Phase 10 (CLI) can partially start after Phase 3 (for versions + config commands)

---

## 6. Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| PyO3 async bridge for `runner()` | High | Use simplified sync callback wrapper in Python shim; test the bridge in Phase 0 with a minimal async function |
| Secret discriminated unions (multi-field dispatch) | Medium | Custom `Deserialize` via `serde_json::Value` intermediary; comprehensive round-trip tests |
| Maturin + uv workspace coexistence | Medium | Validate in Phase 0 before any real code; `maturin develop` must work alongside `uv sync` |
| `VersionDescriptor` as Pydantic field in `cbsd` | Medium | Implement `__get_pydantic_core_schema__` on `#[pyclass]` so Pydantic can validate/serialize the Rust type natively |
| `aioboto3` → `aws-sdk-s3` API differences | Medium | Focus on the 6 operations used; test with Ceph RGW |
| `WorkerBuilder` creates its own asyncio event loop | Medium | `pyo3-async-runtimes::tokio::future_into_py` captures the running loop; verify with a test that mimics `cbsd`'s pattern |

---

## 7. Verification Plan

### Per-phase
- Each phase has unit tests in Rust (`#[test]`)
- Each phase verifies Python imports still work after shim updates
- Existing `cbsd/tests/` test suite must pass after each phase

### End-to-end
- `cbsbuild --help` produces correct output
- `cbsbuild config init --for-containerized-run` generates valid YAML
- `cbsbuild versions create` produces valid version descriptor JSON
- `cbsd` worker can call `runner()` and receive log callbacks
- Full build pipeline works with `do-cbs-compose.sh`

### Integration tests (require tools)
- Git operations: temp repos
- S3: Ceph RGW container (S3-compatible gateway — native choice for a Ceph build system)
- Vault: dev Vault container
- Podman/Buildah/Skopeo: CI with tools installed

---

## 8. Subcommand Detail Plans

Each subcommand has its own detailed document with: description, CLI signature, mermaid sequence diagram, class diagram, Rust implementation plan, and tests.

| Subcommand | Detail Plan | Status |
|------------|-------------|--------|
| `config init` | [subcmd-config-init.md](subcmd-config-init.md) | Done |
| `config init-vault` | [subcmd-config-init-vault.md](subcmd-config-init-vault.md) | Done |
| `versions create` | subcmd-versions-create.md | Pending |
| `versions list` | subcmd-versions-list.md | Pending |
| `build` | subcmd-build.md | Pending |
| `runner build` | subcmd-runner-build.md | Pending |
| `advanced` | — | Empty placeholder, no detail plan needed |
