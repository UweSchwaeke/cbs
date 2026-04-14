# Implementation Plan: cbscore Rust Rewrite

> Extracted from [feature-cbscore-rs.md](feature-cbscore-rs.md) sections 7 and 8.
> For architecture, design principles, and technical design, see the parent document.

---

## Implementation Phases

### Phase 0: Scaffolding (S)

- Create Cargo workspace, all 3 crate skeletons
- Configure Maturin in `pyproject.toml`
- Expose a trivial `cbscore._cbscore.version()` via PyO3
- `cbsbuild/src/main.rs`: Clap command tree scaffold with `#[tokio::main]` — all subcommand stubs (`build`, `runner build`, `versions create`, `versions list`, `config init`, `config init-vault`, `advanced`) defined with their arg structs but returning `todo!()` or a "not yet implemented" error
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

- `cbscore-lib/src/types/errors.rs`: `CbsError` enum (~8 variants, `thiserror`) — public API boundary errors only; internal modules use `anyhow::Result`
- `cbscore-lib/src/logging.rs`: `tracing` setup
- `src/cbscore/_exceptions.py`: Pure Python exception hierarchy
- PyO3 error mapping in `cbscore-python/src/errors.rs`
- Update `src/cbscore/errors.py` to re-export from `_exceptions.py`

**Test**: Unit tests for error types; `from cbscore.errors import CESError; raise CESError("test")` from Python; existing `cbsd` imports still work.

### Phase 2: Version Management + Core Components (M)

- `cbscore-lib/src/types/versions/`: `VersionType`, `parse_version()`, `normalize_version()`, `get_version_type()`, `parse_component_refs()`, `VersionDescriptor` + sub-types with serde JSON
- `cbscore-lib/src/types/core/component.rs`: `CoreComponent`, `CoreComponentLoc`, `load_components()` with serde YAML
- `cbscore-lib/src/versions/create.rs`: `version_create_helper()`
- PyO3 bindings for all above — pure `#[pyclass]` types, `VersionType` as PyO3 enum with string conversion, `VersionDescriptor` with `__get_pydantic_core_schema__` for cbsd compatibility

- CLI handler: `cmds/versions.rs` — `handle_versions_create()` wiring the `versions create` subcommand to `version_create_helper()` (see [subcmd-versions-create.md](subcmd-versions-create.md))

**Test**:
- Port ~30 inline tests from Python `versions/utils.py` to Rust `#[test]`
- JSON round-trip for VersionDescriptor
- `load_components()` against fixture dirs
- PyO3: `from cbscore._cbscore import VersionType, parse_component_refs, VersionDescriptor`
- Verify `cbsdcore`, `cbc`, `cbsd` imports still work
- Re-run baseline subcommand help tests from Phase 0
- `cbsbuild versions create` with fixtures; snapshot `cbsbuild versions create --help`

### Phase 3: Configuration System (M)

- `cbscore-lib/src/types/config.rs`: All config models with serde aliases, `Config::load()`, `Config::store()`
- PyO3 `PyConfig` wrapper with getters and `model_dump_json()`
- Python shim `config.py`
- CLI handlers: `cmds/config.rs` — `handle_config_init()` and `handle_config_init_vault()` wiring the `config init` and `config init-vault` subcommands (see [subcmd-config-init.md](subcmd-config-init.md) and [subcmd-config-init-vault.md](subcmd-config-init-vault.md))

**Test**: YAML round-trip; field alias tests; `Config.load(path)` from Python matches original; `cbsbuild config init --for-containerized-run` generates valid YAML; snapshot `cbsbuild config init --help` and `cbsbuild config init-vault --help`.

### Phase 4: Secret Models (L)

- `cbscore-lib/src/types/secrets/models.rs`: All 16 secret struct types + 4 discriminated union enums with custom `Deserialize`
- `Secrets` container with `load()`, `store()`, `merge()`
- `cbscore-lib/src/secrets/utils.rs`: `find_best_secret_candidate()`
- `cbscore-lib/src/utils/uris.rs` (if needed for matching logic)

**Test**: YAML round-trip for each secret variant; discriminator logic tests; merge tests.

### Phase 5: Vault + Secure Args (M)

- `cbscore-lib/src/vault.rs`: `VaultClient` struct + `VaultAuth` enum via `vaultrs` crate (concrete implementation, no trait hierarchy)
- `cbscore-lib/src/cmd.rs` (partial): `CmdArg`, `SecureArg`, sanitize, `run_cmd()` (sync)

**Test**: SecureArg display masking; sanitize_cmd behavior; Vault integration test (dev Vault container).

### Phase 6: Async Command Executor + Secrets Manager (L)

- `cbscore-lib/src/cmd.rs` (complete): `async_run_cmd()` with tokio::process, streaming, timeout
- `cbscore-lib/src/secrets/mgr.rs`: `SecretsMgr` with `git_url_for()`, `s3_creds()`, `gpg_signing_key()`, `transit()`, `registry_creds()`
- `cbscore-lib/src/secrets/git.rs`: SSH key RAII guard
- `cbscore-lib/src/secrets/signing.rs`: GPG keyring RAII guard
- `cbscore-lib/src/secrets/storage.rs`, `registry.rs`

**SecretsMgr construction**: `async fn SecretsMgr::new(secrets, vault_config) -> Result<SecretsMgr>` loads secrets from files, constructs the VaultClient, and verifies the vault connection in a single call. This matches the Python `__init__` behavior — no caller ever wants a half-initialized SecretsMgr.

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

- `releases/desc.rs`: Release descriptor types (already in `types/` from Phase 4 area, but S3 operations here)
- `releases/s3.rs`: `check_release_exists`, `release_desc_upload`, `release_upload_components`, `check_released_components`, `list_releases`
- `builder/`: `Builder.run()`, `prepare_builder()`, `prepare_components()`, `build_rpms()`, `sign_rpms()`, `s3_upload_rpms()`
- **State Checkpointing**: The builder checks for existing remote artifacts before starting each stage (matching the Python implementation — no local scratch dir checks):
  - Container image already in registry? (`skopeo inspect`)
  - Release descriptor already in S3? (`s3_download_str_obj`)
  - Component builds already in S3? (per-component check)
- Parallel RPM builds via `tokio::task::JoinSet`
- CLI handler: `cmds/versions.rs` — `handle_versions_list()` wiring the `versions list` subcommand to `list_releases()` (see [subcmd-versions-list.md](subcmd-versions-list.md))

**Test**: Release descriptor JSON round-trip; builder integration tests; snapshot `cbsbuild versions list --help`.

### Phase 9: Container Building + Runner (L)

- `containers/build.rs`: `ContainerBuilder` with `build()`, `finish()`
- `containers/component.rs`: `ComponentContainer` with PRE/POST/CONFIG
- `containers/repos.rs`: `ContainerRepo` enum (File/URL/COPR variants) with `install()` method — enum + match, not a trait hierarchy (same pattern as `VaultAuth`)
- `containers/desc.rs`: `ContainerDescriptor::load()` performs **template variable substitution** before YAML parsing
  - The raw YAML file content may contain `{key}` placeholders (Python `str.format()` syntax)
  - Before deserializing, all `{key}` patterns are replaced with values from an optional `HashMap<String, String>`
  - Implementation: single-pass parser (~20 lines, no external deps) — iterate through the template, match `{key}` patterns, replace with values from the map, error on unresolved placeholders (matches Python's `KeyError` behavior, returned as `anyhow::Error` with context)
  - **Do not** use a multi-pass `str::replace` loop (re-substitution bug if a value contains `{another_key}`)
  - **Do not** use an external templating crate (no conditionals, loops, or format specs are used — KISS)
  - Signature: `fn substitute_vars(template: &str, vars: &HashMap<String, String>) -> anyhow::Result<String>`
  - 7 known template variables (constructed in `ContainerBuilder::get_components()`):
    | Variable | Source | Type |
    |----------|--------|------|
    | `version` | `ReleaseComponentVersion.version` | `String` |
    | `el` | `VersionDescriptor.el_version` | `i32` → `.to_string()` |
    | `git_ref` | `ReleaseComponentVersion.version` | `String` |
    | `git_sha1` | `ReleaseComponentVersion.sha1` | `String` |
    | `git_repo_url` | `ReleaseComponentVersion.repo_url` | `String` |
    | `component_name` | `ReleaseComponentVersion.name` | `String` (currently unused in YAML files) |
    | `distro` | `VersionDescriptor.distro` | `String` |
  - Used by all Ceph `container.yaml` files (v17.2, v18.2, v19.2, v20.2) for env vars, labels, and RPM repo URLs
- `runner.rs`: `runner()`, `gen_run_name()`, `stop()`
- **Entrypoint verification**: Verify that `cbscore-entrypoint.sh` correctly installs the Rust-backed wheel inside the Podman container and that the `cbsbuild` binary is available on `PATH` for the recursive `cbsbuild runner build` call. This may require updating the entrypoint script to use `maturin` or `pip install` for the wheel instead of `uv tool install .`.
- **Critical**: PyO3 async binding for `runner()` using `pyo3-async-runtimes`
- CLI handlers: `cmds/builds.rs` — `handle_build()` and `handle_runner_build()` wiring the `build` and `runner build` subcommands (see [subcmd-build.md](subcmd-build.md) and [subcmd-runner-build.md](subcmd-runner-build.md))

**Test**: Container descriptor YAML loading with template variable substitution (known vars, unknown var error, no vars pass-through); runner integration test with Podman; snapshot `cbsbuild build --help` and `cbsbuild runner build --help`.

### Phase 10: Python Shim Cleanup (M)

- Replace all Python implementation files with thin re-export shims delegating to `_cbscore`
- `_exceptions.py` remains as the exception hierarchy definition (used by PyO3 error mapping)
- Remove now-unused Python dependencies (`aioboto3`, `aiofiles`, `hvac`, `click`)
- Verify all `cbsd`/`cbsdcore`/`cbc` tests pass
- Re-run all baseline subcommand help tests

Note: Full elimination of Python code (including `_exceptions.py` and shims) is out of scope for this plan.

---

## Critical Path & Parallelization

```
Phase 0 → 1 → 2 → 3 → 4 → 5 → 6 → 7 (parallel tracks) → 8 → 9 → 10
```

Each CLI handler is now implemented in the phase where its library dependencies are satisfied, rather than deferred to a separate CLI phase. This means `versions create` lands in Phase 2, `config init`/`config init-vault` land in Phase 3, `versions list` lands in Phase 8, and `build`/`runner build` land in Phase 9.

### Parallelization opportunities
- Phase 7 splits into 4 independent tracks (git, podman/buildah, S3, skopeo/images)
