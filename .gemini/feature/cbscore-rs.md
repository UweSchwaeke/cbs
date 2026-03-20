# Design Document: cbscore-rs

## 1. Project Overview

`cbscore-rs` is a Rust-based rewrite of the `cbscore` library and `cbsbuild` CLI tool. The goal is to provide a high-performance, type-safe, and memory-safe implementation that maintains 100% functional parity with the original Python version.

### 1.1 Goals
- **Parity**: Support all existing `cbsbuild` commands and configurations.
- **Performance**: Leverage Rust's concurrency model (Tokio) for faster parallel builds and uploads.
- **Reliability**: Use Rust's strong type system and error handling to reduce runtime errors.
- **Single Binary**: Ship as a static binary for easier deployment in container environments.
- **Library Support**: Expose a clean, public API so the core build logic can be included as a dependency in other Rust projects (e.g., future `cbsd` rewrites).
- **Python Interoperability**: Provide Python bindings for core utilities to ensure existing Python tools like `crt` can leverage the Rust implementation without a full rewrite.

## 2. Architecture & Module Mapping

The project will be structured as a multi-binary workspace:
- `cbscore`: The core library crate.
- `cbsbuild`: The main CLI tool.
- `cbs-runner`: A specialized, minimal binary for execution inside build containers.
- `cbscore-python`: Python extension module using `PyO3` and `Maturin`.

| Python Module | Rust Module / Crate | Description |
|---|---|---|
| `cbscore.config` | `cbscore::config` | `serde` + `serde_yaml` for config management. |
| `cbscore.utils.secrets` | `cbscore::secrets` | Vault integration (via `vaultrs`) and secret merging. |
| `cbscore.utils.git` | `cbscore::tools::git` | Git binary wrapper using `tokio::process`. |
| `cbscore.utils.podman` | `cbscore::tools::podman` | Podman binary wrapper using `tokio::process`. |
| `cbscore.utils.buildah` | `cbscore::tools::buildah` | Buildah binary wrapper using `tokio::process`. |
| `cbscore.utils.s3` | `cbscore::s3` | S3 integration using `aws-sdk-s3`. |
| `cbscore.builder` | `cbscore::builder` | Core build orchestration logic. |
| `cbscore.releases` | `cbscore::releases` | Release descriptor models and S3 logic. |
| `cbscore.versions` | `cbscore::versions` | Version descriptor models and creation logic. |
| `cbscore.cmds` | `cbsbuild::cli` | CLI implementation using `clap`. |

## 3. Data Models (Serde)

All Python `pydantic` models will be ported to Rust `structs` using `serde`. 

### 3.1 Secrets Parity
To maintain compatibility with existing `secrets.yaml` files, the Rust implementation will use a "fat" Enum with `#[serde(untagged)]`. This allows us to replicate the Python implementation's multi-level discriminators (e.g., `creds: vault` combined with `type: transit`) with high type safety and rigorous unit testing.

### 3.2 Path Consistency
We will use the **`camino` crate** (`Utf8PathBuf`) for all shared data models. This ensures that S3 keys and file paths are consistently UTF-8 enforced across different operating systems.

## 4. Key Implementation Details

### 4.1 Concurrency Model
The project will use the **Tokio** runtime. Parallel tasks (like cloning components or uploading RPMs) will be managed using `tokio::task::JoinSet` to maintain parity with Python's `asyncio.TaskGroup`.

### 4.2 Error Handling
We will use a custom error type `CBSError` defined via `thiserror`. Each module will expose its own error variants to ensure "rustic" error propagation.

### 4.3 Tool Integration Strategy (NO-FFI)

We will use a **Pure NO-FFI** approach for all external tools.
- **Mechanism**: Use `tokio::process::Command` to invoke binaries.
- **Handling Huge Output**: We will asynchronously stream `stdout`/`stderr` using `tokio::io::BufReader` to prevent memory exhaustion during large builds.
- **Environment Management**: A internal `CommandExecutor` will handle complex environment states (e.g., `XDG_RUNTIME_DIR`, `STORAGE_DRIVER`) required for rootless Podman-in-Podman builds.

### 4.4 Output Analysis & Error Mapping
- **Output Observers**: We will implement a `StreamParser` trait that analyzes the `stdout`/`stderr` streams in real-time.
- **Failure Reason**: When a process fails, the `CBSError::Tool` variant will be populated with a human-readable reason derived from the parsed output (e.g., "Authentication failed").

## 5. CLI Design & Runner Strategy

### 5.1 Custom Runner (CUSTOM-RUNNER)
We will implement the **CUSTOM-RUNNER** approach with a dedicated `cbs-runner` binary.
- **Signal Forwarding**: `cbs-runner` will explicitly propagate signals (like `SIGTERM`) to sub-processes (like `buildah`) to prevent orphaned containers or corrupted locks.

### 5.2 Container Entrypoint Achievement
We will implement and test two distinct approaches for the container entrypoint:
- **Approach 1: Pure Rust (-PURE-RUST)**: The binary handles all environment setup directly.
- **Approach 2: Bash Wrapper (-BASH)**: A minimal `.sh` script for legacy compatibility.

## 6. Testing Strategy

- **Unit Testing**: Every `.rs` file will contain a `mod tests` block with `mockall` for isolation.
- **Integration Testing**: All containerized tests will use **Podman** only. We will use `testcontainers-rs` to run **Gitea**, **LocalStack**, and **Vault** for verifying end-to-end flows.

## 7. Implementation Phases

1.  **Phase 1: Foundation (Models & Config)**: Implement `VersionDescriptor` and `Config` using `camino`.
2.  **Phase 2: Utilities (Git, S3, Vault)**: Port utility wrappers with `testcontainers` verification.
3.  **Phase 3: Builder Core & Python Bindings**: Port orchestration logic and `PyO3` bindings concurrently to support existing `cbsd` workers.
4.  **Phase 4: cbs-runner**: Implement specialized runner with signal forwarding.
5.  **Phase 5: cbsbuild CLI**: Implement the `clap` CLI.
6.  **Phase 6: Container Integration**: Full build cycle verification using Podman.

## 8. Development & VCS Guidelines

- **Signed Commits**: All commits must be GPG signed.
- **Commit Message Template**:
```text
cbscore-rs: <summary>

<detailed-reason-why-changes-are-needed>

Co-authored-by: Gemini <gemini@google.com>
Signed-off-by: <name> <email>
```

## 9. General Engineering Rules

- **Design Patterns**: Strict adherence to **SOLID**, **DRY**, and **KISS**.
- **Multi-arch Ready**: We will implement an `Arch` enum from the start to resolve existing Python debt regarding hardcoded `x86_64` assumptions.
- **Documentation**: Every public symbol MUST be documented with `///` and include an `# Examples` section with working doctests.
- **Code Clarity**: Functions should generally not exceed **10-15 lines**.

## 10. Logging Strategy

We will use the **`tracing`** crate. Logging levels can be configured independently per module (e.g., `RUST_LOG=cbscore::git=debug`).

## 11. Deployment Strategy

- **Worker Updates**: Remove `uv` and Python from images; replace with static `cbs-runner` binary.
- **Compose Files**: Remove Python source mounts (`./cbscore`) from `podman-compose.cbs.yaml`.

## 12. Python Interoperability

- **PyO3 & Maturin**: Critical utilities will be exposed as a native extension package.
- **CLI Wrapper**: High-level orchestration will be handled via `subprocess` calls to the `cbsbuild` binary.
