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

The project will be structured as a multi-binary workspace:
- `cbscore`: The core library crate.
- `cbsbuild`: The main CLI tool.
- `cbs-runner`: A specialized, minimal binary for execution inside build containers.

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

All Python `pydantic` models will be ported to Rust `structs` with `serde` attributes.

## 4. Key Implementation Details

### 4.1 Concurrency Model
The project will use the **Tokio** runtime. Parallel tasks (like cloning components or uploading RPMs) will be managed using `tokio::task::JoinSet` to maintain parity with Python's `asyncio.TaskGroup`.

### 4.2 Error Handling
We will use a custom error type `CBSError` defined via `thiserror`. Each module will expose its own error variants to ensure "rustic" error propagation.

```rust
#[derive(thiserror::Error, Debug)]
pub enum CBSError {
    #[error("Config error: {0}")]
    Config(String),
    #[error("Tool execution error: {0}")]
    Tool {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
        reason: String,
    },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    // ...
}
```

### 4.3 Tool Integration Strategy (NO-FFI)

We will use a **Pure NO-FFI** approach for all external tools.
- **Mechanism**: Use `tokio::process::Command` to invoke binaries.
- **Handling Huge Output**: We will **never** use `.output()`, which collects the entire output into memory. Instead, we will use `.spawn()` and asynchronously stream `stdout`/`stderr` using `tokio::io::BufReader`.
- **Stream Redirection**: Output can be piped to the log file, the terminal, or a string buffer with a hard size limit (e.g., last 100 lines) to prevent memory exhaustion during huge builds.

### 4.4 Output Analysis & Error Mapping
To understand *why* a tool failed or to track its progress:
- **Output Observers**: We will implement a `StreamParser` trait that analyzes the `stdout`/`stderr` streams in real-time.
- **Regex Parsing**: Module-specific parsers (e.g., `GitParser`, `RpmBuildParser`) will look for specific error patterns or status updates.
- **Failure Reason**: When a process exits with a non-zero code, the `CBSError::Tool` variant will be populated with the captured `stderr` context and a "reason" derived from the parsed output (e.g., "Authentication failed", "Missing dependency: libssl").

## 5. CLI Design & Runner Strategy

### 5.1 Custom Runner (CUSTOM-RUNNER)
We will implement the **CUSTOM-RUNNER** approach with a dedicated `cbs-runner` binary.

### 5.2 Container Entrypoint Achievement
We will implement and test two distinct approaches for the container entrypoint:

#### Approach 1: Pure Rust (-PURE-RUST)
The `cbs-runner` binary handles all environment setup directly.
- **Logic**: Creates `/runner/bin`, initializes `PATH`, and manages scratch space via standard library calls (`std::fs`, `std::env`).
- **Benefit**: Zero external shell dependencies; fastest startup.

#### Approach 2: Bash Wrapper (-BASH)
A simplified version of the original shell script.
- **Logic**: A minimal `.sh` script that sets up the environment and then `exec`s into `cbs-runner`.
- **Benefit**: Easier debugging for operators accustomed to shell-based entrypoints.

## 6. Testing Strategy

### 6.1 Unit Testing
- Every `.rs` file will contain a `mod tests` block.
- **Mocking**: Use `mockall` to isolate logic from external binaries and APIs.

### 6.2 Integration Testing
- **Podman Only**: All containerized tests will use Podman (no Docker).
- **Gitea**: Use `testcontainers-rs` to run Gitea for verifying Git clone/patch operations.
- **LocalStack & Vault**: Use `testcontainers-rs` for S3 and secret retrieval verification.

## 7. Implementation Phases

1.  **Phase 1: Foundation (Models & Config)**: **Verification**: Unit tests for YAML/JSON.
2.  **Phase 2: Utilities (Git, S3, Vault)**: **Verification**: Integration tests with Gitea/LocalStack/Vault.
3.  **Phase 3: Builder Core**: **Verification**: Mocked tool-flow verification.
4.  **Phase 4: cbs-runner**: **Verification**: Parallel testing of `-PURE-RUST` and `-BASH` entrypoints.
5.  **Phase 5: cbsbuild CLI**: **Verification**: End-to-end local CLI tests.
6.  **Phase 6: Container Integration**: **Verification**: Full build cycle using Podman.

## 8. Development & VCS Guidelines

- **Git as VCS**: Every commit must be buildable and pass all tests.
- **Signed Commits**: All commits must be GPG signed.
- **Commit Message Template**:
```text
cbscore-rs: <summary>

<detailed-reason-why-changes-are-needed>

Co-authored-by: Gemini <gemini@google.com>
Signed-off-by: <name> <email>
```

## 9. General Engineering Rules

- **Design Patterns**: Strict adherence to **SOLID**, **DRY**, and **KISS** principles.
- **Documentation**: 
    - Every public module, struct, and method MUST be documented using `///` doc comments.
    - **Examples**: Documentation for public functions MUST include an `# Examples` section with working code snippets (doctests) where applicable.
- **Code Clarity**:
    - **Length Limit**: Functions should generally not exceed **10-15 lines**.
- **Composition over Inheritance**: Use traits and composition to build flexible abstractions.

## 10. Logging Strategy

We will use the **`tracing`** crate for structured logging.
- **Per-Module Levels**: The logging level can be configured independently for each module via the `RUST_LOG` environment variable (e.g., `RUST_LOG=cbscore::git=debug,cbscore::builder=info`).
- **Formatting**: Support for both human-readable (terminal) and JSON (container logs) output formats.
