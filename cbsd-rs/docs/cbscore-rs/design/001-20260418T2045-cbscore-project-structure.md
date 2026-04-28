# cbscore Rust Port — Project Structure & Crate Organization

## Overview

The cbscore-rs project ports the existing `cbscore/` Python package (~9.8k LoC,
~50 source files) to Rust. The port is organized as three new member crates of
the existing `cbsd-rs/` Cargo workspace: a zero-IO **types** crate, a
**library** crate that carries all subsystem wrappers and business logic, and a
**CLI binary** crate that produces `cbsbuild`.

The split mirrors the approach taken in `cbsd-rs/` (proto + server + worker):
isolate the wire-format types from the heavy implementation, and isolate
binary-specific concerns from the library. For cbscore the analogous boundaries
are:

- **Types** that cross process and file boundaries — consumed by the library
  itself, by the CLI, and by `cbsd-rs`'s worker when the subprocess bridge is
  retired. They must be stable, free of IO, and cheap to depend on.
- **Library** — subsystem wrappers (podman / buildah / skopeo / S3 / Vault / git
  / GPG), secrets management, builder pipeline stages, image signing & sync,
  runner. Roughly 80% of the Python package's LoC lands here.
- **CLI binary** — a `clap` tree that mirrors the existing `click` command tree
  (`build`, `runner`, `versions`, `config`, `advanced`). Its sole job is
  argument parsing, tracing setup, and calling into the library.

## Source Package — What Gets Ported

The existing `cbscore/` package has these top-level subsystems (LoC from
`wc -l`, rounded). They are the logical units that drive the crate split below.

| Area                  | Python location                                                     | LoC   |
| --------------------- | ------------------------------------------------------------------- | ----- |
| CLI entry             | `__main__.py`                                                       | ~80   |
| CLI commands          | `cmds/{advanced,builds,config,versions}`                            | ~1400 |
| Config (load/store)   | `config.py`                                                         | ~250  |
| Errors                | `errors.py`                                                         | ~50   |
| Logger                | `logger.py`                                                         | ~25   |
| Core components       | `core/component.py`                                                 | ~100  |
| Version descriptors   | `versions/{desc,utils,create,errors}`                               | ~550  |
| Container descriptors | `containers/{build,component,desc,repos}`                           | ~850  |
| Image sign & sync     | `images/{desc,signing,skopeo,sync}`                                 | ~600  |
| Release descriptors   | `releases/{desc,s3,utils}`                                          | ~460  |
| Builder pipeline      | `builder/{prepare,rpmbuild,signing,upload,builder,utils}`           | ~1450 |
| Runner                | `runner.py`                                                         | ~325  |
| Secrets               | `utils/secrets/{models,mgr,git,registry,signing,storage,utils}`     | ~1200 |
| Subsystem wrappers    | `utils/{buildah,podman,s3,vault,git,uris,paths,containers}`         | ~1500 |
| Subprocess + redact   | `utils/__init__.py` (`SecureArg`, `_sanitize_cmd`, `async_run_cmd`) | ~270  |
| Entrypoint script     | `_tools/cbscore-entrypoint.sh`                                      | ~40   |

## Downstream Consumers

Every in-repo Python consumer imports a small, well-defined slice of `cbscore`.
The Rust port must cover this slice before any consumer can stop importing the
Python package.

| Consumer   | Imports                                                                                                                                                                                                                                                                                                |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `cbc`      | `errors.CESError`, `logger.set_debug_logging`, `versions.errors.VersionError`, `versions.utils.{VersionType, get_version_type, parse_component_refs}`                                                                                                                                                  |
| `crt`      | `versions.utils.parse_version`                                                                                                                                                                                                                                                                         |
| `cbsdcore` | `errors.CESError`, `versions.utils.VersionType`                                                                                                                                                                                                                                                        |
| `cbsd`     | `errors.{CESError, MalformedVersionError}`, `logger.logger` (module-level object), `config.{Config, ConfigError}`, `runner.{stop, gen_run_name, runner}`, `versions.create.version_create_helper`, `versions.desc.VersionDescriptor`, `versions.errors.VersionError`, `core.component.load_components` |
| `cbsd-rs`  | **via subprocess bridge** `cbsd-rs/scripts/cbscore-wrapper.py`: `config.{Config, ConfigError}`, `versions.create.version_create_helper`, `versions.desc.VersionDescriptor`, `errors.MalformedVersionError`, `runner.{runner, RunnerError}`, `versions.errors.VersionError`                             |

Two observations drive the crate split:

1. The narrow consumers (`cbc`, `crt`, `cbsdcore`) only need **types** — errors,
   the tracing target, and `VersionType` / parse helpers. Isolating these in
   `cbscore-types` keeps the dependency graph lean for any downstream that
   imports only the types (no subsystem wrappers, no tokio, no cloud SDKs).
2. The wide consumers (`cbsd`, `cbsd-rs`) need **runner**, **config loading**,
   and **version creation**. These live in `cbscore` (the library crate).
   Retiring `cbscore-wrapper.py` is therefore tied to `cbsd-rs/cbsd-worker`
   depending on the library crate directly — which is straightforward because
   both crates live in the same `cbsd-rs/` Cargo workspace.

## Workspace Layout

The three new cbscore crates slot into the existing `cbsd-rs/` Cargo workspace
alongside the current `cbsd-proto`, `cbsd-server`, `cbsd-worker`, and `cbc`
members. The tree below shows the internal layout of each new crate; existing
workspace members and root metadata (`Cargo.toml`, `Cargo.lock`, `docs/`,
`migrations/`, …) are elided.

```
cbsd-rs/
├── ... (existing cbsd-* and cbc crates, Cargo.toml, docs, etc.) ...
│
├── cbscore-types/              # NEW — shared types crate (library)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── errors.rs           # CbsError, MalformedVersion, UnknownRepository
│       ├── logger.rs           # tracing target hierarchy (cbscore.*)
│       ├── config/
│       │   ├── mod.rs          # Config, SigningConfig, LoggingConfig
│       │   ├── paths.rs        # PathsConfig
│       │   ├── storage.rs      # StorageConfig, S3StorageConfig, ...
│       │   └── vault.rs        # VaultConfig, VaultAppRoleConfig, ...
│       ├── versions/
│       │   ├── mod.rs
│       │   ├── desc.rs         # VersionDescriptor, VersionComponent, ...
│       │   ├── errors.rs       # VersionError, InvalidVersionDescriptor
│       │   └── utils.rs        # VersionType, parse_version, parse_component_refs
│       ├── containers/
│       │   └── desc.rs         # ContainerDescriptor, repo enum, ...
│       ├── images/
│       │   ├── desc.rs         # ImageDescriptor
│       │   └── errors.rs       # ImageDescriptorError
│       ├── releases/
│       │   └── desc.rs         # ReleaseDesc, ReleaseComponent, ArchType
│       └── core/
│           └── component.rs    # CoreComponent yaml structs
│
├── cbscore/                    # NEW — library crate: IO, subprocess, IO-bound logic
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── config.rs           # file IO for types in cbscore-types::config
│       ├── versions/
│       │   ├── mod.rs
│       │   └── create.rs       # version_create_helper (async)
│       ├── core/
│       │   └── component.rs    # load_components (disk walking)
│       ├── containers/
│       │   ├── mod.rs
│       │   ├── build.rs        # build driver
│       │   ├── component.rs    # container-component builder
│       │   └── repos.rs        # copr/file/url repo handling
│       ├── images/
│       │   ├── mod.rs
│       │   ├── signing.rs      # GPG + transit signing
│       │   ├── skopeo.rs       # skopeo copy/sync subprocess
│       │   └── sync.rs         # image sync orchestration
│       ├── releases/
│       │   ├── mod.rs
│       │   ├── s3.rs           # release S3 operations
│       │   └── utils.rs
│       ├── builder/
│       │   ├── mod.rs
│       │   ├── prepare.rs
│       │   ├── rpmbuild.rs
│       │   ├── signing.rs
│       │   ├── upload.rs
│       │   └── utils.rs
│       ├── runner/
│       │   ├── mod.rs          # gen_run_name, stop
│       │   └── run.rs          # podman-based runner (binary as PID 1)
│       ├── secrets/
│       │   ├── mod.rs
│       │   ├── models.rs       # Secrets, SecretsError
│       │   ├── mgr.rs          # SecretsMgr
│       │   ├── git.rs
│       │   ├── registry.rs
│       │   ├── signing.rs
│       │   ├── storage.rs
│       │   └── utils.rs
│       ├── utils/
│       │   ├── mod.rs
│       │   ├── buildah.rs      # buildah subprocess wrapper
│       │   ├── containers.rs
│       │   ├── git.rs          # git subprocess wrapper
│       │   ├── paths.rs
│       │   ├── podman.rs       # podman_run, podman_stop
│       │   ├── s3.rs           # S3 client helpers (aws-sdk-s3)
│       │   ├── subprocess.rs   # async_run_cmd + SecureArg + _sanitize_cmd
│       │   ├── uris.rs
│       │   └── vault.rs        # Vault client (vaultrs)
│
└── cbsbuild/                   # NEW — CLI binary
    ├── Cargo.toml
    └── src/
        ├── main.rs             # init tracing, parse CLI, dispatch
        ├── cli.rs              # top-level Parser + global options
        └── cmds/
            ├── mod.rs
            ├── build.rs        # cbsbuild build
            ├── runner.rs       # cbsbuild runner
            ├── versions.rs     # cbsbuild versions {create,list,...}
            ├── config.rs       # cbsbuild config {init,show,...}
            └── advanced.rs     # cbsbuild advanced
```

## Crate Responsibilities

### `cbscore-types`

Pure types. Zero IO, zero async, no subprocess, no network. Consumed by the
library, the CLI, and — once the subprocess bridge is retired — directly by
`cbsd-rs/cbsd-worker`.

**What goes here:**

- All error types (`CbsError`, `MalformedVersion`, `VersionError`,
  `InvalidVersionDescriptor`, `ReleaseError`, `ImageDescriptorError`, ...).
  Implemented with `thiserror`.
- Config structs (`Config`, `PathsConfig`, `VaultConfig`, `SigningConfig`,
  `StorageConfig`, ...). Serde-derived with
  `#[serde(rename_all = "kebab-case")]` to match the pydantic aliases in
  `cbscore/config.py`.
- Descriptor types: `VersionDescriptor`, `VersionComponent`,
  `ContainerDescriptor`, `ReleaseDesc`, `ImageDescriptor`, `CoreComponent`.
- `VersionType` enum + pure parse helpers (`parse_version`,
  `parse_component_refs`, `get_version_type`, `normalize_version`,
  `get_major_version`, `get_minor_version`) — the first three are what `cbc` and
  `crt` import today; the latter three are public in the Python API and included
  for parity through the shim.
- `tracing` target hierarchy (`cbscore`, `cbscore::runner`, `cbscore::builder`,
  ...) and `set_debug_logging()` equivalent.

**What does NOT go here:**

- File IO (`Config::load`, `Config::store`). Lives in `cbscore`.
- Subprocess calls. Lives in `cbscore`.
- Async code. Lives in `cbscore`.
- Any network or cloud SDK dependency.

### `cbscore`

The library crate. Carries the bulk of the port — subsystem wrappers, secrets,
builder pipeline, runner, image signing & sync, release operations.

**What goes here:**

- Config file IO (load YAML/JSON, store YAML) on top of the types from
  `cbscore-types::config`.
- Subprocess wrappers: `utils::podman`, `utils::buildah`, `utils::skopeo` (under
  `images::skopeo`), `utils::git`. Each one is a thin `tokio::process::Command`
  facade with per-tool error types and argument redaction.
- `utils::subprocess` — `async_run_cmd`, `SecureArg`, `_sanitize_cmd`. The
  redaction logic must be preserved verbatim.
- S3 client helpers (via `aws-sdk-s3`), Vault client helpers (via `vaultrs`),
  GPG + transit signing.
- Secrets manager (models, on-disk storage, git-backed storage, registry/signing
  secrets).
- Builder pipeline (`prepare`, `rpmbuild`, `signing`, `upload`).
- Image signing & sync (`images::signing`, `images::skopeo`, `images::sync`).
- Release S3 operations (`releases::s3`).
- Runner (`runner::run`) — spawns a podman container with `cbsbuild` mounted at
  `/runner/cbsbuild` as the container's PID 1 and re-enters
  `cbsbuild runner build <args>` inside the container. There is no shell
  entrypoint wrapper; the binary is the entrypoint.

**What does NOT go here:**

- Types that need to cross process / FFI boundaries (those live in
  `cbscore-types`).
- CLI argument parsing or clap structs (those live in `cbsbuild`).

### `cbsbuild`

Thin binary crate. Contains the `clap` tree, tracing subscriber setup, log-file
routing, and the top-level error handler that translates library errors into
exit codes + stderr output.

Mirrors the existing click subcommand tree from `cbscore/cmds/`:

- `cbsbuild build <descriptor>` — run a build (via runner).
- `cbsbuild runner` — direct runner subcommand group.
- `cbsbuild versions {create, …}` — version-descriptor commands.
- `cbsbuild config {init, show, …}` — config file helpers.
- `cbsbuild advanced` — escape-hatch subcommands.

Global flags: `-d/--debug`, `-c/--config <path>`. Env var: `CBS_DEBUG`. Exit
codes and stdout/stderr contracts match the Python implementation (see the
CLAUDE.md `Correctness Invariants`).

**Intentional CLI UX break: `--cbscore-path` is dropped.** The Python
`cbsbuild build` accepts a required `--cbscore-path` argument pointing at the
cbscore source tree that the Python runner mounts at `/runner/cbscore`. The Rust
runner mounts the `cbsbuild` binary into the container instead of a source tree,
so the flag has no meaning and is removed — it is **not** kept as a deprecated
no-op.

This is a deliberate deviation from CLI UX parity (Correctness Invariant 2).
Operators and any out-of-tree scripts that currently pass `--cbscore-path` must
be notified before M1 ships; the M1 release notes must call this out, and
invocations that still pass the flag will fail fast with a clap "unexpected
argument" error. No `cbsd-rs` code uses this flag today (the worker calls
`cbscore.runner.runner()` directly via the Python bridge, not via
`cbsbuild build`), so nothing in-tree breaks.

## Crate Dependencies

Provisional `Cargo.toml` dependency sketches — exact versions pinned when the
workspace is created.

### `cbscore-types`

```toml
[dependencies]
camino    = { version = "1", features = ["serde1"] }
chrono    = { version = "0.4", features = ["serde"] }
serde     = { version = "1",   features = ["derive"] }
thiserror = "2"
tracing   = "0.1"
```

`serde_json` and `serde_saphyr` deliberately do **not** appear here — the types
only carry `#[derive(Serialize, Deserialize)]` and never perform IO. The format
crates live in `cbscore` (below), which owns file loading and dumping. Keeping
them out of `cbscore-types` means a lean dependency graph for every downstream
of this crate (the `cbsd-worker` direct dep, any external consumer).

`camino` is required at the types-crate level (not just `cbscore`) because
`Config`, `PathsConfig`, and the various descriptor structs have `Utf8PathBuf`
fields that participate in the `#[derive(Serialize, Deserialize)]` here. The
`serde1` feature is what wires those derives up.

### `cbscore`

```toml
[dependencies]
cbscore-types = { path = "../cbscore-types" }

# async + tracing
tokio       = { version = "1", features = ["full"] }
tokio-util  = "0.7"
tracing     = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
async-trait = "0.1"
futures-util = "0.3"

# cloud SDKs
aws-config  = "1"
aws-sdk-s3  = "1"
vaultrs     = "0.8"
reqwest     = { version = "0.12", features = ["rustls-tls", "json"] }

# crypto + hashing
sha2 = "0.10"

# archives + filesystem
camino         = { version = "1", features = ["serde1"] }
camino-tempfile = "1"
tar            = "0.4"
flate2         = "1"
tempfile       = "3"
which          = "7"

# serde (re-exported types come from cbscore-types)
serde      = { version = "1", features = ["derive"] }
serde_json = "1"
serde_saphyr = "0.0.24"

# errors + helpers
thiserror = "2"
# anyhow deliberately not listed: the library code must not produce
# `anyhow::Error`. Per-subsystem `thiserror` enums carry every failure
# case. anyhow lives only in `cbsbuild`'s top-level error handler.
url       = "2"
chrono    = { version = "0.4", features = ["serde"] }
rand      = "0.9"                 # for gen_run_name
regex     = "1"                   # for `_sanitize_cmd` --pass*= redaction
uuid      = { version = "1", features = ["v4"] }
```

### `cbsbuild`

```toml
[dependencies]
cbscore-types = { path = "../cbscore-types" }
cbscore       = { path = "../cbscore" }

clap              = { version = "4", features = ["derive", "env"] }
tokio             = { version = "1", features = ["full"] }
tracing           = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender  = "0.2"
anyhow            = "1"
```

## Build & Run

```bash
# Build everything
cargo build --workspace
cargo build --workspace --release

# Run the CLI
cargo run --bin cbsbuild -- -c cbs-build.config.yaml versions list

# Tests / lint / format
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
```

## Versioning

The cbscore-rs workspace uses standard semver for its external- facing surface.
The workspace root `Cargo.toml` carries a single
`[workspace.package] version = "x.y.z"` field that every member crate inherits
(`version.workspace = true`). The version is bumped in one commit at release
time.

**When to bump which component:**

| Bump  | Trigger                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Major | A `schema_version` bump on any wire-format file (config YAML, secrets YAML, version-descriptor JSON, release-descriptor JSON, container-descriptor YAML, `cbs.component.yaml`); on-disk layout change; CLI UX break; or any change in a behaviour anchored by the Correctness Invariants in `cbsd-rs/docs/cbscore-rs/CLAUDE.md`. The trigger is the per-format `schema_version` integer, not cross-language byte-equality with pydantic output (see Python Coexistence below). |
| Minor | Additive features (new CLI subcommand, new config field with a default, new `cbscore-types` export).                                                                                                                                                                                                                                                                                                                                                                           |
| Patch | Bug fixes with no observable interface change.                                                                                                                                                                                                                                                                                                                                                                                                                                 |

**Per-format `schema_version`, no migration tool.** Every wire- format file the
Rust port writes carries a top-level `schema_version: u64` field with a
per-file-format namespace. All current formats are v1. Rust fails fast with a
clear error if `schema_version` is missing or above the compiled-in max on read
— absent is **not** treated as v0, there is no implicit- version fallback. Any
change to a wire-format shape (including adding an optional field) bumps the
integer for that format. See design 002 § Wire-Format Versioning for the full
mechanics, the `VersionedX` enum dispatch pattern, and the list of tagged types.

No `cbsbuild migrate` subcommand ships; operators re-edit files by hand or via
their own scripts when a bump lands. Migration tooling is explicitly out of
scope for this plan. The Python implementation is not patched: pydantic's
default `extra = "ignore"` silently drops `schema_version` on read, so
Python-written files arrive without the tag and Rust refuses them until the
operator re-tags.

The M1 release is cbscore-rs 1.0.0 — version 0.x is reserved for in-progress
pre-release builds and carries no stability promise.

## Python Coexistence

Replacing the Python `cbscore/` package cannot happen in one step. Three
migration shapes make up the approach:

1. **No cross-language file interchange.** A given deployment runs either Python
   cbscore or Rust cbscore at any one time, never both against the same on-disk
   files. Cross-language byte-equality of config, descriptor, and output files
   is **not** a requirement. Operators switching from Python to Rust regenerate
   or hand-migrate their `cbs-build.config.yaml` and `secrets.yaml` at cutover
   (see design 002 § Configuration & Secrets Subsystem for the secrets.yaml
   migration recipe). The `#[serde(rename_all = "kebab-case")]` discipline in
   `cbscore-types` (config keys) and the snake_case discipline in descriptor
   structs match the existing on-disk format for operator familiarity and
   minimum hand-migration friction at cutover, not for cross-language
   load-compatibility.

2. **CLI parity (always).** The `cbsbuild` binary exposes the same subcommand
   tree, flags, exit codes, stderr / stdout contracts, and env vars. Python
   wrappers that shell out to `cbsbuild` continue working unchanged.
   `cbsd-rs/scripts/cbscore-wrapper.py` relies on this surface today and will
   continue to do so until option 3 lands.

3. **Direct Rust crate dependency (preferred).** The cbscore crates land as
   members of the existing `cbsd-rs/` Cargo workspace. `cbsd-rs/cbsd-worker`
   then imports the library crate directly and retires `cbscore-wrapper.py`
   along with the embedded Python runtime in the worker container image. This is
   the primary motivation for splitting the Rust port this way.

The in-tree Python consumers (`cbc`, `crt`, `cbsdcore`, `cbsd`) keep importing
the existing Python `cbscore` package unchanged. The Rust rewrite does not ship
a Python binding, extension module, or any other in-process Python interop
layer. A consumer switches to Rust only when it is itself rewritten; until then
both implementations coexist in-tree.

## Runner Container

The runner is cbscore's most distinctive piece: `cbscore/runner.py` spawns a
podman container whose entrypoint (`cbscore/_tools/cbscore-entrypoint.sh`)
re-executes the same CLI inside the container so the actual build runs in a
controlled environment. The host marshals config + secrets into temp files,
mounts the host cbscore codebase, and podman handles the rest.

The Rust port preserves the same shape but changes the mechanics:

- **No shell entrypoint wrapper** — the Python runner mounted a bash script at
  `/runner/entrypoint.sh` that did venv setup, `uv tool install` of the cbscore
  wheel, `$PATH` manipulation, and finally `exec`'d `cbsbuild`. The Rust port
  mounts the compiled `cbsbuild` binary at `/runner/cbsbuild` and uses it as the
  container's PID 1 directly (`podman run --entrypoint /runner/cbsbuild ...`).
  HOME normalisation that the shell did (`HOME=/runner` if unset or `/`) is
  preserved by passing `-e HOME=/runner` to `podman run` from the host runner —
  the flag overrides whatever the image or host process exports, so all edge
  cases (image without HOME, `--user`-altered HOME, rootless podman with weird
  UID maps) are handled without any in-container code. The `CBS_DEBUG=1` →
  `--debug` mapping is handled by `clap`'s `env` feature; the
  `--config /runner/cbs-build.config.yaml runner build <args>` invocation lives
  directly on the host runner's `podman run` command line.
- **Binary mount instead of source mount** — the Python runner bind-mounts the
  host `cbscore/` source tree into `/runner/cbscore`. The Rust runner mounts the
  compiled `cbsbuild` binary at `/runner/cbsbuild` instead. This removes `uv`,
  the cbscore Python wheel, and the venv from the build image. System `python3`
  stays — Ceph's `do_cmake.sh` and several `python3-mgr-*` RPMs require it (see
  design 002 § Open Questions, OQ7 resolution, for the precise scope of what is
  and is not removed; § Container entry point in design 002 § Runner Subsystem
  covers the full `podman run` invocation and the `-e HOME=/runner` flag).
- **Config, secrets, components, scratch, ccache** — same mount paths as today
  (`/runner/cbs-build.config.yaml`, `/runner/cbs-build.secrets.yaml`,
  `/runner/components`, `/runner/scratch`, `/runner/ccache`). Temp files on the
  host are generated exactly as they are now.
- **Container image** — the `distro` field of `VersionDescriptor` continues to
  select the image. No cbscore-specific base image is needed — `cbsbuild` is a
  statically linkable Rust binary.
- **Log streaming** — the host runner keeps the existing callback- based log
  streaming. It is already async on the Python side (`AsyncRunCmdOutCallback`);
  on the Rust side the callback becomes an
  `Fn(&str) -> impl Future<Output = ()>` (or a trait object).
- **Container lifecycle** — `gen_run_name`, `stop`, and the
  `--replace-if-exists` semantics port unchanged.

No changes to the on-disk layout, env vars, or the in-container CLI contract are
in scope for this design. Any change there needs its own design document (see
the `Correctness Invariants` in `cbsd-rs/docs/cbscore-rs/CLAUDE.md`).
