# cbscore-rs

Rust rewrite of `cbscore/`. Replaces the Python 3 + click + pydantic +
aioboto3 + hvac stack with Rust equivalents (exact crate choices decided
in the design phase).

Planned outputs:

- **cbsbuild** — CLI binary. Drop-in replacement for today's Python
  `cbsbuild`: same subcommand tree (`build`, `runner`, `versions`,
  `config`, `advanced`), same config/descriptor formats, same exit
  codes and stdout/stderr contract.
- **cbscore library crate(s)** — reusable pieces (version descriptors,
  config types, podman/buildah/skopeo wrappers, S3/Vault/secrets
  helpers) consumed directly by the sibling `cbsd-worker` crate in
  the same workspace, and by any future Rust rewrites of the Python
  packages (`cbc`, `crt`, `cbsdcore`).

This directory holds the rewrite planning effort: design documents,
plans, and reviews. The rewrite itself will be added as new crates
inside the existing `cbsd-rs/` Cargo workspace — it does not get its
own top-level workspace.

## Goals

- **Functional parity** with the existing `cbscore` Python package.
- **Drop-in replacement** for the `cbsbuild` CLI (same UX, same config
  file formats, same version-descriptor JSON, same on-disk layout) so
  the rewrite can land without coordinated changes to sibling
  repositories.
- **API surface** that covers what today's in-repo Python consumers
  import from `cbscore`: `cbsd`, `cbsdcore`, `cbc`, `crt`. These
  consumers are themselves Python and keep using the existing Python
  `cbscore` package unchanged; the Rust rewrite does not provide a
  binding for them. They switch to Rust only if and when they are
  themselves rewritten.
- **Rust 2024 idioms** — strong typing, `thiserror`/`anyhow` error
  model, `tokio` for async, `clap` for CLI, `serde` for descriptors
  and config, `tracing` for logging.

## Scope (what `cbscore` does today)

The existing Python package (~9.8k LoC) is responsible for:

- **CLI (`cbsbuild`)** — top-level command group with `build`,
  `runner`, `versions`, `config`, and `advanced` subcommands
  (`cbscore/cmds/`, `cbscore/__main__.py`).
- **Configuration** — YAML/JSON config, Vault config, secrets
  loading, storage/signing/logging sections (`cbscore/config.py`,
  `cbscore/utils/vault.py`, `cbscore/utils/secrets/`).
- **Version descriptors** — parsing, validation, creation, version
  type classification (`cbscore/versions/`).
- **Container/image descriptors** — pre/post scripts, package lists,
  repo definitions (copr, file, url), container build orchestration
  via buildah/podman (`cbscore/containers/`, `cbscore/utils/buildah.py`,
  `cbscore/utils/podman.py`).
- **Image signing & sync** — GPG and Vault-transit signing, skopeo
  copy/sync (`cbscore/images/`).
- **Release descriptors & S3 artifacts** — release metadata, RPM
  artifact layout, S3 upload/download
  (`cbscore/releases/`, `cbscore/utils/s3.py`).
- **Build pipeline** — prepare, rpmbuild, sign, upload stages
  (`cbscore/builder/`).
- **Runner** — spins up a podman container that re-enters the same
  CLI inside a controlled environment (`cbscore/runner.py`,
  `cbscore/_tools/cbscore-entrypoint.sh`).
- **Core components** — loading `cbs.component.yaml` descriptors
  (`cbscore/core/component.py`).
- **Utilities** — git operations, path helpers, URI parsing,
  subprocess wrappers with secret redaction.

## Downstream consumers (in-repo)

| Consumer   | Imports from `cbscore`                                                                                                                                                  |
|------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `cbc`      | `errors.CESError`, `logger.set_debug_logging`, `versions.errors.VersionError`, `versions.utils.{VersionType, get_version_type, parse_component_refs}`                    |
| `crt`      | `versions.utils.parse_version`                                                                                                                                          |
| `cbsd`     | `errors.{CESError, MalformedVersionError}`, `logger`, `config.{Config, ConfigError}`, `runner.{stop, gen_run_name, runner}`, `versions.{create, desc, errors}`, `core.component.load_components` |
| `cbsdcore` | `errors.CESError`, `versions.utils.VersionType`                                                                                                                         |

Until these Python packages are themselves migrated, the rewrite must
keep the Python `cbscore` package working — the Python consumers
continue to import the existing Python `cbscore` package unchanged.
Wire-format compatibility (shared JSON/YAML files, CLI output parity)
guarantees that Rust-written artefacts remain loadable by the Python
side during the transition.

### Relationship to `cbsd-rs`

`cbsd-rs/` is the Rust rewrite of `cbsd` and is already in-tree. Its
worker today invokes `cbscore` via a Python subprocess bridge
(`cbsd-rs/scripts/cbscore-wrapper.py`) because the Rust half of the
stack cannot link against a Python library. The cbscore rewrite
lands as new member crates of the same `cbsd-rs/` workspace, which
lets `cbsd-worker` depend on them directly and retires the
subprocess bridge. Keeping the `cbsd-rs` wire contracts stable
(build-descriptor JSON, component tarball layout, exit codes, log
format) is therefore an explicit constraint on this rewrite.

## Configuration

All YAML configuration keys use **kebab-case**, matching today's
`cbs-build.config.yaml` (`vault-addr`, `role-id`, `scratch-containers`,
`log-file`, …). The Rust descriptors and config structs will carry
`#[serde(rename = "kebab-case")]` attributes (or equivalent
`#[serde(rename_all = "kebab-case")]` at the container level) so the
rewrite consumes existing config files unchanged.

## Repository layout

Documentation for the rewrite lives under `cbsd-rs/docs/cbscore-rs/`:

```
cbsd-rs/docs/cbscore-rs/
├── README.md      # this file
├── CLAUDE.md      # instructions for AI-assisted work on this folder
├── design/        # design documents for the Rust rewrite
├── plans/         # implementation plans with progress tracking
└── reviews/       # review documents
```

Doc naming and sequencing rules are defined by the `cbscore-rs-docs`
skill.

The rewrite's Rust code lands as new member crates of the existing
`cbsd-rs/` Cargo workspace, alongside the current `cbsd-proto`,
`cbsd-server`, `cbsd-worker`, and `cbc` crates. Exact crate names
and boundaries are a design-phase deliverable.

## Licensing

This rewrite inherits `cbscore`'s license: **GPL-3.0-or-later**.
