# Plan Review — cbscore Rust Port: Phase 3 First Review + Phase 1 & 2 Regression Check — v6

**Plans reviewed:**

- [`002-20260508T1558-03-storage-and-secrets.md`](../plans/002-20260508T1558-03-storage-and-secrets.md)
  — Phase 3, **first review**
- [`002-20260508T1558-01-types.md`](../plans/002-20260508T1558-01-types.md) —
  Phase 1, regression/cross-reference recheck only
- [`002-20260508T1558-02-subprocess-and-shell-tools.md`](../plans/002-20260508T1558-02-subprocess-and-shell-tools.md)
  — Phase 2, regression/cross-reference recheck only
- [`plans/README.md`](../plans/README.md) — dependency-graph accuracy check

**Prior reviews (all findings closed through v5):**

- v1:
  [`002-20260511T1002-plan-cbscore-rust-port-design-v1.md`](./002-20260511T1002-plan-cbscore-rust-port-design-v1.md)
  — 9 findings (C1, I1, I2, M1–M4, S1–S2); all closed
- v2:
  [`002-20260511T1130-plan-cbscore-rust-port-design-v2.md`](./002-20260511T1130-plan-cbscore-rust-port-design-v2.md)
  — NI1 raised; closed
- v3:
  [`002-20260511T1240-plan-cbscore-rust-port-design-v3.md`](./002-20260511T1240-plan-cbscore-rust-port-design-v3.md)
  — NI1 closed, NF1 (non-blocking), Phase 1 declared ready
- v4:
  [`002-20260511T1400-plan-cbscore-rust-port-design-v4.md`](./002-20260511T1400-plan-cbscore-rust-port-design-v4.md)
  — Phase 2 first review; 6 findings; all closed
- v5:
  [`002-20260511T1530-plan-cbscore-rust-port-design-v5.md`](./002-20260511T1530-plan-cbscore-rust-port-design-v5.md)
  — N-M1 (`nix` undeclared) + N-Nit1 (dup bullet); both closed

**Reviewer:** Staff review, 2026-05-11.

---

## Summary Assessment

Phase 3 is well-structured and largely internally consistent. All five
operations from design 002 §S3 operations are present and correctly placed; the
`S3Error` / `VaultError` placement is coherent with the framework-error
carve-out from the error taxonomy; and the Phase 3 §Out of scope is accurate on
lift-out invariants. Three findings require attention before implementation
starts. Two are minor correctness or consistency gaps (Commit 2 missing a
commit-size rationale paragraph, and an unresolved implementation question
around Vault token caching scope), and one is a minor housekeeping gap (Phase 1
§Out of scope does not yet acknowledge that `S3Error` and `VaultError` are
deferred to Phase 3, as it does for `parse_version` family and
`paths.versions`). There are no blockers. Phase 3 is ready for implementation
start subject to the three minor findings being resolved in the plan text before
the first commit lands.

Phases 1 and 2 are free of regression from Phase 3's addition.

---

## Phase 1 + Phase 2 — Regression Check

No regressions. Specific cross-references checked:

- **Phase 2 §End-of-phase acceptance lift-out grep:** The grep targets only
  `subprocess.rs`, `git.rs`, and `git/errors.rs`. Phase 3 adds `utils/s3.rs`,
  `utils/vault.rs`, `secrets/`, and `config.rs` — none of these are lift-out
  candidates and none alter the grep's scope. Check remains valid.
- **Phase 1 §Out of scope parse_version drift note:** Adding S3/Vault wrappers
  in Phase 3 does not affect the parse_version drift. The drift is about
  `cbscore-types` vs `cbscore` placement of pure parsing functions; S3/Vault are
  IO-side concerns in `cbscore` by design. No contradiction.
- **`cbscore` Cargo.toml dep list (Phase 1 Commit 1):** Phase 3 adds
  `aws-config = "1"`, `aws-sdk-s3 = "1"`, and `vaultrs = "0.8"`. Phase 1 Commit
  1 §Files (plan line 98) says the `cbscore/Cargo.toml` spec includes "the
  IO-side crates that fill in over Phases 2–5". Design 001 §Cargo Sketch lines
  390–396 lists all three crates explicitly (`aws-config = "1"`,
  `aws-sdk-s3 = "1"`, `vaultrs = "0.8"`, `reqwest = …`). The claim in Phase 3
  §Depends on is accurate.
- **N-M1 (`nix` dep) from v5:** Scope unaffected by Phase 3.
- **N-Nit1 (dup bullet, Phase 2 Commit 3):** Scope unaffected by Phase 3.

---

## Strengths

- **Complete S3 operation surface.** Commit 1 lists all five operations from
  design 002 §S3 operations (lines 1156–1165) with correct signatures and the
  correct 404-to-`Ok(false)` semantic for `check_release_exists`. Nothing is
  missing.
- **`S3Error` / `VaultError` placement rationale is explicit.** The plan
  correctly cites design 002 §Error Taxonomy lines 239–240 (framework errors
  that cannot be exhaustively matched) and draws the analogy to `GitError` in
  Phase 2. The reasoning is sound and consistent with the error taxonomy's
  design.
- **Vault auth order.** The plan correctly names the three auth methods in the
  Python-matching order (explicit token → AppRole → userpass) and cites design
  002 line 636.
- **`VaultConfig` import chain is correct.** Commit 2 takes `VaultConfig` from
  `cbscore-types::config::vault` (Phase 1 Commit 3), not from a re-definition.
  The import chain is coherent.
- **`Secrets::load` implementation path is clearly specified.** Commit 3 §Design
  constraints (plan line 221–222) explains that `Secrets::load` uses
  `serde_saphyr` + `VersionedSecrets::into_latest()` from Phase 1 Commit 5. This
  dispels the apparent gap flagged in the review brief.
- **`dump_to_runner` mount-table alignment.** The plan cites the correct mount
  point `/runner/cbs-build.secrets.yaml` from design 002 §Runner Subsystem mount
  table (line 784). The host-side write and the runner-side mount are aligned.
- **`Config::store` parent-dir-create.** Commit 4 preserves the `mkdir -p`
  semantic from design 002 line 498–505, with the correct
  `tokio::fs::create_dir_all` equivalent and the correct motivating example
  (fresh workstation path).
- **YAML-only store.** Commit 4 correctly writes YAML unconditionally from
  `Config::store` and dispatches format only on load, matching design 002 line
  498 and the Python behaviour.
- **Commit 4 size rationale present.** The ~250 LOC rationale paragraph is
  coherent and explains the separation of concerns from Commit 3 (`config IO` is
  a pure-types-plus-fs concern; `secrets` is an async-Vault-resolving concern).
- **Phase 3 §Out of scope on lift-out invariants.** The claim that `utils::s3`
  and `utils::vault` are not lift-out candidates is correct per design 001
  §Lift-out invariants, which names only `utils::subprocess` and `utils::git`.
- **Integration test `#[ignore]` gate.** Gating integration tests on `#[ignore]`
  with documented env vars (`AWS_ENDPOINT_URL`, vault dev server address) is the
  right approach for tests that require external services. The env-var contract
  is explicit, which is what operators and CI maintainers need.

---

## Blockers

None.

---

## Major Concerns

None.

---

## Minor Issues

### P3-M1 (MINOR) — Commit 2 missing commit-size rationale paragraph

**Where:** Phase 3, Commit 2 (`utils::vault` wrapper) — no §Commit-size
rationale paragraph present.

**What the plan says:** Commit 2 is sized at ~300 LOC. This is below the
400-line floor defined in CLAUDE.md §Commit Granularity ("below 200, consider
whether the commit is meaningful alone" — with the implied 400 lower boundary of
the sweet spot). Phase 2 Commits 3 (~150 LOC) and 5 (~230 LOC) both received
rationale paragraphs in response to v4 finding M1. Commit 2's ~300 LOC sits
closer to the floor than either of those, and no rationale is present.

**Why it matters:** The pattern set in Phase 2 — "commits below the sweet spot
carry a rationale paragraph" — exists to prevent reviewers (and future `git log`
readers) from questioning whether the commit boundary is arbitrary. Without the
paragraph, an implementer might decide to bundle Commit 2 with Commit 3 at
implementation time, which would conflate the Vault HTTP wrapper (a
self-contained SDK facade) with the async secrets-resolution logic (which calls
into it). That bundling is the wrong split.

**Resolution:** Add a §Commit-size rationale paragraph to Commit 2 explaining
that ~300 LOC sits below the 400-line sweet spot but that `utils::vault` is a
semantically complete and independently testable SDK facade (KV reads, auth,
token renewal), and that bundling it with Commit 3 (`secrets::mgr`, async Vault
calls, file IO) would tie two separable concerns — the HTTP wrapper and the
secrets-orchestration layer — into a single blast radius. The paragraph can be
modelled on Commit 5's rationale from Phase 2.

---

### P3-M2 (MINOR) — Vault token caching ownership is under-specified

**Where:** Phase 3, Commit 2 §Design constraints (plan lines 145–147).

**What the plan says:** "Token caching with renewal: when the issued token has a
TTL, schedule a renewal at TTL/2 via a background task. Drop the cache when
`VaultError::TokenRenewalFailed` surfaces; force a re-login."

**What the design says:** Design 002 §Vault (lines 632–636) says only: "The Rust
port uses `vaultrs`, which supports KV v1/v2 reads, AppRole login, userpass
login, and token renewal. Authentication order matches the Python." The token
caching detail is not in the design — it is plan-level elaboration that the plan
added beyond the design scope.

**The concern:** The plan says "schedule a renewal at TTL/2 via a background
task" but does not specify who owns the background task. The described module
surface is "free async functions; no struct state" — yet a TTL/2 background
renewal task requires a persistent handle and a cancel mechanism. The two
requirements are in tension:

- If the caching and the background task live in a `OnceCell<VaultClient>` at
  module scope, the background task has no structured lifetime and cannot be
  cleanly shut down. This is an operational concern: a long-lived `cbsbuild`
  process or a `cbsd-worker` that stays connected for hours will leak background
  tasks if the vault client is re-initialized.
- If the caching lives in a struct (`VaultClientHandle` or similar), the
  function signatures change to `kv_read(&self, …)` rather than
  `kv_read(config: &VaultConfig, …)`, and `secrets::mgr::resolve_vault_refs`
  must hold a reference to the struct rather than calling the free function.
  This is compatible with the plan's intent but is not stated.

The Python `utils/vault.py` does not cache tokens between calls — it
authenticates per invocation (or relies on `hvac`'s session-level caching).
Matching Python behaviour (per-call auth) would be simpler and would eliminate
the ownership question at the cost of extra Vault RTTs per secrets-resolution
pass.

**Why it matters:** If the implementer takes the "free functions + `OnceCell`"
path, the background task has no cancellation mechanism, which is an operational
concern for long-lived processes. If the implementer takes the "struct with a
handle" path without the plan specifying it, the function signatures in Commit 3
(`resolve_vault_refs(&mut self, vault: &VaultClient)`) need updating.

**This is minor, not a blocker,** because: the mismatch will surface at
implementation time; matching Python's per-call auth is an acceptable and
simpler fallback; and the impact is confined to `utils::vault` and its one
caller in `secrets::mgr`.

**Resolution:** One of three options:

1. **Simplify to match Python:** Remove the token-caching / background-renewal
   sentence from §Design constraints. Let `vaultrs` handle per-call auth. Note
   that this is a deliberate decision and can be revisited if Vault RTTs become
   observable in secrets-resolution benchmarks.
2. **Specify the struct shape explicitly:** Change the module from "free async
   functions" to a thin `VaultClient` struct (wrapping the `vaultrs` client),
   and update `resolve_vault_refs`'s signature accordingly.
3. **Scope the caching to the call site:** Specify that the issued token is
   cached only for the duration of a single `resolve_vault_refs` call (passed in
   via a `&mut Option<Token>` accumulator or similar), not across calls. This
   avoids the background-task problem while still saving RTTs within a multi-ref
   resolution pass.

Any of these closes the concern. The current text is ambiguous enough to cause a
correctness gap at implementation time.

---

### P3-M3 (MINOR) — Phase 1 §Out of scope does not acknowledge deferred

`S3Error` / `VaultError`

**Where:** Phase 1, §Out of scope section.

**What the plan says:** Phase 1 §Out of scope explicitly flags that
`Config.paths.versions` is deferred to seq-004 and that the six parse-family
functions are deferred to Phase 2. These are design-to-plan drift notes, present
so that implementers reading Phase 1 in isolation understand why those items are
absent.

**What is missing:** Phase 1, Commit 2 §Files (plan line 168–169) carries this
note in the §Design rules block: "Those wraps land in Phase 3 alongside the IO
modules that produce them." This sentence is correct but buried in the
implementation detail of Commit 2. Phase 1 §Out of scope has no corresponding
entry that says "`S3Error` and `VaultError` are intentionally absent from Phase
1 — they are deferred to Phase 3 alongside the IO modules that produce them."

**Why it matters:** Someone reading Phase 1 §Out of scope to understand the full
error taxonomy surface they need to implement will see `GitError` missing from
the Phase 1 Commit 2 error file list (that was caught in prior reviews), and
will now also notice `S3Error` and `VaultError` absent from Commit 2's error
list. Unlike `GitError` — which is placed in `cbscore` for lift-out reasons and
is explicitly mentioned in Phase 2 — `S3Error` and `VaultError` have no
cross-reference from Phase 1 §Out of scope to where they land. The note in
Commit 2 §Design rules is easy to miss when scanning Phase 1.

**Severity context:** This is a documentation consistency issue, not a
correctness gap. The implementer will not miss `S3Error` / `VaultError` because
Phase 3 fully specifies them. The concern is that the §Out of scope section in
Phase 1 sets a precedent for being the complete list of what is deferred, and
two items are deferred without appearing there.

**Resolution:** Add a bullet to Phase 1 §Out of scope, modelled on the
`paths.versions` and parse-family bullets already there:

> `S3Error` (wrapping `aws_sdk_s3` framework errors) and `VaultError` (wrapping
> `vaultrs::error::ClientError`) are intentionally absent from Phase 1's error
> taxonomy. Both are framework-error wrappers per design 002 §Error Taxonomy
> lines 239–240 and land in Phase 3 alongside the IO modules that produce them
> (`utils::s3` and `utils::vault`, respectively).

---

## Suggestions

### P3-S1 — Consider a CI-visible toggle for integration tests

**Where:** Phase 3, Commit 1 §Testable (plan lines 115–119) and Commit 2
§Testable (plan lines 158–163).

Both commits specify `#[ignore]`-able integration tests against local MinIO and
Vault dev. The `#[ignore]` approach is correct. A useful enhancement is to note
that these tests can be un-ignored via the standard Cargo convention
(`cargo test -- --include-ignored` or a feature flag) and that CI can be
configured to run them in a separate job when a MinIO / Vault sidecar is
available. The plan currently leaves this implicit.

This is non-blocking. The `#[ignore]` gate works correctly. The suggestion is to
add a one-line note per integration-test §Testable block naming the Cargo
invocation pattern (`cargo test -- --include-ignored` or
`CBSCORE_INTEGRATION_TESTS=1 cargo test -- --include-ignored`) so that future CI
configurers know where to look.

---

### P3-S2 — `Config::store` should use `tokio::fs` not `std::fs`

**Where:** Phase 3, Commit 4 §Design constraints (plan line 256).

The plan says `Config::store` creates the parent dir via
`tokio::fs::create_dir_all` (correct) and writes to disk, but the store function
itself is defined as `pub fn Config::store(&self, …)` — a sync `fn`, not an
`async fn`. If the implementation follows the sync-fn signature, it must use
`std::fs` (blocking), which is wrong in an async context. If it follows
`tokio::fs`, the signature must be `async fn store`.

Design 002's sketch (lines 506–507) also uses `pub fn store(…)` without `async`,
which is the Python-parity signature for the non-async Python method. In a
fully-async `cbscore`, config IO should be async — especially `store`, which
does at least two syscalls (`create_dir_all` + `write`).

**Resolution:** The plan should specify `async fn` for `Config::store` (and
`Config::load`) explicitly. The current plan text uses `pub fn` for both. Given
that Phase 1's description already placed `Config::load` / `Config::store` as
async-context IO, an explicit `async` qualifier in the signature would prevent
an implementer from writing a blocking sync implementation. This is non-blocking
since the design 002 sketch uses the same ambiguous `pub fn` form, but aligning
the plan to the tokio async model is the right long-term call.

---

## Open Questions

### OQ1 — `Secrets::load` not listed as a function in any commit's file spec

**Where:** Phase 3, Commit 3 §Design constraints (plan lines 221–222).

The §Design constraints block states: "`Secrets::load` itself is YAML parsing
through `serde_saphyr` + `VersionedSecrets::into_latest()` (Phase 1 Commit 5).
Phase 3 wires the file IO; Phase 1 owns the wire-format dispatch."

This is clear on the implementation path, but `Secrets::load` does not appear in
Commit 3's §Files list nor in Commit 4's §Files list. Given that `Config::load`
is an explicit function in Commit 4's §Files spec, the omission of
`Secrets::load` from any §Files entry is at least a visual asymmetry. Two
interpretations:

1. `Secrets::load` is intended to live in the same `config.rs` file as
   `Config::load` / `Config::store` (Commit 4), making it part of Commit 4's
   surface. But the §Files spec for Commit 4 only mentions
   `cbscore/src/config.rs` and focuses entirely on `Config`.
2. `Secrets::load` is implied by Commit 3's `SecretsMgr::load_files` — i.e., the
   YAML file reading for a `Secrets` value is done inline within
   `load_files(paths)` rather than as a named method on the `Secrets` type.

Both interpretations are workable. **The question is:** should `Secrets::load`
appear as an explicit function in a §Files entry, and if so, which commit owns
it? Clarifying this before implementation avoids a situation where the
implementer puts it in `secrets/mod.rs` while the reviewer expected it in
`config.rs`, or vice versa.

**Suggested resolution:** Add a one-liner to Commit 3's §Files under
`secrets/mgr.rs` or `secrets/mod.rs` noting that `Secrets::load(path)` is a
private helper that reads a single file via `serde_saphyr` +
`VersionedSecrets::into_latest()`, used by `load_files`. Or add it explicitly to
Commit 4's `config.rs` surface as a public parallel to `Config::load`.

---

### OQ2 — Phase 4 mount contract at the Phase 3 boundary

**Where:** Phase 3, Commit 3 §Design constraints (plan lines 213–216).

The plan correctly states that `dump_to_runner` writes to a path the runner
mounts at `/runner/cbs-build.secrets.yaml`. This is the host-side half of a
two-part contract. The runner-side half — mounting the tempfile at the
container-side path — is Phase 4's responsibility. The plan acknowledges this
("the runner reads its own copy at `/runner/cbs-build.secrets.yaml`") but does
not specify the contract that Phase 4 must honour.

**The question is:** Is the tempfile path returned from `dump_to_runner` as a
return value (so that the Phase 4 runner can pass it to
`podman run --volume <path>:/runner/cbs-build.secrets.yaml`), or is the path
passed into `dump_to_runner` by the runner? The current signature is
`pub async fn dump_to_runner(&self, path: &Utf8Path) -> Result<(), SecretsError>`,
which means the runner supplies the path. This is the right design — the runner
owns the tempfile creation and passes the path in. But it means the runner
(Phase 4) must know to create a `camino-tempfile`, call `dump_to_runner`, and
then mount the result. The plan does not state this contract explicitly.

This is not a gap in Phase 3 itself — `dump_to_runner`'s signature is clear. It
is a flag for the Phase 4 author to note: the mount contract is a caller
responsibility, not something `dump_to_runner` enforces. If the Phase 4 plan
omits the tempfile-creation step, the container will start without the secrets
file. Consider noting this dependency boundary explicitly in the Phase 3 §Out of
scope or in a cross-reference: "Phase 4 is responsible for creating the tempfile
at the host path, passing it to `dump_to_runner`, and mounting the result into
the container."

---

## Verdict

**Phase 3 has no blockers and no major concerns. Three minor findings (P3-M1,
P3-M2, P3-M3) should be resolved in the plan text before the first commit
lands.** P3-M2 is the most consequential: the token-caching / background-renewal
sentence is ambiguous enough to produce an incorrect implementation (background
task with no cancellation path) if taken at face value. P3-M1 and P3-M3 are
housekeeping.

**Phase 3 meets the bar for implementation start** once the three minor findings
are addressed. None of the findings require rethinking the commit structure or
the module layout.

**New findings by severity:** 0 blockers, 0 major concerns, 3 minor (P3-M1,
P3-M2, P3-M3), 2 suggestions (P3-S1, P3-S2), 2 open questions (OQ1, OQ2).

**Phases 1 + 2 are free of regression** from Phase 3's addition. All prior
closed findings remain closed.
