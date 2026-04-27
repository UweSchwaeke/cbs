# CLAUDE.md — cbscore-rs

Rust rewrite of `cbscore/`. Replaces the Python 3 + click + pydantic +
aioboto3 + hvac stack with Rust equivalents (exact crate choices decided in the
design phase).

## Skills

Always consult these skills during implementation:

- **`/rust-2024`** — Rust 2024 edition: project structure, error handling, trait
  design, async patterns (tokio), clap, serde, thiserror, anyhow, tracing.
- **`/git-commit-messages`** — Commit message formatting and logical change
  boundaries. Ceph project conventions.
- **`/git-autonomous-commits`** — Autonomous git operations: staging, pre-commit
  checks, self-review, commit strategy.
- **`/cbscore-rs-docs`** — Where to place and how to name design documents,
  plans, and review documents for `cbscore` and `cbscore-rs` packages.

## Three-Phase Workflow

Work proceeds in three ordered phases — do not jump ahead unless the user
explicitly asks:

1. **Plan** — produce design + plan documents under `cbsd-rs/docs/cbscore-rs/`,
   then pause for human review.
2. **Review** — record review outcomes as versioned review documents and revise
   the design/plan accordingly.
3. **Implement** — land the new crates inside the existing `cbsd-rs/` Cargo
   workspace commit by commit, following the approved plan.

## Workspace Layout

The `cbsd-rs/` Cargo workspace already exists and hosts the cbsd rewrite crates.
The cbscore rewrite lands as **additional member crates of the same workspace**
— there is no separate `cbscore-rs/` Cargo workspace. Exact crate names and
boundaries are a design-phase deliverable. **Current expectation** (provisional
— update this block once the design lands):

```
cbsd-rs/
├── Cargo.toml          # existing workspace root
├── Cargo.lock
├── cbsd-proto/         # existing — cbsd wire types
├── cbsd-server/        # existing — cbsd REST API + WebSocket server
├── cbsd-worker/        # existing — cbsd worker (WS client + subprocess)
├── cbc/                # existing — CLI client for cbsd
├── cbscore-proto/      # NEW — shared cbscore types: descriptors,
│                       #       config, errors (no IO)
├── cbscore/            # NEW — library crate: runner, builder,
│                       #       podman/buildah/skopeo wrappers,
│                       #       S3/Vault/secrets, git/uri helpers
└── cbsbuild/           # NEW — CLI binary, clap tree mirroring
                        #       cbscore/cmds/
```

- **`cbscore-proto`** — VersionDescriptor, ContainerDescriptor, ReleaseDesc,
  Config, SigningConfig, scope/type enums, error types. Zero IO dependencies
  (serde, serde_json, serde_yaml, chrono only). Depended on by the `cbscore`
  library, `cbsbuild`, and `cbsd-worker` for compile-time wire-format agreement.
- **`cbscore`** — subsystem wrappers (podman/buildah/skopeo, S3 via aws-sdk,
  Vault via vaultrs, GPG/transit signing, git, secrets store), build-pipeline
  stages (prepare, rpmbuild, sign, upload), and the podman-based runner.
- **`cbsbuild`** — `clap` v4 CLI mirroring `cbscore/cmds/` (`build`, `runner`,
  `versions`, `config`, `advanced`). Preserves the existing UX byte-for-byte.

## Build & Test

Run from `cbsd-rs/`:

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace
cargo fmt --all --check
```

These operate on every workspace member, existing and new alike.

## Pre-Commit Checks

Before every commit, run these checks **in order** on all modified files:

```bash
cargo fmt --all                # 1. format
cargo clippy --workspace       # 2. lint (fix any warnings)
cargo check --workspace        # 3. compile check
```

All three must pass with zero errors and zero warnings before staging.
Planning-phase commits that touch only `cbsd-rs/docs/cbscore-rs/` may skip these
— they do not modify any Rust code.

## Git Conventions

```bash
# All commits use this form:
git -c commit.gpgsign=false commit -s
```

- DCO sign-off (`-s`) required on every commit
- Never GPG-sign commits autonomously
- Autonomous commits where Claude made changes MUST include exactly one
  `Co-authored-by` trailer after the message body. Never stack multiples. Use
  the model name matching the active Claude instance, e.g.:

  ```
  Co-authored-by: Claude Opus 4.7 <noreply@anthropic.com>
  ```

  This applies to all commits touching the cbscore rewrite — the new member
  crates inside the `cbsd-rs/` Cargo workspace and the docs at
  `cbsd-rs/docs/cbscore-rs/`, including subprojects, documentation, and tooling
  (`.claude/skills/`, etc.).

- Separate `git add` and `git commit` commands (not chained)
- Ceph project commit message style

## Commit Granularity

Each commit should be the **smallest compilable, testable, logical unit** — but
never so small that it's meaningless in isolation.

- When a planned commit has naturally separable subsystems with clean dependency
  boundaries, **split at those boundaries** to reduce blast radius.
- When parts are tightly coupled (one doesn't work without the other), **keep
  them together** — splitting would create broken intermediate commits.
- Target ~400–800 authored lines per commit. Above 800, look for a natural
  split. Below 200, consider whether the commit is meaningful alone.
- A descriptor module + the subsystem wrapper consuming it = two commits if the
  descriptor module is independently testable. One commit if the wrapper is the
  only way to exercise it.
- **The test:** Can someone reviewing this commit understand its purpose in one
  sentence? Can the previous commit compile and pass tests? Could this commit be
  reverted without breaking unrelated functionality?

## Design & Plans

- **Design documents (authoritative):** `cbsd-rs/docs/cbscore-rs/design/`
  - Workspace layout, CLI surface, descriptor/config types, runner, incremental
    migration plan.
  - If code and design disagree, **fix the code**.
- **Implementation plans:** `cbsd-rs/docs/cbscore-rs/plans/`
  - Phased commit plan with progress tracking tables.
  - **Update plan progress tables after completing each commit.**
- See `/cbscore-rs-docs` skill for file naming and directory conventions
  (sequence numbers, timestamps, review versioning).

## Key Reference Files

Python source to port / consult for behaviour:

- `cbscore/src/cbscore/__main__.py` — CLI entry point
- `cbscore/src/cbscore/cmds/{builds,versions,config,advanced}.py` — CLI
  subcommand tree (mirror in `clap`)
- `cbscore/src/cbscore/config.py` — top-level config model (pydantic → serde)
- `cbscore/src/cbscore/versions/{desc,utils,create,errors}.py` — most
  widely-consumed submodule; imported by `cbc`, `crt`, `cbsd`, `cbsdcore`
- `cbscore/src/cbscore/runner.py` +
  `cbscore/src/cbscore/_tools/cbscore-entrypoint.sh` — podman-based runner and
  its in-container entrypoint
- `cbscore/src/cbscore/builder/` — RPM build / sign / upload stages
- `cbscore/src/cbscore/utils/secrets/` — secrets model, git, signing
- `cbscore/src/cbscore/utils/{buildah,podman,s3,vault,git}.py` — subsystem
  wrappers

Current Python-side bridges (compatibility anchors):

- `cbsd/cbslib/worker/builder.py` — biggest in-process consumer of `cbscore`
- `cbc/src/cbc/cmds/_shared.py`, `crt/src/crt/crtlib/manifest.py` —
  version-parsing consumers
- `cbsd-rs/scripts/cbscore-wrapper.py` — Python subprocess bridge from the Rust
  `cbsd-worker`; retired once the cbscore rewrite crates land in `cbsd-rs/`

## Principles

Rust code in this workspace must follow these principles. Review each one before
landing non-trivial changes.

### SOLID

- **S — Single Responsibility.** Each module, trait, struct, or function has one
  reason to change. If you can describe a unit's purpose with "X _and_ Y", split
  it.
- **O — Open/Closed.** Types should be open for extension, closed for
  modification. In Rust this usually means defining a trait and adding new
  implementors, rather than growing a `match` arm on a closed enum that
  downstream code can't extend.
- **L — Liskov Substitution.** Any `impl Trait for T` must honour every contract
  documented on the trait (return-value invariants, panics, error cases,
  ordering). Code written against the trait must keep working with every
  implementor.
- **I — Interface Segregation.** Prefer many small, focused traits over one wide
  trait. A caller that only needs `Read` should not have to depend on
  `Write + Seek` as well.
- **D — Dependency Inversion.** High-level modules depend on abstractions
  (traits), not concrete types. Inject dependencies via generics
  (`T: SomeTrait`) or trait objects (`&dyn SomeTrait`) rather than hard-wiring a
  concrete type.

### DRY — Don't Repeat Yourself

Every piece of knowledge has a single authoritative representation. If two
modules encode the same rule, constant, format, or SQL/YAML key, they will
drift; extract the shared piece into one place. (Copying three lines is
sometimes cheaper than a premature abstraction; copying an invariant never is.)

### KISS — Keep It Simple

The simplest solution that satisfies the requirement wins. Reject cleverness
that saves a line at the cost of readability. If a reviewer has to re-read a
block twice to understand what it does, rewrite it.

### Visibility — as private as possible, as public as needed

Every item (function, struct, field, variable, module) starts private. Widen
visibility only when a concrete caller needs it, and pick the narrowest widening
that works:

```
default (private) → pub(super) → pub(crate) → pub
```

Never export `pub` items "in case someone needs them later". Widening visibility
later is cheap; narrowing it once downstream code depends on it is expensive.

### Function hygiene

- **Single target.** Each function does one thing, and its name describes that
  thing without "and".
- **Parameter count.** Four or fewer parameters. Five or more is almost always a
  sign that some of them belong together in a struct (or that the function is
  doing too much).
- **Size.** 10–20 lines of real code is the target. Above 20, extract helpers;
  above 40, reconsider the decomposition.
- **Pass large values by reference.** When a parameter's type is bigger than ~32
  bytes, take it by reference (`&T` / `&mut T`) instead of by value. Small
  `Copy` types (`u32`, `NonZeroU64`, small enums) pass by value as usual. When
  in doubt, check with `std::mem::size_of::<T>()`.

### Documentation

Every item with visibility wider than `private` carries a doc comment: modules
(`//!`), structs, enums, enum variants, traits, trait items, type aliases,
constants, statics, functions, methods, and public struct fields. Private items
get a doc comment whenever the reason for a non-obvious choice lives in the code
rather than in a design document.

- **Shape.** First line is a one-sentence summary in imperative mood. Additional
  paragraphs describe the _why_, the contract, and any surprising invariants.
  Use the conventional sections where they apply: `# Errors`, `# Panics`,
  `# Safety`, `# Examples`.
- **Examples.** Every public function and method carries an `# Examples` block.
  The example must compile as a doctest. If the function does IO (subprocess,
  filesystem, S3, Vault), use `no_run` so `cargo test --doc` does not execute it
  — but still write the example so readers see real calling code. If a useful
  example cannot be written (e.g. a trivial getter, or a method whose only valid
  caller is internal machinery), say so in one line instead of leaving the
  section empty or fabricating a contrived example.
- **Enforcement.** `#![warn(missing_docs)]` on every non-test crate in the
  workspace. Doctests run as part of `cargo test` and must stay green. Broken
  examples are treated as broken code.

Example of the expected shape:

````rust
/// Load a cbscore config from `path`, accepting YAML or JSON.
///
/// The format is inferred from the file extension — `.yaml` /
/// `.yml` parse as YAML, anything else parses as JSON.
///
/// # Errors
///
/// Returns [`ConfigError`] if the file cannot be read, is empty,
/// or does not match the schema. Errors are logged before
/// returning.
///
/// # Examples
///
/// ```no_run
/// use cbscore::config::Config;
/// use std::path::Path;
///
/// let config = Config::load(Path::new("cbs-build.config.yaml"))?;
/// # Ok::<(), cbscore::config::ConfigError>(())
/// ```
pub fn load(path: &Path) -> Result<Config, ConfigError> { /* ... */ }
````

## Correctness Invariants

These are easy to get wrong. Document and test them:

1. **Round-trip wire-format stability.** Config YAML/JSON, secrets YAML,
   version-descriptor JSON, release-descriptor JSON, container-descriptor YAML,
   and `cbs.component.yaml` must round-trip on the Rust side (write → load →
   equal) and remain stable across cbscore-rs versions. Cross-language
   byte-equality with pydantic output is **not** a requirement: a given
   deployment runs either Python cbscore or Rust cbscore at any one time, not
   both against the same on-disk files. Operators migrating from Python to Rust
   regenerate or hand-migrate their files at cutover (see § Configuration &
   Secrets Subsystem in the design doc for the secrets.yaml migration recipe).

2. **CLI UX parity.** `cbsbuild` subcommand names, flags, env vars (`CBS_DEBUG`,
   `-c/--config`, …), exit codes, and stdout/stderr contracts remain the same
   unless a design document says otherwise. Scripts and operators consume this
   output.

3. **On-disk layout parity.** Scratch, ccache, `_versions/`, RPM output
   directories (e.g. `${scratch}/ceph/<ver>`), and log file locations match the
   Python implementation. The runner mounts these by absolute path into the
   builder container (`/runner/scratch`, `/runner/components`, …) — the
   in-container paths are a contract, not an implementation detail.

4. **Wire-key casing matches existing format.** Config structs use
   `#[serde(rename_all = "kebab-case")]` (`vault-addr`, `role-id`,
   `scratch-containers`, `log-file`, …); descriptor structs (version, release,
   container) keep snake_case as serde's default. This matches the existing
   on-disk file format for operator familiarity and minimises the hand-migration
   effort at cutover (see Invariant 1). Case mismatches silently deserialize to
   defaults and are very easy to miss in review — use explicit
   `#[serde(rename = "…")]` for any field whose Rust identifier does not match
   the wire key under the chosen `rename_all` policy.

5. **Secret redaction.** Preserve the `SecureArg` / `_sanitize_cmd` behaviour
   from `cbscore/utils/__init__.py`. Never log raw passwords, passphrases, or
   `--pass*` arguments. Sanitise subprocess command lines before tracing them.

6. **Python-side compatibility.** `../cbscore/` and its downstream Python
   consumers (`cbsd`, `cbsdcore`, `cbc`, `crt`) must not break during the
   rewrite unless an explicit migration plan has been approved. Both
   implementations may coexist in-tree until each consumer is migrated or
   retired.

7. **Runner container reproducibility.** The runner spins up a podman container
   that re-enters the same binary inside (`cbscore-entrypoint.sh` → `cbsbuild`).
   The in-container CLI surface, env vars (`CBS_DEBUG`), and mount paths must
   match the host-side expectations — the host marshals config/secrets into temp
   files that the container consumes at fixed paths.

## When editing the Python package

Changes to `../cbscore/` itself should be rare during this rewrite. If they
happen, follow the repo-wide Python guidance (uv workspaces, `ruff`,
`basedpyright`) and keep them behaviour- preserving unless the design document
explicitly calls out a change.
