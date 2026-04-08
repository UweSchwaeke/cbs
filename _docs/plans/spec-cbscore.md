# cbscore Specification & Architectural Analysis

This document provides a deep dive into the `cbscore` package, based on its current implementation (v1.0.1). It defines what the package does, its architecture, and how it is utilized within the CBS ecosystem.

## 1. Project Overview: What `cbscore` Does
`cbscore` is the foundational library and engine for the Clyso Build System (CBS). Its primary purpose is to automate the **end-to-end lifecycle of building software components (specifically Ceph) into signed container images**.

It handles:
*   **Isolation**: Orchestrates build environments using Podman containers to ensure clean, reproducible builds.
*   **Source Management**: Clones git repositories, manages worktrees, and applies version-specific patches.
*   **Artifact Generation**: Executes build scripts to produce RPMs.
*   **Security & Integrity**: Signs RPMs with GPG and container images with `cosign` (via HashiCorp Vault Transit).
*   **Distribution**: Uploads RPMs to S3 (creating YUM repositories on the fly) and pushes container images to registries.
*   **Metadata Tracking**: Maintains JSON descriptors for versions and releases to track what was built, by whom, and with which dependencies.

---

## 2. Architecture & Internal Structure

The package is organized into several functional domains:

### A. The Runner (`cbscore.runner`)
This is the entry point for a "top-level" build.
*   **The Orchestrator**: It prepares a "scratch" environment, mounts necessary secrets and source paths, and spawns a **Podman container**.
*   **Recursive Execution**: Inside this container, it invokes `cbsbuild runner build`, which uses the same `cbscore` library to perform the actual compilation. This ensures the build happens in a controlled, isolated OS environment (e.g., Rocky Linux 9) regardless of the host OS.

### B. The Builder (`cbscore.builder`)
The core logic that runs *inside* the isolated environment.
*   **`builder.py`**: Coordinates the sequence: `prepare_builder` (dnf installs) -> `prepare_components` (git/patches) -> `build_rpms` -> `sign_rpms` -> `upload_rpms`.
*   **`prepare.py`**: A state manager handling git cloning and a hierarchical patching system (applying patches based on major/minor/patch version matches).
*   **`rpmbuild.py`**: Invokes the `build_rpms.sh` scripts defined in component directories.

### C. Container Engine (`cbscore.containers`)
Instead of using standard Dockerfiles, `cbscore` uses a declarative `container.yaml` system and **Buildah**.
*   **`build.py`**: Uses the Python `buildah` wrapper to create a new container, install built RPMs, and apply configurations (ENV, labels, annotations).
*   **`component.py`**: Parses component-specific container requirements, allowing components to define their own `PRE` (repo setup) and `POST` (cleanup/config) scripts.

### D. Utilities & Integration (`cbscore.utils`)
*   **Secrets Manager (`utils.secrets`)**: Abstracts secret retrieval from local YAML files or **HashiCorp Vault**. It handles Git credentials (SSH/HTTPS), S3 keys, and Registry auth.
*   **Tool Wrappers**: Async wrappers around CLI tools (`git`, `rpm`, `podman`, `buildah`, `skopeo`, `cosign`).

---

## 3. Usage & Entrypoints

### CLI Tool (`cbsbuild`)
*   `cbsbuild config init`: Interactive environment setup.
*   `cbsbuild versions create`: Generates a `VersionDescriptor` JSON defining a release.
*   `cbsbuild build <descriptor>.json`: Starts the entire containerized build process.

### Library Usage
Other packages (like `cbsd`, the daemon) import `cbscore` to:
*   Validate version descriptors.
*   Programmatically trigger the `runner` for scheduled builds.
*   Access the `SecretsMgr` for cross-system credential handling.

---

## 4. Typical Build Workflow
1.  **Input**: A `VersionDescriptor` (JSON) and a `cbscore.config.yaml`.
2.  **Runner**: Starts a Podman container.
3.  **Bootstrap**: Inside the container, installs build-essential tools.
4.  **Fetch**: Clones component repos and applies hierarchical patches.
5.  **Build**: Runs component-specific scripts to create RPMs.
6.  **Sign**: Signs RPMs with GPG.
7.  **Publish**: Pushes RPMs to S3 and updates YUM metadata.
8.  **Image Build**: Uses Buildah to create an image, installs the RPMs from S3, and pushes to the Registry.
9.  **Finalize**: Signs the container image with Cosign via Vault Transit.

---

## 5. Debuggability & Observability
A core requirement for the build system is the ability to inspect, trace, and debug every stage of the lifecycle.

*   **Step-Level Transparency**: Every distinct action (e.g., applying a specific patch, running a dnf install, signing a single RPM) must be logged with its full context:
    *   Exact CLI command executed.
    *   Environment variables used.
    *   Working directory state.
    *   Streaming stdout and stderr.
*   **Container Persistence**: On failure, the system should optionally preserve the build container and the scratch environment. This allows developers to manually enter the environment (`podman exec`) to investigate build failures in situ.
*   **State Checkpointing**: The architecture should allow for resuming builds from specific steps (e.g., skip RPM build if artifacts already exist in the scratch dir) to facilitate iterative debugging without full restarts.
*   **Structured Logging**: Transitioning to structured (JSON) logging ensures that the `cbsd` daemon can parse and report fine-grained progress and errors to the `cbc` client.

## 6. Architectural Considerations for Rewrite
*   **Self-Referential Execution**: The pattern of the runner calling itself inside a container provides isolation but complicates logging and debugging. The rewrite should prioritize a "transparent proxy" approach for logs.
*   **CLI Reliance**: Heavy dependency on wrapping external CLI tools. A rewrite could explore native libraries (e.g., `GitPython`, `pygit2`) for certain tasks, while maintaining CLI wrappers for system-level tools like `rpmbuild`.
*   **Async Consistency**: The codebase is primarily `asyncio`, but often bridges to synchronous subprocesses. Consistent async I/O handling is critical.
*   **Extensibility**: Several `FIXME` notes indicate hardcoded architectures (x86_64) and OS versions (el9). A rewrite should prioritize a plugin or configuration-based approach to support multi-arch and multi-OS builds.
*   **Granular Debugging**: The rewrite must treat "Debuggability" as a first-class citizen. This means providing hooks for interactive shells on failure and ensuring that no step is an "opaque box."
