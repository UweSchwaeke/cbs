# seq-004 — Configurable `VersionDescriptor` Location

## Status

**Approved — finalized and ready for M1 implementation.** Audited at v2
(`reviews/004-20260513T1003-plan-configurable-version-descriptor-location-v2.md`,
verdict `49d6f78`); zero findings across CRITICAL / MAJOR / MINOR / SUGGESTION /
OPEN QUESTION. Implements design 004 Migration table steps 1–4 (step 5 is
post-M1, deferred to seq-003). Interleaves between seq-002 Phase 6 Commit 4 and
Commit 5.

**Review trail:**

- Plan drafted `1144458` (2026-05-13).
- Corpus pass v17 `4f27d2f` — 5 findings (N1, N2, S1, S2, Q1 across seq-002 +
  seq-004) → closed in `e3cb122` → v18 `a806158` confirmation, clean.
- Focused v1 `c197927` — 2 MINORs (N1 §Depends on misattribution, N2 surviving
  hedges in Commit 3 §Files) → closed in `b595554` → v2 `49d6f78` confirmation,
  clean.

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

seq-004 interleaves between **seq-002 Phase 6 Commit 4 and Commit 5**.

The recommended order is: land Phase 6 Commits 1–4, then this plan's three
commits, then Phase 6 Commits 5–6 (visibility audit + M1 smoke gate). This way
the smoke gate exercises the configurable resolver instead of the transitional
hardcoded path, and the visibility audit (Commit 5) covers seq-004's newly-added
items too. The Phase 6 plan's §Out of scope block records this interleave point
and the slip-handling fallback explicitly (lines 67–77 of
`002-20260508T1558-06-cbsbuild-cli.md`); if seq-004 slips, the M1 smoke gate
(Commit 6) runs against the hardcoded path with a note that `--versions-dir` is
not yet exercised, and re-runs after seq-004 lands.

Step 5 of design 004's Migration table — the interactive `config init` "Versions
path" prompt and the bypass-mode pre-fill — is **deliberately out of scope**
here. It lives under design 003 (interactive config init), which is post-M1 /
seq-003.

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
  (`/cbs/_versions`).** Owned by design 003 / seq-003; lands post-M1.
- **Wire-format schema bump.** OQ6 — `Config.schema_version` stays at 1 because
  this is a pre-M1 0.x change; the schema is mutable until M1 ships.

## Commit 1 — `cbscore-types`: paths field, `VersionType::as_dir_name`, `descriptor_path`

Land the pure-type additions in `cbscore-types`. No IO, no async. All three
pieces are testable in isolation via doctests and round-trip serde tests on
`Config`.

**Files:**

- `cbsd-rs/cbscore-types/src/config/paths.rs` — append
  `versions: Option<Utf8PathBuf>` to `PathsConfig`, marked `#[serde(default)]`
  so existing YAML files (which omit the field) parse cleanly. Keep the existing
  field ordering and the `#[serde(rename_all = "kebab-case")]` attribute on the
  struct. The YAML key resolves to `versions` (a single word; kebab-case is a
  no-op).
- `cbsd-rs/cbscore-types/src/versions/utils.rs` — add
  `impl VersionType { pub fn as_dir_name(&self) -> &'static str }` returning
  `"release"`, `"dev"`, `"test"`, `"ci"`. The strings match Python's
  `cbscore/versions/utils.py:VersionType` serde value names (snake_case per
  CLAUDE.md correctness invariant 4) and are the filesystem directory
  components, locked in by design 004 OQ3 and the type-encoded-in-layout
  invariant.
- `cbsd-rs/cbscore-types/src/versions/desc.rs` — add
  `pub fn descriptor_path(root: &Utf8Path, ty: VersionType, version: &str) -> Utf8PathBuf`,
  implemented as `root.join(ty.as_dir_name()).join(format!("{version}.json"))`.
  This is the single source of truth for the `<root>/<type>/<VERSION>.json`
  layout; every other code path that needs it imports this helper.

**Design constraints:**

- **No schema-version bump.** Per design 004 OQ6, this is a pre-M1 0.x schema
  extension; `Config.schema_version` stays at 1.
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
  parses as `None`, re-serialises with the field present-but-null or omitted
  depending on `#[serde(skip_serializing_if = "Option::is_none")]` (apply that
  attribute if existing path fields use it, for consistency).
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
  `std::io::Error` leaked through).
- Integration test slot for the Phase 6 Commit 6 M1 smoke gate: add
  `--versions-dir <tempdir>` to one invocation so the gate exercises the
  resolved-CLI-flag path end-to-end. (This is a one-line addition to the
  existing test fixture; the gate itself is Phase 6's responsibility.)

## End-of-feature acceptance (interleave gate)

After all three commits land, before seq-002 Phase 6 Commits 5–6 (visibility
audit + M1 smoke gate) run:

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
- README `Related plans › seq-004` bullet updated to link this plan file
  (`004-20260513T0900-configurable-version-descriptor-location.md`) and to mark
  the seq as **Landed** (or whichever status keyword matches the README's
  existing usage when the work completes).

When this gate is green, seq-002 Phase 6 Commits 5–6 run and include
`--versions-dir` in the M1 smoke gate's fixtures (Commit 6) so the gate
certifies the final M1-1.0.0 CLI surface.
