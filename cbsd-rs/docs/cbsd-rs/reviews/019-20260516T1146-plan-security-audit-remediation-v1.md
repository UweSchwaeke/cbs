# Review — Plan: Security Audit Remediation (Unified), v1

| Field         | Value                                                                                           |
| ------------- | ----------------------------------------------------------------------------------------------- |
| Review        | 019 — plan-security-audit-remediation, v1                                                       |
| Reviewing     | `cbsd-rs/docs/cbsd-rs/plans/019-20260516T1033-security-audit-remediation.md` (Plan v2, unified) |
| Designs       | WCP v11 (`019-20260426T1154`) + audit-rem v8 (`019-20260514T1040`)                              |
| Soundness ref | `019-20260516T0952-design-wcp-soundness-v1.md`                                                  |
| Date          | 2026-05-16                                                                                      |
| Reviewer      | Staff Engineer — independent; no trust in implementer claims                                    |

---

## Methodology

Every claim in the plan that references a source file, line number, function
signature, or existing code was verified by reading the actual file at the
stated location. No assumption was made about the current state of the codebase.
Where a plan claim could not be verified or was found inaccurate, the finding is
recorded with a precise source citation.

The review evaluates: (1) coverage of WCP D1–D7, audit-rem D1–D13, and
soundness-review gaps G1–G10; (2) source-claim accuracy; (3) per-commit
smell-test compliance per the `git-commits` skill; (4) new findings not present
in prior reviews.

---

## Summary (top findings by severity)

**Finding 1 — Critical (D12, -20 pts):** Commit 2 will not compile as specified.
The plan's package list for commit 2 is
`cbsd-proto + cbsd-server + cbsd-worker (indirectly)` — but `cbsd-worker` is not
listed in the packages table entry, only in the design rationale prose. Adding
`ServerMessage::UnauthorizedBuildAction` to `cbsd-proto` triggers a compile
failure in `cbsd-worker` because `cbsd-worker/src/ws/handler.rs` contains an
exhaustive `match server_msg { ... }` at line 186 with no catch-all. Until a
match arm for the new variant is present in `cbsd-worker`, the workspace does
not compile. Per the `git-commits` smell test #2 (previous commit compiles),
commit 2 as written produces a broken HEAD.

**Finding 2 — Minor (D9, -5 pts):** Commit 1's "Notable pitfalls" section
describes the dispatch-ordering inversion as affecting `handler.rs:519`
(`handle_build_started`). Source code confirms that the normal-dispatch path at
`handler.rs:519` fires AFTER the active entry already has its correct
`connection_id` set (per `dispatch.rs:122–134`). The ordering inversion is only
on the reconnect path at `handler.rs:661–665`. An implementer reading the plan
literally could incorrectly modify the normal-path flow.

**Finding 3 — Minor (D10, -5 pts):** Commit 5's component field in the summary
table is `cbsd-rs/server` but the commit's package section explicitly lists
`cbsd-rs/cbc` as a co-touched package. Per the `git-commits` skill convention,
workspace-spanning commits use `cbsd-rs:` not a single crate prefix. The subject
line in the table (`cbsd-rs/server: bound log tail...`) would be incorrect if
the commit also modifies `cbc/`.

No further critical or significant issues were found. All three findings are
addressable with plan text corrections only — no redesign is required.

---

## Coverage table

### WCP design requirements (D1–D7)

| WCP req | Gap(s)     | Plan commit(s)          | Status                                   |
| ------- | ---------- | ----------------------- | ---------------------------------------- |
| D1      | G1, G7, G8 | 2 (auth), 3 (reconnect) | Covered                                  |
| D2      | G2         | 2                       | Covered                                  |
| D3      | G9         | 2 (receipt field)       | Covered                                  |
| D4      | G3, G10    | 1 (rollback + ordering) | Covered (partial; ack-timer cancel in 2) |
| D5      | G4         | 4                       | Covered                                  |
| D6      | G6         | 6                       | Covered                                  |
| D7      | G5         | 5                       | Covered                                  |

### Audit-remediation design requirements (D1–D13)

| Audit req | Audit finding(s)   | Plan commit(s)           | Status  |
| --------- | ------------------ | ------------------------ | ------- |
| D1        | F1 (CBSD_DEV)      | 7                        | Covered |
| D2        | F2                 | 10                       | Covered |
| D3        | F4                 | 11 (write), 12 (trigger) | Covered |
| D4        | F5                 | 13                       | Covered |
| D5        | F7                 | 8                        | Covered |
| D6        | F8                 | 9                        | Covered |
| D7        | F10                | 16                       | Covered |
| D8        | F11                | 17                       | Covered |
| D9        | F5 (URI policy)    | 13                       | Covered |
| D10       | F13                | 14 (wrap), 15 (audit)    | Covered |
| D11       | WCP open #1        | 19                       | Covered |
| D12       | WCP open #2        | 20                       | Covered |
| D13       | WCP open #3, SI-18 | 18 (test), 21 (impl)     | Covered |

### Soundness-review gaps (G1–G10)

| Gap | Description                                  | Commit | Status  |
| --- | -------------------------------------------- | ------ | ------- |
| G1  | Build-scoped auth absent                     | 2      | Covered |
| G2  | `build_output` ownership missing             | 2      | Covered |
| G3  | Rollback fn absent                           | 1      | Covered |
| G4  | Empty components not rejected                | 4      | Covered |
| G5  | Bounded log tail not implemented             | 5      | Covered |
| G6  | Worker supervisor absent                     | 6      | Covered |
| G7  | Migration blind swap                         | 3      | Covered |
| G8  | Idle status leaks across workers             | 3      | Covered |
| G9  | `ActiveAssignmentReceipt` absent             | 2      | Covered |
| G10 | Dispatch ordering inversion — reconnect path | 1      | Covered |

All WCP D1–D7, audit-rem D1–D13, and gaps G1–G10 are assigned to at least one
commit. Coverage is complete.

---

## Source-claim verification

Every plan citation below was read at the stated file and line. Results are
recorded verbatim.

| Plan claim                                                                             | File / lines                               | Verified?     | Notes                                                                                                                                                                    |
| -------------------------------------------------------------------------------------- | ------------------------------------------ | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `handle_build_started` at `handler.rs:519` invoked without ownership check             | `cbsd-server/src/ws/handler.rs:519`        | Yes           | Line 519: `dispatch::handle_build_started(state, build_id.0).await;` — no `connection_id` arg.                                                                           |
| Reconnect-path inversion at `handler.rs:657–667`                                       | `cbsd-server/src/ws/handler.rs:661–665`    | Yes (partial) | Line 661 calls `handle_build_started`; line 665 sets `ab.connection_id`. Inversion confirmed — BUT this is the reconnect path, not the normal-dispatch path at line 519. |
| `dispatch.rs:122–135` sets `connection_id` at insertion time                           | `cbsd-server/src/ws/dispatch.rs:122–134`   | Yes           | `queue.active.insert(build_id, ActiveBuild { connection_id: connection_id.clone(), ... })` — correct on the normal path.                                                 |
| `write_build_output` at `logs/writer.rs` lacks `connection_id` arg                     | `cbsd-server/src/logs/writer.rs:56–64`     | Yes           | Signature: `write_build_output(log_writer, log_watchers, log_dir, pool, build_id: i64, start_seq, lines)` — no `connection_id`.                                          |
| Migration blind swap at `handler.rs:265–308`                                           | `cbsd-server/src/ws/handler.rs:265–308`    | Yes           | Line 279: `ab.connection_id = connection_id.clone()` — no `builds.worker_id` consult.                                                                                    |
| Idle handler at `handler.rs:717–766` leaks across workers (G8)                         | `cbsd-server/src/ws/handler.rs:716–766`    | Yes           | Filters `ab.connection_id != connection_id` AND `Disconnected \| Dead` states, no `builds.worker_id` check.                                                              |
| `update_build_state` does not clear six columns (G3)                                   | `cbsd-server/src/db/builds.rs:221–240`     | Yes           | Updates only `state` and `error = COALESCE(?, error)`. Does not clear `worker_id`, `trace_id`, `started_at`, `finished_at`, `build_report`.                              |
| Ack-timeout at `dispatch.rs:240–275` uses generic `update_build_state`                 | `cbsd-server/src/ws/dispatch.rs:240–275`   | Yes           | Calls `update_build_state(..., "queued", None)` — no column-clearing rollback.                                                                                           |
| `handle_build_accepted` at `dispatch.rs:284–300` does not verify connection owns entry | `cbsd-server/src/ws/dispatch.rs:284–300`   | Yes           | No connection ownership check in that range.                                                                                                                             |
| `ServerMessage` enum in `ws.rs` has no `UnauthorizedBuildAction` and no catch-all      | `cbsd-proto/src/ws.rs:22–54`               | Yes           | Variants: `BuildNew`, `BuildRevoke`, `Welcome`, `Error`. No `#[serde(other)]`.                                                                                           |
| `cbsd-worker/src/ws/handler.rs` exhaustive match on `ServerMessage`                    | `cbsd-worker/src/ws/handler.rs:178–186+`   | Yes           | `serde_json::from_str::<ServerMessage>` at 178; exhaustive `match server_msg` follows — no `_` arm.                                                                      |
| Active build is a local variable in the worker (G6)                                    | `cbsd-worker/src/ws/handler.rs:154`        | Yes           | `let mut active_build: Option<ActiveBuild> = None;` — scoped to the connection loop.                                                                                     |
| Components empty-check gap in `routes/builds.rs` (G4)                                  | `cbsd-server/src/routes/builds.rs:89–97`   | Yes           | Loop over `components` never executes if empty; no explicit guard.                                                                                                       |
| `MAX_TAIL_LINES` is 10,000 (plan says reduce to 1,000)                                 | `cbsd-server/src/routes/builds.rs:437`     | Yes           | `const MAX_TAIL_LINES: u32 = 10_000;`                                                                                                                                    |
| Log tail reads full file (G5)                                                          | `cbsd-server/src/routes/builds.rs:474–475` | Yes           | `tokio::fs::read_to_string(&log_path).await?` — full file read.                                                                                                          |
| `cbsd-proto/Cargo.toml` has no `[dev-dependencies]`                                    | `cbsd-proto/Cargo.toml`                    | Yes           | Only `[dependencies]` section: `serde`, `serde_json`, `chrono`. No dev-deps.                                                                                             |
| `mod tests` block already imports `use super::*` + build types                         | `cbsd-proto/src/ws.rs:134–140`             | Yes           | `use super::*; use crate::arch::Arch; use crate::build::{...}`                                                                                                           |
| `cbsd-common` crate does not exist today                                               | `cbsd-rs/Cargo.toml` workspace members     | Yes           | `members = ["cbsd-proto", "cbsd-server", "cbsd-worker", "cbc"]` — no `cbsd-common`.                                                                                      |
| `default_tail_n()` returns 30, not 50                                                  | `cbsd-server/src/routes/builds.rs:432–434` | Yes           | `fn default_tail_n() -> u32 { 30 }` — plan commit 5 changes `cbc` default to 50, not `server`.                                                                           |

All cited source lines were confirmed accurate with the single exception noted
in Finding 2 (the plan's framing of `handler.rs:519` as the inversion site is
misleading — the inversion exists only on the reconnect path, which the line-667
citation confirms; both citations are accurate but the surrounding prose
conflates the two paths).

---

## Per-commit smell-test results

Format: each commit is evaluated against the five `git-commits` smell tests: (1)
one-sentence purpose, (2) previous commit compiles, (3) revertable, (4)
testable, (5) no dead code.

### Phase 1 — WCP foundational

**Commit 1** — rollback DB operation and dispatch ordering fix.

- Test 1: "Builds rolled back to queued no longer carry stale provenance from
  the previous assignment." Pass.
- Test 2: Prerequisite is baseline (no prior WCP commit). Pass.
- Test 3: Rollback fn deletion would not break unrelated functionality. Pass.
- Test 4: Rollback-clears-all-columns unit test + dispatch-ack integration test.
  Pass.
- Test 5: Rollback fn is called from `dispatch.rs` ack-timeout and any
  `handle_build_rejected` transient-reject path in the same commit (plan
  requires migration of existing callers). Pass — contingent on the implementer
  also migrating the ack-timeout callsite in the same commit. The plan is
  explicit about this.

**Commit 2** — build-scoped authorization on lifecycle messages.

- Test 1: "Any worker whose lifecycle message is not for its assigned build
  receives an UnauthorizedBuildAction response." Pass.
- Test 2 — **FAIL.** `cbsd-proto` gains
  `ServerMessage::UnauthorizedBuildAction`. `cbsd-worker/src/ws/handler.rs:186+`
  has an exhaustive match on `ServerMessage` with no catch-all. Unless a match
  arm for the new variant is present in `cbsd-worker` in this same commit, the
  workspace does not compile. The packages table entry omits `cbsd-worker` from
  the listed modified packages. This is the critical finding of the review.
- Test 3: Pass (assuming compile issue is resolved).
- Test 4: Cross-worker spoof rejection tests listed. Pass.
- Test 5: `ActiveAssignmentReceipt` added to `ActiveBuild` — first reader is
  commit 3 (plan states this). Pass — this is an explicit "write here, read in
  commit 3" arrangement, which is acceptable.

**Commit 3** — DB-backed ownership on reconnect and idle status.

- Test 1: "Reconnecting workers are validated against builds.worker_id before
  reacquiring active ownership." Pass.
- Test 2: Depends on commits 1 + 2 being in place. Pass (assuming commit 2
  compile issue resolved).
- Test 3: Reverting removes the two-phase check but leaves G7/G8 open again; no
  unrelated functionality breaks. Pass.
- Test 4: Cross-worker `worker_status(Building)` test + same-worker migration
  test listed. Pass.
- Test 5: Helper for two-phase ownership check introduced and called by
  reconnect and idle handlers in the same commit. Pass.

**Commit 4** — reject empty components.

- Test 1: "Build submissions with empty components arrays are rejected at all
  ingress points." Pass.
- Test 2: Depends only on baseline. Orthogonal to commits 1–3. Pass.
- Test 3: Reverting restores the gap; no other functionality breaks. Pass.
- Test 4: REST + periodic + scheduler trigger tests listed. Pass.
- Test 5: Shared validator module introduced and called by all three ingress
  paths. Pass.

Size exception (~180 LOC): the plan's justification (focused validator, mixing
with adjacent commits would mix concerns) is reasonable. No natural split exists
that passes smell test 5.

**Commit 5** — bounded log tail.

- Test 1: "The log tail endpoint reads at most MAX_TAIL_BYTES from the end of
  the file regardless of total log size." Pass.
- Test 2: Orthogonal to commits 1–4. Pass.
- Test 3: Reverting restores full-file read; no other route is affected. Pass.
- Test 4: 4-MiB OOM test + UTF-8 boundary test listed. Pass.
- Test 5: The commit also touches `cbsd-rs/cbc`. The component prefix in the
  summary table (`cbsd-rs/server`) is incorrect for a workspace-spanning commit.
  Per the `git-commits` skill, the subject should use `cbsd-rs:` as the prefix.
  This is a **convention violation (D10)**, not a compile issue.

**Commit 6** — process-level supervisor for worker build state.

- Test 1: "The worker's active build state survives a websocket disconnect."
  Pass.
- Test 2: Orthogonal to server-side commits 1–5. Pass.
- Test 3: Reverting returns to local-variable active build state; no other
  package is broken. Pass.
- Test 4: Subprocess-survives-disconnect test + spool-overflow test listed.
  Pass.
- Test 5: Supervisor is the sole user of its own module in this commit; the
  websocket loop is refactored to delegate to it. No dead code. Pass.

### Phase 2 — Audit-remediation cross-cutting

**Commit 7** — strict CBSD_DEV parsing and loopback guard.

- Tests 1–5: All pass. The `cbsd-common` crate is introduced alongside its first
  users (`cbsd-server`, `cbsd-worker`) in the same commit. The plan explicitly
  calls out "introducing the crate alongside its first users keeps the commit
  smell-test clean." Smell test 5 passes.

**Commit 8** — tarball containment and decompression cap.

- Tests 1–5: All pass. Contained to `cbsd-worker`. Detailed test fixture list is
  complete.

**Commit 9** — cap REST body and WebSocket message sizes.

- Tests 1–5: All pass. Axum layer + worker connect config changed together.
  Spans `cbsd-server` + `cbsd-worker`; correct prefix `cbsd-rs:` used in the
  table.

**Commit 10** — reject OAuth callback with unverified email.

- Tests 1–5: All pass. Focused single-handler fix. The ~130-LOC exception is
  well-justified.

**Commit 11** — split `periodic:manage` capability.

- Tests 1–5: All pass. Capability constants + handlers + seed migration land
  together. The migration drops the legacy capability — this is a breaking
  schema change correctly included in the same commit.

**Commit 12** — re-validate scopes at trigger time.

- Tests 1–5: All pass. Feature-gated soft-delete test is a clean pattern. The
  plan's explicit "do not fork migrations directory" instruction is correct.

**Commit 13** — redact bearer tokens from URI logging.

- Tests 1–5: All pass. One-character server redirect fix + UI hash extraction +
  TraceLayer policy all land together. Correct prefix `cbsd-rs:` for
  workspace-spanning commit.

**Commit 14** — wrap token material in `Secret<T>`.

- Tests 1–5: All pass. The plan correctly identifies that wire types deriving
  `Serialize` with token fields will fail to compile after the wrap, and lists
  the fix. `trybuild` tests provide compile-time guarantees.

**Commit 15** — redact token material from tracing call sites.

- Tests 1–5: All pass. At-floor ~200 LOC. The plan's justification (targeted
  policy enforcement, not architecturally splittable) is sound.

**Commit 16** — index `api_keys.key_prefix`.

- Tests 1–5: All pass. Single migration + sqlx cache. The ~50-LOC exception is
  well-justified (pure schema change with no application logic).

**Commit 17** — enforce HTTPS host and atomic config write in `cbc`.

- Tests 1–5: All pass. Contained to `cbsd-rs/cbc`.

**Commit 18** — SI-18 regression test for `ServerMessage`.

- Tests 1–5: All pass. This commit IS the test, with no production code added.
  The plan correctly notes `[dev-dependencies]` must be added to
  `cbsd-proto/Cargo.toml` alongside the test (no existing dev-deps section). The
  note to drop `Hash` from `ServerMessageTag` per v8 review NF-2-v8 is correctly
  incorporated.

### Phase 3 — WCP-extension

**Commit 19** — `Building` during accepted-phase reconnect.

- Tests 1–5: All pass. Extends the supervisor from commit 6. Correct dependency
  on commit 6 noted.

**Commit 20** — resolve dead workers by DB state and receipt.

- Tests 1–5: All pass. 4-row (state × receipt) table-driven resolution. Correct
  dependency on commits 1 + 2 noted.

**Commit 21** — migration revoke and terminal-pending drain.

- Tests 1–5: All pass. `BuildRevoke.reason` field uses
  `Option<BuildRevokeReason>` with `skip_serializing_if` — this is the
  SI-18-compatible pattern. The plan correctly identifies that splitting would
  create non-compiling intermediates. The ~700-LOC upper-bound exception is
  justified.

---

## New findings

### Finding 1 — Commit 2 exhaustive-match compile failure [Critical]

**Evidence**: `cbsd-worker/src/ws/handler.rs:178–186+` deserialises
`ServerMessage` from the wire and then performs an exhaustive `match` on the
result. No `_` or `#[allow(non_exhaustive)]` arm exists. The `ServerMessage`
enum in `cbsd-proto/src/ws.rs:22–54` currently has four variants; commit 2 adds
a fifth (`UnauthorizedBuildAction`). Adding a variant to `cbsd-proto` causes a
compile error in every crate that matches exhaustively on the enum.
`cbsd-worker` is such a crate.

**Plan package list for commit 2**: `cbsd-proto`, `cbsd-server`
(`ws/handler.rs`, `ws/dispatch.rs`, `logs/writer.rs`). `cbsd-worker` is not
listed.

**Impact**: commit 2 as written produces a workspace that does not compile. Per
the `git-commits` smell test #2, this is a commit boundary violation.

**Required fix (two options):**

Option A (preferred) — add `cbsd-worker` to commit 2's package list with a
minimal handler for `UnauthorizedBuildAction`. The worker receiving this message
is a server diagnostic response; the correct worker action is to log a WARN and
continue. This is also semantically correct: the worker should be aware when the
server rejects one of its lifecycle messages.

Option B — add `cbsd-worker` to commit 2's package list with a catch-all `_` arm
gated on a `// TODO commit N` comment. This is the minimal compile fix but
produces dead code until commit 19 fills in the handler. Smell test #5 would
fail. Option A is strictly superior.

### Finding 2 — Commit 1 "Notable pitfalls" misleads on ordering scope [Minor]

**Evidence**: `dispatch.rs:122–134` sets `connection_id` on the active entry at
insertion time, which is correct and happens before `handler.rs:519` can fire.
The ordering inversion exists only on the reconnect path at `handler.rs:661–665`
(confirmed: line 661 calls `handle_build_started`; line 665 sets
`ab.connection_id`).

**Plan text** (commit 1, "Notable pitfalls"): "The dispatch ordering fix is
subtle: today, `handle_build_started` is invoked at `handler.rs:519` BEFORE the
`connection_id` is set on the active entry."

This sentence, taken literally, asserts an inversion on the normal-dispatch path
at line 519. The normal-dispatch path does NOT have the inversion. The sentence
continues "(`handler.rs:657–667` for the reconnect path)" which clarifies the
intent, but an implementer parsing quickly could misread the normal-dispatch
path as also having the inversion and introduce an unnecessary and possibly
harmful reordering there.

**Required fix**: revise the sentence to lead with the correct description:
"Today, the reconnect path at `handler.rs:661–665` has an inversion:
`handle_build_started` is called before `ab.connection_id = connection_id` is
set. The normal-dispatch path at `handler.rs:519` does not have this inversion;
`dispatch.rs:122–134` populates `connection_id` at active-entry insertion time,
which is before any handler fires. This commit corrects the reconnect path and
verifies the normal path is already correct."

### Finding 3 — Commit 5 component prefix mismatch [Minor]

**Evidence**: commit 5's package section lists `cbsd-server` and `cbsd-rs/cbc`
as modified packages. The summary table's `Component` column lists
`cbsd-rs/server`. Per the `git-commits` skill convention (confirmed in the
plan's own overview section): workspace-spanning commits use `cbsd-rs:` as the
prefix, not a single crate name.

**Required fix**: change the summary-table `Component` entry for commit 5 from
`cbsd-rs/server` to `cbsd-rs`, and update the suggested subject line accordingly
to `cbsd-rs: bound log tail with reverse block scanning`.

---

## Strengths

**Complete gap-to-commit mapping**: every soundness-review gap G1–G10 and both
design's requirements D1–D13/D1–D7 are mapped to exactly one or two commits with
clear rationale. No gap falls between commits.

**Ordering constraints are correctly minimal**: the plan imposes hard ordering
only where genuine data dependencies exist (W1→W2→W3, AR7→AR8→AR9, Phase 3 on
Phase 1). Phase 2 is correctly declared independent. This avoids false
serialisation that would lengthen implementation time.

**cbsd-common introduction is smell-test-clean**: commit 7 introduces the new
crate alongside its first two callers (`cbsd-server`, `cbsd-worker`). No
dead-code commit exists. The plan explicitly calls this out.

**Commit 18 is exactly the right scope**: the SI-18 regression test is a
stand-alone commit with no production code, correctly placed after the
`Secret<T>` work (commits 14–15) and before the protocol extension that depends
on SI-18's protection (commit 21). The `Hash` drop per NF-2-v8 is incorporated.

**Commit 21 forward/backward-compatible wire change**: using
`Option<BuildRevokeReason>` with `skip_serializing_if = "Option::is_none"` is
the correct approach for a backward-compatible extension of `BuildRevoke`. The
plan identifies the SI-18 test as the regression gate for this extension and
places commit 18 before commit 21.

**Rollback function migration is explicit**: commit 1 explicitly requires that
existing `update_build_state("queued", ...)` callsites in `dispatch.rs`
(ack-timeout) and `handle_build_rejected` migrate to the new rollback function
in the same commit. This is the correct atomicity requirement — partial
migration would leave stale-provenance bugs on those paths.

**Size exceptions are justified and pre-accepted**: the plan surfaces four small
commits (4, 10, 15, 16) and two upper-bound commits (2, 6) with clear
justifications. Commits 10 and 16 are genuinely not splittable or combinable
without mixing security concerns.

---

## Open questions

1. **Commit 2 — does the worker ignore `UnauthorizedBuildAction` or act on it?**
   The plan describes the server-side semantics in detail but does not specify
   the worker's response to receiving this message. Should the worker log a WARN
   and continue, or should it abort the current build? The answer affects the
   match arm content in `cbsd-worker`, which must be present in commit 2.

2. **Commit 12 — soft-delete feature gate**: the
   `cfg(feature = "soft-delete-schema")` gate prevents the
   `D3-T-owner-soft-deleted` test from running in CI until the feature is
   enabled. Is there a plan to enable this gate once the schema migration lands?
   If not, the test provides no CI coverage for soft-delete behaviour.

3. **Commit 19 — server-side handling of accepted-phase `Building` report**: the
   plan says the server "treats it as authoritative receipt of
   `build_accepted`." Does this imply the server transitions `receipt` from
   `AwaitingReceipt` to `ReceivedByWorker` and cancels the dispatch-ack timer on
   this path? The WCP spec should be the authority here, but the plan text is
   ambiguous.

4. **Commit 20 — server-restart receipt recovery**: the plan correctly states
   receipt state is in-memory only and is not reconstructed on restart. The
   existing fail-in-flight policy handles the `dispatched` state. What happens
   to builds in `dispatched` state during a server restart that were in
   `ReceivedByWorker` — do they fail immediately on restart recovery, or does
   the worker's reconnect re-establish receipt state through commit 19's
   `Building` report?

---

## Confidence score

| Item                                                | Points | Description                                                                                                                                                                                                                                                      |
| --------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Starting score                                      | 100    |                                                                                                                                                                                                                                                                  |
| D12: Commit 2 package list missing cbsd-worker      | -20    | `cbsd-proto` gains `ServerMessage::UnauthorizedBuildAction`; exhaustive match at `cbsd-worker/src/ws/handler.rs:186+` produces a compile failure without a match arm for the new variant. Commit 2 as written violates smell test #2 (previous commit compiles). |
| D9: Commit 1 ordering description misleads on scope | -5     | Normal-dispatch path at `handler.rs:519` does not have the inversion; the plan's prose implies it does. Reconnect-path inversion at `handler.rs:661–665` is the actual issue. Misleading text could cause incorrect modification of the normal path.             |
| D10: Commit 5 component prefix incorrect            | -5     | Package section lists both `cbsd-server` and `cbsd-rs/cbc`; summary table shows `cbsd-rs/server`. Workspace-spanning commits must use `cbsd-rs:` per the `git-commits` skill and the plan's own stated convention.                                               |
| **Total**                                           | **70** |                                                                                                                                                                                                                                                                  |

Score interpretation: 70 — significant issue (compile failure in commit 2) must
be resolved before implementation begins. The other two findings are plan-text
corrections only.

---

## Verdict

**Approve with conditions.**

The plan is structurally sound. Coverage is complete. Source citations are
accurate. Ordering constraints are correct and minimal. The commit boundary
violation in commit 2 (exhaustive-match compile failure) is a plan-text defect —
it cannot be discovered at implementation time without producing a broken
commit. It must be corrected before implementation of commit 2 begins.

Required before proceeding with commit 2:

1. Add `cbsd-worker` to commit 2's package list. Specify the match arm for
   `ServerMessage::UnauthorizedBuildAction` in `cbsd-worker/src/ws/handler.rs`.
   Recommended behavior: log WARN with `build_id` and `action`, continue without
   closing the connection.

The other two findings (ordering description scope, component prefix) are
plan-text corrections that improve implementer safety and should be fixed before
implementation but are not blockers for any commit other than 5.
