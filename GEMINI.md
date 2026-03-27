# GEMINI.md - CES Build System (CBS)

## Project Overview

The **CES Build System (CBS)** is a comprehensive toolset designed for building, managing, and releasing containers, specifically optimized for **Ceph** and other complex software components. It follows a service-oriented architecture, providing both local CLI tools and a scalable REST service for distributed builds.

### Key Technologies
- **Python 3.13+**: Primary programming language.
- **[uv](https://docs.astral.sh/uv/)**: Fast Python package manager and workspace runner.
- **[Podman](https://podman.io/) & [Buildah](https://buildah.io/)**: Rootless container build tools.
- **[FastAPI](https://fastapi.tiangolo.com/)**: REST API framework for the build service (`cbsd`).
- **[Celery](https://docs.celeryq.dev/) & [Redis](https://redis.io/)**: Distributed task queue for build orchestration.
- **[Pydantic](https://docs.pydantic.dev/)**: Data validation and configuration management.
- **[Click](https://click.palletsprojects.com/)**: Framework for building CLI tools (`cbc`, `cbsbuild`, `crt`).

### Workspace Architecture
CBS is a Python monorepo managed with `uv` workspaces:

- **`cbscore/`**: Core library and the `cbsbuild` CLI tool. Handles local builds, configuration, and artifact management (RPMs, container images).
- **`cbsd/`**: The CBS service daemon. A REST API server that schedules builds on worker nodes via Celery.
- **`cbsdcore/`**: Shared libraries and API definitions for the `cbsd` service.
- **`cbc/`**: Command-line client for interacting with the `cbsd` REST service.
- **`crt/`**: Ceph Release Tool, specifically for managing the lifecycle of Ceph releases.
- **`components/`**: YAML-based definitions for components (e.g., Ceph) and build scripts.
- **`container/`**: Dockerfiles and build scripts for containerizing CBS itself.

---

## Building and Running

### Setup
```bash
# Install dependencies and setup the virtual environment
uv sync --all-packages
```

### Development & Testing
```bash
# Linting and formatting
uv run ruff check
uv run ruff format --check

# Type checking
uv run basedpyright .

# Run tests
uv run pytest cbsd/tests/ -v
```

### Local Build Service (Podman Compose)
CBS can be run locally as a full system (server, worker, redis) using the provided compose script:
```bash
# Launch the full CBS service locally
./do-cbs-compose.sh
```

### Local CLI Usage (`cbsbuild`)
1. **Initialize configuration**:
   ```bash
   uv run cbsbuild config-init
   ```
2. **Create a version descriptor**:
   ```bash
   uv run cbsbuild versions create -c ceph@<hash> ces-v<version>
   ```
3. **Build from a descriptor**:
   ```bash
   uv run cbsbuild build ./_versions/<path_to_json>
   ```

---

## Development Conventions

### Coding Style
- **Linter**: [Ruff](https://docs.astral.sh/ruff/) is used for both linting and formatting.
- **Type Safety**: [Pyright](https://github.com/microsoft/pyright) (via `basedpyright`) is enforced for static type checking.
- **Async**: Asynchronous programming is used extensively for API and build management.

### Contribution Guidelines
- **Commits**: Follow Ceph project practices.
- **Signing**: GPG-signed commits are required.
- **DCO**: All commits must have a `Signed-off-by` line (Developer Certificate of Origin).
- **Licensing**: 
    - `cbsd` (service) is **AGPLv3**.
    - CLI tools (`cbscore`, `crt`, `cbc`) are **GPLv3**.
    - Refer to `COPYING-AGPL3` and `COPYING-GPL3` in the root.

### Infrastructure Requirements
- **Rootless Podman/Buildah**: Ensure your environment is configured for rootless container operations.
- **Storage**: A high-throughput disk is recommended for the build scratch and `ccache` directories.
