# seq-004 — Configurable `VersionDescriptor` Location

## Status

**Approved — lands post-M2 as a 1.x.0 backwards-compatible minor add.** Audited
at v2
(`reviews/004-20260513T1003-plan-configurable-version-descriptor-location-v2.md`,
verdict `49d6f78`); zero findings across CRITICAL / MAJOR / MINOR / SUGGESTION /
OPEN QUESTION. Implements design 004 Migration table steps 1–4. Step 5 is owned
by seq-003 (the interactive `cbsbuild config init` minor add).

**Repositioning note (post-M2 reframe).** The plan was originally drafted to
interleave between seq-002 Phase 6 Commit 4 and Commit 5 so the M1 visibility
audit (Phase 6 Commit 5) and the M1 smoke gate (Phase 6 Commit 6) would
naturally cover seq-004's surface. That interleave did not happen: Phase 6
landed end-to-end without seq-004, and Phase 7 (M2 cut) followed. seq-004 now
lands on top of the M2 release as a backwards-compatible additive change —
existing operator configs (without `paths.versions`) keep working byte-
identically via the `<git-root>/_versions` fallback. The implementer owns
visibility decisions for the newly-added symbols at the point of introduction
(no separate post-hoc audit). The M1 smoke gate is not extended —
`cbsbuild build` is a read site and does not accept `--versions-dir`; resolver
coverage lives in Commit 3's unit tests.

**Review trail:**

- Plan drafted `1144458` (2026-05-13).
- Corpus pass v17 `4f27d2f` — 5 findings (N1, N2, S1, S2, Q1 across seq-002 +
  seq-004) → closed in `e3cb122` → v18 `a806158` confirmation, clean.
- Focused v1 `c197927` — 2 MINORs (N1 §Depends on misattribution, N2 surviving
  hedges in Commit 3 §Files) → closed in `b595554` → v2 `49d6f78` confirmation,
  clean.
- Repositioning sweep (2026-05-21) — interleave language replaced with post-M2
  framing; no semantic change to the three commits or their files.
- Post-review sweep (2026-05-21) — addressed v3 design-review findings: reframed
  OQ6 schema-version rationale on "bump policy deferred across all designs"
  basis (no longer asserts a carve-out in design 002); dropped the M1 smoke-gate
  fixture extension claim (the flag is not on `cbsbuild build`); made
  `#[serde(skip_serializing_if = "Option::is_none")]` mandatory; closed OQ-A
  (`Config.schema_version = 1` on HEAD) and OQ-B (`VersionType` already uses
  `rename_all = "lowercase"`, so `as_dir_name` strings match the serde wire);
  cited the visibility-audit commit by hash.
- v4 review sweep (2026-05-21) — design 004 only: removed a residual carve-out
  claim in §Migration tail ("every change bumps' rule applies only to changes
  that alter the interpretation of existing fields"); added the mandatory
  `skip_serializing_if` to the §Design Sketch §Config schema code block and the
  §Migration table Step 1 entry; neutralised the §OQ6 "expected outcome"
  speculation about future bump-policy resolution. Plan was clean; no plan edits
  in this pass.
- v5 confirmation pass (2026-05-21) — plan only: forward-pointer inventory in
  §End-of-feature acceptance promoted from "five" to "six" entries; missing
  entry added for `cbsbuild/src/cmds/versions.rs:9` (the module-level doc that
  says "Python-parity hardcoded path; seq-004 makes it configurable" — Commit 3
  rewrites the bullet). v4 findings all confirmed closed.

## Progress

| #   | Commit                                                                                | ~LOC | Status  |
| --- | ------------------------------------------------------------------------------------- | ---- | ------- |
| 1   | `cbscore-types: add Config.paths.versions, VersionType::as_dir_name, descriptor_path` | ~120 | Pending |
| 2   | `cbscore: add versions::resolve_root + VersionError::NoDescriptorRoot`                | ~200 | Pending |
| 3   | `cbsbuild: --versions-dir flag + versions create write-path cutover`                  | ~180 | Pending |

**Estimate:** ~500 LOC, 3 commits.

## Goal

Replace the hardcoded `<git-root>/_versions/<type>/<VERSION>.json` write path
that seq-002 Phase 6 Commit 2 lands for parity with Python, with the
configurable shape design 004 specifies:

- `Config.paths.versions: Option<Utf8PathBuf>` in `cbs-build.config.yaml`.
- `--versions-dir <PATH>` flag on `cbsbuild versions create`.
- Resolver precedence CLI > config > `<git-root>/_versions` fallback.
- Single `descriptor_path(root, type, version)` helper that encodes the
  `<root>/<type>/<VERSION>.json` layout in one place.

The default fallback preserves byte-identical Python behaviour for operators who
change nothing: descriptors land under `<git-root>/_versions/<type>/`. The
change is fully backwards-compatible for the existing operator population.

## Depends on

- **seq-002 Phase 1** — the `cbscore-types` crate exists, `PathsConfig` already
  carries `components`, `scratch`, `scratch_containers`, `ccache`, and
  `VersionType` is declared in `versions/utils.rs`. seq-004 Commit 1 adds the
  `versions` field plus the `as_dir_name` accessor and the `descriptor_path`
  helper without touching the existing fields.
- **seq-002 Phase 2 Commit 4** — `cbscore::utils::git::repo_root` exists. The
  resolver in seq-004 Commit 2 calls it as the OQ2 fallback.
- **seq-002 Phase 3 Commit 4** — `cbscore::config::Config::load` exists and
  returns a `Config` carrying `paths` (including the new `paths.versions` field
  after Commit 1 lands).
- **seq-002 Phase 1 Commit 3** — `cbscore-types/src/versions/desc.rs` exists.
  seq-004 Commit 1 appends `descriptor_path` to this file (and adds
  `VersionType::as_dir_name` to `versions/utils.rs`).
- **seq-002 Phase 4 Commit 1** — `cbscore/src/versions/desc.rs` exists (IO side;
  `read_descriptor` + `write_descriptor`). seq-004 Commit 2's
  `cbscore/src/versions/resolve.rs` sits alongside this file under
  `cbscore/src/versions/`, and seq-004 Commit 3 calls `write_descriptor` from
  the refactored write path. Phase 4 Commit 1 §Files settles that
  `write_descriptor` calls `tokio::fs::create_dir_all` internally (same
  `mkdir -p` semantic as `Config::store`), so seq-004's write site does not
  duplicate the parent-create.
- **seq-002 Phase 6 Commit 2** — `cbsbuild versions create` exists and carries
  the hardcoded write path that Commit 3 of this plan refactors.

Design references: design 004 (this plan implements its Migration table steps
1–4) and design 002 §Capability Mapping (Utf8PathBuf from camino).

## Sequencing

seq-004 lands **on top of the M2 release**, after every seq-002 phase (M0 / M1 /
M2) is on `main`. The original "interleave between seq-002 Phase 6 Commits 4 and
5" plan slipped — Phase 6 landed end-to-end without seq-004 — so the work now
happens in a straight three-commit sequence with no remaining dependency on the
seq-002 phase order.

What this implies in practice:

- The three commits land in order (Commit 1 → 2 → 3) and the workspace gate
  (`cargo fmt --all --check`, `cargo clippy --workspace`,
  `cargo test --workspace`) runs after each one.
- Visibility decisions for the new symbols are made by the implementer at the
  point of introduction (per CLAUDE.md §Visibility — `pub(crate)` until a
  concrete cross-crate caller exists, `pub` otherwise). There is no post-hoc
  workspace-wide visibility audit; that was Phase 6 Commit 5
  (`1e68afb cbsd-rs: visibility audit (demote pub → pub(crate) workspace-wide)`)
  and already ran. Symbol-by-symbol notes:
  - `Config.paths.versions` — `pub` field on a `pub struct`; required by the
    serde-driven on-disk wire shape.
  - `VersionType::as_dir_name` and `descriptor_path` — `pub` because
    `cbsbuild::cmds::versions` (a sibling crate) reads them through the
    `cbscore-types` boundary.
  - `cbscore::versions::resolve_root` — `pub` for the same reason; the
    `cbsbuild` CLI is the immediate cross-crate caller.
  - `VersionError::{NoDescriptorRoot, DescriptorRootResolve, DescriptorRootNotUtf8}`
    — `pub` variants on an already-`pub` error type.
- The M1 smoke gate (`cbsd-rs/cbsbuild/tests/m1_smoke.rs`) is **not** extended
  by seq-004. The gate invokes `cbsbuild build`, which is a read site and per
  design 004 §OQ4 takes the descriptor path as an explicit argument — it never
  calls `resolve_root` and `--versions-dir` is not a `cbsbuild build` flag (it's
  only on `cbsbuild versions create`). Coverage of the resolver and the
  `--versions-dir` flag lives entirely in Commit 3 §Testable's unit tests, which
  exercise the precedence ladder and the OQ5 error-text path end-to-end.

Step 5 of design 004's Migration table — the interactive `config init` "Versions
path" prompt and the bypass-mode pre-fill — is **deliberately out of scope**
here. It lives under design 003 (interactive config init), which is the seq-003
post-M1 minor add.

## Out of scope

- **Read-side auto-discovery.** `cbsbuild build VERSION --type dev` resolving
  `<root>/dev/<VERSION>.json` is rejected in design 004 OQ4. Every read site
  keeps taking the descriptor path as an explicit argument.
- **Multi-root search path** (`Config.paths.versions: Vec<Utf8PathBuf>`).
  Non-Goal per design 004 §Non-Goals.
- **Migration tooling for existing `_versions/` trees.** OQ5 — operators doing
  nothing keep working via the fallback; operators relocating the root run their
  own `cp -r`.
- **`config init` versions prompt + systemd / containerized bypass pre-fill
  (`/cbs/_versions`).** Owned by design 003 / seq-003; post-M1 minor add.
- **Wire-format schema bump.** `Config.schema_version` stays at 1 — see design
  004 §OQ6 for the rationale. Short version: the schema-version bump policy in
  design 002 §Wire-Format Versioning is deferred across every design currently
  in flight (no design in the corpus bumps `schema-version`); seq-004 adopts the
  same posture and leaves the marker at 1. Operationally the new field is
  additive and optional (`#[serde(default)]` +
  `#[serde(skip_serializing_if = "Option::is_none")]`) so files round-trip
  through both old and new binaries without operator action.

## Commit 1 — `cbscore-types`: paths field, `VersionType::as_dir_name`, `descriptor_path`

Land the pure-type additions in `cbscore-types`. No IO, no async. All three
pieces are testable in isolation via doctests and round-trip serde tests on
`Config`.

**Files:**

- `cbsd-rs/cbscore-types/src/config/paths.rs` — append
  `versions: Option<Utf8PathBuf>` to `PathsConfig`, marked
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. Both attributes
  are mandatory, not consistency-with-siblings: `default` lets existing YAML
  files (which omit the field) parse cleanly, and `skip_serializing_if` lets
  files written by a new binary with the field unset serialise as absent so they
  round-trip through both old and new binaries (the round-trip claim in §OQ6 /
  design 004 §OQ6 depends on this attribute being present, not optional). The
  existing `ccache` field on HEAD uses exactly this pair (`paths.rs:42`); mirror
  that. Keep the existing field ordering and the
  `#[serde(rename_all = "kebab-case")]` attribute on the struct. The YAML key
  resolves to `versions` (a single word; kebab-case is a no-op).
- `cbsd-rs/cbscore-types/src/versions/utils.rs` — add
  `impl VersionType { pub fn as_dir_name(&self) -> &'static str }` returning
  `"release"`, `"dev"`, `"test"`, `"ci"`. These match the serde wire strings
  produced by `#[serde(rename_all = "lowercase")]` on `VersionType` (verified on
  HEAD at `utils.rs:36`) and Python's `cbscore/versions/utils.py:VersionType`
  serde value names, locked in by design 004 OQ3 and the type-encoded-in-layout
  invariant. The doctest asserts the consistency between `as_dir_name()` and
  `serde_json::to_string(&v).unwrap().trim_matches('"')` for all four variants,
  so a future change to either side surfaces immediately.
- `cbsd-rs/cbscore-types/src/versions/desc.rs` — add
  `pub fn descriptor_path(root: &Utf8Path, ty: VersionType, version: &str) -> Utf8PathBuf`,
  implemented as `root.join(ty.as_dir_name()).join(format!("{version}.json"))`.
  This is the single source of truth for the `<root>/<type>/<VERSION>.json`
  layout; every other code path that needs it imports this helper.

**Design constraints:**

- **No schema-version bump.** `Config.schema_version` stays at 1 (the value on
  HEAD at `cbscore-types/src/config/versioned.rs:65`). See §Out of scope for the
  rationale and design 004 §OQ6 for the full argument — short version: bump
  policy is deferred across every design currently in flight, and the new field
  is additive optional so round-trip stability holds in the meantime.
- **Wire-key casing** (CLAUDE.md correctness invariant 4):
  `rename_all = "kebab-case"` is already on `PathsConfig`, so the `versions`
  Rust identifier auto-maps to the YAML key `versions`. No explicit
  `#[serde(rename = …)]` needed.
- **No new dependencies.** `camino::Utf8PathBuf` is already in
  `cbscore-types/Cargo.toml` (Phase 1 Commit 1). `serde`, `serde_json` (dev-deps
  for round-trip tests) likewise.
- **Pure functions.** `descriptor_path` and `as_dir_name` are zero-allocation in
  the type sense (one `&'static str` and a `format!` per call); they live in
  `cbscore-types` per the types-vs-IO split (CLAUDE.md correctness invariant 1
  round-trip surface stays in the types crate).

**Testable:**

- Doctests on `descriptor_path` and `as_dir_name` with concrete examples per
  CLAUDE.md §Documentation Examples.
- Round-trip serde test in `cbscore-types/tests/config.rs`: a `Config` with
  `paths.versions = Some("/srv/cbs/versions")` round-trips through YAML and JSON
  byte-stable.
- Round-trip serde test with `paths.versions` **absent** in the input YAML:
  parses as `None`, re-serialises with the field omitted (not present-but- null)
  — `#[serde(skip_serializing_if = "Option::is_none")]` is mandatory on the
  field per §Files. Without it, the round-trip claim in §OQ6 breaks; with it,
  asserting that the re-serialised YAML does not contain the substring
  `versions:` is the right shape for the test.
- Unit test in `versions/desc.rs`:
  `descriptor_path(Utf8Path::new("/r"), VersionType::Dev, "19.2.3")` returns
  `Utf8PathBuf::from("/r/dev/19.2.3.json")`. Repeat for all four variants of
  `VersionType` to lock the directory names.

## Commit 2 — `cbscore`: `resolve_root` + `VersionError::NoDescriptorRoot`

Land the precedence-resolving function in the `cbscore` library crate. Has IO
(calls `git::repo_root` async, captures cwd) and async, distinguishing it
cleanly from Commit 1.

**Files:**

- `cbsd-rs/cbscore/src/versions/resolve.rs` — new sub-module carrying the
  resolver. `cbsd-rs/cbscore/src/versions/mod.rs` gains `pub mod resolve;` plus
  `pub use resolve::resolve_root;` so callers reach it as
  `cbscore::versions::resolve_root`. This file-per-IO-function layout matches
  `versions/desc.rs` (Phase 4 Commit 1's `read_descriptor`) and
  `versions/create.rs` (Phase 6 Commit 2's `version_create_helper`). Add two
  items:
  - `pub async fn resolve_root(cli: Option<&Utf8Path>, config: &Config) -> Result<Utf8PathBuf, VersionError>`
    implementing the precedence:
    1. If `cli.is_some()` → call `canonicalize_root(p).await` and return.
    2. Else if `config.paths.versions.is_some()` → call
       `canonicalize_root(p).await` and return.
    3. Else → call `cbscore::utils::git::repo_root().await` (Phase 2 Commit 4),
       return `repo_root.join("_versions")` (no canonicalize — git's
       `--show-toplevel` already returns an absolute, symlink-resolved path). On
       `Err` from `repo_root`, capture cwd best-effort
       (`std::env::current_dir().ok().and_then(|p| Utf8PathBuf::try_from(p).ok()).unwrap_or_else(|| Utf8PathBuf::from("<unknown>"))`)
       and return `Err(VersionError::NoDescriptorRoot { cwd })`. Never propagate
       the raw `std::io::Error` from `current_dir`; that would bypass the OQ5
       friendly text.
  - `async fn canonicalize_root(p: &Utf8Path) -> Result<Utf8PathBuf, VersionError>`
    — private helper that calls `tokio::fs::canonicalize(p.as_std_path())` to
    produce an absolute, symlink-resolved path. On `Err` (most commonly `ENOENT`
    because the operator-supplied directory does not exist yet), return
    `Err(VersionError::DescriptorRootResolve { path: p.to_owned(), source })`.
    If the resolved path is non-UTF-8, return
    `Err(VersionError::DescriptorRootNotUtf8 { path })` with the lossy string
    form. Canonicalization runs before every downstream consumer
    (`descriptor_path`, the patch walker, the runner mount line) so the
    `debug_assert!(root.is_absolute())` in `descriptor_path` (Commit 1 of this
    plan) is guaranteed to hold. Operators relocating the descriptor store must
    `mkdir -p` the target before passing `--versions-dir` / setting
    `paths.versions`; the canonicalize step fails with a clean
    operator-actionable error otherwise.
- `cbsd-rs/cbscore-types/src/versions/errors.rs` — add three variants to
  `VersionError` (which already lives in `cbscore-types` per Phase 1 Commit 2's
  error taxonomy):
  - `NoDescriptorRoot { cwd: Utf8PathBuf }` — the OQ5 "no override, no git, no
    fallback" case.
  - `DescriptorRootResolve { path: Utf8PathBuf, source: std::io::Error }` —
    `canonicalize` failed on an operator-supplied path (most commonly ENOENT
    because the directory doesn't exist yet).
  - `DescriptorRootNotUtf8 { path: String }` — `canonicalize` succeeded but the
    resolved absolute path is non-UTF-8. Implement each `Display` arm in the
    same file. `Utf8PathBuf` is already a dep of `cbscore-types` via `camino`
    (Phase 1 Commit 1); rendering pure string formatting does **not** call any
    `cbscore` IO function, so no layering violation occurs. The
    `NoDescriptorRoot` `Display` arm produces the OQ5 four-line text:
  ```text
  cannot resolve descriptor store location.
    no --versions-dir flag was supplied,
    no `paths.versions` field is set in cbs-build.config.yaml,
    and the current directory ({cwd}) is not inside a git checkout.
    set one of the above to choose where descriptors live.
  ```
  The `DescriptorRootResolve` arm produces an operator-actionable message naming
  the path and the underlying error, with hint text pointing at `mkdir -p`. The
  `DescriptorRootNotUtf8` arm names the rejected path. Both variants and their
  pinned Display text are documented in design 004 §Resolver as the canonical
  specification; this commit implements them.

**Design constraints:**

- **OQ5 friendly error text.** The `Display` impl renders the full four-line
  message naming both override surfaces. Tested explicitly (see Testable).
- **Async surface.** `resolve_root` is `async` because `git::repo_root` is async
  (Phase 2 Commit 4). Sync wrappers are not needed — `cbsbuild versions create`
  is async end-to-end per Phase 6.
- **No global state.** The resolver is a pure function over its two inputs; no
  caching of git roots, no env-var fallback. Repeated calls in one process
  re-evaluate.
- **Tracing.** Emit `tracing::debug!` at each of the three precedence branches
  naming which surface won (`"resolving descriptor root: cli flag"`,
  `"resolving descriptor root: config field"`,
  `"resolving descriptor root: git fallback"`). Target `"cbscore::versions"`.
  Aids operator debugging of "where did my descriptor land?" without needing
  `CBS_DEBUG=1` to trace through `versions create` from the top.

**Testable:**

- Unit test: CLI flag wins over config field. Pass both, assert the resolved
  canonicalized path matches.
- Unit test: config field wins over fallback. Pass `cli = None`, config field
  pointing at a real `tempfile::TempDir`, assert the canonicalized path of that
  tempdir is returned (no git call).
- Unit test: `canonicalize_root` errors on a non-existent path. Pass
  `--versions-dir /tmp/does-not-exist-<random>`, assert
  `Err(VersionError::DescriptorRootResolve { path, source })` with
  `source.kind() == NotFound`.
- Unit test: `canonicalize_root` resolves a symlink. Create a real dir, symlink
  to it, pass the symlink path, assert the returned path is the symlink target
  (not the symlink itself).
- Unit test: fallback works inside a temp git repo. `git init` a temp dir, `cd`
  into it, call `resolve_root(None, &config_with_no_versions)`, assert the
  result is `<tempdir>/_versions`.
- Unit test: fallback fails cleanly outside a git checkout. `cd` to a temp
  directory that is **not** a git repo, call the resolver, assert
  `Err(VersionError::NoDescriptorRoot { cwd })` and that `cwd == <tempdir>`.
- Unit test: `Display` impl produces the OQ5 four-line message with the cwd
  substituted. Snapshot-compare against the expected string.
- Unit test (`#[cfg(target_os = "linux")]`): when `current_dir()` itself fails,
  the cwd renders as `<unknown>` rather than panicking. Simulate the failure by
  creating a temp directory, `cd`-ing into it, `rmdir`-ing it, then calling
  `resolve_root` — on Linux, `getcwd(2)` returns `ENOENT` against a deleted cwd
  and Rust's `std::env::current_dir()` propagates that as `Err`. Non-Linux
  platforms behave differently here; the test is gated on Linux rather than
  coded for portability. The `<unknown>` rendering path is otherwise trivially
  correct by inspection of the `unwrap_or_else` chain.

## Commit 3 — `cbsbuild versions create`: `--versions-dir` flag + write-path cutover

Wire the resolver and helper into the CLI. Refactors the hardcoded write path
that Phase 6 Commit 2 landed.

**Files:**

- `cbsd-rs/cbsbuild/src/cmds/versions.rs` —
  - Add to the `create` subcommand's clap args struct:
    ```rust
    #[arg(long, value_name = "PATH")]
    versions_dir: Option<Utf8PathBuf>,
    ```
  - In the `create` handler, replace the entire existing path-construction block
    (the
    `repo_root().await?.join("_versions").join(type.as_dir_name()).join(...)`
    chain that Phase 6 Commit 2 lands; the exact spelling does not matter —
    every line of it is replaced) with:
    ```rust
    let root = cbscore::versions::resolve_root(
        args.versions_dir.as_deref(),
        &config,
    ).await?;
    let path = cbscore_types::versions::desc::descriptor_path(
        &root, version_type, &desc.version,
    );
    if path.exists() {
        return Err(VersionError::AlreadyExists { path }.into());
    }
    cbscore::versions::desc::write_descriptor(&desc, &path).await?;
    ```
    `write_descriptor` (Phase 4 Commit 1) calls `tokio::fs::create_dir_all` on
    `path.parent()` internally before the JSON write; no separate `mkdir -p`
    step is needed at this call site.
  - Remove any direct `git::repo_root` call from this command path; the resolver
    owns that fallback now.
- `cbsd-rs/cbsbuild/src/cmds/versions.rs` `--help` strings — document
  `--versions-dir` exactly once, naming the precedence and the `_versions`
  fallback:
  ```text
  --versions-dir <PATH>
      Override the descriptor store root for this invocation. Precedence:
      this flag, then Config.paths.versions in cbs-build.config.yaml,
      then <git-root>/_versions if invoked inside a git checkout.
  ```

**Design constraints:**

- **CLI UX parity for the no-flag path** (CLAUDE.md correctness invariant 2).
  Operators who do not pass `--versions-dir` and do not set `paths.versions` see
  byte-identical behaviour to Python — same destination path, same
  `EEXIST`-style refusal-to-overwrite, same pretty-printed JSON.
- **Help text precedence ordering.** Match the order of the precedence
  evaluation (CLI > config > git fallback) so an operator reading
  `cbsbuild versions create --help` can predict where their descriptor will land
  without reading the resolver source.
- **No new env var.** Per design 004 §Non-Goals — paths come from config or CLI;
  no `CBS_VERSIONS_DIR`.
- **Drop the FIXME.** The Python source carries
  `# FIXME: make this configurable` at
  `cbscore/src/cbscore/cmds/versions.py:88`. The Rust port closes that FIXME;
  the commit message names it explicitly.

**Testable:**

- Unit test: passing `--versions-dir /tmp/cbs-test-versions` writes the
  descriptor to `/tmp/cbs-test-versions/<type>/<VERSION>.json`. Use a
  `tempfile::TempDir`; assert the file exists at the expected path after the
  command runs.
- Unit test: with `--versions-dir` unset but
  `Config.paths.versions = Some("/tmp/cfg-versions")`, the write lands under
  `/tmp/cfg-versions/<type>/`.
- Unit test: with both unset and the test cwd inside a temp git repo, the write
  lands under `<tempdir>/_versions/<type>/` — byte-identical to Python parity.
- Unit test: with both unset and the test cwd outside any git repo, the command
  exits non-zero with the OQ5 error text on stderr (no panic, no
  `std::io::Error` leaked through). No M1 smoke-gate extension in this commit —
  `cbsbuild build` does not accept `--versions-dir` (per design 004 §OQ4 the
  build subcommand is a read site that takes the descriptor path as an explicit
  argument), so adding the flag to the gate's existing invocation would produce
  a clap parse error. The resolver itself is fully covered by the four unit
  tests above.

## End-of-feature acceptance

After all three commits land:

- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace`, `cargo fmt --all --check` all pass with zero
  warnings.
- `cbsbuild versions create -t dev v0.0.1` (no flag, inside a git checkout)
  writes `<git-root>/_versions/dev/v0.0.1.json` — Python parity check.
- `cbsbuild versions create -t dev v0.0.1 --versions-dir /tmp/x` writes
  `/tmp/x/dev/v0.0.1.json` — resolver-CLI-flag check.
- `cbsbuild versions create -t dev v0.0.1` outside any git checkout, with no
  flag and no `paths.versions` set, exits non-zero with the OQ5 four-line error
  message — fallback-failure check.
- The six forward-pointing comments scattered across the workspace that
  reference seq-004 as future work resolve to real symbols and the comments
  themselves are deleted (or rewritten, where they sit inside permanent doc
  blocks) by the commit that introduces the referenced symbol. As of 2026-05-21
  the inventory is:
  - `cbsd-rs/cbscore-types/src/versions/utils.rs` (line 22) — resolves in
    Commit 1.
  - `cbsd-rs/cbscore-types/src/versions/errors.rs` (line 19) — resolves in
    Commit 2.
  - `cbsd-rs/cbscore/src/versions.rs` (line 10, inside the crate-level `//!`
    module doc) — Commit 2 rewrites the module doc when it adds
    `pub mod resolve;` and `pub use resolve::resolve_root;`. The implementer
    updates the doc-block prose rather than deleting a single TODO line.
  - `cbsd-rs/cbscore/src/utils/git.rs` (lines 231–232) — resolves in Commit 2.
  - `cbsd-rs/cbsbuild/src/cmds/versions.rs` (line 9, inside the crate-level
    `//!` module doc — "Python-parity hardcoded path; seq-004 makes it
    configurable") — Commit 3 rewrites the bullet to describe the configurable
    shape (CLI flag, config field, fallback) rather than deleting it; the module
    doc remains as permanent documentation.
  - `cbsd-rs/cbsbuild/src/cmds/versions.rs` (line 166, inside the `create`
    handler) — resolves in Commit 3 as the handler's write-path block is
    replaced by the `resolve_root` + `descriptor_path` chain.
- Plans README progress table updates: the §"Related plans › seq-004" bullet
  drops the "Pending" framing, the plan's own progress table flips all three
  rows to `Done`. (Same commit boundary as Commit 3 so the README state matches
  the on-disk reality.)
