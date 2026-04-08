# GEMINI.md - CES Build System (CBS)

## Project Overview

The **CES Build System (CBS)** is a collection of tools designed by **Clyso** to automate and simplify the creation and release of containers, specifically focusing on **Ceph** and related components. It operates as a monorepo containing multiple Python packages that together form a complete build infrastructure.

### Key Components

- **`cbscore`**: The core library and the `cbsbuild` CLI tool, used to build containers locally.
- **`cbsd`**: The "CBS as a Service" daemon. A **FastAPI** REST server that manages a work queue via **Redis** and **Celery** to schedule builds on worker nodes.
- **`cbc`**: The CLI client for the `cbs` service.
- **`crt`**: A CLI tool for managing Ceph release lifecycles.
- **`cbsdcore`**: Shared core logic used by the daemon and workers.
- **`components/`**: Contains build recipes (e.g., `ceph`) and version-specific descriptors.

## Technical Stack

- **Language**: Python 3.13+
- **Package Management**: [uv](https://docs.astral.sh/uv/)
- **API Framework**: FastAPI
- **Task Queue**: Celery with Redis as the broker.
- **Environment**: Podman (via `podman-compose`) for local service setup.
- **Linting & Formatting**: [Ruff](https://beta.ruff.rs/)
- **Type Checking**: [basedpyright](https://github.com/DetachHead/basedpyright)
- **Testing**: [pytest](https://docs.pytest.org/)

## Building and Running

### Development Setup

```bash
# Install dependencies and sync workspace
uv sync

# Run all pre-commit checks (Lefthook)
uv run ruff check
uv run ruff format --check
uv run basedpyright .
```

### Running the CBS Service Locally

The project includes a `podman-compose` setup to run the full stack (server, worker, redis).

```bash
# Start the CBS stack
./do-cbs-compose.sh up

# Stop the CBS stack
./do-cbs-compose.sh down
```

### Running Tests

```bash
# Install workspace packages and test dependencies
uv sync --all-packages --group test

# Run the test suite
uv run pytest cbsd/tests/ -v
```

## Development Conventions

### Code Style
- **Python**: Adhere to PEP 8. Use **Ruff** for linting and formatting.
- **Typing**: Use static type hints and verify with **basedpyright**.
- **Shell**: Format shell scripts with `shfmt` (invoked via Lefthook).

### Commit Guidelines
- **Format**: Follow Ceph project commit practices.
- **Requirements**: All commits must be **GPG-signed** and include a **DCO (Developer Certificate of Origin)** sign-off (`git commit -s`).
- **Lefthook**: Use `lefthook run pre-commit` to ensure compliance before pushing.

### Licensing
- `cbsd` (service): AGPLv3
- `cbscore`, `cbc`, `crt` (CLI tools): GPLv3
- Always check the `LICENSE` file within each sub-package for specific details.
