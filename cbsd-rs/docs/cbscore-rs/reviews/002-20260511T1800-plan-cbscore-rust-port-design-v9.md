# Plan Review — cbscore Rust Port (v9)

**Scope:** Confirmation pass — verify the five v8 findings (P4-B1, P4-M1, P4-N1,
P4-N2, P4-N3) are cleanly closed by commits `068898f` and `2095494`, then
fresh-eyes sweep of the entire four-phase corpus plus the README for new issues
introduced by the fixes.

**Artefacts reviewed:**

- `002-20260508T1558-04-runner.md` — Phase 4 (M1.3, modified by both commits)
- `002-20260508T1558-01-types.md` — Phase 1 (M0, modified by `2095494`)
- `002-20260508T1558-02-subprocess-and-shell-tools.md` — Phase 2 (M1.1,
  unchanged since v8)
- `002-20260508T1558-03-storage-and-secrets.md` — Phase 3 (M1.2, unchanged since
  v8)
- `plans/README.md` — index (unchanged since v8)

---

## 1. Summary Assessment

All five v8 findings are closed by the two commits. The architectural rewrite
for two-tier cleanup (P4-B1) is coherent and implementable. The vault-mount fix
(P4-M1) correctly reflects Python semantics. The error-variant additions (P4-N1,
P4-N2) are present in Phase 1 C2's spec, and the Phase 1 §Out of scope wording
(P4-N3) now correctly distributes IO across phases. One minor issue was
introduced: the Phase 4 C1 §Testable negative test still uses a hedge
(`Err(VersionError::…)`) rather than the now-named concrete variant, in contrast
with Phase 4 §Design constraints which was updated correctly. This is a small
consistency gap, not a blocker. **The corpus is ready for implementation start
subject to closing that one minor.**

---

## 2. v8 Finding Closures

### P4-B1 — CLOSED

Commit `068898f` replaces the single-RAII-guard description with an explicit
two-tier strategy. Phase 4 C3 §State machine §4 Cleanup now describes:

- **Normal exit path** — an `async fn cleanup(...)` called on every return path
  before the function returns; awaits `tokio::fs::remove_*` and `podman_stop`
  correctly.
- **Panic / future-drop fallback** — a sync RAII guard whose `Drop` impl uses
  `std::fs::remove_file` / `std::fs::remove_dir_all` (not `tokio::fs::*`) and
  spawns a detached `std::process::Command` for `podman stop --time 1` (not the
  async `podman_stop`). All errors swallowed.
- **Guard consumption** — the normal-exit async path consumes the guard
  (`std::mem::forget`-style) so cleanup runs exactly once.

The §Design constraints "Cleanup always runs" bullet matches. The §Testable
section gained both a "Cleanup-on-normal-exit" test (async path) and a revised
"Cleanup-on-panic" test (sync Drop fallback, with `#[ignore]` note). The
two-tier story is internally coherent and implementable.

One subtle observation (not a blocker — documented in §5 Minor Issues as V9-N1):
the plan says the guard is "consumed (`std::mem::forget`-style) by the
normal-exit cleanup" but does not specify whether `cleanup()` owns the guard
internally or whether the caller forgets the guard after `cleanup()` returns. If
`cleanup()` takes ownership and does `mem::forget` at the end, a `?`-return
inside `cleanup()` before the `forget` would allow `Drop` to run after partial
async cleanup, risking double-stop. This is a low-risk implementation-time
decision, but the plan leaves it underspecified.

### P4-M1 — CLOSED

Commit `068898f` fixes both the §State machine §1 Preparing bullet and the
mount-layout table row:

- §1 Preparing: "cbs-build.vault.yaml is NOT written to a tempfile.
  `config.vault: Option<Utf8PathBuf>` (per design 002 line 449) is a path to an
  existing on-disk file. The runner mounts that path directly read-only."
- Mount table: host-side column reads "config.vault path (existing, if set)";
  mount-point column reads "/runner/cbs-build.vault.yaml (read-only)".

The read-only flag is conveyed only for the vault row, which is correct — the
other mounts do not carry explicit flags in the table, matching the pre-fix
style. No ambiguity that other mounts are assumed read-only; `:Z` on
`/var/lib/containers` is the only other mount option present and it is an
SELinux relabel, not a read/write flag. Row count remains 9 (8 unconditional + 1
conditional vault). Closes the finding cleanly.

### P4-N1 — CLOSED

Commit `2095494` updates Phase 1 C2 `runner/errors.rs` spec to enumerate:
`Cancelled` (SIGTERM / outer cancellation), `Timeout` (internal
`tokio::time::timeout` elapsed), `Command(CommandError)` and
`Podman(PodmanError)` via `#[from]`. Phase 4 Commit 3 cross-reference is
present. The variant names match those used in Phase 4 C3 §Testable.

### P4-N2 — CLOSED

Commit `2095494` adds `MissingSchemaVersion` and
`UnknownSchemaVersion { found, max_supported }` to Phase 1 C2
`versions/errors.rs` spec, parallel to `ConfigError`. Phase 4 C1 §Design
constraints hedge text ("or the equivalent variant from Phase 1's VersionError")
is replaced with the concrete names `VersionError::MissingSchemaVersion` and
`VersionError::UnknownSchemaVersion { found, max_supported }`. The
cross-reference to `ConfigError` versions is explicit.

Note: the §Testable negative test in Phase 4 C1 (line 110–111) still uses
`Err(VersionError::…)` — the design constraints bullet was updated but the test
description was not. Reported as V9-N1 below.

### P4-N3 — CLOSED

Commit `2095494` updates Phase 1 §Out of scope from:

> Any IO. `cbscore::config::Config::load`, secrets-manager IO, descriptor-store
> walks — all land in Phase 3.

to:

> Any IO. `cbscore::config::Config::load` and secrets-manager IO land in Phase
> 3; `cbscore::versions::desc::{read,write}_descriptor` lands in Phase 4;
> descriptor- store walks land in seq-004 (Phase 6).

The breakdown correctly matches the actual phase assignments. No contradiction
with Phase 3 §Goal or §Out of scope.

---

## 3. Blockers

None. All v8 blockers are resolved.

---

## 4. Major Concerns

None. All v8 major concerns are resolved; the fresh-eyes sweep found no new
major issues.

---

## 5. Minor Issues

### V9-N1 — Phase 4 C1 §Testable negative test still hedges on VersionError variant

**What.** Phase 4 C1 §Design constraints was correctly updated (by `2095494`) to
name `VersionError::MissingSchemaVersion` explicitly. But the §Testable negative
test immediately below it still reads:

> Negative test: read a JSON file missing `schema_version` →
> `Err(VersionError::…)` per Phase 1 Commit 5's error variants.

The hedge `Err(VersionError::…)` is the same pattern that P4-N2 fixed in the
design constraints bullet; the testable description was not updated in the same
pass.

**Why it matters.** Phase 1 Commit 5 declares
`VersionError::MissingSchemaVersion` (added by `2095494`). The test description
should assert that specific variant so the implementer writes
`assert_matches!(result, Err(VersionError::MissingSchemaVersion))`, not just
`assert!(result.is_err())`. The §Design constraints bullet and the corresponding
Commit 5 negative tests (which do name the concrete variants) set a precedent;
the §Testable line here should match.

**Fix.** Replace the last bullet in Phase 4 C1 §Testable with:

> Negative test: read a JSON file missing `schema_version` →
> `Err(VersionError::MissingSchemaVersion)`.

---

## 6. Suggestions

### V9-S1 — Clarify guard-consumption ordering within cleanup to foreclose double-cleanup risk

The plan says the guard is "consumed (`std::mem::forget`-style) by the
normal-exit cleanup" but leaves the mechanism unspecified. Two implementations
are possible:

1. `cleanup(guard)` takes ownership, does the async work, then calls
   `std::mem::forget(guard)` at the end.
2. `cleanup(...)` borrows the guard's fields, returns, then the caller calls
   `std::mem::forget(guard)`.

Option 1 is simpler but has an edge: if `cleanup()` uses `?` to propagate an
error before the `forget`, `guard.drop()` runs and the sync fallback fires after
partial async cleanup (double-stop). Option 2 avoids the problem because the
guard is only forgotten after `cleanup()` returns. Add one sentence to the
§State machine §4 Cleanup bullet recommending Option 2 (caller forgets after
cleanup returns) or stating explicitly that `cleanup()` must not `?`-return
before consuming the guard.

### V9-S2 — Document the two-layer timeout wrapping rule explicitly

The plan has two `Timeout`-family types at different layers:

- `CommandError::Timeout { after }` — emitted by `async_run_cmd` when its
  internal timeout fires.
- `RunnerError::Timeout` — emitted by the runner when `tokio::time::timeout`
  wraps the spawn+wait step.

These are distinct. When the runner's 4-hour `tokio::time::timeout` fires, it
cancels the `async_run_cmd` future via future-drop (not via `async_run_cmd`'s
own internal timeout), so the runner sees `tokio::time::error::Elapsed` and maps
it to `RunnerError::Timeout`. `async_run_cmd`'s RAII guard kills the child
process. This path is not currently described in the §Design constraints text,
which only says "on elapsed, read the cidfile and call `podman_stop`". A
sentence clarifying that outer cancellation of `async_run_cmd` is handled by its
own RAII drop guard (Phase 2 C1 contract) and does NOT produce
`CommandError::Timeout` would prevent an implementer from confusingly wrapping
the wrong error. This is a documentation-only gap; the design is architecturally
sound.

---

## 7. Open Questions

None.

---

## 8. Fresh-Eyes Sweep

### Phase 1 §Out of scope — no new contradiction

The updated wording correctly anchors Config + secrets IO to Phase 3, descriptor
IO to Phase 4, and descriptor-store walks to seq-004 / Phase 6. It does not
contradict Phase 3 §Goal (which scopes to config/secrets IO only) or Phase 4
§Depends on. The `cbscore-types` zero-IO claim in Phase 1 §Goal is unaffected —
`versions::desc` IO lives in `cbscore/src/versions/desc.rs`, not in
`cbscore-types/src/versions/desc.rs` (distinct crates).

### RunnerError::Timeout vs CommandError::Timeout — not a conflict

Both variants exist but operate at different layers. `CommandError::Timeout` is
internal to `async_run_cmd` (fired by its own timeout, owned by Phase 2). The
runner's `tokio::time::timeout` cancels `async_run_cmd` via future-drop when the
4-hour build limit elapses; that cancellation does NOT produce
`CommandError::Timeout` (Phase 2 C1 internal-timeout-only contract). The runner
maps the elapsed `tokio::time::error::Elapsed` to `RunnerError::Timeout`.
`RunnerError::Command(CommandError)` wraps `CommandError` only when
`async_run_cmd` itself times out internally (a child process sub-step timing
out). The two types do not collide. The wrapping rule is traceable from the
Phase 2 C1 constraints but is not spelled out in Phase 4 — see V9-S2 above.

### Detached `std::process::Command` for Drop-path podman stop — acceptable

The plan says the sync fallback "spawns a detached `std::process::Command` for
`podman stop --time 1 <cid>` (fire-and-forget)". A detached process means the
runner does not wait for podman stop to complete before returning from `Drop`.
For a panic / future-drop path this is acceptable: `Drop` has no mechanism to
block the panic unwind indefinitely, and a 1-second bounded podman stop is not
worth holding the process hostage. The consequence — the runner exits before the
container is guaranteed stopped — is the stated best-effort contract. This is
consistent with the plan's "all errors swallowed" language. No issue.

### Vault mount `:Z` / `:ro` — consistent

The mount table carries `:Z` only on `/var/lib/containers`, matching Python
`runner.py` line 264. The vault row carries `(read-only)` in the mount-point
column, which is correct and specific to that one conditional mount. Other
mounts do not carry explicit flags in the table; this is a documentation-style
choice (not a technical gap) that v8 suggestion P4-S2 already noted was optional
hardening. No new inconsistency was introduced.

### Phase 4 C1 §Testable negative test — reported as V9-N1 above.

### LOC arithmetic — estimate unchanged, still sound

The architectural changes in `068898f` added approximately 30 lines to C3 (state
machine §4, design constraints "Cleanup" bullet, two testable bullets). The
progress table still shows ~700 for C3 and total ~1050. Given the estimate is
stated as approximate (~), the unchanged number is reasonable. No update needed.

### Phase 1 zero-IO guarantee — no contradiction

Phase 1 §Goal says `cbscore-types` is zero-IO. Phase 1 §Out of scope now says
`cbscore::versions::desc::{read,write}_descriptor` lands in Phase 4. These are
`cbscore` functions (IO-bearing), not `cbscore-types` functions. Phase 1 C4's
`cbscore-types/src/versions/desc.rs` contains only type definitions. The zero-IO
invariant on `cbscore-types` is intact.

---

## Verdict

**All five v8 findings are closed. The corpus is ready for implementation
start.** One new minor (V9-N1): the Phase 4 C1 §Testable negative-test
description still hedges with `Err(VersionError::…)` instead of the now-named
`Err(VersionError::MissingSchemaVersion)`. Fix is a one-line edit before the
first implementation commit. No new blockers; no new major concerns.

- **New findings by severity:** 0 blockers, 0 major, 1 minor, 2 suggestions.
- **Phase 4 (and the corpus) meets the bar for implementation start** pending
  the one-line V9-N1 fix.
