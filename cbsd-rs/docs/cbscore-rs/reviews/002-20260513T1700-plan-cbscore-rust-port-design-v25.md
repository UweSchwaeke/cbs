# cbscore-rs Plan Review v25 — Pre-impl Audit Pass 5 Closure Confirmation

## Scope

Focused confirmation review of the 17 pre-implementation audit pass-5 findings
(E2-1, E2-2, E3-1, E3-2, E3-3, E4-1, E4-2, E4-3, E4-4, E5-1, E5-2, E6-1, E6-2,
E7-1, E8-1, E8-2, E8-3) closed in commit `139f186`. Each closure is verified
against the plan files modified in that commit. The v24 conditional (N1 /
`ComponentError::Parse`) is confirmed closed by commit `153a23d` (landed before
`139f186`). Prior-review findings (v1–v24) are out of scope.

## Method

1. Read the diff stat for `139f186` (seven plan files modified).
2. Read all seven plan files in full.
3. Execute the verification grep for each of the 17 findings.
4. Spot-check the five no-drift items from the checklist.
5. Run `prettier --check` on all seven edited files.
6. Check for new contradictions — in particular E3-2 "no Debug" vs §Testable
   blocks, and E6-1 for scope completeness.

## Closure Verification

### E2-1 — RAII drop guard: sync-only kill, zombie semantics

**Status: CLOSED — fully correct.**

- `grep "Reaping happens in the guard" plans/002-…-02-*.md` returns zero hits.
  Old text is gone. Pass.
- Phase 2 Commit 1 §Design constraints names `Child::start_kill()` explicitly:
  "`Drop` impl calls **`Child::start_kill()` only** — `Drop` is sync and cannot
  `.await`, so true reaping via `Child::wait().await` is **not** performed by
  the guard." Pass.
- The zombie semantics are stated: "The zombie child is left for the OS to reap
  when the parent process exits (or for an explicit caller to reap via the
  explicit `async fn cleanup` path on normal exit)." Pass.
- The rationale — sync-only kill is intentional to avoid blocking the executor
  and prevent re-entering the tokio runtime from `Drop` — is documented. Pass.

### E2-2 — Phase 7 Commit 1 channel: unbounded + sync rationale

**Status: CLOSED — fully correct.**

- Phase 7 Commit 1 §Subscriber layer design names
  `tokio::sync::mpsc::UnboundedSender<String>` (line 186) constructed via
  `tokio::sync::mpsc::unbounded_channel()` (line 187). Pass.
- Rationale present: "bounded would deadlock the tracing thread because
  `on_event` is `&self` synchronous and cannot `.await` on backpressure;
  unbounded is the only safe choice for a tracing layer's send path" (lines
  187–190). Pass.
- Receiver side also named: `mpsc::UnboundedReceiver<String>` (line 201). Pass.

### E3-1 — SkopeoOpts credential args via `CmdArg::Secure(SecureSkopeoCreds)`

**Status: CLOSED — fully correct.**

- Phase 2 Commit 3 §Design constraints has the "Secret redaction for
  `--src-creds` / `--dest-creds`" bullet. The credential argument is pinned to
  `CmdArg::Secure(Box::new(SecureSkopeoCreds { ... }))` with the explicit
  "**never** as `CmdArg::Plain(format!("user:{password}"))`" prohibition. Pass.
- `SecureSkopeoCreds::redacted()` produces `"<user>:****"` is stated. Pass.
- §Testable has the "**Credential redaction:**" bullet asserting the password
  appears as `****` and username in plain text, matching the `<user>:****`
  shape. Pass.

### E3-2 — Credential leaf types without `#[derive(Debug)]`

**Status: CLOSED — fully correct.**

- Phase 1 Commit 3 §Design constraints has the "Credential leaf types do NOT
  derive `Debug`" bullet. Pass.
- All ten affected types are enumerated: `GitCreds`, `GitPlainCreds`,
  `GitVaultCreds`, `StorageCreds`, `StoragePlainCreds`, `StorageVaultCreds`,
  `SigningCreds`, `SigningPlainCreds`, `SigningVaultCreds`, `RegistryCreds`.
  Pass.
- Derive set pinned: `#[derive(Clone, Serialize, Deserialize)]` only — no
  `Debug`. Pass.
- The mechanism is stated: "absence of `Debug` prevents accidental exposure at
  compile time." Authors needing diagnostics must write a hand-redacted
  formatter. Pass.
- No §Testable bullet anywhere in the plan corpus asserts `{:?}` formatting on a
  credential type (grep across all seven plan files returns zero hits). No
  contradiction introduced. Pass.

### E3-3 — SSH-key tempfile mode-0600 assertion

**Status: CLOSED — fully correct.**

- Phase 3 Commit 3 §Testable has a "Mode-0600 assertion on the SSH key tempfile
  written by `secrets/git.rs` when extracting a `GitVaultCreds::Ssh` payload:
  the file is not world-readable (parallel to the `dump_to_runner` mode test)."
  bullet at line 321. Pass.
- This is a separate bullet from the `dump_to_runner` mode assertion at line 320
  — the parallel structure is explicit. Pass.

### E4-1 / E4-2 / E4-3 / E4-4 — Pinned Display text for Rust-only error variants

**Status: CLOSED — fully correct.**

Phase 1 Commit 2 §Design rules "Operator-facing Display text for Rust-only
variants is pinned" block enumerates:

- `ConfigError::MissingSchemaVersion`:
  `"missing 'schema-version' key in {path}"`. Pass.
- `ConfigError::UnknownSchemaVersion`:
  `"unsupported schema-version {found} in {path} (max supported: {max_supported}); upgrade cbscore-rs"`.
  Pass.
- `VersionError::MissingSchemaVersion`:
  `"missing 'schema_version' key in {path}"` (snake — descriptor JSON). Pass.
- `VersionError::UnknownSchemaVersion`: "parallel to ConfigError with snake
  `schema_version`" — the exact string is derivable by substituting snake casing
  into the ConfigError template. The format is pinned by reference; acceptable
  as a pin. Pass.
- `ComponentError::DuplicateComponentName`:
  `"duplicate component name '{name}': first defined at {first}, redefined at {second}"`.
  Pass.
- `ComponentError::MissingSchemaVersion`:
  `"missing 'schema-version' key in component file {path}"`. Pass.
- `ComponentError::UnknownSchemaVersion`: "parallel to ConfigError." Pass.
- `ComponentError::Yaml`: `"YAML parse error in {path}: {message}"`. Pass.
- `BuilderError::MissingScript`: `"required build script not found at {path}"`.
  Pass.

Phase 3 Commit 2 §Files pins Display text for all four `VaultError` variants:

- `PathNotFound`: `"vault path '{mount}/{path}' not found"`. Pass.
- `AuthFailed`: `"vault {method} auth failed: {source}"`. Pass.
- `RequestFailed`: `"vault request failed: {source}"`. Pass.
- `BadResponse`: `"vault returned an unexpected response: {message}"`. Pass.

### E5-1 / E5-2 — Corpus-wide documentation requirement

**Status: CLOSED — fully correct.**

- Phase 1 Commit 2 §Design rules has a "**Documentation requirement
  (corpus-wide).**" paragraph naming: every public item carries a doc comment
  per CLAUDE.md §Documentation; every public function/method has an `# Examples`
  block; IO-bearing functions use `no_run`; `#![warn(missing_docs)]` on every
  non-test crate gates the zero-warnings clippy policy. Pass.
- The rule is stated as applying to every later phase's public items — "This
  rule applies to every later phase's pub items — flagging here once rather than
  repeating in every commit spec." Pass.

### E6-1 — Acceptance criteria reformulated to "structurally equivalent"

**Status: PARTIALLY CLOSED — residual minor (N1).**

The primary target (Commit 5 §Design constraints acceptance criterion 2) is
correctly updated:

- Phase 6 Commit 5 acceptance criterion 2 reads "structurally equivalent" (not
  "byte-identical"). The cardinality / NEVRA / file-list / dependencies
  checklist is spelled out in full. `BUILDTIME` and `BUILDHOST` are named as the
  reason byte-for-byte equality is unachievable. Pass.
- Phase 7 Commit 3 acceptance criterion 3 reads "structurally equivalent" with
  the same cardinality / NEVRA / file-list / dependencies checklist. Pass.

**Residual (MINOR — N1):** Phase 6 §Goal section (lines 32–39) still carries the
old "byte-identical" language in two sentences:

> End the phase with the M1 acceptance gate: `cbsbuild build` against the real
> `ceph` component produces byte- identical RPM payloads to the Python
> implementation.

> the M1 acceptance test (Commit 5) builds the real `ceph` component and the
> produced RPMs match the Python output byte-for-byte.

These sentences are in the §Goal summary, not in the authoritative acceptance
criteria block, but they directly contradict the corrected Commit 5 acceptance
criterion and the E6-1 rationale (that `BUILDTIME`/`BUILDHOST` make
byte-for-byte equality unachievable). An implementer reading the Phase 6 plan
encounters the §Goal claim first, then finds the §Design constraints criteria
later — the contradiction would cause confusion at the M1 gate.

**Resolution:** Two one-sentence edits in Phase 6 §Goal:

- Line 33: Replace "produces byte-identical RPM payloads to the Python
  implementation" with "produces an RPM set structurally equivalent to the
  Python implementation (same cardinality, NEVRA, file list, and dependencies)".
- Line 39: Replace "RPMs match the Python output byte-for-byte" with "RPMs are
  structurally equivalent to the Python output".

### E6-2 — `write_descriptor` trailing-newline claim replaced

**Status: CLOSED — fully correct.**

- Phase 4 Commit 1 §Design constraints no longer claims "Python writer emits a
  newline-terminated file" as an assumed fact. Pass.
- New text: "The trailing-byte behaviour (newline or no newline) matches Python
  `cbscore/versions/desc.py` byte-for-byte where descriptors are written to
  operator-visible files. The implementer reads the Python writer at port time
  and matches its exact terminator — no assumption baked into this plan about
  whether Python emits a trailing `\n`; verify against the Python source as the
  first step of Commit 1." Pass.

### E7-1 — `LoggingConfig { log_file: Utf8PathBuf }` pinned

**Status: CLOSED — fully correct.**

- Phase 1 Commit 3 §Files for `config/mod.rs` describes:
  - `LoggingConfig` with one field `log_file: Utf8PathBuf`, wire-key `log-file`
    (kebab via `#[serde(rename_all = "kebab-case")]`). Pass.
  - `Config.logging: Option<LoggingConfig>` with `#[serde(default)]` — default
    is no file appender. Pass.
- Reference to `cbscore/config.py` `LoggingConfig` at line 156 is cited for the
  Python-parity rationale. Pass.

### E8-1 — SIGTERM-during-cleanup `Arc<AtomicBool>` coordination

**Status: CLOSED — fully correct.**

- Phase 4 Commit 3 §State machine §4 has a "**SIGTERM-during-cleanup
  coordination.**" section. Pass.
- It introduces a shared `Arc<AtomicBool>` cleanup-in-progress flag. Pass.
- SIGTERM handler no-ops on `flag.load(Acquire)` being set; `cleanup`'s first
  action is `flag.store(true, Release)`. Pass.
- Alternative (tolerate `PodmanError::ContainerNotFound`) also documented. Pass.

### E8-2 — `opts.force` clear ordering: `remove_dir_all` then `create_dir_all`

**Status: CLOSED — fully correct.**

- Phase 5 Commit 1 §Design constraints "**Clear-then-fetch ordering** (pinned)"
  bullet names `tokio::fs::remove_dir_all(scratch/<component>)` first, then
  `tokio::fs::create_dir_all(scratch/<component>)`. Pass.
- SIGTERM-mid-clear recovery semantic is explicit: "the scratch dir is left
  absent — the next build with the same VERSION rebuilds from scratch. This is
  the accepted recovery semantic for `force = true`." Pass.

### E8-3 — Final-batch flush when `Receiver` returns `None`

**Status: CLOSED — fully correct.**

- Phase 7 Commit 1 §Subscriber layer design "**Final-batch flush on channel
  close (pinned):**" bullet pins the behaviour: when the unbounded `Sender` is
  dropped the `Receiver` loop receives `None` from `recv().await`; on `None`,
  the batcher task emits any pending partial batch as a final
  `WorkerMessage::BuildOutput` before exiting. Pass.
- Explicit statement: "No log lines are silently dropped on build completion."
  Pass.

## No-Drift Check

1. **Phase 5 commit structure:** 7 commits; `core::component` is Commit 2 (table
   row 2: `` `cbscore: add core::component module (load_components IO)` ``).
   Pass.
2. **Phase 1 Commit 2 logger enumeration:** 22 named targets confirmed
   (`cbscore` plus 21 sub-targets across `config`, `core::component`, `secrets`,
   `runner`, `builder`, `builder::prepare`, `builder::rpmbuild`,
   `builder::signing`, `builder::upload`, `containers`, `images::skopeo`,
   `images::signing`, `images::sync`, `releases`, `utils::buildah`,
   `utils::git`, `utils::podman`, `utils::s3`, `utils::subprocess`,
   `utils::vault`, `versions`). Pass.
3. **Phase 3 Commit 3 `Secrets` struct:** uses `HashMap<String, GitCreds>`,
   `HashMap<String, StorageCreds>`, `HashMap<String, SigningCreds>`,
   `HashMap<String, RegistryCreds>` — all four families present at lines
   245–247. Pass.
4. **Design 002 §Secrets:** all four families (`GitCreds`, `StorageCreds`,
   `SigningCreds`, `RegistryCreds`) present (lines 608, 655, 699, 768). Pass.
5. **Design 004 `write_descriptor` mkdir-p:** `create_dir_all` lives inside
   `write_descriptor`; call site does not repeat it (lines 347–348). Pass.
6. **Design 005 patch walker target:** `"cbscore::builder::prepare"` present at
   line 504 of design 005. Pass.
7. **CLAUDE.md correctness invariants:** exactly 6 numbered items (1–6); item 6
   is "Runner container reproducibility". Pass.
8. **Prettier:** `prettier --check` passes on all seven edited plan files. Pass.
9. **No new contradictions from E3-2:** zero `{:?}` formatting assertions on
   credential types in §Testable blocks across all seven plan files. Pass.
10. **v24 N1 (`ComponentError::Parse`) pre-requisite:** commit `153a23d` (landed
    before `139f186`) closes the v24 conditional. Phase 5 Commit 2 §Design
    constraints now reads "returns the **last** per-file error variant
    encountered (`ComponentError::Yaml`, `MissingSchemaVersion`, or
    `UnknownSchemaVersion`) only if **no** components were successfully loaded"
    — no `ComponentError::Parse` reference remains. Pass.

## Findings

### N1 — Phase 6 §Goal still claims "byte-identical" RPM output

**Severity: MINOR**

**Location:** Phase 6 plan (`002-20260508T1558-06-cbsbuild-cli.md`), §Goal
section, lines 32–33 and 38–39.

**Problem:** The §Goal section contains two residual "byte-identical" /
"byte-for-byte" claims about RPM output:

- Line 33: "produces byte- identical RPM payloads to the Python implementation."
- Line 39: "the produced RPMs match the Python output byte-for-byte."

The E6-1 closure correctly updated the authoritative acceptance criteria in
Commit 5 §Design constraints (line 353 onwards) to "structurally equivalent" and
explained why byte-for-byte equality is unachievable (`BUILDTIME`, `BUILDHOST`).
However, the §Goal section — which appears at the very top of the phase
description and is the first text an implementer reads — was not updated. The
contradiction between §Goal and the Commit 5 §Design constraints is directly
observable in the same plan document. An implementer who reads §Goal and writes
a test expecting byte-for-byte equality will fail against the Commit 5
acceptance criteria.

**Resolution:** Two one-sentence edits in Phase 6 §Goal (lines 32–33 and 38–39):

- Replace "produces byte- identical RPM payloads to the Python implementation"
  with "produces an RPM set structurally equivalent to the Python implementation
  (same cardinality, NEVRA, file list, and dependencies per Commit 5 §Design
  constraints)".
- Replace "the produced RPMs match the Python output byte-for-byte" with "the
  produced RPMs are structurally equivalent to the Python output".

This is a one-commit, two-sentence edit; no re-review required.

## Verdict

**CONDITIONAL — E2..E8 (17 findings) closed; pre-impl audit pass 5 fully
resolved with one residual minor (N1).**

N1 (Phase 6 §Goal "byte-identical" claim contradicts the E6-1 closure in Commit
5 §Design constraints) is a two-sentence edit with no design ambiguity. Once N1
is closed:

> **Approve — E2..E8 (17 findings) closed; pre-impl audit pass 5 fully resolved;
> design corpus + plan corpus ready for Phase 1 implementation start.**
