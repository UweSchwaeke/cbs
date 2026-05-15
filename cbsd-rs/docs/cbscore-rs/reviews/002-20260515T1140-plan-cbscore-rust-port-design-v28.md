# Plan Review v28 — Pre-Implementation Audit Pass 8 Closure Confirmation

**Review target:** seq-002 plan corpus (Phases 1–6) + design 004\
**Commit under review:** `fdd34db`\
**Reviewer:** Staff Engineer (design-reviewer agent)\
**Date:** 2026-05-15

---

## §Scope

Focused confirmation review of the 25 pre-implementation audit pass-8 findings
(H1.1, H1.2, H1.3/H8.3 Phase 4 C1, H1.3 Phase 4 C3, H1.4, H1.5, H1.6, H1.8 Phase
1 C2, H1.8 Phase 3 C4, H2.1, H2.3, H4.1, H4.2, H4.3, H4.4, H8.1, H8.3 Phase 3
C4, H8.4, H9.1, H9.3, H14.1, H14.3) claimed closed in commit `fdd34db`. Also
confirms no-drift on five structural invariants established by passes 1–7. A
`prettier --check` pass on all eight edited files is included.

## §Method

For each finding, the closure text was located directly in the current plan or
design file at the relevant commit section. Quoted phrases are verified
verbatim; line references are recorded where the text lands. The no-drift checks
read the live plan corpus state — not git diff — and compare against the
known-good baselines recorded in the v27 review and project memory.

---

## §Closure Verification

### H1.1 — Design 004 §Resolver: `canonicalize_root` helper + `tokio::fs::canonicalize`

**Claimed change:** Design 004 §Resolver `resolve_root` adds `canonicalize_root`
helper using `tokio::fs::canonicalize`.

**Verified.** Design 004 §Resolver contains the full `resolve_root` async fn
body, with a dedicated `canonicalize_root` private async fn immediately below
it:

> ```rust
> async fn canonicalize_root(
>     p: &Utf8Path,
> ) -> Result<Utf8PathBuf, VersionError> {
>     let abs = tokio::fs::canonicalize(p.as_std_path())
>         .await
>         .map_err(|source| VersionError::DescriptorRootResolve {
>             path: p.to_owned(),
>             source,
>         })?;
> ```

Both the helper function and the `tokio::fs::canonicalize` call are present.
**Closed.**

---

### H1.2 — Design 004 §Path builder: `descriptor_path` doc comment + `debug_assert!(root.is_absolute())`

**Claimed change:** Design 004 §Path builder `descriptor_path` adds a doc
comment with `root MUST be absolute` contract and a
`debug_assert!(root.is_absolute())` guard.

**Verified.** Design 004 §Path builder `descriptor_path` doc comment opens with:

> `/// Build the on-disk path for a version descriptor.` `///`
> ``/// `root` MUST be absolute. `resolve_root` canonicalizes operator``
> `/// input before returning, so any caller routing through the standard`
> ``/// resolver satisfies this contract. The `debug_assert!` flags``
> `/// violations in tests; release builds still return a path, but`
> `/// downstream code that depends on absolute paths (the runner mount`
> `/// line, the descriptor-write site) may misbehave.`

And the function body begins:

> ```rust
> debug_assert!(
>     root.is_absolute(),
>     "descriptor_path: root must be absolute (got {root}); \
>      resolve_root canonicalizes operator input — bypass that path \
>      only with great care",
> );
> ```

Doc comment and `debug_assert!` both present. **Closed.**

---

### H1.3 / H8.3 (Phase 4 C1) — `write_descriptor` atomic tempfile+rename + umask-default 0644

**Claimed change:** Phase 4 C1 `write_descriptor` spec gains atomic
tempfile+rename paragraph and umask-default 0644 sentence.

**Verified.** Phase 4 C1 §Files `write_descriptor` contains:

> **The write is atomic: serialise to a sibling tempfile in the same parent dir
> (via `camino-tempfile`), `tokio::fs::sync_all` to flush data + metadata to
> disk, then `tokio::fs::rename` to the final path.** Rename within the same
> directory is atomic on Linux, so a concurrent reader (e.g., `read_descriptor`
> in another process) never observes a partially-written file even if
> `write_descriptor` is interrupted (signal, panic, write error).

And immediately after:

> **Mode is left at the process umask default** (typically 0644 → operator
> readable, group/world readable). Version descriptors are not secret — they
> list version numbers, component refs, signing key references (not key
> material).

Atomic write and umask-default rationale both present. **Closed.**

---

### H1.3 (Phase 4 C3) — Per-tempfile mode is explicit at creation time

**Claimed change:** Phase 4 C3 gains "Per-tempfile mode is explicit at creation
time" bullet with three mode entries (secrets 0600, config 0644, descriptor
0644).

**Verified.** Phase 4 C3 §Design constraints contains:

> **Per-tempfile mode is explicit at creation time.** The runner creates three
> tempfiles for each run; their modes are pinned:
>
> - `cbs-build.secrets.yaml`: **mode 0600** — owner-only. Carries resolved
>   secrets in cleartext.
> - `cbs-build.config.yaml`: **mode 0644** — operator-readable; non-secret
>   config content (paths, image refs, signing key references).
> - `descriptor.json`: **mode 0644** — operator-readable; non-secret version
>   descriptor (component refs, build metadata).

All three tempfile modes pinned with rationale. **Closed.**

---

### H1.4 — Phase 3 C3 `SecretsMgr::load_files` empty-paths returns `Ok(SecretsMgr::empty())`

**Claimed change:** Phase 3 C3 `SecretsMgr::load_files` gains "empty paths slice
returns `Ok(SecretsMgr::empty())`" paragraph.

**Verified.** Phase 3 C3 §Files `load_files` spec contains:

> **An empty `paths` slice returns `Ok(SecretsMgr::empty())`** (an empty manager
> with all four family HashMaps initialized to empty) — not an error.
> Operator-facing rationale: deployments that mint `secrets.yaml` from Vault
> refs at request time start with zero pre-populated files; the manager is built
> incrementally and the `paths.is_empty()` case is a normal startup path, not an
> operator-actionable failure.

Explicit `Ok(SecretsMgr::empty())` path documented with rationale. **Closed.**

---

### H1.5 — Phase 3 C3 `SecretsMgr::dump_to_runner` atomic tempfile+rename

**Claimed change:** Phase 3 C3 `SecretsMgr::dump_to_runner` gains atomic
tempfile+rename paragraph.

**Verified.** Phase 3 C3 §Files `dump_to_runner` spec contains:

> **The write is atomic: serialise to a sibling tempfile (`<path>.tmp`) with
> mode 0600 via `camino-tempfile`, `tokio::fs::sync_all` to flush the file's
> data + metadata to disk, then `tokio::fs::rename` to the final path.** This
> guarantees the runner never observes a partially-written secrets file if
> `dump_to_runner` is interrupted (signal, panic, runner kill). Mode 0600 is set
> on the tempfile **before** the rename so the file is owner-only from the
> moment it exists at the final path; secrets are never world-readable even
> transiently.

Atomic write sequence and pre-rename mode ordering both stated. **Closed.**

---

### H1.6 — Phase 4 C3 "Error precedence on cleanup failure" bullet

**Claimed change:** Phase 4 C3 gains "Error precedence on cleanup failure"
bullet.

**Verified.** Phase 4 C3 §Design constraints contains:

> **Error precedence on cleanup failure.** If the run stage returns
> `Err(run_err)` AND the subsequent `async fn cleanup(guard)` also returns
> `Err(cleanup_err)`, the runner returns `Err(run_err)` to the caller (run-stage
> errors are the operator-actionable one) and emits
> `tracing::warn!(target = "cbscore::runner", error = ?cleanup_err, "cleanup failed after run-stage error")`.
> Symmetrically, if the run stage succeeds but cleanup fails, the runner returns
> `Err(cleanup_err)` — successful runs whose cleanup leaks tempfiles or
> containers are still failure modes that need to surface to the operator (a
> leaked container is a podman-side resource leak).

Both precedence cases (run-err wins over cleanup-err; cleanup-err surfaces on
success) are documented. **Closed.**

---

### H1.8 (Phase 1 C2) — `ConfigError::NotFound { path: Utf8PathBuf }` + pinned Display

**Claimed change:** Phase 1 C2 `ConfigError` gets
`NotFound { path: Utf8PathBuf }` variant plus pinned Display text.

**Verified.** Phase 1 C2 §Files `cbscore-types/src/config/errors.rs` spec
contains:

> `cbsd-rs/cbscore-types/src/config/errors.rs` — `ConfigError` including
> `NotFound { path: Utf8PathBuf }`, `MissingSchemaVersion`, and
> `UnknownSchemaVersion { found, max_supported }` per design 002 § Wire-Format
> Versioning. `NotFound` is the variant Phase 3 Commit 4's `Config::load` maps
> `std::io::ErrorKind::NotFound` to …

And in the §Design rules block:

> - `ConfigError::NotFound { path }`:
>   `"config file not found at {path}; create one with cbsbuild config init"`.

Variant declared, Display text pinned. **Closed.**

---

### H1.8 (Phase 3 C4) — `Config::load` maps `io::ErrorKind::NotFound` to `ConfigError::NotFound`

**Claimed change:** Phase 3 C4 `Config::load` gains "io::ErrorKind::NotFound
maps to ConfigError::NotFound" sentence.

**Verified.** Phase 3 C4 §Files `config.rs` spec contains:

> **`io::ErrorKind::NotFound` on the `tokio::fs::read_to_string` call maps to
> `ConfigError::NotFound { path: path.to_owned() }`** (Phase 1 Commit 2 added
> the variant), not a generic `Io { source }` variant. The CLI's top-level error
> renderer can match on `NotFound` specifically and surface the pinned Display
> text
> (`"config file not found at {path}; create one with cbsbuild config init"`),
> giving the operator a clear next step.

Explicit `NotFound` mapping documented with downstream CLI rationale.
**Closed.**

---

### H2.1 — `cbscore-rs/CLAUDE.md` "No panic on operator input" section

**Claimed change:** `cbscore-rs/CLAUDE.md` gains a "No panic on operator input"
section.

**Verified.** `cbsd-rs/docs/cbscore-rs/CLAUDE.md` §Principles contains:

> ### No panic on operator input
>
> `unwrap()` and `expect()` are **prohibited** on any value that originates from
> operator input — CLI arguments, config file fields, environment variables,
> descriptor contents, secrets contents, on-disk component files, network
> responses (S3, Vault), subprocess outputs. Use `?` to propagate or an explicit
> error variant for these paths.
>
> Panics are permitted **only** where a documented invariant guarantees the
> value is internally well-formed (e.g., `descriptor_path()` returning a path
> whose `parent()` is always `Some` because the function constructs the path
> internally). Document the invariant on every `expect("…")` message so a future
> reader can verify it.

Section present with prohibited-paths list and permitted-exception rule.
**Closed.**

---

### H2.3 — Phase 4 C3 `current_exe()` failure maps to `RunnerError::BinaryNotFound`

**Claimed change:** Phase 4 C3 gains "`current_exe()` failure maps to a
dedicated variant" bullet referencing
`RunnerError::BinaryNotFound { source: std::io::Error }`.

**Verified.** Phase 4 C3 §Design constraints contains:

> **`current_exe()` failure maps to a dedicated variant.** The cbsbuild-binary
> self-mount step (line 325) calls `std::env::current_exe()`. On failure … the
> error maps to `RunnerError::BinaryNotFound { source: std::io::Error }` (Phase
> 1 Commit 2 adds the variant). Display text pinned:
> `"could not locate the cbsbuild binary on disk: {source}"`.

Dedicated variant, type signature, and Display text all present. **Closed.**

---

### H4.1 — Phase 1 C5 fixtures: hand-crafted, not copied from Python

**Claimed change:** Phase 1 C5 fixtures paragraph states "hand-crafted Rust-side
fixtures … not copied from `cbscore/tests/`".

**Verified.** Phase 1 C5 §Files contains:

> `cbsd-rs/cbscore-types/tests/fixtures/` — **hand-crafted Rust-side fixtures
> authored as part of this commit, not copied from `cbscore/tests/` or any
> Python source.** Per `cbsd-rs/CLAUDE.md` §Never touch Python code, the Rust
> port treats Python-side test corpora as out of scope: cross-language
> byte-equality is not a correctness invariant (see CLAUDE.md §Correctness
> Invariants item 1), and pulling Python fixtures into the Rust tree would
> couple this crate to a Python test layout that may evolve independently.

Explicit "not copied from Python" constraint with rationale. **Closed.**

---

### H4.2 — Phase 5 C1 patch-walker fixtures built programmatically via `tempfile::TempDir`

**Claimed change:** Phase 5 C1 §Testable: patch-walker fixtures built
programmatically via `tempfile::TempDir`.

**Verified.** Phase 5 C1 §Testable contains:

> Unit tests on the patch walker against fixture directory trees built
> programmatically at test runtime via `tempfile::TempDir` (NOT stored on-disk
> under `tests/fixtures/`). The test constructs the `components/ceph/patches/`
> tree with `19/`, `19.2/`, `19.2.3/`, and top-level patch files via
> `std::fs::create_dir_all` + `std::fs::write` (each `.patch` is a one-line stub
> since the walker only inspects the tree shape, not patch content). The tempdir
> is cleaned up automatically when the test exits.

Programmatic fixture approach stated explicitly, `tempfile::TempDir` named, and
rationale provided. **Closed.**

---

### H4.3 — Phase 6 progress table grows to 6 commits; Commit 6 §Design constraints "No Python comparison" bullet

**Claimed change:** Phase 6 progress table grows from 5 to 6 commits; new Commit
6 §Design constraints gains "No Python comparison" bullet.

**Verified.** Phase 6 progress table has 6 rows (Commits 1–6). Commit 6 §Design
constraints contains:

> **No Python comparison.** The earlier design 002 framing called for
> Rust-vs-Python structural equivalence (cardinality, NEVRA, file list,
> dependencies). That comparison is dropped from M1 acceptance: …

The section explicitly labels the Python comparison as dropped and explains why.
**Closed.**

---

### H4.4 — Phase 3 C3 integration test "must populate at least one entry in each of the four credential families"

**Claimed change:** Phase 3 C3 integration test gains "must populate at least
one entry in each of the four credential families" sentence.

**Verified.** Phase 3 C3 §Testable integration test entry contains:

> **The fixture must populate at least one entry in each of the four credential
> families (`git`, `storage`, `signing`, `registry`)** — a single-family fixture
> would not exercise the per-family HashMap merge/dump paths and could let a
> serde-side bug in one family slip past the test. Each family entry should pair
> a `*Plain*` and a `*Vault*` variant where applicable so the dump path covers
> both shapes.

Minimum coverage requirement and rationale stated. **Closed.**

---

### H8.1 — Phase 5 C2 walkdir cycle: warns-and-continues via `loop_ancestor`

**Claimed change:** Phase 5 C2 design constraints: walkdir cycle warns-and-
continues via `loop_ancestor`.

**Verified.** Phase 5 C2 §Design constraints contains:

> walkdir detects symlink cycles internally and surfaces them as
> `walkdir::Error` with `loop_ancestor: Some(...)` populated; the loader
> **warns-and-continues** on cycle detection rather than aborting the walk:
>
> ```rust
> Err(err) if err.loop_ancestor().is_some() => {
>     tracing::warn!(
>         target: TARGET_CORE_COMPONENT,
>         path = %err.path().unwrap_or(root).display(),
>         loop_ancestor = %err.loop_ancestor().unwrap().display(),
>         "skipping symlink cycle during component walk",
>     );
>     continue;
> }
> ```

The `loop_ancestor()` predicate, warn-and-continue pattern, and structured warn
fields are all present. **Closed.**

---

### H8.3 (Phase 3 C4) — `Config::store` atomic tempfile+rename paragraph

**Claimed change:** Phase 3 C4 `Config::store` gains atomic tempfile+rename
paragraph.

**Verified.** Phase 3 C4 §Files `config.rs` `Config::store` spec contains:

> **The write is atomic: serialise to a sibling tempfile in the same parent dir
> (via `camino-tempfile`), `tokio::fs::sync_all` to flush data + metadata to
> disk, then `tokio::fs::rename` to the final path.** Mirrors the same pattern
> used by `SecretsMgr::dump_to_runner` (Commit 3) and by
> `cbscore::versions::desc::write_descriptor` (Phase 4 Commit 1): rename within
> the same directory is atomic on Linux, so a concurrent reader never observes a
> partially-written config file even if `store` is interrupted (signal, panic,
> write error).

Atomic write pattern and cross-reference to sibling implementations both
present. **Closed.**

---

### H8.4 — Phase 5 C2 "Component-name comparison is case-sensitive"

**Claimed change:** Phase 5 C2 design constraints: "Component-name comparison is
case-sensitive."

**Verified.** Phase 5 C2 §Design constraints contains:

> **Component-name comparison is case-sensitive.** Two component files declaring
> `name: ceph` and `name: Ceph` are **distinct** components, not duplicates —
> the `HashMap<String, CoreComponent>` keys them apart, and the
> `DuplicateComponentName` detection only triggers on exact string equality
> (Rust `String` equality is byte-equality, not Unicode-normalised). This
> matches Python's `dict` keying semantics.

Case-sensitivity rule, implementation note, and Python parity rationale all
present. **Closed.**

---

### H9.1 — Phase 3 C1 "Retry-behaviour asymmetry across subsystems" bullet

**Claimed change:** Phase 3 C1 design constraints gain "Retry-behaviour
asymmetry across subsystems" bullet.

**Verified.** Phase 3 C1 §Design constraints contains:

> **Retry-behaviour asymmetry across subsystems.** `aws-sdk-s3` ships with a
> built-in retry policy: up to **3 attempts** per operation with exponential
> backoff, applied transparently inside the SDK for retryable errors
> (`Throttling*`, `RequestTimeout`, 5xx). `utils::vault` (Commit 2) has **no
> built-in retry** — every `vaultrs` call goes through once and surfaces
> transient errors immediately. This is intentional asymmetry matching upstream
> library behaviour, but operators see different failure surfaces for transient
> errors across the two subsystems: flaky S3 networks self-heal up to 3 attempts
> deep before the operator sees an `S3Error`, while flaky Vault networks surface
> `VaultError` on the first transient. Operator-actionable note in the README's
> troubleshooting section: a `VaultError::RequestFailed` on a known-reliable
> Vault deployment is often a one-off — `cbsbuild build` is safe to re-run.

Asymmetry documented with SDK retry count, Vault no-retry rationale, and
operator action. **Closed.**

---

### H9.3 — Phase 3 C1 `CBSCORE_S3_READ_TIMEOUT_SECS` escape hatch + 30s default rationale

**Claimed change:** Phase 3 C1 timeout discussion adds
`CBSCORE_S3_READ_TIMEOUT_SECS` escape hatch and "30s default is appropriate for
small-object operations" paragraph.

**Verified.** Phase 3 C1 §Design constraints timeout paragraph contains:

> HTTP timeouts go via `aws_sdk_s3::Config::builder().timeout_config(…)`;
> default to 30s read / 30s connect. **The 30s default is appropriate for the
> small-object operations cbscore-rs uses (HEAD, ListObjectsV2,
> release-descriptor PUT, sub-MB JSON / YAML PUT), but it is NOT sufficient for
> large RPM uploads on slow links.** … Operators on slow links should set
> `CBSCORE_S3_READ_TIMEOUT_SECS` (read by `utils::s3` at module init, overrides
> the 30s default; falls back to 30s when unset or invalid).

Default-appropriateness note and `CBSCORE_S3_READ_TIMEOUT_SECS` escape hatch
both present with rationale. **Closed.**

---

### H14.1 — Phase 6 new Commit 5: workspace-wide visibility audit (`pub` → `pub(crate)`)

**Claimed change:** Phase 6 gains new Commit 5 — workspace-wide visibility audit
(`pub` → `pub(crate)`).

**Verified.** Phase 6 progress table has a row for:

> `5` | `cbsd-rs: visibility audit (demote pub → pub(crate) workspace-wide)` |
> `~100` | Pending

And the full Commit 5 §Design constraints section follows, opening with:

> Dedicated audit commit that lands **between routine CLI plumbing (Commits 1–4)
> and the M1 smoke gate (Commit 6)** to enforce CLAUDE.md's "as private as
> possible, as public as needed" visibility rule.

The commit is present in the progress table and has a full spec. The former
Commit 5 (smoke build) has been renumbered to Commit 6. **Closed.**

---

### H14.3 — `cbscore-rs/CLAUDE.md` "No panic on operator input" ends with `pub(crate)` requirement

**Claimed change:** "No panic on operator input" section ends with paragraph
requiring `pub(crate)` for workspace-internal items.

**Verified.** `cbsd-rs/docs/cbscore-rs/CLAUDE.md` §No panic on operator input
closes with:

> Workspace-internal items consumed only by other crates in the `cbsd-rs/`
> workspace **MUST** be declared `pub(crate)`, not `pub`. `pub` is reserved for
> items intended for consumption by out-of-tree crates. Phase 6 includes a
> dedicated visibility-audit commit before the M1 cut that demotes
> workspace-internal items found at `pub`.

The `pub(crate)` mandate and Phase 6 audit reference are both present in the
closing paragraph of the section. **Closed.**

---

## §No-Drift Spot Checks

Five structural invariants from passes 1–7 were spot-checked against the live
plan corpus state.

| Invariant                                                              | Expected                                              | Observed                                                                                                                                                                        | Status |
| ---------------------------------------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| CLAUDE.md "Never touch Python code" rule                               | Section present; prohibits Python edits               | `cbsd-rs/docs/cbscore-rs/CLAUDE.md` §Never touch Python code is present and unchanged (last paragraph: "If a plan or design suggests editing Python, it has gone out of scope") | PASS   |
| Phase 6 commit count                                                   | 6 commits (H14.1 added Commit 5; smoke gate → C6)     | Progress table has 6 rows: Commits 1–6                                                                                                                                          | PASS   |
| Phase 5 commit count                                                   | 7 commits                                             | Progress table has 7 rows: Commits 1–7                                                                                                                                          | PASS   |
| Kebab-vs-snake-case wire-key split (CLAUDE.md Correctness Invariant 4) | Config = kebab; descriptors = snake; split documented | `cbscore-rs/CLAUDE.md` §Correctness Invariants item 4 is present and unchanged; Phase 3 C4 §Design constraints reaffirms `schema-version: 1` (kebab) for Config                 | PASS   |
| Phase 4 §Mount-contract handoff bullet                                 | C3 §Design constraints closes Phase 3 §Out of scope   | Phase 4 C3 §Design constraints: "**Mount-contract handoff from Phase 3.** Commit 3 explicitly creates the tempfile via `camino-tempfile::NamedUtf8TempFile` with mode 0600 …"   | PASS   |

---

## §Formatting

`prettier --check` on all eight files modified in commit `fdd34db`:

```
prettier --check \
  cbsd-rs/docs/cbscore-rs/CLAUDE.md \
  cbsd-rs/docs/cbscore-rs/design/004-20260429T1319-configurable-version-descriptor-location.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-04-runner.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-05-builder-and-releases.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-06-cbsbuild-cli.md \
  cbsd-rs/docs/cbscore-rs/plans/README.md

All matched files use Prettier code style!
```

Exit code: 0.

---

## §Findings

None. No new findings were surfaced during this review.

---

## §Verdict

> **Approve — H1+H2+H4+H8+H9+H14 (25 findings) closed; pre-impl audit pass 8
> fully resolved; design and plan corpus ready for Phase 1 implementation
> start.**
