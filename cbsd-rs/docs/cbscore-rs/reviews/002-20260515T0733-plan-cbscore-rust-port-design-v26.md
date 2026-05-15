# cbscore-rs Plan Review v26 — Pre-impl Audit Pass 6 Closure Confirmation

## Scope

Focused confirmation review of the 10 pre-implementation audit pass-6 findings
(F1-1, F1-2, F1-3, F1-4, F2-1, F2-2, F2-3, F2-4, F4-2, F5-1) closed in commit
`7a5b271`. Each closure is verified against the plan and design files modified
in that commit. The v25 conditional (N1 / Phase 6 §Goal "byte-identical"
language) was a prerequisite and is confirmed closed before this pass begins.
Prior-review findings (v1–v25) are out of scope.

## Method

1. Read the diff stat for `7a5b271` (five files modified: design 001, Phase 1
   plan, Phase 2 plan, Phase 3 plan, Phase 5 plan).
2. Read all five modified files in full.
3. Execute the verification check for each of the 10 findings in order.
4. Spot-check the no-drift items from the checklist.
5. Run `prettier --check` on all five edited files.
6. Check for new contradictions introduced by the closures.

## Closure Verification

### F1-1 + F1-2 — Per-capability `CBSCORE_TEST_*` env vars for Phase 5

integration tests

**Status: CLOSED — fully correct.**

The finding required each of Phase 5 Commits 1–6 to name a specific
`CBSCORE_TEST_*` env var (with a "set X to enable" message) for its
`#[ignore]`-gated integration test. Each is verified:

**Commit 1 (`builder::prepare`):** §Testable integration test bullet reads
"Opted in via `CBSCORE_TEST_GIT_REMOTE=<url>` (defaults to skipping when unset);
`#[ignore]`-skipped with a 'set CBSCORE_TEST_GIT_REMOTE to enable' message."
Pass.

**Commit 2 (`core::component`):** §Testable integration test bullet reads "Opted
in via `CBSCORE_TEST_COMPONENTS_DIR=<path>` (typically the operator's local
`components/` tree); `#[ignore]`-skipped with a 'set CBSCORE_TEST_COMPONENTS_DIR
to enable' message when unset." Pass.

**Commit 3 (`builder::rpmbuild`):** §Testable integration test bullet reads
"Opted in via `CBSCORE_TEST_RPMBUILD=1` (the host must have a working `rpmbuild`
binary on PATH); `#[ignore]`-skipped with a 'set CBSCORE_TEST_RPMBUILD=1 to
enable' message when unset." Pass.

**Commit 4 (`containers` + `images::sync`):** §Testable integration test bullet
reads "Opted in via `CBSCORE_TEST_PODMAN=1` (host must have a working `podman`
binary on PATH); `#[ignore]`-skipped with a 'set CBSCORE_TEST_PODMAN=1 to
enable' message when unset." Pass.

**Commit 5 (`builder::signing` + `images::signing`):** §Testable integration
test bullet reads "Opted in via `CBSCORE_TEST_GPG_KEYRING=<path>` (path to a
test GPG keyring with `--pinentry-mode loopback` compatible setup);
`#[ignore]`-skipped with a 'set CBSCORE_TEST_GPG_KEYRING to enable' message when
unset." The Vault transit signing test reuses the `CBSCORE_TEST_VAULT_ADDR` /
`CBSCORE_TEST_VAULT_TOKEN` env-var contract from Phase 3 Commit 2; the bullet
names both vars and the "`#[ignore]`-skipped with the same message pattern when
unset" note is present. Pass.

**Commit 6 (`builder::upload` + `releases`):** §Testable integration test bullet
reads "additionally requires `CBSCORE_TEST_S3_BUCKET=<bucket>` to name the test
bucket; `#[ignore]`-skipped with a 'set AWS_ENDPOINT_URL +
CBSCORE_TEST_S3_BUCKET to enable' message when either is unset." The companion
AWS\* env vars are noted as reused from Phase 3 Commit 1. Pass.

### F1-3 — Phase 2 Commit 1 RAII smoke test: `CBSCORE_TEST_SIGNAL_PROBE=1`,

"if flaky" hedge gone

**Status: CLOSED — fully correct.**

Phase 2 Commit 1 §Testable RAII drop-guard smoke test bullet reads: "Test is
opted in via `CBSCORE_TEST_SIGNAL_PROBE=1` env var; without it, the test is
`#[ignore]`-skipped with a 'set CBSCORE_TEST_SIGNAL_PROBE=1 to enable' message."
The phrase "optional, `#[ignore]` if flaky in CI" (or any equivalent hedge) is
absent. The test is pinned unconditionally behind the env-var gate with no
flakiness escape hatch. Pass.

### F1-4 — Phase 2 Commit 2 cidfile example: `tempfile::TempDir`, no

hard-coded `/tmp/cid`

**Status: CLOSED — fully correct.**

Phase 2 Commit 2 §Testable command construction bullet reads: "The cidfile path
comes from a per-test `tempfile::TempDir` (never a hard-coded `/tmp/cid` which
would race between parallel test runs)." The explicit parenthetical prohibition
on `/tmp/cid` is present. No hard-coded `/tmp/` path survives in the §Testable
block. Pass.

### F2-1 — Phase 3 Commit 1: S3 uploads idempotent by key, orphan-accept

semantic

**Status: CLOSED — fully correct.**

Phase 3 Commit 1 §Design constraints has a dedicated "**S3 uploads are
idempotent by key.**" bullet. It pins all three required semantics:

- PUT to the same key replaces the existing object silently — overwrite semantic
  confirmed ("PUT to the same key replaces").
- No rollback or cleanup step — partial-upload orphan-accept is explicit ("There
  is no rollback or cleanup step").
- Long-term orphan cleanup is operator policy via S3 lifecycle rules — explicit
  ("configure S3 lifecycle rules on the bucket").
- Python-parity rationale is present ("This matches Python's existing
  behaviour").

Pass.

### F2-2 — Phase 5 Commit 7: scratch dir left in place on stage failure

**Status: CLOSED — fully correct.**

Phase 5 Commit 7 (`builder::run_build`) §Design constraints has the "**Scratch
dir left in place on stage failure.**" bullet. It pins:

- Failure path: `run_build` short-circuits without clearing
  `config.paths.scratch/<component>/`.
- Operator recovery: inspect then re-run with `opts.force=true`.
- `opts.force=true` path: Phase 5 Commit 1 `prepare` clears via
  `remove_dir_all` + `create_dir_all`.
- Python parity: "matches Python `cbsbuild`'s behaviour: build failures leave
  the scratch state for inspection rather than auto-cleaning."
- Scope of RAII coverage: "Each stage's RAII guards (Phase 2 Commit 1 subprocess
  kill; Phase 5 Commit 4 `BuildahWorkingContainer`) handle their own
  child-process / container cleanup; the scratch directory itself is
  operator-owned state."

Pass.

### F2-3 — Phase 3 Commit 3: `resolve_vault_refs` retry-safe semantics

**Status: CLOSED — fully correct.**

Phase 3 Commit 3 §Design constraints has the "**`resolve_vault_refs` is
retry-safe.**" bullet. It pins the in-place-mutation idempotency semantics
precisely:

- Already-resolved `*Plain*` entries are no-ops on re-call ("already- resolved
  plain entries are idempotent no-ops (the match arm finds `*Plain*` and
  skips)").
- Remaining `*Vault*` entries are retried ("the remaining vault-ref entries are
  retried").
- Caller contract on `Err`: "do not call `dump_to_runner` until a subsequent
  `resolve_vault_refs` returns `Ok` (otherwise the dumped YAML carries
  unresolved vault refs that the in-container build cannot dereference)."
- Consequence: "Retry-safety eliminates the need for atomic rollback."

Pass.

### F2-4 — Phase 5 Commit 4: `BuildahWorkingContainer` RAII guard naming

`buildah unmount + buildah rm`

**Status: CLOSED — fully correct.**

Phase 5 Commit 4 §Design constraints has the "**`BuildahWorkingContainer` RAII
guard for cleanup on failure.**" bullet. It pins:

- The type name `BuildahWorkingContainer`. Pass.
- `Drop` impl calls `buildah unmount <container-id>` +
  `buildah rm <container-id>` synchronously (fire-and-forget, errors swallowed).
  Both cleanup commands named explicitly. Pass.
- The mirror reference: "mirrors the Phase 2 Commit 1 RAII drop-guard pattern."
  Pass.
- The failure mode it prevents: "without it, a failed build leaves an orphan
  container that future `buildah` invocations may collide with." Pass.
- The success path: "The success path consumes the guard explicitly via a
  `commit` method that destructures it before tagging the committed image."
  Pass.

Pass.

### F4-2 — Phase 1 Commit 1: `nix` under `[dev-dependencies]`, no platform

gating

**Status: CLOSED — fully correct.**

Phase 1 Commit 1 §Files `cbscore/Cargo.toml` block describes the `nix` dep as
follows: "`[dev-dependencies]` adds `nix` with the `signal` feature for the
Phase 2 Commit 1 RAII drop-guard smoke test." The placement is unambiguously
`[dev-dependencies]`, not `[dependencies]`. The rationale is explicitly
documented: "it's a test-only probe with no production caller."

No `[target.'cfg(unix)'.dependencies]` block appears anywhere in the
`cbscore/Cargo.toml` spec — platform gating is absent, matching the stated
rationale: "No platform gating: the workspace is Linux-only by domain (podman +
Ceph RPMs), matching the rest of `cbsd-rs/`."

The parenthetical "added beyond the design 001 §Cargo Sketch" note is present,
flagging it as a follow-up §Cargo Sketch edit on design 001 (alongside
`tracing-subscriber`) — non-blocking. Pass.

### F5-1 — `debug_filter()` rename: returns `EnvFilter`, binary installs;

Design 001 §Downstream Consumers table updated; zero `set_debug_logging` hits in
non-Python text

**Status: CLOSED — fully correct.**

**Phase 1 Commit 2 §Files `logger.rs` description:**

The description names `pub fn debug_filter() -> tracing_subscriber::EnvFilter`
explicitly and states the no-install contract in three places:

1. "**constructs and returns** an `EnvFilter` reading `RUST_LOG` / `CBS_DEBUG`".
2. "It does **NOT** call `.init()` or install a global subscriber — the binary
   boundary (`cbsbuild::main`, `cbc::main`, etc.) does the installation."
3. "(Renamed from the earlier `set_debug_logging` to make the no-install
   contract explicit in the function name.)"

The rationale is also present: "Returning the `EnvFilter` (rather than
installing it) keeps `cbscore-types` free of global-state mutation and
background IO — the env-var read happens at the binary's explicit invocation of
`debug_filter()`, not at module load." Pass.

**Design 001 §Downstream Consumers table (`cbc` row):**

The `cbc` row reads: "`cbscore_types::logger::debug_filter` (the Rust
counterpart of Python's `logger.set_debug_logging`; returns an `EnvFilter`,
caller installs)". The Rust symbol path is
`cbscore_types::logger::debug_filter`; the table correctly names the return type
(`EnvFilter`) and documents that installation is the caller's responsibility.
Pass.

**Design 001 §Crate Responsibilities:**

Line 225 also names `debug_filter()` with the parenthetical "(returns an
`EnvFilter`; binary boundary installs; no global-state mutation in
`cbscore-types`)". Pass.

**`set_debug_logging` grep across plan and design corpus:**

`grep -rn "set_debug_logging" plans/ design/` returns three hits:

1. `plans/002-…-01-types.md:173` — the rename note itself ("Renamed from the
   earlier `set_debug_logging`"). This is an explanatory reference, not a
   surviving functional occurrence. Pass.
2. `design/001-…-cbscore-project-structure.md:65` — the `cbc` row describes
   `cbscore_types::logger::debug_filter` as "the Rust counterpart of Python's
   `logger.set_debug_logging`". This parenthetical names the old Python function
   for orientation; the Rust name throughout is `debug_filter`. Pass.
3. `design/002-…-cbscore-rust-port-design.md:1400` — "Python consumers that
   import `from cbscore.logger import set_debug_logging` continue to call into
   the existing Python `cbscore` package". This is the Python- interop paragraph
   describing the Python consumer's import path. It is correct and intentionally
   unchanged — it describes Python code, not the Rust API. Pass.

Zero hits name `set_debug_logging` as the Rust function name or as a Rust symbol
to implement. Pass.

## No-Drift Check

1. **Phase 5 commit structure:** 7 commits; `core::component` is Commit 2 (table
   row 2: `` `cbscore: add core::component module (load_components IO)` ``).
   Unchanged from v25. Pass.

2. **Phase 3 Commit 3 `Secrets` struct:** Uses `HashMap<String, GitCreds>`,
   `HashMap<String, StorageCreds>`, `HashMap<String, SigningCreds>`,
   `HashMap<String, RegistryCreds>` — all four families present. Unchanged.
   Pass.

3. **Phase 1 Commit 2 logger.rs target enumeration:** 22 named targets confirmed
   (`cbscore` plus 21 sub-targets: `cbscore::config`,
   `cbscore::core::component`, `cbscore::secrets`, `cbscore::runner`,
   `cbscore::builder`, `cbscore::builder::prepare`,
   `cbscore::builder::rpmbuild`, `cbscore::builder::signing`,
   `cbscore::builder::upload`, `cbscore::containers`, `cbscore::images::skopeo`,
   `cbscore::images::signing`, `cbscore::images::sync`, `cbscore::releases`,
   `cbscore::utils::buildah`, `cbscore::utils::git`, `cbscore::utils::podman`,
   `cbscore::utils::s3`, `cbscore::utils::subprocess`, `cbscore::utils::vault`,
   `cbscore::versions`). Unchanged. Pass.

4. **Phase 6 Commit 5 acceptance criterion 2:** Reads "structurally equivalent"
   (not byte-identical). Phase 6 §Goal section also reads "structurally
   equivalent" (v25 N1 closure confirmed). Pass.

5. **Phase 7 Commit 1 subscriber layer:** Uses
   `tokio::sync::mpsc::UnboundedSender<String>` via
   `tokio::sync::mpsc::unbounded_channel()`; final-batch flush on channel close
   pinned ("when the unbounded `Sender` is dropped the `Receiver` loop receives
   `None` from `recv().await`; on `None`, the batcher task emits any pending
   partial batch as a final `WorkerMessage::BuildOutput` before exiting").
   Unchanged. Pass.

6. **CLAUDE.md correctness invariants:** Exactly 6 numbered items (1–6); item 6
   is "Runner container reproducibility"; Python compat is not a separate item
   (item 1 carries the "cross-language byte-equality is not a requirement"
   disclaimer). Unchanged. Pass.

7. **`prettier --check`:** Passes on all five files edited in `7a5b271`
   (`design/001-…`, `plans/002-…-01-types.md`,
   `plans/002-…-02-subprocess-and-shell-tools.md`,
   `plans/002-…-03-storage-and-secrets.md`,
   `plans/002-…-05-builder-and-releases.md`). Pass.

## Findings

None. All 10 closures are correct, complete, and consistent with the rest of the
plan corpus. No new contradictions were introduced.

## Verdict

> **Approve — F1+F2+F4-2+F5-1 (10 findings) closed; pre-impl audit pass 6 fully
> resolved; design corpus + plan corpus ready for Phase 1 implementation
> start.**
