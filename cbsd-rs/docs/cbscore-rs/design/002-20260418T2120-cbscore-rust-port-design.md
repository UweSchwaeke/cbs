# cbscore Rust Port — Architecture & Subsystem Design

## Overview

This document describes the architecture for reimplementing `cbscore/` in Rust
(2024 edition), replacing the current Python 3 + `click` + `pydantic` +
`aioboto3` + `hvac` stack. The motivating goals are:

- **Static typing end-to-end.** The Python package leans heavily on pydantic for
  runtime validation of config files, secrets files, version descriptors,
  container descriptors, release descriptors, and `cbs.component.yaml`. In Rust
  those become compile-time-checked serde structs with
  `#[serde(rename_all = "kebab-case")]`, removing a whole class of runtime
  surprises.
- **One async runtime.** Today `cbscore` mixes sync `click` entry points with
  selectively-async code (`runner.py`, `utils/s3.py`, subprocess I/O). The Rust
  port runs on a single `tokio` multi- thread runtime from `main`.
- **No embedded Python in the build container.** The current Python runner
  mounts the cbscore source tree into the builder container and invokes
  `python -m cbscore` inside. The Rust port mounts the `cbsbuild` binary
  instead, removing the need for a Python interpreter or the `cbscore` wheel in
  the builder image.
- **Retire the `cbsd-rs` Python bridge.** `cbsd-rs/cbsd-worker` currently drives
  the Python `cbscore` via `cbsd-rs/scripts/cbscore-wrapper.py`. Once the
  cbscore Rust crates land as members of the `cbsd-rs/` Cargo workspace, the
  worker depends on the Rust library crate directly and the subprocess wrapper
  is removed.
- **Preserve every wire contract.** Config, secrets, descriptors, CLI flags,
  exit codes, on-disk paths, log format, and the in- container mount layout are
  all unchanged. Rewrite ≠ redesign.

The workspace split (`cbscore-types`, `cbscore`, `cbsbuild`, added as new member
crates of the existing `cbsd-rs/` Cargo workspace) is specified in
[`001-…-cbscore-project-structure.md`](./001-20260418T2045-cbscore-project-structure.md).
This document builds on that split — it does **not** restate crate boundaries or
dependency lists. It focuses on how each logical subsystem maps to Rust, what
state and invariants each one owns, and how the parts plug together.

## Current Architecture (Python)

```
                     ┌─────────────────────────────────────────┐
                     │                 cbscore                 │
                     │            (Python library)             │
 ┌───────────┐  ───► │                                         │
 │ cbsbuild  │       │   ┌────────────┐   ┌────────────────┐   │
 │  (click)  │       │   │   config   │   │ versions /     │   │
 │   CLI     │       │   │ + secrets  │   │   containers / │   │
 └─────┬─────┘       │   │ + vault    │   │   images /     │   │
       │             │   └──────┬─────┘   │   releases     │   │
       │             │          │         │    descriptors │   │
       │             │          ▼         └────────┬───────┘   │
       │             │   ┌────────────────────────┐│           │
       │             │   │        builder         │◄           │
       │             │   │ prepare / rpmbuild /   │            │
       │             │   │ signing / upload       │            │
       │             │   └────────────┬───────────┘            │
       │             │                │                        │
       │             │                ▼                        │
       │             │   ┌────────────────────────┐            │
       │             │   │        runner          │            │
       │             │   │ (podman re-enters CLI) │            │
       │             │   └──────────┬─────────────┘            │
       │             │              │ subprocess               │
       │             │              │ podman / buildah /       │
       │             │              │ skopeo / git / gpg /     │
       │             │              │ rpmbuild / aws-cli-less  │
       │             │              ▼                          │
       │             │   ┌────────────────────────┐            │
       │             │   │  builder container     │            │
       │             │   │  (python -m cbscore    │            │
       │             │   │   in a fresh el9 box)  │            │
       │             │   └────────────────────────┘            │
       │             └─────────────────────────────────────────┘
       │
       │  imports:
       ▼
 ┌──────────────┐  ┌────────┐  ┌─────┐  ┌─────┐  ┌─────────────┐
 │  cbsd-rs     │  │  cbsd  │  │ cbc │  │ crt │  │  cbsdcore   │
 │ (via .py     │  │ (live  │  │     │  │     │  │             │
 │  wrapper)    │  │  proc) │  │     │  │     │  │             │
 └──────────────┘  └────────┘  └─────┘  └─────┘  └─────────────┘
```

**Problems / friction points:**

- **Split sync/async model.** `click` is synchronous; `cbscore`'s `runner`,
  `utils/s3`, `utils/podman`, `utils/buildah` are `asyncio`-based. Commands
  shuttle between the two via ad-hoc `asyncio.run()` entry points.
- **Pydantic aliases everywhere.** The in-repo YAML uses kebab-case, the Python
  code uses snake_case, and every config / secrets / descriptor model has to
  declare `validate_by_alias` + `serialize_by_alias`. Easy to get wrong. serde's
  `rename_all = "kebab-case"` at the container level fixes this uniformly.
- **Embedded Python runtime in the builder container.** The runner bind-mounts
  `cbscore/` into the container and invokes `python -m cbscore` inside. The
  builder image must carry a Python 3.13 interpreter, the `cbscore`
  dependencies, and must remain cbscore-source-compatible with the host.
- **Secrets redaction depends on runtime care.** `SecureArg` / `_sanitize_cmd`
  in `cbscore/utils/__init__.py` are correct, but every `run_cmd` /
  `async_run_cmd` caller has to remember to construct `Password` / `PasswordArg`
  objects rather than bare strings. The Python type system catches none of this.
- **In-process imports from multiple siblings.** `cbsd`, `cbsdcore`, `cbc`,
  `crt` all import symbols directly from `cbscore`. The rewrite keeps those
  imports working by leaving the Python `cbscore` package in-tree unchanged
  until each consumer is itself rewritten.
- **Pseudo-API: the subprocess bridge.** `cbsd-rs/cbsd-worker` ships a
  hand-written `cbscore-wrapper.py` that `import`s `cbscore` and drives a build.
  The wrapper is a second API surface with no type checking between the Rust
  worker and the Python module.

## Target Architecture (Rust)

```
 ┌──────────────────────────────────────────────────────────────┐
 │                        cbscore-rs                            │
 │                                                              │
 │   ┌──────────────┐     ┌───────────────────────────────────┐ │
 │   │  cbsbuild    │────►│              cbscore              │ │
 │   │  (clap CLI)  │     │         (library crate)           │ │
 │   └──────┬───────┘     │                                   │ │
 │          │             │  config IO │ version IO │ secrets │ │
 │          │             │  runner    │ builder    │ images  │ │
 │          │             │  releases  │ subprocess │ vault   │ │
 │          │             │            wrappers               │ │
 │          │             └────┬──────────────┬───────────────┘ │
 │          │                  │              │                 │
 │          │                  ▼              ▼                 │
 │          │         ┌──────────────┐  ┌──────────────────┐    │
 │          │         │ cbscore-types│  │    subprocess    │    │
 │          │         │  (zero-IO)   │  │ (podman/buildah/ │    │
 │          │         │  descriptors │  │  skopeo/git/gpg) │    │
 │          │         │  config      │  └──────┬───────────┘    │
 │          │         │  errors      │         │                │
 │          │         │  versions    │         ▼                │
 │          │         └──────────────┘  ┌─────────────────┐     │
 │          │                           │ builder container   │  │
 │          │          podman run ────► │ (static cbsbuild    │  │
 │          │          mounts cbsbuild  │  bind-mount — no    │  │
 │          │                           │  python needed)     │  │
 │          │                           └─────────────────┘     │
 │          │                                                   │
 └──────────┴───────────────────────────────────────────────────┘
            │
            │  direct Cargo dep (same workspace)
            ▼
       ┌──────────────┐
       │   cbsd-rs    │ ───► direct crate dep on cbscore
       │ (cbsd-worker)│     (retires cbscore-wrapper.py)
       └──────────────┘

       The Python consumers (cbsd, cbc, crt, cbsdcore) continue to
       `import cbscore` from the existing Python package; they are
       not shown in this diagram because they never cross into the
       Rust side.
```

**Key changes from Python:**

- **One async runtime.** `cbsbuild` sets up a `tokio` multi-thread runtime in
  `main`, every other entry point runs as an async fn.
- **Types live in `cbscore-types`.** All serde structs plus error enums sit in a
  zero-IO crate that everyone else depends on. This is the stable surface shared
  by the library, the `cbsbuild` CLI, and the `cbsd-worker` crate (via a direct
  crate dep once the subprocess bridge is retired).
- **Runner mounts the binary, not the source.** The builder container no longer
  needs a Python interpreter. `cbsbuild` is a single static binary mounted at
  `/runner/cbsbuild` and runs as the container's PID 1 directly — no shell
  entrypoint wrapper.
- **Secrets redaction is type-enforced.** A `SecureArg` trait + a `SecretArg`
  enum make it a type error to pass a bare `&str` as a password to the
  subprocess runner.
- **Python consumers stay on the Python `cbscore` package.** The rewrite does
  not ship a Python binding. `cbsd-rs/cbsd-worker` is the only consumer that
  moves to Rust directly (via a Cargo crate dep); `cbc`, `crt`, `cbsd`, and
  `cbsdcore` continue to `import cbscore` from the existing Python package
  unchanged until each is itself rewritten in Rust.

## Capability Mapping

Versions are the **desired** minimum pins for the workspace root `Cargo.toml`;
actual versions are resolved at `cargo add` time and should be bumped to the
latest compatible release when a crate is first introduced. Rows whose
"technique" is a binary tool (`git`, `gpg2`) have no crate version — they are
marked `—`.

| Capability                  | Rust crate / technique                              | Version                                                         | Notes                                                                                                                                                                                                                                                                                                       |
| --------------------------- | --------------------------------------------------- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CLI                         | `clap` v4 derive                                    | `clap` 4                                                        | Mirrors `click` command tree in `cbscore/cmds/`                                                                                                                                                                                                                                                             |
| Config / descriptor parsing | `serde`, `serde_saphyr`, `serde_json`               | `serde` 1, `serde_saphyr` 0.0.24, `serde_json` 1                | `#[serde(rename_all = "kebab-case")]` across all YAML-backed structs                                                                                                                                                                                                                                        |
| Error types                 | `thiserror` (library); `anyhow` (binary boundary)   | `thiserror` 2, `anyhow` 1                                       | All public error types live in `cbscore-types`; `anyhow` only at `cbsbuild/src/main.rs` for display                                                                                                                                                                                                         |
| Logging                     | `tracing`, `tracing-subscriber`, `tracing-appender` | `tracing` 0.1, `tracing-subscriber` 0.3, `tracing-appender` 0.2 | Target hierarchy mirrors `cbscore.<module>` layout                                                                                                                                                                                                                                                          |
| Async runtime               | `tokio` (multi-thread, full features)               | `tokio` 1                                                       | No sync CLI glue — `main` enters tokio immediately                                                                                                                                                                                                                                                          |
| Subprocess + redaction      | `tokio::process` + custom `SecureArg` trait         | `tokio` 1 (re-use)                                              | Ports `utils/__init__.py` (`async_run_cmd`, `SecureArg`, `_sanitize_cmd`). Lives at `cbscore::utils::subprocess`; lift-out candidate to a future `cbscommon-rs` (see design 001 § Lift-out invariants)                                                                                                      |
| podman / buildah / skopeo   | `tokio::process` subprocess wrappers                | `tokio` 1 (re-use)                                              | Behavioural port of `utils/{podman,buildah}.py`, `images/skopeo.py`                                                                                                                                                                                                                                         |
| git                         | `tokio::process` + `git` binary                     | `tokio` 1 (re-use); `git` binary `>= 2.23`                      | Matches Python: `utils/git.py` is 401 LoC of subprocess wrappers. Floor `git 2.23` (Aug 2019) is set by `git switch` and `git branch --show-current`, used by `cbscommon/git/cmds.py`. Lives at `cbscore::utils::git`; lift-out candidate to a future `cbscommon-rs` (see design 001 § Lift-out invariants) |
| GPG signing                 | `tokio::process` + `gpg2` binary                    | `tokio` 1 (re-use); `gpg2` binary `>= 2.1`                      | Sequoia considered but rejected: `rpm --addsign` invokes gpg internally, so the gpg binary cannot be removed. Floor `gpg 2.1` (2014) is set by `--pinentry-mode loopback`                                                                                                                                   |
| Vault transit signing       | `vaultrs`                                           | `vaultrs` 0.8                                                   | For `utils/vault.py` + `images/signing.py` transit path                                                                                                                                                                                                                                                     |
| AWS S3                      | `aws-sdk-s3`                                        | `aws-config` 1, `aws-sdk-s3` 1                                  | Replaces `aioboto3` in `utils/s3.py` + `releases/s3.py`                                                                                                                                                                                                                                                     |
| Vault KV                    | `vaultrs`                                           | `vaultrs` 0.8 (re-use)                                          | Replaces `hvac` in `utils/vault.py`                                                                                                                                                                                                                                                                         |
| HTTP (OIDC/OAuth callbacks) | `reqwest` + `rustls`                                | `reqwest` 0.13 (features: `rustls-tls`, `json`)                 | Only used transitively (Vault / S3 SDK) in cbscore                                                                                                                                                                                                                                                          |
| Randomness                  | `rand`                                              | `rand` 0.9                                                      | `gen_run_name()` in `runner.py`                                                                                                                                                                                                                                                                             |
| Paths / fs                  | `camino` (UTF-8 paths)                              | `camino` 1 (feature `serde1`)                                   | All cbscore API boundaries use `Utf8Path` / `Utf8PathBuf`. Bridge to `std::path::Path` only at FFI points where third-party crates require it                                                                                                                                                               |
| Temp files                  | `tempfile` + `camino-tempfile`                      | `tempfile` 3, `camino-tempfile` 1                               | Match Python's `tempfile.mkstemp` usage in `runner.py`; `camino-tempfile` keeps tempfile paths as `Utf8PathBuf`                                                                                                                                                                                             |

## Error Taxonomy

The Python package uses a shallow exception hierarchy rooted at `CESError` (from
`cbscore/errors.py`). Every subsystem defines its own subclass. In Rust, each
subsystem gets a `thiserror`-derived enum, and all of them live in
`cbscore-types` so they can be matched across crates.

| Python                          | Rust (in `cbscore-types`)         | Source module       |
| ------------------------------- | --------------------------------- | ------------------- |
| `CESError`                      | `CbsError` (root enum or marker)  | `errors`            |
| `MalformedVersionError`         | `CbsError::MalformedVersion`      | `errors`            |
| `NoSuchVersionError`            | `CbsError::NoSuchVersion`         | `errors`            |
| `UnknownRepositoryError`        | `CbsError::UnknownRepository`     | `errors`            |
| `VersionError`                  | `VersionError`                    | `versions::errors`  |
| `InvalidVersionDescriptorError` | `VersionError::InvalidDescriptor` | `versions::errors`  |
| `NoSuchVersionDescriptorError`  | `VersionError::NoSuchDescriptor`  | `versions::errors`  |
| `ConfigError`                   | `ConfigError`                     | `config`            |
| `SecretsError`                  | `SecretsError`                    | `utils::secrets`    |
| `SecretsMgrError`               | `SecretsError::Manager`           | `utils::secrets`    |
| `PodmanError`                   | `PodmanError { retcode, stderr }` | `utils::podman`     |
| `BuildahError`                  | `BuildahError`                    | `utils::buildah`    |
| `CommandError`                  | `CommandError`                    | `utils::subprocess` |
| `ReleaseError`                  | `ReleaseError`                    | `releases`          |
| `ImageDescriptorError`          | `ImageDescriptorError`            | `images::errors`    |
| `ContainerError`                | `ContainerError`                  | `containers`        |
| `BuilderError`                  | `BuilderError`                    | `builder`           |
| `MissingScriptError`            | `BuilderError::MissingScript`     | `builder`           |
| `RunnerError`                   | `RunnerError`                     | `runner`            |

**Design rules:**

- Each subsystem has a single `thiserror` enum with one variant per
  distinguishable failure. Wrapping errors use `#[from]`.
- Variants never carry boxed `dyn Error` unless the underlying error is a
  framework error (`reqwest`, `aws_sdk_s3`, …) that cannot be exhaustively
  matched.
- At the binary boundary (`cbsbuild/src/main.rs`) errors collapse into
  `anyhow::Error` for display only. Library code never uses `anyhow`.
- Error `Display` impls match the Python `__str__` output so that CLI stderr
  lines stay visually identical ("error: …").

## Wire-Format Versioning

Every wire-format file that the Rust port reads from disk (or S3) carries a
top-level `schema_version: u64` field. The scheme is a **Rust-side-only**
concern — Python `cbscore/` is not touched. The goal is traceability and
fail-fast error reporting, not in-binary migration.

### Versioned files

Each format has its own independent `schema_version` namespace. `Config` at v1
bears no relation to `Secrets` at v1; they advance on separate clocks.

| File                            | Top-level type        | Current version |
| ------------------------------- | --------------------- | --------------- |
| `cbs-build.config.yaml`         | `Config`              | v1              |
| `cbs-build.secrets.yaml`        | `Secrets`             | v1              |
| `cbs-build.vault.yaml`          | `VaultConfig`         | v1              |
| `cbs.component.yaml`            | `CoreComponent`       | v1              |
| `container.yaml`                | `ContainerDescriptor` | v1              |
| Version descriptor (`.json`)    | `VersionDescriptor`   | v1              |
| Image descriptor (`.json`)      | `ImageDescriptor`     | v1              |
| Release descriptor (S3 `.json`) | `ReleaseDesc`         | v1              |
| Component release desc (S3)     | `ReleaseComponent`    | v1              |
| `build-report.json`             | `BuildArtifactReport` | v1              |

The Python `BuildArtifactReport.report_version: int` field is renamed to
`schema_version: u64` in the Rust port for uniformity with the other formats.
Any Python consumer of `build-report.json` that reads the old `report_version`
key must switch to `schema_version` (see Migration Strategy).

### Rules

- **Type.** `schema_version: u64`. Not semver, not a string. One integer,
  monotonic, per format.
- **Placement.** First key in the file, before any other field. Makes `head -1`
  diagnostics and streaming dispatch trivial.
- **Current state.** Every format is **v1** today.
- **Absent is an error.** Rust fails fast with a clear message ("missing
  `schema_version` for <file>, expected v<N>") if the field is not present on
  read. Absent is **not** v0; there is no implicit-version fallback.
- **Every change bumps.** Renaming, removing, retyping, or **adding an optional
  field** all bump the integer. The version doubles as a human-readable "this is
  a different file" marker; additive changes are not exempt. **This rule applies
  from the M1 release onward.** During M0–M1 the schema is still being defined
  and per-format `schema_version: 1` accumulates every change up to the M1 1.0.0
  cut; the first post-1.0 change to any format is the first bump. Pre-M1
  cbscore-rs is a 0.x release with no stability promise (see design 001 §
  Versioning).
- **No migration tool.** There is no `cbsbuild migrate`. Operators re-edit files
  by hand when a bump lands, or rely on their own scripts. Tooling is explicitly
  out of scope for this plan and may return in a future design.
- **Unknown-version handling.** Rust reading a `schema_version` higher than the
  compiled-in max fails fast with a clear message instructing the operator to
  upgrade cbscore-rs. Reading a lower, still-known version goes through the
  enum's `into_latest()` transform (see below); if no transform exists (the
  common case until a bump lands), that path is also a hard error.

### Python-side impact

None. The Python models are **not** patched. Pydantic v2's default
`extra = "ignore"` silently drops the `schema_version` field on read, and Python
never emits it on write. Files that round-trip through Python therefore lose the
tag, and Rust will refuse to re-read them until the operator restores the tag.
This is the expected workflow: Rust-read files either come from a Rust write or
from an operator who has manually tagged them.

### Implementation pattern

Each versioned type has:

1. One or more `TypeVn` structs — the canonical shape at version `n`. Old
   versions stay in the source as deprecated types so that version-mismatch
   error messages can reference them explicitly.
2. A sibling `VersionedType` enum tagged on `schema_version`.
3. An `into_latest()` method on the wrapper that returns the current-version
   struct. Until a bump lands, the method only has the `VN` arm (where `N` is
   the current version) and is effectively a `match` that unwraps.
4. A public `Type` alias pointing at the current struct, and public `load()` /
   `store()` functions that go through the wrapper.

Sketch for `Config`:

```rust
use serde::{Deserialize, Serialize};

/// Current shape of `cbs-build.config.yaml`, v1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigV1 {
    // ... current fields (paths, storage, signing, logging,
    //                     secrets, vault, ...)
}

/// Wrapper that serde dispatches on `schema_version`.
///
/// `schema_version` is a `u64` integer on disk. The enum uses
/// serde's internal tag; the exact attribute dance to match an
/// integer-valued tag is an implementation detail (a hand-rolled
/// `Deserialize` may be needed if serde's default string-matching
/// for internal tags does not accept integer tags directly — the
/// goal is `schema_version: 1` on disk, not `"schema_version": "1"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "schema_version")]
pub enum VersionedConfig {
    #[serde(rename = "1")]
    V1(ConfigV1),
    // V2(ConfigV2) added when the first schema bump lands. At
    // that point `ConfigV1` stays in the source as a deprecated
    // type so the match arm can produce a clear "please re-tag"
    // error message.
}

impl VersionedConfig {
    /// Returns the latest-version struct.
    ///
    /// Today this is a trivial unwrap. When v2 lands, the V1 arm
    /// either auto-upgrades (a simple Rust-side transform) or
    /// returns an error telling the operator to re-tag the file.
    /// Policy is decided at bump time.
    pub fn into_latest(self) -> Result<ConfigV1, ConfigError> {
        match self {
            VersionedConfig::V1(cfg) => Ok(cfg),
        }
    }
}

/// Public alias — always the current struct.
pub type Config = ConfigV1;

/// Read `cbs-build.config.yaml` / `.json`.
///
/// # Errors
///
/// Returns [`ConfigError::MissingSchemaVersion`] if the file has
/// no `schema_version` key, and
/// [`ConfigError::UnknownSchemaVersion { found, max_supported }`]
/// if the integer is above the compiled-in max.
pub fn load(path: &Path) -> Result<Config, ConfigError> { /* ... */ }

/// Write `cbs-build.config.yaml` / `.json`.
///
/// Always emits `schema_version: 1` as the first key.
pub fn store(cfg: &Config, path: &Path) -> Result<(), ConfigError> {
    /* serialise VersionedConfig::V1(cfg.clone()) */
}
```

The same pattern applies to `Secrets`, `VaultConfig`, `CoreComponent`,
`ContainerDescriptor`, `VersionDescriptor`, `ImageDescriptor`, `ReleaseDesc`,
`ReleaseComponent`, and `BuildArtifactReport`. Each gets its own `VersionedX`
enum tagged on `schema_version` with independent integer namespaces.

### Interaction with other wire-format decisions

- **Git-secrets break** (§ Secrets). `schema_version: 1` lands on `secrets.yaml`
  alongside the new `type:` tag on git entries. The v1 shape is "new git
  discriminator + new `schema_version` tag"; no v0 ever ships from Rust.
- **`BuildArtifactReport.report_version`.** Renamed to `schema_version` for
  uniformity. This is a breaking change to the `build-report.json` key name;
  since this file is produced by Rust and consumed by the runner (and by
  `cbsd`/`cbsd-rs`), all Rust-side callers must switch to reading
  `schema_version` in the same commit that introduces the rename.
- **Descriptor snake_case.** `VersionDescriptor`, `ReleaseDesc`,
  `ReleaseComponent`, `ImageDescriptor`, `BuildArtifactReport` continue to use
  snake_case keys (no `rename_all`). The `schema_version` key is snake_case
  regardless; no special treatment needed.
- **Config/secrets/vault kebab-case.** Unchanged — the `schema_version` key is
  the exception: it stays snake_case in YAML as well, because it is a
  versioning-layer concept not a domain field. If literal consistency with
  kebab-case is preferred, the name can be `schema-version` in the YAML while
  staying `schema_version` in the Rust field via
  `#[serde(rename = "schema-version")]`. Decide at implementation time; a single
  choice applies to all formats.

## Configuration & Secrets Subsystem

The Python config model is defined in `cbscore/config.py` and is the subsystem
most widely imported by sibling projects (`cbsd` imports `Config` /
`ConfigError` directly). The Rust port maps it to serde-derived structs.
Cross-implementation file interchange is **not** a requirement: a given
deployment runs either Python cbscore or Rust cbscore, never both against the
same on-disk files. Operators migrating from Python to Rust regenerate or
hand-migrate their config and secrets files at cutover.

### Types (in `cbscore-types::config`)

```rust
// In cbscore-types/src/config/mod.rs

use camino::Utf8PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub paths:    PathsConfig,
    #[serde(default)]
    pub storage:  Option<StorageConfig>,
    #[serde(default)]
    pub signing:  Option<SigningConfig>,
    #[serde(default)]
    pub logging:  Option<LoggingConfig>,
    #[serde(default)]
    pub secrets:  Vec<Utf8PathBuf>,
    #[serde(default)]
    pub vault:    Option<Utf8PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PathsConfig {
    pub components:         Vec<Utf8PathBuf>,
    pub scratch:            Utf8PathBuf,
    pub scratch_containers: Utf8PathBuf,
    #[serde(default)]
    pub ccache:             Option<Utf8PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VaultConfig {
    pub vault_addr:   String,
    #[serde(default)] pub auth_user:    Option<VaultUserPassConfig>,
    #[serde(default)] pub auth_approle: Option<VaultAppRoleConfig>,
    #[serde(default)] pub auth_token:   Option<String>,
}
```

`camino` must appear in `cbscore-types/Cargo.toml` with the `serde1` feature
because the `Utf8PathBuf` fields above participate in the
`#[derive(Serialize, Deserialize)]` on the types crate side; depending on
`camino` only from `cbscore` would not be sufficient.

One implementation detail: the Python `Config.store()` round-trips through JSON
before dumping YAML so that `Path` objects serialise correctly. In Rust
`Utf8PathBuf` implements `Serialize` directly and the round-trip is unnecessary.
`Config::store` produces YAML via `serde_saphyr::to_string` (two-space indent,
flow-style off). Round-trip equivalence on the Rust side (write → load → equal)
is the contract; cross-language byte-equality with pydantic output is not
required since Python and Rust cbscore do not share files.

### IO (in `cbscore::config`)

Loading and storing live in the `cbscore` library crate:

```rust
use camino::Utf8Path;

impl Config {
    /// Load config from `path`. YAML if extension is `.yaml`/`.yml`;
    /// JSON otherwise. Matches the Python implementation exactly.
    pub fn load(path: &Utf8Path) -> Result<Config, ConfigError> { /* ... */ }

    /// Store config to `path` as YAML.
    ///
    /// Creates the parent directory if it does not exist
    /// (`std::fs::create_dir_all` semantics — equivalent to `mkdir -p`).
    /// Mirrors Python `config_path.parent.mkdir(exist_ok=True,
    /// parents=True)` in `cmds/config.py:302`. Callers (notably
    /// `cbsbuild config init` writing to
    /// `~/.config/cbsd/${deployment}/worker/cbscore.config.yaml` on a fresh
    /// workstation) rely on this — they do not pre-create the parent.
    pub fn store(&self, path: &Utf8Path) -> Result<(), ConfigError> { /* ... */ }
}
```

### Secrets

`cbscore/utils/secrets/` models three distinct families — **git**, **signing**,
and **registry** — each with its own discrimination scheme on the Python side.
The Rust port does **not** apply a single uniform pattern across all three; each
family's wire- format treatment is tailored to how the Python model actually
discriminates its variants today. Only the git family requires a wire-format
break; signing and registry ports are straight translations.

#### Git secrets (wire-format break — explicit `type:` tag)

The Python `GitSSHSecret`, `GitTokenSecret`, `GitHTTPSSecret`,
`GitVaultSSHSecret`, and `GitVaultHTTPSSecret` models discriminate the inner
variant by shape (which keys are present: `ssh-key` vs `token` vs `password`).
Deployed `secrets.yaml` files **do not** carry a `type:` field for git entries
today. Serde's `untagged` matching on YAML is fragile enough (non-deterministic
variant selection when fields overlap) that the Rust port introduces an explicit
inner discriminator, mirroring the per-family discriminator pattern already used
by `SigningCreds` and `RegistryCreds`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "creds")]
enum GitCreds {
    #[serde(rename = "plain")]  Plain(GitPlainCreds),
    #[serde(rename = "vault")]  Vault(GitVaultCreds),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum GitPlainCreds {
    #[serde(rename = "ssh")]
    Ssh   { username: String, #[serde(rename = "ssh-key")] ssh_key: String },
    #[serde(rename = "token")]
    Token { username: String, token:    String },
    #[serde(rename = "https")]
    Https { username: String, password: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum GitVaultCreds {
    #[serde(rename = "ssh")]
    Ssh   { username: String, #[serde(rename = "ssh-key")] ssh_key: String, key: String },
    #[serde(rename = "https")]
    Https { username: String, password: String, key: String },
}
```

The explicit `type:` tag is a deliberate format choice for the Rust port because
shape-based discrimination on the Python side relied on pydantic's runtime
keyset inspection, while serde's `untagged` matching on YAML is
non-deterministic when fields overlap. **This is not a transition compat break**
— Python and Rust cbscore do not share files. Operators migrating from Python
re-tag their `secrets.yaml` once at cutover (see Operator Transition below).

**Rust reads only the tagged shape.** An entry without a `type:` tag is a serde
error.

#### Signing secrets (no change — `type:` already present)

The Python signing-secret models already carry a `type:` field in deployed
`secrets.yaml` (used by the Python discriminator to pick the leaf variant). The
Rust port translates directly with `#[serde(tag = "type")]` — no new field,
deployed files already conform:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum SigningCreds {
    // exact variant shapes mirror cbscore/utils/secrets/models.py;
    // confirm field-for-field at implementation time.
    // ...
}
```

Signing entries port directly with the existing `type:` discriminator. No format
changes; round-trip equivalence on the Rust side is the contract.

#### Registry secrets (no change — single leaf per `creds`)

The Python registry-secret models define one leaf shape per outer `creds` value
— `plain` always means a username/password pair, `vault` always means a keyref.
There is no inner ambiguity to discriminate, so a single-level tag on the outer
`creds` field is sufficient:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "creds")]
enum RegistryCreds {
    // exact variant shapes mirror cbscore/utils/secrets/models.py;
    // confirm field-for-field at implementation time.
    // #[serde(rename = "plain")]  Plain { ... }
    // #[serde(rename = "vault")]  Vault { ... }
}
```

Registry entries port directly. Round-trip equivalence on the Rust side is the
contract.

#### Operator transition

A given deployment runs either Python cbscore or Rust cbscore at any one time,
never both against the same on-disk files. Operators switching from Python to
Rust perform a one-time migration of their `secrets.yaml`:

- No migration subcommand ships. Operators do two things by hand (or via any
  one-shot script they own):
  1. Add `schema_version: 1` as the first key of the file (per § Wire-Format
     Versioning).
  2. Add the `type:` tag to each git entry.
- M1 release notes must include a worked example of the migrated YAML for each
  of the five git variants (plain ssh/token/https + vault ssh/https), with the
  `schema_version` header included in the example.
- Python `cbscore/` is **not** patched and keeps writing its existing shape. A
  deployment that has switched to Rust does not write back to a Python-readable
  file; the Rust implementation owns its on-disk layout from the cutover onward.

Python `Secrets.merge()` reduces to a `SecretsMgr` struct holding a
`Vec<SecretEntry>` with "merge additional secrets" + "dump merged set to disk
for the runner to mount".

### Vault

`cbscore/utils/vault.py` (184 LoC) wraps `hvac`. The Rust port uses `vaultrs`,
which supports KV v1/v2 reads, AppRole login, userpass login, and token renewal.
Authentication order matches the Python: explicit token → AppRole → userpass.

## Version Descriptors & Creation

### Descriptor (in `cbscore-types::versions::desc`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
// NO `rename_all` attribute here — keys stay snake_case
// (`signed_off_by`, `el_version`), matching serde's default of
// using the Rust identifier as the wire key. snake_case is chosen
// to match the existing Python output for operator familiarity;
// cross-language compatibility is not a constraint.
pub struct VersionDescriptor {
    pub version:        String,
    pub title:          String,
    pub signed_off_by:  VersionSignedOffBy,
    pub image:          VersionImage,
    pub components:     Vec<VersionComponent>,
    pub distro:         String,
    pub el_version:     u32,
}
```

`VersionSignedOffBy`, `VersionImage`, and `VersionComponent` follow the same
rule — no `rename_all`, field identifiers are the JSON keys. Read/write match
the Python (`VersionDescriptor.read` / `.write` — JSON with 2-space indent,
newline-terminated).

**Wire-format distinction across the codebase.** Config and secrets files are
YAML with kebab-case keys (`#[serde(rename_all = "kebab-case")]`). Version /
release / container / image / core-component descriptors are JSON with
snake_case keys (**no** `rename_all`). The split is a hard invariant — review
every new struct for which side it falls on before adding a `rename_all`
attribute.

### `VersionType` and parsing (in `cbscore-types::versions::utils`)

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VersionType { Release, Dev, Test, Ci }

pub fn parse_version(s: &str) -> Result<ParsedVersion, MalformedVersion>;
pub fn get_version_type(name: &str) -> Result<VersionType, VersionError>;
pub fn parse_component_refs(components: &[String])
    -> Result<HashMap<String, String>, VersionError>;

/// `ces-v12.3.4-suffix` → `"12.3"`; errors if minor is missing.
/// Mirrors `cbscore/versions/utils.py::get_major_version`.
pub fn get_major_version(v: &str) -> Result<String, MalformedVersion>;

/// `ces-v12.3.4-suffix` → `"12.3.4"`; returns `None` if patch is
/// missing. Mirrors `cbscore/versions/utils.py::get_minor_version`.
pub fn get_minor_version(v: &str) -> Result<Option<String>, MalformedVersion>;

/// Re-emits a parsed version in canonical form:
/// `<prefix>-v<major>.<minor>[.<patch>][-<suffix>]`.
/// Mirrors `cbscore/versions/utils.py::normalize_version`.
pub fn normalize_version(v: &str) -> Result<String, MalformedVersion>;
```

The Python regex is preserved literally (`cbscore/versions/utils.py` lines
45-56). The Rust implementation uses the `regex` crate with the same verbose
pattern. `parse_component_refs` matches `^([\w_-]+)@([\d\w_./-]+)$`.

`get_major_version` / `get_minor_version` / `normalize_version` are not imported
by any in-tree consumer today, but they are part of cbscore's public API and are
included for CLI / library parity.

### Version creation (in `cbscore::versions::create`)

`version_create_helper` (async) in `cbscore/versions/create.py` takes component
name+ref pairs, resolves each git SHA via subprocess `git ls-remote`, and
assembles a `VersionDescriptor`. The Rust port preserves the same signature; git
calls go through `cbscore::utils::git` (subprocess).

The Python `cbsbuild versions create` command writes the resulting descriptor to
a hardcoded `<git-root>/_versions/<type>/<VERSION>.json` path
(`cbscore/cmds/versions.py:88`, with an explicit
`# FIXME: make this configurable` comment). The Rust port treats the
descriptor-store location as configurable; the design lives in
[design 004](004-20260429T1319-configurable-version-descriptor-location.md) and
is currently in the discussion phase. Until that lands, the Rust port preserves
the Python behaviour (hardcoded `<git-root>/_versions/<type>` path).

## Runner Subsystem

The runner is the most distinctive piece of cbscore and the part the Rust port
changes most visibly — binary mount replaces source mount.

### State machine

```
 Idle
   │
   ├── runner(...)
   ▼
 Preparing ───── error ───► Failed
   │
   ├── setup components dir (temp)
   ├── write config + secrets to temp files
   ▼
 Spawning ───── podman error ───► Failed ────► Cleanup
   │
   ├── podman run --cidfile ... cbsbuild ...
   ▼
 Running ──── async_run_cmd drives stdout/stderr ────► Finished
   │                                                │
   │  stop()   ─────► podman_stop(--cidfile) ──┐    │
   │                                           ▼    ▼
   │                                         Stopped  Finished(rc)
   │
   └────────────────────────────► Cleanup (always)
                                    │
                                    ├── rm temp components dir
                                    └── rm temp config/secrets files
```

### In-container mount layout

Exact paths — must match the current Python runner bit-for-bit (see
`cbscore/runner.py` line 255-274 and `cbscore/_tools/cbscore-entrypoint.sh`).

| Host path (generated)       | Mount point                      | Rust port change                                                 |
| --------------------------- | -------------------------------- | ---------------------------------------------------------------- |
| tempfile `descriptor.json`  | `/runner/<name>.json`            | —                                                                |
| `cbsbuild` binary (self)    | `/runner/cbsbuild`               | **new** — replaces `/runner/cbscore` source mount; runs as PID 1 |
| tempfile config             | `/runner/cbs-build.config.yaml`  | —                                                                |
| tempfile secrets            | `/runner/cbs-build.secrets.yaml` | —                                                                |
| config vault (if set)       | `/runner/cbs-build.vault.yaml`   | —                                                                |
| `paths.scratch`             | `/runner/scratch`                | —                                                                |
| `paths.scratch_containers`  | `/var/lib/containers:Z`          | —                                                                |
| components aggregate (temp) | `/runner/components`             | —                                                                |
| `paths.ccache` (if set)     | `/runner/ccache`                 | —                                                                |

### Container entry point

There is no shell entrypoint script. The Python implementation needed one
(`cbscore/_tools/cbscore-entrypoint.sh`, ~60 lines) because cbscore was
source-mounted and the entrypoint had to download `uv`, create a Python 3.13
venv, `uv tool install` the cbscore wheel, and prepend `/runner/bin` to `$PATH`
before invoking `cbsbuild`. With a single static `cbsbuild` binary, none of that
setup exists; the binary becomes the container's PID 1 directly.

The host-side runner spawns podman with `cbsbuild` as the entrypoint command,
supplying the in-container CLI invocation as podman command-line arguments:

```rust
podman_run()
    .arg("--entrypoint").arg("/runner/cbsbuild")
    .arg("-e").arg("HOME=/runner")
    .arg("-e").arg("CBS_DEBUG")
    // ... mounts, --cidfile, --timeout, other env vars ...
    .arg(image_ref)
    .arg("--config").arg("/runner/cbs-build.config.yaml")
    .arg("runner").arg("build")
    .args(user_args)
    .spawn()?;
```

The `--debug` flag is not passed explicitly; `cbsbuild` reads it from the
`CBS_DEBUG` env var via `clap`'s `env` feature
(`#[arg(long, env = "CBS_DEBUG")] debug: bool`). The host runner forwards
`CBS_DEBUG` into the container with `podman run -e CBS_DEBUG`.

The `-e HOME=/runner` flag preserves the Python shell entrypoint's
`HOME=/runner` defaulting. The Python entrypoint set `HOME` only when it was
unset or equal to `/`; the Rust port sets it unconditionally on the podman
command line because `-e HOME=/runner` overrides whatever the image or the host
process exports, which fixes the same edge cases (`--user`-altered HOME, image
without HOME, rootless podman with weird UID maps) without any in-container
code. Tools that fall back to `$HOME` for cache/state (`buildah`, `podman`,
`pip`, ...) write to `/runner/.<dotfile>` in the container's writable layer;
nothing leaks onto the host because `/runner` itself is not bind-mounted (only
specific subpaths under it are).

### Running name generation

`gen_run_name(prefix="ces_")` uses `random.choices(ascii_lowercase, k=10)`. Rust
equivalent with `rand`:

```rust
pub fn gen_run_name(prefix: Option<&str>) -> String {
    use rand::seq::IteratorRandom;
    let mut rng = rand::rng();
    let suffix: String = ('a'..='z').choose_multiple(&mut rng, 10).into_iter().collect();
    format!("{}{}", prefix.unwrap_or("ces_"), suffix)
}
```

### Timeout & cancellation

The Python runner passes a 4-hour default timeout to `podman run --timeout`
**and** wraps `async_run_cmd` in `asyncio.wait_for`. The Rust port keeps both:
`tokio::time::timeout` wrapping the `tokio::process::Command::spawn` + `wait`,
and `--timeout` on the podman command line.

On `tokio::time::timeout` elapsed or `Future::drop` (cancellation), the runner
reads the cidfile and calls `podman_stop(name=cid)` — matching the Python
`except (asyncio.CancelledError, TimeoutError)` block in `utils/podman.py` lines
118-126.

### SIGTERM propagation

`stop(name=None, timeout=1)` calls `podman stop --time 1 <name>` (or `--all`).
Podman forwards SIGTERM to the container's PID 1, which is `cbsbuild` itself —
there is no shell wrapper to traverse. `cbsbuild` installs a tokio signal
handler for SIGTERM that cooperatively cancels the running build future.
Graceful timeout is bounded by the outer `podman stop --time`.

## Build Pipeline

`cbscore/builder/` decomposes into four ordered stages, already cleanly
separated in the Python source. Each stage becomes a free async function in
`cbscore::builder::<stage>`, and the `Builder` struct collapses into a single
orchestration function.

```
┌──────────────────────────────────────────────────────────────┐
│ Input: VersionDescriptor, Config, SecretsMgr, scratch_path   │
└──────────┬───────────────────────────────────────────────────┘
           ▼
 ┌──────────────────┐
 │    prepare       │  cbscore::builder::prepare
 │  - validate      │  - builder/prepare.py
 │  - fetch sources │  - loads components, clones git repos,
 │  - resolve repos │    writes per-component BuildComponentInfo
 └────────┬─────────┘
          ▼
 ┌──────────────────┐
 │    rpmbuild      │  cbscore::builder::rpmbuild
 │  - per-component │  - builder/rpmbuild.py
 │  - rpmbuild -bs  │  - spawns rpmbuild, collects RPMs,
 │  - artifact dir  │    writes ComponentBuild reports
 └────────┬─────────┘
          ▼
 ┌──────────────────┐
 │    signing       │  cbscore::builder::signing
 │  (optional)      │  - builder/signing.py
 │  - GPG detached  │  - gates on Config.signing;
 │  - transit sign  │    supports both gpg and transit
 └────────┬─────────┘
          ▼
 ┌──────────────────┐
 │    upload        │  cbscore::builder::upload
 │  - RPMs → S3     │  - builder/upload.py
 │  - image → reg   │  - gates on Config.storage;
 │  - release.json  │    writes release descriptor
 └────────┬─────────┘
          ▼
┌──────────────────────────────────────────────────────────────┐
│ Output: ReleaseDesc + ArtifactReport written next to scratch │
└──────────────────────────────────────────────────────────────┘
```

### Stage contracts

Each stage is a plain async function (`&Config`, `&VersionDescriptor`,
stage-specific inputs) → `Result<StageReport, BuilderError>`. The orchestrator
composes them:

```rust
pub async fn run_build(
    desc: &VersionDescriptor,
    config: &Config,
    opts: &BuildOptions,
) -> Result<BuildArtifactReport, BuilderError> {
    let prep      = prepare::run(desc, config, opts).await?;
    let rpms      = rpmbuild::run(desc, config, &prep).await?;
    let signed    = signing::run(desc, config, &rpms).await?;
    let uploaded  = upload::run(desc, config, &signed).await?;
    Ok(BuildArtifactReport::new(prep, rpms, signed, uploaded))
}
```

`skip_build` and `force` (Python `Builder.__init__` kwargs) become fields on
`BuildOptions`.

### Cancellation

Every stage is an async fn; dropping the future cancels cleanly. The runner owns
the top-level future and can `tokio::select!` it against a SIGTERM signal.

## Subprocess & Secret Redaction

`cbscore/utils/__init__.py` defines the subprocess wrapper that the entire
library runs on top of:

- `run_cmd(cmd, env)` — sync wrapper around `subprocess.run`
- `async_run_cmd(cmd, outcb, timeout, cwd, extra_env)` — async wrapper with
  line-granular stream callback (the Python `reset_python_env` flag is
  intentionally **not ported** — the Rust `cbsbuild` binary has no
  venv-shadowing problem to solve)
- `SecureArg` / `Password` / `PasswordArg` / `SecureURL` — secret carriers with
  `__str__` that redacts
- `_sanitize_cmd(cmd)` — walks a command, replaces secret args with `"****"` and
  handles the `--pass[phrase]` flag pattern

The Rust port preserves this surface with type-level guarantees.

### `SecureArg` trait

```rust
// In cbscore::utils::subprocess

pub trait SecureArg {
    /// Rendered cleartext — only used when actually spawning.
    fn plaintext(&self) -> Cow<'_, str>;
    /// Rendered redacted — used for tracing / error messages.
    fn redacted(&self) -> Cow<'_, str> { Cow::Borrowed("****") }
}

pub enum CmdArg {
    Plain(String),
    Secure(Box<dyn SecureArg + Send + Sync>),
}

impl Debug for CmdArg {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CmdArg::Plain(s)  => Debug::fmt(s, f),
            CmdArg::Secure(a) => f.write_str(&a.redacted()),
        }
    }
}
```

Concrete `SecureArg` impls cover the Python types:

```rust
pub struct Password(String);
pub struct PasswordArg { arg: String, password: Password }
pub struct SecureUrl    { template: String, args: Vec<(String, CmdArg)> }
```

Tracing lines never see plaintext because the `Debug`/`Display` impls emit the
redacted form. The plaintext path only hits `tokio::process::Command::arg`.

### `async_run_cmd`

```rust
pub async fn async_run_cmd(
    cmd:    &[CmdArg],
    opts:   RunOpts<'_>,
) -> Result<RunOutcome, CommandError>;

pub struct RunOpts<'a> {
    pub timeout:           Option<Duration>,
    pub cwd:               Option<&'a Utf8Path>,
    pub extra_env:         Option<&'a HashMap<String, String>>,
    pub out_cb:            Option<Box<dyn FnMut(&str) -> BoxFuture<'static, ()> + Send>>,
}

pub struct RunOutcome { pub rc: i32, pub stdout: String, pub stderr: String }
```

- Uses `tokio::process::Command` with stdout/stderr piped.
- Streams both pipes concurrently (`tokio::spawn` per pipe, joined at the end)
  and calls the `out_cb` per line when present; otherwise accumulates into the
  returned `String`s.
- Honours `timeout` via `tokio::time::timeout` **internally**.

**Timeout / cancellation contract.** `async_run_cmd` owns its timeout entirely.
The caller never observes cancellation as a `Future::drop`:

1. **Internal timeout fires** → call `Child::start_kill()` (SIGKILL on unix),
   `Child::wait().await` to reap, return `Err(CommandError::Timeout { after })`
   with partial stdout / stderr captured up to the kill. This is a _behaviour
   change_ vs. the Python, which re-raises `asyncio.TimeoutError` to the caller;
   the Rust port converts timeout to a domain error so the call site has one
   error type to match on.
2. **Future dropped by outer cancellation** (e.g. a `tokio::select!` branch
   loses) → `Child::start_kill()` runs in the `Drop` impl of an internal RAII
   guard, so the child is killed even if the future is not polled to completion.
   Reaping happens in the guard's `Drop`, best-effort.

**Runner cleanup path.** `runner::run` is the only caller that cares about the
cidfile + `podman_stop` dance:

```rust
match async_run_cmd(&cmd, opts).await {
    Ok(out) if out.rc == 0 => { /* success */ }
    Ok(out)                  => { /* non-zero exit */ }
    Err(CommandError::Timeout { .. }) => {
        // child already killed by async_run_cmd; we still need to
        // tell podman to remove the container (so --replace works
        // on the next run).
        if let Ok(cid) = tokio::fs::read_to_string(&cidfile).await {
            podman_stop(Some(cid.trim()), Duration::from_secs(1)).await.ok();
        }
        return Err(RunnerError::Timeout);
    }
    Err(e) => return Err(RunnerError::Command(e)),
}
```

Defence-in-depth: the caller may _also_ wrap the `async_run_cmd` call in an
outer `tokio::time::timeout` with a longer budget, but the inner timeout is
load-bearing — the outer one only catches runaway internal tasks (streaming
callbacks stuck in user code, etc.), not the subprocess itself.

### `--passphrase` / `--pass` sanitiser

`_sanitize_cmd` has a special case: bare strings like `--passphrase foo` (two
tokens) and `--passphrase=foo` (one token). The Rust port keeps both:

```rust
fn redact_inline(s: &str) -> Cow<'_, str> {
    // "(--pass(?:phrase)?[\s=]+)[^\s]+" → group1 + "****"
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(--pass(?:phrase)?[\s=]+)[^\s]+").unwrap()
    });
    re.replace_all(s, "$1****")
}
```

## Image Sign & Sync

Two distinct subsystems with a shared GPG / transit-signing dependency.

### Skopeo driver

`cbscore/images/skopeo.py` wraps `skopeo copy` and `skopeo inspect`. The Rust
port uses the same subprocess pattern. Input is a source image URI + destination
URI + optional TLS / auth flags. The Python `skopeo_image_exists` and
`skopeo_copy` become free async functions.

### Image signing

`cbscore/images/signing.py` implements two signing backends:

1. **GPG detached signatures.** Invokes `gpg2 --detach-sign` on the image
   manifest. Needs a GPG home dir; cbscore generates one from the secrets store
   in a tempdir and points `GNUPGHOME` at it for the subprocess.
2. **Vault transit signing.** POSTs the manifest digest to Vault transit and
   captures the returned signature. Uses `vaultrs`.

Which backend is used is gated on `Config.signing.gpg` vs
`Config.signing.transit`. Both may be absent, in which case signing is skipped
(recent Python commit `d2e8a91 cbscore: make signing optional`).

### Image sync

`cbscore/images/sync.py` orchestrates "copy from source registry to destination
registry, optionally signing along the way". The Rust port keeps the shape; its
only interesting choice is the **order of operations** — sign before push, not
after — which matches Python and is a tested precondition of the downstream
registry tooling.

## Releases & S3

### Descriptor

`cbscore/releases/desc.py` defines the release data model. The Rust port is
almost a line-by-line translation:

```rust
// `snake_case` on the enum is a Rust→Rust identifier convention,
// not a wire-format change: the variant name `X86_64` would
// otherwise serialise to `X8664` in JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchType { X86_64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildType { Rpm }

// NO `rename_all` on structs below — keys stay snake_case
// (`build_type`, `os_version`, `repo_url`, `release_rpm_loc`, ...),
// matching serde's default of using the Rust identifier as the
// wire key. snake_case is chosen to match the existing Python
// output for operator familiarity; cross-language compatibility
// is not a constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInfo {
    pub arch:       ArchType,
    pub build_type: BuildType,
    pub os_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseComponentVersion {
    // flattens ReleaseComponentHeader + BuildInfo, per Python
    pub name:       String,
    pub version:    String,
    pub sha1:       String,
    pub arch:       ArchType,
    pub build_type: BuildType,
    pub os_version: String,
    pub repo_url:   String,
    pub artifacts:  ReleaseArtifacts,
}
```

Python uses pydantic's multiple-inheritance to flatten
`ReleaseComponentHeader + BuildInfo` into `ReleaseComponentVersion`. Rust
flattens at the struct level.

### S3 operations

`cbscore/releases/s3.py` (283 LoC) + `cbscore/utils/s3.py` (376 LoC) translate
to the `aws-sdk-s3` crate. The key operations:

- `check_release_exists(bucket, loc, version)` — HEAD object
- `check_released_components(...)` — list objects with prefix
- `release_desc_upload(...)` — PUT object with body
- `release_upload_components(...)` — bulk PUT with content-type detection
- `s3_upload_rpms(...)` — used by `builder::upload`

Auth is AWS-SDK-native (env or shared credential file); the Python uses
`aioboto3` which reads the same env vars, so there's no behaviour change at the
deployment level.

## Core Components

`cbscore/core/component.py` loads `cbs.component.yaml` files from a list of
directories. The Rust port reads the YAML using the types in
`cbscore-types::core::component` and walks directories with
`tokio::fs::read_dir`.

`cbsd` imports the Python signature
`load_components(paths: list[Path]) -> dict[str, CoreComponentLoc]` from the
existing Python `cbscore` package and continues to do so; the Rust port exposes
an equivalent function on the `cbscore` library crate for the Rust-side
consumers (`cbsbuild`, `cbsd-worker`).

## Logging

The Python package uses a hand-rolled logger hierarchy rooted at
`logging.getLogger("cbscore")`, with `getChild()` calls in every module
producing `cbscore.runner`, `cbscore.builder`, `cbscore.utils.podman`, etc. The
`-d/--debug` CLI flag calls `set_log_level(DEBUG)` on the root logger;
`CBS_DEBUG=1` does the same inside the runner container.

In Rust this maps onto `tracing`:

- Each module sets its target: `tracing::info!(target: "cbscore::runner", ...)`
  — by default the target defaults to the module path, which already matches the
  Python naming convention if modules are kept in sync.
- The `cbsbuild` binary configures an `EnvFilter`. Default: `cbscore=info`.
  `CBS_DEBUG=1` → `cbscore=debug`.
- `Config.logging.log_file` routes a file appender via
  `tracing-appender::rolling::never`, preserving current behaviour (a single log
  file, no rotation). Log rotation is deliberately left to the host
  (`logrotate`) and out of scope for the port.
- In-container logging writes to `/runner/logs/cbs-build.log` (set by
  `runner::run` on `new_config.logging`).

Python consumers that import `from cbscore.logger import set_debug_logging`
continue to call into the existing Python `cbscore` package — the Rust library
does not expose this symbol across a language boundary. Rust-side debug output
is controlled by `CBS_DEBUG=1`, which the `EnvFilter` the Rust library installs
picks up at startup.

## CLI Surface

The `cbsbuild` CLI structure, mirroring `cbscore/cmds/`:

```
cbsbuild [-d|--debug] [-c|--config PATH] <subcommand>

├── build <descriptor.json>
│      Run a build via the runner. Options: --skip-build, --force,
│      --tls-verify/--no-tls-verify.
│
├── runner
│   ├── run <descriptor.json>           (what `build` actually does)
│   ├── stop [--name NAME]              (podman stop by name, or --all)
│   └── ...
│
├── versions
│   ├── create -c COMPONENT@REF [...] <version>
│   ├── list [--path DIR]
│   ├── show <descriptor.json>
│   └── validate <descriptor.json>
│
├── config
│   ├── init          (interactive)
│   ├── show
│   ├── check
│   └── ...
│
└── advanced
    └── ... (escape-hatch subcommands; rare)
```

- **Global flags match the Python click flags exactly.** `-c/--config` defaults
  to `cbs-build.config.yaml`. `-d/--debug` reads env var `CBS_DEBUG`.
- **Exit codes.** `errno.ENOTRECOVERABLE` (131) on unhandled panic / error,
  `errno.EINVAL` (22) on missing config — ported from `cbscore/__main__.py`
  lines 73-77 and `cbscore/cmds/__init__.py` lines 43-49.
- **`--cbscore-path` is deliberately dropped.** The Python `cbsbuild build`
  requires `--cbscore-path PATH`; the Rust runner mounts the binary itself into
  the container and the flag is no longer meaningful. This is an intentional CLI
  UX parity break (Correctness Invariant 2) recorded in full under the
  `cbsbuild` crate section of design 001. Operators and any out-of-tree scripts
  that pass `--cbscore-path` must be notified before M1 ships; invocations that
  still pass it will fail with a clap "unexpected argument" error.

## Migration Strategy

The rewrite cannot happen in one cut-over. Four milestones, in order:

### M0 — New crates + types

Add `cbscore-types` as a new member of the existing `cbsd-rs/` Cargo workspace.
The crate carries the full descriptor / config / error surface and no IO. Gate
on `cargo test` round-tripping a curated set of real-world config and descriptor
YAML/JSON files. No Python dependency changes yet; the Python `cbscore`
continues to serve production.

### M1 — Library crate feature parity

Implement `cbscore/src/lib.rs` piece by piece, in dependency order that matches
the subsystem diagram: subprocess → podman/buildah/ skopeo → git → s3 → vault →
secrets → config IO → runner → builder stages → releases → images. Each crate
subsystem is a commit per the `git-commits` skill rules. End state: `cargo run`
the `cbsbuild` CLI, execute a build of the real `ceph` component from
`components/ceph`, and compare the produced RPM set to the Python output.
Acceptance: same RPMs (byte-identical RPM payloads — these are the artifacts
operators consume), and a Rust-written release descriptor that round-trips
through cbscore-rs (write → load → equal) and contains the same semantic content
as the Python equivalent. Cross-language byte-equality on the release descriptor
is not required since Python and Rust cbscore do not share files.

M1 is cbscore-rs **1.0.0**. Python `cbscore/` ships unmodified. Every
wire-format file Rust reads or writes carries `schema_version: 1` (see §
Wire-Format Versioning); files without the tag are rejected with a clear error.
Additionally, git-secret entries require the new `type:` discriminator (see §
Secrets). Operators moving a deployment from Python to Rust re-tag their
`secrets.yaml`, `cbs-build.config.yaml`, `cbs.component.yaml`, and sibling files
once by hand. Python can still read Rust- written files (pydantic ignores the
unknown `schema_version` and `type:` fields), so compatibility is one-way. See
design 001 § Versioning for the semver discipline.

### M2 — Direct crate dependency from `cbsd-rs`

`cbsd-rs/cbsd-worker` replaces `scripts/cbscore-wrapper.py` with a Cargo
dependency on `cbscore`. This retires the Python bridge and removes the Python
3.13 interpreter + `cbscore` install from the worker container image. The
`cbsd-worker` → `cbscore` surface is small (`runner`, `Config`,
`version_create_helper`, `VersionDescriptor`, errors) — see the wrapper file's
imports for the exact list.

### M3 — Python consumer migration or retirement

The remaining Python consumers (`cbc`, `crt`, `cbsdcore`, `cbsd`) continue
importing from the existing Python `cbscore` package unchanged — the Rust
rewrite does not provide a Python binding. `cbsd` and `cbsdcore` are themselves
candidates for future Rust rewrites (mirroring the `cbsd-rs` effort); `cbc` and
`crt` may be rewritten in Rust alongside or after them. Once every Python
consumer is migrated or retired, the Python `cbscore/` package is deleted from
the repository.

### Rollout considerations

A worker fleet may share the source content of a `secrets.yaml` (e.g., a k8s
ConfigMap, an NFS mount, a config-management-managed file). During a rolling
upgrade where some workers are still on Python cbscore and some are on Rust
cbscore:

- Python workers continue reading and writing the existing un-tagged shape.
- Rust workers reject any `secrets.yaml` lacking `schema_version: 1` or the
  per-entry `type:` tag for git secrets (see § Wire-Format Versioning and §
  Configuration & Secrets Subsystem).

To avoid Rust workers hard-failing during rollout, **tag the source
`secrets.yaml` first** — add `schema_version: 1` and the per-git-entry `type:`
tags before deploying the first Rust worker. Pydantic's `extra = "ignore"`
silently drops the extra keys on the Python side, so tagging the file early does
not break workers still running Python cbscore. After every worker has been
upgraded, the source file remains in the new shape; no re-tag step at the end of
the rollout.

This guidance applies regardless of mount mechanism: the runner copies
`secrets.yaml` to a tmpfile and mounts the tmpfile into the podman container, so
the source file is read at runner-spawn time and any in-flight build works on
its own snapshot — but the tag must be present at the source the moment a Rust
worker first reads it.

### Rollback

Every milestone is independently revertable. All of M0–M3 happen inside
`cbsd-rs/` (Rust crates + worker dep) and the existing Python `cbscore/` package
(which stays in-tree throughout). Rolling back M2 means reinstating
`cbscore-wrapper.py` and the Python runtime in the worker image. No database
migrations, no cross-repo coordination.

## Open Questions

- **git: `git2` crate vs subprocess.** **Resolved: subprocess to `git` via
  `tokio::process`.** `git2` binds libgit2 (C, BSD-licensed, plus
  OpenSSL/libssh2 transitively); subprocess to `git` matches Python behaviour
  exactly, integrates transparently with `ssh-agent`, `~/.ssh/config`, and
  `git-credential-*`, and keeps the builder image's existing `git` package as
  the only dependency. Minimum required version: **git 2.23** (August 2019), set
  by `git switch` and `git branch --show-current` used in
  `cbscommon/git/cmds.py`. All currently supported builder bases ship git well
  above this floor: Alpine 3.21 (git 2.47), Debian 12 (git 2.39), Ubuntu 22.04
  (git 2.34), RHEL/Rocky/Alma 9 (git 2.39), RHEL/Rocky/Alma 8 AppStream (git
  2.43). The implementation lives behind a `cbscore::utils::git` module so it
  can be swapped later if a specific operation justifies linking libgit2.
- **GPG: sequoia-openpgp vs subprocess.** **Resolved: subprocess to `gpg2` via
  `tokio::process`.** RPM signing goes through `rpm --addsign`, which itself
  invokes gpg internally; replacing gpg with `sequoia-openpgp` would not let us
  drop the gpg binary, since `rpm-sign` requires it transitively. The only
  direct gpg call (`gpg --import --batch` for keyring import in
  `utils/secrets/signing.py`) is trivial and gains nothing from a pure-Rust
  port. Minimum required version: **gpg 2.1** (loopback pinentry mode, used to
  pass passphrases non-interactively to rpm via `_gpg_sign_cmd_extra_args`). All
  currently supported builder bases ship gpg well above this floor: Alpine 3.21
  (gpg 2.4), Debian 12 (gpg 2.2), RHEL 9 / Rocky 9 / Alma 9 (gpg 2.3). The
  implementation lives behind a `cbscore::utils::gpg` module so it can be
  swapped later if signature verification of arbitrary blobs becomes a use case.
- **`camino` vs `std::path`.** **Resolved: `camino` (`Utf8Path` / `Utf8PathBuf`)
  at all cbscore API boundaries, paired with `camino-tempfile` for tempfile
  interop.** cbscore paths are consumed as strings throughout — subprocess args,
  log messages, serialized config (YAML/JSON), container mount specs, S3 keys —
  so the contract is "UTF-8, validated at construction". `std::path::Path` would
  force `.to_str().ok_or(...)?` at every boundary or `.to_string_lossy()` (which
  silently corrupts non-UTF-8 paths). The Python implementation already assumes
  UTF-8 paths implicitly via `Path.as_posix()`. Use `camino` v1 with the
  `serde1` feature; bridge to `std::path::Path` only at FFI points where
  third-party crates require it.
- **Serde YAML crate.** `serde_yaml` is unmaintained. **Resolved:
  `serde_saphyr`** — the serde adapter for the saphyr YAML implementation.
  Round-trip fidelity against Python's `yaml.safe_dump` is verified via
  golden-file tests at M0 (see § Configuration & Secrets Subsystem).
- **`reset_python_env` not ported.** **Resolved: do not port the flag.** The
  Python `_reset_python_env` workaround scrubs the venv `python3` directory from
  `PATH` so child processes (`rpmbuild`, `do_cmake`, etc.) find the system
  `/usr/bin/python3` rather than the venv's Python. This is necessary only
  because Python cbscore runs from a `uv`-managed venv. The Rust `cbsbuild`
  binary is invoked directly with no venv on `PATH`, and the runner mounts
  `cbsbuild` (not a venv) into the builder container — so the workaround is moot
  on both host and in-container. The existing
  `cbsd-rs/scripts/cbscore-wrapper.py` bridge keeps the flag in Python until the
  wrapper retires at M2; nothing on the Rust side picks it up.
- **Release-descriptor backward compatibility.** **Resolved: no cross-language
  compatibility constraint.** The Rust port may pick whatever JSON shape `serde`
  produces from the descriptor structs. There is no file interchange in steady
  state — Python and Rust executables do not read each other's release
  descriptors after the cutover. Existing Python-written release descriptors in
  S3 are regenerated or migrated at cutover time (see Migration Strategy).
  Round-trip tests on the Rust side (serialize → parse → equal) are sufficient;
  no golden-file test against pydantic output is required.
- **Builder image Python dependency.** **Resolved: drop only the cbscore Python
  wheel (and its `uv` / venv installation), keep system `python3`.** Removing
  system `python3` from the builder image is impossible: Ceph's `do_cmake.sh`
  invokes Python during cmake configuration; many RPMs in the Ceph manifest
  require Python (`python3-rtslib`, `python3-saml`, `python3-mgr-*`, etc.); RPM
  specs themselves embed Python in `%prep` / `%build` / `%install` phases. What
  the Rust port removes from the current `cbscore-entrypoint.sh`:
  - `curl https://astral.sh/uv/install.sh | sh` (lines 31-35).
  - `uv venv --python 3.13` and `source venv/bin/activate` (lines 39-43).
  - `uv tool install` of the cbscore wheel (lines 48-52).
  - The host-side mount of `${RUNNER_PATH}/cbscore` (the cbscore source tree).

  The Rust port mounts the `cbsbuild` static binary at `/runner/cbsbuild`
  (matching the mount table in § Runner Subsystem) and uses it as the
  container's PID 1 directly — there is no shell entrypoint wrapper. System
  `python3` (provided by the EL9-derived base image) remains as a transitive
  build dep for Ceph and other components — this is **not** a "Python in the
  builder" dependency from cbscore's perspective; it is a Ceph build dependency
  that cbscore inherits.

- **Interactive `config init`.** **Resolved: deferred — out of M1 scope.** M1
  ships `cbsbuild config init` with the existing non-interactive flag modes
  only: `--for-systemd-install`, `--for-containerized-run`, and the per-field
  overrides (`--components`, `--scratch`, `--containers-scratch`, `--ccache`,
  `--vault`, `--secrets`). Running `cbsbuild config init` without any of these
  flags produces an error with a usage hint, not an interactive prompt. The
  interactive prompt-based UX is designed separately in
  [design 003](003-20260427T1255-interactive-config-init.md) and implemented
  post-M1.
