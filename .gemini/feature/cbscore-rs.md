# Design Document: cbscore-rs

## 1. Project Overview

`cbscore-rs` is a Rust-based rewrite of the `cbscore` library and `cbsbuild` CLI tool. The goal is to provide a high-performance, type-safe, and memory-safe implementation that maintains 100% functional parity with the original Python version.

### 1.1 Goals
- **Parity**: Support all existing `cbsbuild` commands and configurations.
- **Performance**: Leverage Rust's concurrency model (Tokio) for faster parallel builds and uploads.
- **Reliability**: Use Rust's strong type system and error handling to reduce runtime errors.
- **Single Binary**: Ship as a static binary for easier deployment in container environments.
- **Library Support**: Expose a clean, public API so the core build logic can be included as a dependency in other Rust projects (e.g., future `cbsd` rewrites).

## 2. Architecture & Module Mapping

The project will be structured as a library (`cbscore-rs`) with a CLI wrapper (`cbsbuild-rs`).

| Python Module | Rust Module / Crate | Description |
|---|---|---|
| `cbscore.config` | `config` | `serde` + `serde_yaml` for config management. |
| `cbscore.utils.secrets` | `secrets` | Vault integration (via `vaultrs`) and secret merging. |
| `cbscore.utils.git` | `git` | Git binary wrapper using `tokio::process` or `git2`. |
| `cbscore.utils.podman` | `podman` | Podman binary wrapper or `bollard` API. |
| `cbscore.utils.buildah` | `buildah` | Buildah binary wrapper. |
| `cbscore.utils.s3` | `s3` | S3 integration using `aws-sdk-s3`. |
| `cbscore.builder` | `builder` | Core build orchestration logic. |
| `cbscore.releases` | `releases` | Release descriptor models and S3 logic. |
| `cbscore.versions` | `versions` | Version descriptor models and creation logic. |
| `cbscore.cmds` | `cli` | CLI implementation using `clap`. |

## 3. Data Models (Serde)

All Python `pydantic` models will be ported to Rust `structs` with `serde` attributes.

### 3.1 Version Descriptor
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionDescriptor {
    pub version: String,
    pub title: String,
    pub signed_off_by: SignedOffBy,
    pub image: VersionImage,
    pub components: Vec<VersionComponent>,
    pub distro: String,
    pub el_version: i32,
}
```

### 3.2 Config
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub paths: PathsConfig,
    pub storage: Option<StorageConfig>,
    pub signing: Option<SigningConfig>,
    pub logging: Option<LoggingConfig>,
    pub secrets: Vec<PathBuf>,
    pub vault: Option<PathBuf>,
}
```

## 4. Key Implementation Details

### 4.1 Concurrency Model
The project will use the **Tokio** runtime. Parallel tasks (like cloning components or uploading RPMs) will be managed using `tokio::task::JoinSet` or `futures::stream::FuturesUnordered` to maintain parity with Python's `asyncio.TaskGroup`.

### 4.2 Error Handling
A custom `Error` enum using `thiserror` will be used to unify errors from Git, S3, Vault, and IO operations.

### 4.3 Tool Integration Strategies

We evaluate two primary approaches for interacting with external tools (`git`, `podman`, `buildah`):

#### Approach 1: Binary Wrapping (NO-FFI)
This mirrors the current Python implementation.
- **Evaluation**: Standard Rust `Command` or the `duct` crate can be used to build and execute shell commands. This is highly reliable in rootless/containerized environments where the environment variables and binary versions are strictly controlled.
- **Pros**: No complex C dependencies (libgit2, etc.); easier static linking (musl); 1:1 behavior with manual CLI usage.
- **Cons**: Overhead of process spawning; requires parsing STDOUT/STDERR strings.

#### Approach 2: API/FFI Integration (FFI)
- **Evaluation**: Uses dedicated libraries like `git2` (libgit2 bindings) for Git and `bollard` for Podman/Docker.
- **Pros**: Better performance (no fork/exec); structured data instead of string parsing.
- **Cons**: `git2` is difficult to cross-compile statically; `bollard` requires a running Podman/Docker socket service, which may not be available inside the build container; Buildah has no stable FFI for its core operations.

**Selected Strategy**: **NO-FFI** for `podman` and `buildah` to ensure compatibility with rootless builds. **Hybrid** for `git` (using `git2` for read operations and wrapping the binary for complex writes if needed).

## 5. CLI Design & Runner Strategy

The `cbsbuild-rs` CLI will use `clap` with subcommands mirroring the current structure.

### 5.1 Task Runner Implementation

The task runner currently executes inside a container via `cbscore-entrypoint.sh`. 

#### Approach A: Unified Binary (NO-CUSTOM-RUNNER)
The main `cbsbuild-rs` binary includes the `runner build` subcommand.
- **Implementation**: The container entrypoint script is simplified. Instead of installing `uv` and the Python package, it simply invokes the pre-copied `cbsbuild-rs` binary.

#### Approach B: Dedicated Runner (CUSTOM-RUNNER)
Create a separate, minimal executable (e.g., `cbs-runner`) specifically for execution inside the build container.
- **Implementation**: This binary contains only the logic required to build components and sign RPMs. It results in a smaller binary size and faster container startup.
- **Pros**: Clear separation of concerns; minimal attack surface in build containers.

### 5.2 Container Entrypoint Achievement
By using a static Rust binary, the `cbscore-entrypoint.sh` can be reduced to:
```bash
#!/bin/bash
export PATH="/runner/bin:$PATH"
# No uv/pip/sync needed!
cbsbuild-rs --config "/runner/cbs-build.config.yaml" runner build "$@"
```

## 6. Implementation Phases

1.  **Phase 1: Foundation (Models & Config)**: Implement `VersionDescriptor`, `ReleaseDesc`, and `Config` models with YAML/JSON support.
2.  **Phase 2: Utilities (Git, S3, Vault)**: Port the utility wrappers.
3.  **Phase 3: Builder Core**: Port the `Builder` logic, including component preparation and build orchestration.
4.  **Phase 4: CLI Wrapper**: Implement the `clap` CLI and integrate the core logic.
5.  **Phase 5: Container Runner**: Port the `podman_run` logic for containerized builds and optimize the entrypoint.
6.  **Phase 6: Verification & Testing**: Run side-by-side tests with the Python version.

## 7. Migration Plan

Initially, `cbscore-rs` will exist alongside `cbscore`. Users can opt-in to using the Rust version for performance testing. Once parity and stability are confirmed, the Python version can be deprecated.

## 8. Development & VCS Guidelines

- **Git as VCS**: Every commit must be buildable and pass all existing tests.
- **Signed Commits**: All commits must be GPG signed.
- **Commit Message Template**:
```text
cbscore-rs: <summary>

<detailed-reason-why-changes-are-needed>

Co-authored-by: <name> <email>
Signed-off-by: <name> <email>
```
