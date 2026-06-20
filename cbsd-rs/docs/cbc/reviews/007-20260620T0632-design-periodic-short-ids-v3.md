# Design Review: 007 — Periodic Short IDs (v3)

**Document:** `docs/cbc/design/007-20260619T2132-periodic-short-ids.md`
**Reviewer mandate:** adversarial; full independent pass, not a delta check.
Every claim re-verified against source; the v1/v2 reviews distrusted as well as
the revision. **Scope:** client-side change to `cbc periodic`
(`cbsd-rs/cbc/src/periodic.rs`).

---

## Summary

This is a full independent re-derivation against the source, not a check that
the v2 edits were applied. The design is correct as written. The two carried v2
design findings are now closed:

- **N6 (empty-prefix semantics)** is documented with the correct `starts_with`
  reasoning, and the clap-requires-the-arg nuance is stated.
- **N5 (extra fetch + benign TOCTOU)** now has its own "Cost and consistency"
  paragraph with the right justification (resolved value is a stable full UUID;
  server re-checks existence/permission, so a stale resolve surfaces as a clean
  404/403).

The B1 fix from v2 (the `resolve_worker_id`-style 403 fallback, justified
against the real flat capability model) is intact and re-verified in code. The
v3 revision additionally hoists the decision logic into a pure
`resolve_from_fetch(Result<Vec<PeriodicListItem>, Error>, prefix)` helper in the
design body, which I verified is genuinely pure, type-checks against the real
`CbcClient::get` signature, and exercises a 403 arm that is constructible in an
in-crate unit test. No previously-resolved finding has regressed.

**Verdict: GO.** Every open v2 design finding is resolved; the residual items
are documentation-only and below the deduction threshold.

---

## Verification performed (re-derived in code)

Read in full and cross-checked against the design's claims:
`cbc/src/periodic.rs`, `cbc/src/worker.rs` (the cited precedent),
`cbc/src/client.rs`, `cbc/src/error.rs`, `cbc/Cargo.toml`,
`cbsd-server/src/routes/periodic.rs`, `cbsd-server/src/db/periodic.rs`, and all
four prior review documents.

Confirmed true in code:

- **403 surfaces as `Error::Api { status: 403, .. }`.** `client.rs::request`
  maps any non-success response to
  `Err(Error::Api { status: status.as_u16(), message })` (client.rs L232–250).
  `client.get` routes through `request` (L107–110). The design's match arm
  `Err(Error::Api { status: 403, .. }) => return Ok(prefix.to_string())` is
  valid and fires on a real 403.
- **`Error::Api` is constructible in a cbc unit test.** `error.rs` L15–21 is a
  bare `pub enum Error` with `Api { status: u16, message: String }`. `status` is
  `u16`, so `Error::Api { status: 403, message: String::new() }` type-checks
  directly; `Error` is already in scope at periodic.rs L25. The design's claim
  that the 403 branch is unit-testable from constructed values without an HTTP
  mock is sound.
- **The async wrapper type-checks.** `CbcClient::get` is
  `pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error>`
  (client.rs L107). In
  `resolve_from_fetch(client.get("periodic").await, prefix)`, `T` is inferred as
  `Vec<PeriodicListItem>` from the helper's first parameter type.
  `PeriodicListItem` is `Deserialize` (periodic.rs L198–204), so the
  `DeserializeOwned` bound is satisfied. The wrapper compiles as written.
- **`resolve_from_fetch` is genuinely pure.** As written (design L104–120) it
  takes an already-resolved `Result`, matches the 403 arm, propagates other
  errors, and filters by `starts_with` — no `await`, no client, no I/O. Pure and
  fully unit-testable.
- **Cap gating is exactly as the design states.** `list_tasks` (routes L435) and
  `get_task` (L475) gate on `periodic:view`. `update_task` (L529) checks
  `can_manage_task` — and does so on a row it fetched first, never touching
  `periodic:view`. `can_manage_task` = `periodic:manage:any` OR
  (`periodic:manage:own` AND owner) (L142–145). The manage-without-view shape is
  real and server-tested (`no_manage_cap_denies_even_for_own_task`, L215+). So
  the 403 fallback is required, not optional, exactly as argued.
- **Superset invariant.** `db::list_tasks` has no owner filter
  (`... ORDER BY created_at`, db L116–135); `get_task` is `WHERE id = ?` (db
  L70–113). Every task `get` can return is in `list`. The invariant holds.
- **No new dependency.** `cbc/Cargo.toml` has no `uuid` dep; the helpers are
  pure string ops (`match_indices('-')`, `starts_with`). The non-goal is
  accurate.

---

## v2 finding disposition

| v2 finding   | Status   | Notes                                               |
| ------------ | -------- | --------------------------------------------------- |
| N6 (low)     | RESOLVED | Empty-prefix semantics documented (design L144–149) |
| N5 (low)     | RESOLVED | Extra fetch + TOCTOU documented (design L151–158)   |
| B1 (carried) | INTACT   | 403 fallback + flat-cap reasoning unchanged/correct |

### N6 — RESOLVED

The new "Empty prefix" paragraph (design L144–149) states `"".starts_with(_)` is
true for every id, that an empty argument therefore matches all tasks (sole task
→ resolves, otherwise ambiguous / "no task" on an empty list), that this mirrors
`resolve_worker_id`, and that clap already requires the positional argument so
an empty string only arises via an explicit `""`. I verified the semantics
against `resolve_worker_id` (worker.rs L144–147, identical `starts_with` filter)
— the description is exactly correct.

### N5 — RESOLVED

The "Cost and consistency" paragraph (design L151–158) now acknowledges the one
extra `GET /api/periodic` per id-taking command and the benign TOCTOU window,
with the correct justification: the resolved value is a full UUID handed to the
server, which performs its own authoritative existence/permission checks, so a
stale resolution surfaces as the server's 404/403 rather than acting on the
wrong task. This is the sentence v1/v2 asked for.

### B1 — INTACT (no regression)

The "403 fallback (required)" paragraph (design L131–142) is unchanged in
substance and remains correct: flat capability set, no implication; the four
manage commands gate on `periodic:manage:*` via `can_manage_task` and never
check `periodic:view`; without the fallback a manage-only caller would lose
mutation by any id. Re-verified in the server. Not regressed by the v3 edits.

---

## New findings (full independent pass)

### D2 disposition — resolution helpers structurally resemble `resolve_worker_id` (no deduction)

`resolve_from_fetch` / `resolve_periodic_id` share the shape of
`resolve_worker_id` (fetch → 403-fallback → `starts_with` → 0/1/many). This is
**consciously not** a D2 deduction: the two operate on different item types
(`PeriodicListItem` vs `WorkerInfo`) with different return shapes (`String` vs
`(String, String)` plus a `"unknown"` name sentinel), and deduplicating would
require generifying over a trait and editing out-of-scope `worker.rs`. The
design explicitly scopes only the _resolution_ half as mirroring the precedent
and the _display_ half as net-new (design L86–88), which I confirmed
(`worker list` shows names, not truncated IDs). Disposition: not a defect.

### N8 — Empty-prefix "no task" path is consistent with code (no deduction)

The design says an empty prefix resolves "to the sole task when exactly one
exists and is otherwise reported as ambiguous (or 'no task' when the list is
empty)" (L146–147). Verified against `resolve_from_fetch`: an empty list yields
`matches.len() == 0` → the no-match error, whose design wording is
`no periodic task matching '<prefix>'` (L179) — for `prefix == ""` that renders
`matching ''`, which is slightly odd but harmless and matches the established
`resolve_worker_id` behavior. No change required.

---

## Strengths

- The pure/async split in the design body (`resolve_from_fetch` + thin
  `resolve_periodic_id`) makes the regression-prevention branch (403 fallback)
  directly unit-testable — the exact gap v2's plan review flagged, addressed at
  the design level so the plan can simply follow it.
- Superset invariant stated and tied to the actual SQL; the no-server-change
  constraint is correctly defended against the rejected server-side alternative.
- `delete` echoes the resolved full UUID before the destructive call (design
  L173–175), mirroring `worker deregister` — good defensive UX.
- The "character- vs component-granularity" alternative is reasoned out and the
  choice is consistent with `get` accepting any character prefix.

---

## Confidence scoring (design)

| Item                                | Points   |
| ----------------------------------- | -------- |
| Starting score                      | 100      |
| (v2 D11: empty-prefix — N6)         | resolved |
| (v2 D11: extra-fetch / TOCTOU — N5) | resolved |
| **Total**                           | **100**  |

100/100 — ready to implement. Both residual v2 deductions are cleared by the
revision; no new defect was found on the independent pass. The D2 and N8 items
above are explicitly dispositioned as non-deductions.

---

## Findings ordered by severity

1. **D2 (no deduction):** resolution helpers structurally resemble
   `resolve_worker_id`; consciously not deducted (different types/returns; dedup
   would touch out-of-scope `worker.rs`).
2. **N8 (no deduction):** empty-prefix no-match renders `matching ''`; harmless,
   matches the `resolve_worker_id` precedent.

**Recommendation:** GO. The design is implementable as written.
