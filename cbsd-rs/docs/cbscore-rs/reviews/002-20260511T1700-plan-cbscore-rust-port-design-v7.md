# Plan Review — cbscore Rust Port: Phase 3 Confirmation (v6 Closures) — v7

**Plans reviewed:**

- [`002-20260508T1558-03-storage-and-secrets.md`](../plans/002-20260508T1558-03-storage-and-secrets.md)
  — Phase 3, v6 finding confirmation pass
- [`002-20260508T1558-01-types.md`](../plans/002-20260508T1558-01-types.md) —
  Phase 1, fresh-eyes sweep for v6 ripple effects

**Prior reviews:**

- v1:
  [`002-20260511T1002-plan-cbscore-rust-port-design-v1.md`](./002-20260511T1002-plan-cbscore-rust-port-design-v1.md)
  — 9 findings; all closed
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
  — N-M1 (`nix` undeclared) + N-Nit1 (dup bullet); both noted as non-blocking;
  `nix` dep added by `83b7a7d`; dup bullet remains cosmetic
- v6:
  [`002-20260511T1600-plan-cbscore-rust-port-design-v6.md`](./002-20260511T1600-plan-cbscore-rust-port-design-v6.md)
  — Phase 3 first review; 7 findings (0 blockers, 3 MINOR, 2 SUGGESTIONS, 2 OPEN
  QUESTIONS)

**Closing commits reviewed:**

| Commit    | Finding(s) closed                    |
| --------- | ------------------------------------ |
| `3412504` | P3-M3                                |
| `46c1c63` | P3-M1, P3-M2, P3-S1, P3-S2, OQ1, OQ2 |

**Reviewer:** Staff review, 2026-05-11.

---

## Summary Assessment

All seven v6 findings are closed cleanly. The two commits are precisely scoped,
the added text is internally consistent, and the P3-M2 ripple into Commit 3's
`resolve_vault_refs` signature was applied correctly and completely — no
dangling `VaultClient` references survive anywhere in Phase 3. The fresh-eyes
sweep found one new minor issue: the OQ2 §Out of scope bullet names
`camino-tempfile` as the Phase 4 tempfile crate but the Phase 1 Commit 1
`cbscore/Cargo.toml` spec does not enumerate it explicitly (it defers to design
001 §Cargo Sketch, which does list it at line 402). This is the same pattern as
the v5 N-M1 `nix` finding — a dep named in a plan section but absent from the
plan-level dep sketch. Severity is identical: minor and non-blocking, because
the dep exists in design 001 and the gap is caught at compile time. No other new
issues were found.

---

## v6 Findings: Closure Confirmation

### P3-M1 — Commit 2 missing commit-size rationale — CLOSED

**Closing commit:** `46c1c63`

**Confirmed:**

- A `**Commit-size rationale:**` paragraph is present in Phase 3 Commit 2
  immediately before the `**Testable:**` block (plan lines 171–178).
- The paragraph states that ~300 LOC sits below the 400-line sweet spot, then
  explains why `utils::vault` is nonetheless a standalone commit: the SDK facade
  (KV reads, auth-method selection, per-call auth contract) is independently
  testable against a local `vault server -dev` instance, and bundling with
  Commit 3 (`secrets::mgr`, async Vault calls, file IO) would tie two separable
  concerns — the HTTP wrapper and the secrets-orchestration layer — into a
  single blast radius.
- The rationale is coherent and matches the pattern set by Phase 2 Commits 3
  and 5.

---

### P3-M2 — Vault token caching under-specified — CLOSED

**Closing commit:** `46c1c63`

**Confirmed (§Design constraints, Commit 2):**

- The "Token caching with renewal: when the issued token has a TTL, schedule a
  renewal at TTL/2 via a background task" sentence is gone.
- A replacement bullet reads: "**No token caching across calls.** The wrapper
  re-authenticates per Vault call, matching the Python `utils/vault.py`
  behaviour." The bullet then cites: minimal token-in-memory window (one call
  duration), full Vault audit signal (every operation logged), and zero blast
  radius from a stolen-token attack on a long-lived `cbsd-worker` (Phase 7
  context). A forward note states that caching can be revisited as a separate
  design if RTTs become observable, and that introducing it would require a
  struct shape (`VaultClient { … }`) and a cancellation/ownership story — both
  out of scope here.
- The three-bullet security rationale (Python parity, audit signal,
  blast-radius) exactly matches the v6 resolution direction.

**Confirmed (ripple into Commit 3):**

- `resolve_vault_refs` signature (plan lines 219–224) now reads
  `pub async fn resolve_vault_refs(&mut self, config: &VaultConfig) -> Result<(), SecretsError>`.
  The `&VaultClient` argument is gone; `&VaultConfig` is correct.
- An explanatory parenthetical states: "Takes `&VaultConfig` (not a
  `&VaultClient` struct) because the Vault wrapper is free async functions per
  Commit 2."
- The §Testable bullet for `resolve_vault_refs` (plan lines 267–271) now reads
  "stub `kv_read` (substituting the `utils::vault::kv_read` call via dependency
  injection or a feature-gated test double)" — all `VaultClient` language is
  gone, and the dependency-injection framing is correct for a free-function
  wrapper.
- No remaining `VaultClient` reference exists in Phase 3 outside of one
  intentional forward mention in the §Design constraints no-caching paragraph:
  "`VaultClient { … }`" as a hypothetical struct shape to illustrate what
  caching would require. This is clearly a hypothetical and not a signature
  reference; it is not a dangling reference.

---

### P3-M3 — Phase 1 §Out of scope missing S3Error / VaultError — CLOSED

**Closing commit:** `3412504`

**Confirmed:**

- A new bullet in Phase 1 §Out of scope (plan lines 52–57) reads: "`S3Error`
  (wrapping `aws_sdk_s3` framework errors) and `VaultError` (wrapping
  `vaultrs::error::ClientError`) are intentionally absent from Phase 1's error
  taxonomy. Both are framework-error wrappers per design 002 §Error Taxonomy
  lines 239–240 and land in Phase 3 alongside the IO modules that produce them
  (`utils::s3` and `utils::vault`, respectively). They live in `cbscore`, not
  `cbscore-types`, matching the `GitError` placement pattern from Phase 2."
- All three required elements are present: citation of design 002 §Error
  Taxonomy lines 239–240, crate placement (`cbscore`, not `cbscore-types`), and
  the `GitError` analogy from Phase 2.
- The bullet follows the `paths.versions` and parse-family bullets immediately,
  so §Out of scope is now a complete list of all deferred surfaces.

---

### P3-S1 — Integration test CI toggle note — CLOSED

**Closing commit:** `46c1c63`

**Confirmed:**

- Commit 1 §Testable (plan lines 125–129): the integration-test bullet now ends
  with "Un-ignore in CI via `cargo test -- --include-ignored` once the MinIO /
  LocalStack sidecar is available."
- Commit 2 §Testable (plan lines 184–189): the integration-test bullet now ends
  with "Un-ignore in CI via `cargo test -- --include-ignored` once the dev-Vault
  sidecar is configured."
- Both add the exact Cargo invocation pattern asked for in the suggestion.

---

### P3-S2 — `Config::load` / `Config::store` should be `async fn` — CLOSED

**Closing commit:** `46c1c63`

**Confirmed:**

- Commit 4 §Files (plan lines 285–295) now declares both functions as
  `pub async fn`:
  - `pub async fn Config::load(path: &Utf8Path) -> Result<Config, ConfigError>`
    — reads via `tokio::fs::read_to_string`.
  - `pub async fn Config::store(&self, path: &Utf8Path) -> Result<(), ConfigError>`
    — writes via `tokio::fs::write`, creates parent dir via
    `tokio::fs::create_dir_all`.
- A parenthetical note states: "**Both functions are `async fn`** because they
  do filesystem IO via `tokio::fs`. (Design 002's sketch lines 506–507 uses
  `pub fn` matching the Python signature; the Rust port is fully async, so the
  IO operations become `async fn` to avoid blocking the tokio runtime.)"
- The deliberate divergence from the design 002 sketch is explicitly called out.

---

### OQ1 — `Secrets::load` not listed in any §Files entry — CLOSED

**Closing commit:** `46c1c63`

**Confirmed:**

- Commit 3 §Files, `secrets/models.rs` entry (plan lines 207–212) now reads:
  "Also hosts a private helper
  `fn Secrets::load(path: &Utf8Path) -> Result<Secrets, SecretsError>` that
  performs the single-file YAML parse via `serde_saphyr` +
  `VersionedSecrets::into_latest()` (Phase 1 Commit 5). Not a public parallel to
  `Config::load` — it's called only by `SecretsMgr::load_files` (below) and is
  scoped accordingly."
- The helper is definitively placed in `secrets/models.rs`, not in `config.rs`.
  The "private" qualifier is explicit. The caller is named.
- §Design constraints (plan lines 257–260) is updated to match: "
  `Secrets::load` (the private helper in `models.rs`) is YAML parsing through
  `serde_saphyr` + `VersionedSecrets::into_latest()` (Phase 1 Commit 5)." The
  wording is consistent with the §Files entry.
- No remaining asymmetry with `Config::load` — the distinction between a public
  IO function (`Config::load` in `config.rs`) and a private parsing helper
  (`Secrets::load` in `secrets/models.rs`) is now clearly stated.

---

### OQ2 — Phase 4 mount contract not stated explicitly — CLOSED

**Closing commit:** `46c1c63`

**Confirmed:**

- A new bullet in Phase 3 §Out of scope (plan lines 61–69) reads: "**Runner-side
  mount of the dumped secrets file** is a Phase 4 responsibility. Phase 3's
  `dump_to_runner(path: &Utf8Path)` takes the host-side tempfile path as an
  argument and writes the merged-and-resolved Secrets YAML to it. The Phase 4
  runner is responsible for (a) creating the host tempfile via `camino-tempfile`
  with mode 0600, (b) calling `SecretsMgr::dump_to_runner` with the resulting
  path, and (c) passing the path to
  `podman run --volume <path>:/runner/cbs-build.secrets.yaml`. Phase 3 does not
  enforce this contract — flagging it here so the Phase 4 plan author wires the
  steps together explicitly."
- The three-step contract (create tempfile, call `dump_to_runner`, mount) is
  present. Phase 3's non-enforcement role is explicit.
- The `podman run --volume` mounting syntax matches the runner-subsystem mount
  table in design 002 §Runner Subsystem (line 784).

---

## Fresh-Eyes Sweep

### Stray `VaultClient` references in Phase 3

Searched the full Phase 3 plan for all `VaultClient` occurrences. Three hits:

1. Line 163: "struct shape (`VaultClient { … }`)" — in the no-caching rationale
   paragraph, as a hypothetical struct shape illustrating what caching would
   require. Clearly labelled as a hypothetical. Not a signature reference.
2. Line 223: "Takes `&VaultConfig` (not a `&VaultClient` struct)" — the
   explanatory parenthetical confirming the old type is gone. Not a live
   reference.
3. No hit in Commit 3 §Testable, §Design constraints, or any other commit
   cross-reference.

No dangling `VaultClient` references remain. Clean.

### OQ2 §Out of scope bullet: `camino-tempfile` dep declaration

**What the new bullet says:** Phase 4 creates the tempfile via `camino-tempfile`
(plan line 65).

**Where `camino-tempfile` is declared:**

- Design 001 §Cargo Sketch (line 402): `camino-tempfile = "1"` — present and
  pinned.
- Design 002 §Capability Mapping (line 203): `camino-tempfile` named as a
  tempfile dep alongside `tempfile`.
- Phase 1 Commit 1, `cbscore/Cargo.toml` spec (plan lines 102–109): "depends on
  `cbscore-types`, `tokio` full features, `tracing`, `regex`, `which`, plus the
  IO-side crates that fill in over Phases 2–5 (pin all per design 001)" — this
  is a forward reference to design 001, not an explicit enumeration.

**The gap:** `camino-tempfile` is named in a Phase 3 §Out of scope bullet (as a
Phase 4 responsibility) but is not listed by name in the Phase 1 Commit 1
Cargo.toml spec or in any Phase 2 or Phase 3 Cargo.toml spec. The only
plan-corpus entry that specifies `cbscore`'s dep list is Phase 1 Commit 1
§Files, which defers the IO-side enumeration to design 001. Design 001 does list
`camino-tempfile`, so the dep is not "missing" from the corpus — but it is not
surfaced at plan level. This is the same pattern as the v5 N-M1 `nix` finding: a
crate name appears in plan prose but the plan-level dep sketch does not
enumerate it explicitly.

**Severity:** Minor and non-blocking. Design 001 is the authoritative dep
source; Phase 1 explicitly defers to it. The implementer will not miss
`camino-tempfile` because the dep exists in design 001 and the gap surfaces as a
compile error (not a silent regression). However, the `camino-tempfile`
reference appears in a Phase 3 §Out of scope bullet that will be read by the
Phase 4 plan author — and that author will need to add `camino-tempfile` to the
`cbscore` Cargo.toml spec in Phase 4 Commit 1 or wherever the runner module is
added. The Phase 3 bullet does not flag this explicitly.

**Resolution direction:** Add a parenthetical to the OQ2 §Out of scope bullet,
e.g., "(add `camino-tempfile = "1"` to `cbscore/Cargo.toml` in Phase 4 Commit 1;
it is listed in design 001 §Cargo Sketch line 402 but absent from the plan-level
dep spec)." Alternatively, annotate Phase 1 Commit 1's Cargo.toml spec with
`camino-tempfile` by name, as was done for `nix` following v5 N-M1. Either
approach closes the gap.

### P3-M3 Phase 1 §Out of scope: Commit 2 §Design rules back-reference

The v6 finding asked whether Phase 1 Commit 2's §Design rules paragraph would
need a back-reference to the §Out of scope bullet now that the bullet exists.
Current state: Commit 2 §Design rules (plan line 174–175) reads "Those wraps
land in Phase 3 alongside the IO modules that produce them." The §Out of scope
bullet (plan line 52–57) now carries the full deferred-error statement. The two
texts are consistent — the §Design rules sentence is accurate and the §Out of
scope bullet is the authoritative cross-reference. No back-reference is
required; the asymmetry between detailed §Out of scope and brief §Design rules
inline is acceptable and mirrors the existing style for the parse-family
deferral (§Out of scope full text, Commit 4 §Design constraints one-liner).

### LOC arithmetic check

500 + 300 + 500 + 250 = 1550. The progress table header reads "~1550 LOC, 4
commits." Consistent.

### `Config::load` / `Config::store` async consistency across Phase 1 and Phase 2

Phase 1 §Out of scope (plan line 43) reads: "Any IO.
`cbscore::config::Config::load`, secrets-manager IO, descriptor-store walks —
all land in Phase 3." The statement defers `Config::load` to Phase 3 without
specifying `async` — which is fine, because that section only says where the
function lands, not its signature. The now-async signature is specified in Phase
3 Commit 4. No contradiction.

Phase 2 has no reference to `Config::load` or `Config::store`. No regression.

### Phase 3 Commit 2 §Testable token-renewal test case cleanup

The pre-v6 §Testable for Commit 2 included "verify token renewal extends the
lease" as an integration-test assertion. The v6 fix removed that assertion along
with the background-renewal design constraint (`46c1c63`). The replacement
integration-test assertion is "verify per-call auth produces a fresh token on
each `kv_read`." This is consistent with the no-caching design constraint. No
orphan test language remains.

---

## New Findings

### N-M1 (MINOR) — `camino-tempfile` unnamed in plan-level dep sketch

**Where:** Phase 3 §Out of scope, OQ2 bullet (plan line 65); Phase 1 Commit 1
§Files, `cbscore/Cargo.toml` spec (plan lines 102–109).

**What the plan says:** The new OQ2 §Out of scope bullet tells the Phase 4 plan
author that the runner is responsible for creating the host tempfile via
`camino-tempfile`. No plan-level Cargo.toml spec names `camino-tempfile`
explicitly; Phase 1 Commit 1 defers the IO-side dep enumeration to design 001.

**Why it matters:** Design 001 §Cargo Sketch (line 402) does list
`camino-tempfile = "1"`, so the dep is not absent from the corpus. The risk is
that the Phase 4 plan author, reading Phase 3's OQ2 bullet as instructions to
"use `camino-tempfile`," may not trace back to design 001 and may add the dep
without a pin or at the wrong crate. The gap is minor: it surfaces as a compile
error at implementation time (not a silent regression), and the authoritative
dep is in design 001. The parallel to v5 N-M1 (`nix` crate) is exact.

**Resolution:** In Phase 3 §Out of scope, append a parenthetical to the
`camino-tempfile` reference: "(listed in design 001 §Cargo Sketch line 402; add
to `cbscore/Cargo.toml` in Phase 4 Commit 1)." Alternatively, add
`camino-tempfile = "1"` by name to Phase 1 Commit 1's `cbscore` Cargo.toml spec
with a note "(Phase 4 runner)". Either form closes the gap before the Phase 4
plan is written.

---

## Verdict

**All seven v6 findings are confirmed closed. One new minor finding (N-M1:
`camino-tempfile` unnamed in the plan-level dep sketch) was found during the
fresh-eyes sweep.**

**New findings by severity:** 0 blockers (CRITICAL), 0 IMPORTANT, 1 MINOR
(N-M1), 0 suggestions, 0 open questions.

**Phase 3 (and the corpus as a whole) meets the bar for implementation start.**
N-M1 is non-blocking: it affects a Phase 4 dep reference, not Phase 3's own
implementation, and will surface as a compile error rather than a silent
regression. The Phase 3 implementer may proceed. The Phase 4 plan author should
add `camino-tempfile` to the plan-level dep spec when writing Phase 4 Commit 1.
