# Requirements Specification: cbscore Rust Rewrite

> **Document type:** Software Requirements Specification (SRS)
> **Status:** Draft

## Table of Contents

- [1. Purpose and Scope](#1-purpose-and-scope)
- [2. Stakeholders](#2-stakeholders)
- [3. Functional Requirements](#3-functional-requirements)
  - [3.1 Configuration Management](#31-configuration-management)
  - [3.2 Version Management](#32-version-management)
  - [3.3 Release Discovery](#33-release-discovery)
  - [3.4 Build Execution (Host-side)](#34-build-execution-host-side)
  - [3.5 Build Orchestration (Container-side)](#35-build-orchestration-container-side)
  - [3.6 Python Interoperability](#36-python-interoperability)
- [4. Non-Functional Requirements](#4-non-functional-requirements)
  - [4.1 Backward Compatibility](#41-backward-compatibility)
  - [4.2 Security](#42-security)
  - [4.3 Performance](#43-performance)
  - [4.4 Observability](#44-observability)
  - [4.5 Reliability](#45-reliability)
  - [4.6 Deployment](#46-deployment)
- [5. External System Integrations](#5-external-system-integrations)
- [6. Constraints](#6-constraints)

---

## 1. Purpose and Scope

### 1.1 Purpose

This document defines the business-level requirements for rewriting cbscore from Python to Rust. It specifies WHAT the system must do (behavior, constraints, acceptance criteria) without prescribing HOW (no implementation details, no Rust code).

### 1.2 Scope

cbscore is the core build automation library for CBS (Ceph Build System). It automates the lifecycle from source code through signed container image deployment:

- Configuration of build environments
- Version descriptor creation and management
- RPM package building, signing, and distribution
- Container image creation, registry push, and cryptographic signing
- S3 artifact management
- HashiCorp Vault secrets integration

The system is consumed by three Python packages (`cbsd`, `cbsdcore`, `cbc`) and provides a CLI tool (`cbsbuild`). The rewrite must preserve all existing behavior while providing Rust implementations accessible through Python FFI bindings.

### 1.3 Definitions

| Term | Meaning |
|------|---------|
| **Version descriptor** | JSON file declaring what to build: components, git refs, image coordinates, sign-off |
| **Release descriptor** | JSON file in S3 describing a published build: architecture, components, artifact locations |
| **Core component** | A buildable unit (e.g., Ceph) defined by a `cbs.component.yaml` file |
| **Transit signing** | Container image signing using HashiCorp Vault's Transit secrets engine via cosign |
| **Runner** | The Podman container that executes the build pipeline in isolation |

---

## 2. Stakeholders

| Stakeholder | Role | Primary capabilities used |
|-------------|------|---------------------------|
| Build engineers | Create releases manually via CLI | `versions create`, `config init`, `build` |
| CI/CD pipelines | Automated build execution | `versions create`, `build` (batch mode) |
| CBS daemon (cbsd) | Programmatic build scheduling | `runner()`, `version_create_helper()`, `load_components()` via Python FFI |
| CBS client (cbc) | Developer CLI for version queries | `get_version_type()`, `parse_component_refs()` via Python FFI |
| CBS daemon core (cbsdcore) | Data model sharing | `VersionType`, `CESError` via Python FFI |
| Container release tool (crt) | Release management | `parse_version()` via Python FFI |
| Operations teams | Environment setup | `config init`, `config init-vault` |

---

## 3. Functional Requirements

### 3.1 Configuration Management

#### SRS-0010: Interactive Configuration Wizard

The system SHALL provide an interactive wizard (`cbsbuild config init`) that generates a complete build configuration file (`cbs-build.config.yaml`).

**Acceptance criteria:**
- Prompts the user for: component paths, scratch directories, ccache path, vault config path, S3 storage settings, registry URL, signing key names, secrets file paths
- Previews the assembled YAML before writing
- Confirms before overwriting an existing file
- Enforces `.yaml` file extension

---

#### SRS-0020: Batch Configuration Shortcuts

The system SHALL support non-interactive configuration via shortcut flags that pre-fill standard paths.

**Acceptance criteria:**
- `--for-systemd-install` pre-fills container-standard paths and writes to `~/.config/cbsd/<deployment>/worker/cbscore.config.yaml`
- `--for-containerized-run` pre-fills the same container-standard paths using the default config path
- Pre-filled paths: `/cbs/components`, `/cbs/scratch`, `/var/lib/containers`, `/cbs/ccache`, `/cbs/config/vault.yaml`, `/cbs/config/secrets.yaml`

---

#### SRS-0030: Vault Authentication Configuration

The system SHALL provide a standalone wizard (`cbsbuild config init-vault`) for generating Vault authentication configuration files.

**Acceptance criteria:**
- Supports three auth methods: UserPass (username + password), AppRole (role_id + secret_id), Token (single token string)
- Validates Vault URL (must be http or https, must have host)
- Rejects empty tokens with an error
- Returns immediately (no-op) if `--vault` path points to an existing file
- Asks for overwrite confirmation if the target file exists without `--vault`
- Serializes to YAML with keys: `vault-addr`, `auth-user`, `auth-approle`, or `auth-token`

---

#### SRS-0040: Configuration Loading and Storage

The system SHALL load and store configuration from YAML files with hyphenated field names matching the existing Python format.

**Acceptance criteria:**
- `Config::load(path)` reads YAML and populates all config sections
- `Config::store(path)` writes valid YAML that `Config::load` can read back identically
- Field names use kebab-case in YAML (e.g., `scratch-containers`, `vault-addr`, `auth-approle`)
- All config sections are optional (paths, storage, signing, vault, secrets, logging)
- `Config::get_secrets()` loads and merges all secrets files referenced in the config
- `Config::get_vault_config()` loads vault configuration from the referenced path

---

### 3.2 Version Management

#### SRS-0050: Version String Parsing

The system SHALL parse version strings matching the pattern `[prefix-][v]M[.m[.p]][-suffix]` where M, m, p are numeric.

**Acceptance criteria:**
- `"ces-v19.2.1-rc1"` parses to: prefix=ces, major=19, minor=2, patch=1, suffix=rc1
- `"v99.99"` parses to: prefix=none, major=99, minor=99, patch=none, suffix=none
- `"99"` parses to: prefix=none, major=99, minor=none, patch=none, suffix=none
- Invalid inputs (e.g., `"ces-v"`, `"abc"`, `""`) return an error
- All 33 parse test cases and 19 normalize test cases from the Python codebase pass

---

#### SRS-0060: Version Normalization

The system SHALL normalize version strings by ensuring the `v` prefix is present and rejecting versions without at least major.minor components.

**Acceptance criteria:**
- `"ces-99.99.1-asd"` normalizes to `"ces-v99.99.1-asd"` (adds `v`)
- `"ces-v99.99.1"` normalizes to `"ces-v99.99.1"` (unchanged)
- `"ces-v99"` is rejected (missing minor)
- `"99"` is rejected (missing minor)

---

#### SRS-0070: Version Descriptor Creation

The system SHALL create version descriptor JSON files declaring what to build, including components, git refs, image coordinates, and sign-off information.

**Acceptance criteria:**
- Accepts a version string, version type (dev/release/test/ci), component refs (`NAME@VERSION`), component paths, distro, EL version, registry, and image name
- Validates that all referenced components exist in the component paths
- Applies URI overrides for component git repositories
- Captures the current git user (name + email) as the sign-off author
- Generates a human-readable version title (e.g., "Release Development CES version 24.11.0 (GA 1)")
- Writes JSON to `<output-dir>/<type>/<version>.json`
- Fails if the output file already exists (no overwrites)
- Warns (non-fatal) if no matching image descriptor exists

---

#### SRS-0080: Component Definition Loading

The system SHALL load core component definitions from `cbs.component.yaml` files in specified directories.

**Acceptance criteria:**
- Scans directories recursively for `cbs.component.yaml` files
- Each component has: name, git repository location (URL + optional path), local build location
- Components are identified by directory name
- Multiple component paths can be specified (merged)

---

#### SRS-0090: Version Type Classification

The system SHALL classify version strings into types: release, dev, test, ci.

**Acceptance criteria:**
- `"release"` maps to Release type
- `"dev"` maps to Dev type
- `"test"` maps to Test type
- `"ci"` maps to CI type
- Unknown type names return an error

---

### 3.3 Release Discovery

#### SRS-0100: Release Listing from S3

The system SHALL list published releases from an S3-compatible object store.

**Acceptance criteria:**
- Queries the configured S3 releases bucket for `.json` descriptor files
- Displays version names in default mode
- In verbose mode (`-v`), displays per-architecture details: build type, OS version, components with versions, SHA1s, repo URLs, and artifact locations
- Malformed JSON entries are skipped with a warning (not a crash)
- `--from` overrides the S3 URL from config

---

### 3.4 Build Execution (Host-side)

#### SRS-0110: Containerized Build Launch

The system SHALL launch a containerized build pipeline (`cbsbuild build`) that prepares the host environment and delegates build execution to an isolated Podman container.

**Acceptance criteria:**
- Loads configuration, validates descriptor file exists
- Validates entrypoint script (exists, regular file, executable, not symlink)
- Validates log file path (must not already exist, creates parent directories)
- Applies signing overrides from `--sign-with-gpg-id` and `--sign-with-transit` flags
- Exports secrets to a temporary file mounted into the container
- Launches Podman with correct volume mounts (descriptor, cbscore, entrypoint, config, secrets, vault, scratch, containers storage, components, ccache, logs)
- Podman runs with: `--security-opt label=disable`, `--security-opt seccomp=unconfined` (if applicable), `--device /dev/fuse`, host networking

---

#### SRS-0120: Graceful Cancellation

The system SHALL handle Ctrl+C gracefully during builds, stopping the container and cleaning up resources.

**Acceptance criteria:**
- Ctrl+C triggers a cancellation token
- The running Podman container is stopped via `podman stop`
- Temporary files (secrets, config, components dir) are cleaned up
- No orphaned Podman containers remain after cancellation

---

#### SRS-0130: Build Timeout

The system SHALL enforce a configurable timeout on the build process.

**Acceptance criteria:**
- Default timeout: 14,400 seconds (4 hours)
- Overridable via `--timeout` CLI flag
- On timeout expiry, the container is stopped and an error is reported

---

### 3.5 Build Orchestration (Container-side)

#### SRS-0140: Three-Level Artifact Caching

The system SHALL check for existing artifacts before each build stage to avoid redundant work.

**Acceptance criteria:**
- **Level 1 (Image):** If container image already exists in the registry (`skopeo inspect`), skip the entire build
- **Level 2 (Release):** If a release descriptor exists in S3 for this version+architecture, skip component building and proceed to container build
- **Level 3 (Component):** If individual component RPMs exist in S3 for the correct version/architecture/OS, reuse them
- `--force` bypasses Level 2 and 3 caches (not Level 1)

---

#### SRS-0150: RPM Package Building

The system SHALL build RPM packages from component source code.

**Acceptance criteria:**
- Executes component-provided build scripts in isolated topdir structures
- Supports parallel builds across components
- Supports ccache integration if configured
- `--skip-build` flag skips RPM compilation entirely
- Build failures for any component halt the pipeline with a clear error

---

#### SRS-0160: RPM Signing

The system SHALL sign RPM packages with GPG if a signing key is configured.

**Acceptance criteria:**
- Signs all RPMs recursively in the build output directory
- Uses GPG key loaded from Vault or local files
- Uses `rpm --addsign` with loopback pinentry mode (non-interactive)
- GPG keyring created in a temporary directory, deleted after use
- Signing is skipped (with warning) if no GPG key is configured

---

#### SRS-0170: S3 Artifact Upload

The system SHALL upload build artifacts (RPMs, release descriptors) to S3.

**Acceptance criteria:**
- Uploads RPMs to the configured S3 artifacts bucket under component-specific paths
- Uploads release descriptor JSON to the releases bucket
- Supports public ACL on uploaded objects
- Upload failures report the S3 location and error details

---

#### SRS-0180: Container Image Building

The system SHALL build container images using Buildah from the built RPM packages.

**Acceptance criteria:**
- Creates a container from the configured base distro image (default: `rockylinux:9`)
- Applies PRE scripts (keys, repos, packages), installs component packages, applies POST scripts, applies CONFIG (env vars, labels, annotations)
- Template variable substitution in container YAML files (7 known variables: version, el, git_ref, git_sha1, git_repo_url, component_name, distro)
- Commits the container image with `--squash`
- Pushes to the configured registry with authentication

---

#### SRS-0190: Container Image Signing

The system SHALL sign container images using cosign with Vault Transit if transit signing is configured.

**Acceptance criteria:**
- Signs the image digest (not tag) using `cosign sign --key=hashivault://<key>`
- Passes VAULT_ADDR, VAULT_TOKEN, and TRANSIT_SECRET_ENGINE_PATH as environment variables
- Uploads the signature to the image registry
- Signing is skipped (with warning) if no transit key is configured
- Signing failures are reported as errors (build is not considered successful)

---

### 3.6 Python Interoperability

#### SRS-0200: Python FFI Bindings

The system SHALL provide Python bindings (via PyO3) that allow existing Python consumers to call Rust implementations without code changes.

**Acceptance criteria:**
- `cbsd` can import and call: `runner()`, `stop()`, `gen_run_name()`, `version_create_helper()`, `load_components()`, `Config`, `VersionDescriptor`, `VersionType`
- `cbc` can import and call: `set_debug_logging()`, `get_version_type()`, `parse_component_refs()`, `VersionType`, `CESError`, `VersionError`
- `cbsdcore` can import: `VersionType`, `CESError`
- `crt` can import: `parse_version()`
- All imports use existing module paths (e.g., `from cbscore.versions.utils import parse_version`)

---

#### SRS-0210: Exception Hierarchy Preservation

The system SHALL map Rust error types to Python exception classes matching the existing hierarchy.

**Acceptance criteria:**
- `CESError` is the base exception class
- `ConfigError`, `VersionError`, `MalformedVersionError`, `NoSuchVersionError`, `RunnerError` are subclasses of `CESError`
- `raise CESError("test")` works from Python
- `except MalformedVersionError` catches malformed version errors from Rust

---

#### SRS-0220: Pydantic Model Compatibility

The system SHALL ensure `VersionDescriptor` is usable as a Pydantic model field in `cbsd`'s `WorkerBuildEntry`.

**Acceptance criteria:**
- `VersionDescriptor` implements `__get_pydantic_core_schema__` for Pydantic V2 integration
- `model_dump_json()` produces valid JSON
- Round-trip: construct in Python, serialize to JSON, deserialize back — fields preserved

---

#### SRS-0230: Async Runner Bridge

The system SHALL expose the `runner()` function as an async Python function callable from `asyncio` event loops.

**Acceptance criteria:**
- `await runner(desc_path, cbscore_path, config, opts)` works from Python async code
- Log callbacks fire during execution (not batched until completion)
- GIL is released during long-running Rust operations
- Cancellation via Python (e.g., `task.cancel()`) propagates to the Rust side

---

## 4. Non-Functional Requirements

### 4.1 Backward Compatibility

#### SRS-0240: CLI Compatibility

The system SHALL preserve all existing CLI command signatures, argument names, defaults, and help text.

**Acceptance criteria:**
- `cbsbuild --help` output is functionally equivalent to the Python version
- All subcommand help texts match (verified via snapshot tests)
- `--tls-verify` accepts `0/1/true/false/True/False` (BoolishValueParser)
- No new required arguments on existing commands
- Default values match Python defaults

---

#### SRS-0250: Configuration File Compatibility

The system SHALL read and write configuration files identical to those produced by the Python implementation.

**Acceptance criteria:**
- Existing `cbs-build.config.yaml` files parse without errors
- Existing `cbs-build.vault.yaml` files parse without errors
- Existing secrets YAML files parse without errors
- Written YAML files are readable by the Python implementation (during transition period)

---

#### SRS-0260: JSON Format Compatibility

The system SHALL produce and consume JSON files (version descriptors, release descriptors) identical to the Python format.

**Acceptance criteria:**
- Version descriptor JSON field names match: `version`, `title`, `signed_off_by`, `image`, `components`, `distro`, `el_version`
- Release descriptor JSON schema matches existing S3 artifacts
- Existing JSON files produced by Python parse correctly in Rust

---

#### SRS-0270: Python Import Path Preservation

The system SHALL maintain all existing Python import paths during the transition period.

**Acceptance criteria:**
- Python shim modules re-export Rust implementations at original paths
- `from cbscore.config import Config, ConfigError` works
- `from cbscore.versions.utils import parse_version, VersionType` works
- `from cbscore.runner import runner, stop, gen_run_name` works
- `from cbscore.errors import CESError, MalformedVersionError` works
- `from cbscore.logger import set_debug_logging` works

---

### 4.2 Security

#### SRS-0280: Secret Masking in Logs

The system SHALL never log secret values (passwords, tokens, private keys, API keys) in any log output at any level.

**Acceptance criteria:**
- Passwords appear as `****` in log output
- SSH keys, GPG passphrases, Vault tokens, S3 secret IDs never appear in logs
- Command lines logged with `--passphrase <value>` patterns masked
- Environment variables containing secrets are not logged
- Verified by grep over trace-level log output of a full build

---

#### SRS-0290: Temporary Credential Cleanup

The system SHALL clean up temporary credential files (GPG keyrings, SSH keys, exported secrets) after use, even on error paths.

**Acceptance criteria:**
- GPG keyring temporary directory is deleted after signing (or on failure)
- SSH key files for git operations are deleted after the git operation completes
- Exported secrets files (mounted into Podman) are deleted after build completes or fails
- Cleanup occurs via RAII (Drop) as a safety net; explicit cleanup is preferred

---

#### SRS-0300: Vault Token Handling

The system SHALL obtain Vault tokens on-demand and not persist them to disk.

**Acceptance criteria:**
- Vault authentication occurs during `SecretsMgr` initialization
- Vault tokens are held in memory only
- Vault tokens are not logged, serialized, or written to any file
- Connection validation occurs immediately after authentication

---

#### SRS-0310: Container Security Options

The system SHALL apply appropriate security options when running Podman containers for builds.

**Acceptance criteria:**
- SELinux labeling disabled (`--security-opt label=disable`)
- Seccomp unconfined when required (`--security-opt seccomp=unconfined`)
- FUSE device mounted (`--device /dev/fuse:/dev/fuse:rw`) for Buildah overlay mounts
- No privileged mode — uses specific capabilities instead

---

### 4.3 Performance

#### SRS-0320: Parallel Component Builds

The system SHALL build RPM packages for independent components in parallel.

**Acceptance criteria:**
- Multiple components build concurrently (not sequentially)
- Build parallelism scales with available CPU cores
- A failure in one component does not block other in-progress builds (but overall pipeline fails)

---

#### SRS-0330: Parallel S3 Operations

The system SHALL perform S3 downloads in parallel when listing releases.

**Acceptance criteria:**
- Multiple release descriptor JSON files are fetched concurrently
- List + download pipeline is faster than sequential (measured)

---

#### SRS-0340: Build Timeout Compliance

The system SHALL complete or fail within the configured timeout.

**Acceptance criteria:**
- Default timeout: 4 hours
- Timeout is enforced at the Podman container level
- On timeout, container is killed and resources are cleaned up
- Timeout error message includes elapsed time

---

### 4.4 Observability

#### SRS-0350: Structured Logging

The system SHALL produce structured log output at configurable levels.

**Acceptance criteria:**
- Log levels: ERROR, WARN, INFO, DEBUG, TRACE
- Per-module filtering via `RUST_LOG` environment variable
- INFO level shows key milestones: build start/end, cache hits/misses, image push
- DEBUG level shows component details, S3 operations, subprocess commands
- TRACE level shows function entry/exit with arguments and durations

---

#### SRS-0360: Python Logging Integration

The system SHALL bridge Rust log events to Python's logging module when running under PyO3.

**Acceptance criteria:**
- Rust tracing events appear in Python's `logging` hierarchy under `cbscore.*`
- `set_debug_logging()` switches log level to DEBUG at runtime
- Log level changes propagate to both Rust and Python

---

#### SRS-0370: Subprocess Output Streaming

The system SHALL stream subprocess stdout/stderr in real-time via callbacks, not buffer until completion.

**Acceptance criteria:**
- Build output is visible to the user as it's produced
- Log file writing (if configured) captures output in real-time
- Callbacks receive structured events: Started, Stdout(line), Stderr(line), Finished(exit_code)

---

#### SRS-0380: Build Parameter Logging

The system SHALL log all build parameters at startup for debugging.

**Acceptance criteria:**
- Descriptor path, config paths, scratch paths, signing config, storage config, timeouts are logged at DEBUG level
- Secrets values are NOT logged (only key names, not values)
- Container name, volume mounts, and security options are logged

---

### 4.5 Reliability

#### SRS-0390: Idempotent Build Operations

The system SHALL produce identical results when run with the same inputs.

**Acceptance criteria:**
- Re-running a build with the same descriptor and configuration produces the same artifacts
- Three-level caching ensures no redundant work on re-runs
- No dependency on wall-clock time, random values, or uncontrolled external state

---

#### SRS-0400: Graceful Error Recovery

The system SHALL handle transient failures gracefully and provide actionable error messages.

**Acceptance criteria:**
- S3 download 404 is handled as "not found" (not a crash)
- Malformed JSON entries in S3 are skipped with a warning
- Component loading skips missing/invalid components with a warning
- All error messages include: what failed, where (file/URL/path), and the underlying cause

---

#### SRS-0410: Resource Cleanup on Failure

The system SHALL clean up all resources (containers, temp files, network connections) even when the pipeline fails.

**Acceptance criteria:**
- Podman containers are stopped/removed on failure
- Temporary directories and files are deleted on failure
- No resource leaks after any error path (verified by post-test inspection)

---

### 4.6 Deployment

#### SRS-0420: Dual Installation Methods

The system SHALL be installable via both Python (`pip install` / `maturin develop`) and Rust (`cargo install`) toolchains.

**Acceptance criteria:**
- `maturin develop` builds the Python wheel with Rust native extensions
- `uv sync --all-packages` resolves the workspace correctly
- `cargo build --release` produces a standalone `cbsbuild` binary
- Both installation methods produce a functional `cbsbuild` command

---

#### SRS-0430: Container Deployability

The system SHALL run inside Podman containers for the container-side build pipeline.

**Acceptance criteria:**
- The `cbsbuild` binary can be installed inside a container via the entrypoint script
- All paths are correctly remapped to `/runner/...` inside the container
- The recursive call (`cbsbuild build` → container → `cbsbuild runner build`) works end-to-end

---

## 5. External System Integrations

| System | Protocol | Operations | Auth |
|--------|----------|-----------|------|
| **HashiCorp Vault** | HTTP/HTTPS | KVv2 read, Transit sign | AppRole, UserPass, Token |
| **S3 (Ceph RGW)** | HTTP/HTTPS (S3 API) | Upload, download, list objects | Access ID + Secret ID |
| **Container Registry** | HTTP/HTTPS (OCI) | Push, pull, inspect, list tags | Username + Password |
| **Podman** | Local subprocess | Run, stop, inspect containers | N/A (local) |
| **Buildah** | Local subprocess | Create, configure, commit images | Registry creds for push |
| **Skopeo** | Local subprocess | Copy, inspect, list tags | Registry creds |
| **Cosign** | Local subprocess + Vault | Sign image digests | Vault Transit key |
| **Git** | SSH / HTTPS | Clone, checkout, fetch, config | SSH keys or HTTPS creds |
| **GPG** | Local subprocess | RPM signing | Private key + passphrase |
| **rpmbuild** | Local subprocess | Build RPMs | N/A |
| **DNF** | Network (YUM repos) | Install build dependencies | N/A |

---

## 6. Constraints

### 6.1 Technology Constraints

- Rust 2024 edition
- Tokio async runtime
- PyO3 for Python FFI (Maturin build system)
- Must coexist with existing Python code during transition (Phase 10 removes Python shims)

### 6.2 Operational Constraints

- Single architecture supported: x86_64 (ArchType enum extensible for future)
- Single OS supported: EL9 / Rocky Linux 9 (extensible)
- Build containers require: FUSE device, disabled SELinux labeling, host networking

### 6.3 Data Format Constraints

- Config: YAML with kebab-case field names
- Version descriptors: JSON with snake_case field names
- Release descriptors: JSON with snake_case field names
- Secrets: YAML with discriminated unions (dispatched on `creds` and `type` fields)
- Component definitions: YAML (`cbs.component.yaml`)
