# Design Review: 007 — Periodic Short IDs (v2)

**Document:** `docs/cbc/design/007-20260619T2132-periodic-short-ids.md`
**Reviewer mandate:** adversarial; every claim re-verified against source, the
v1 review distrusted as well as the revision. **Scope:** client-side change to
`cbc periodic` (`cbsd-rs/cbc/src/periodic.rs`).

---

## Summary

This is a full independent pass, not a delta check. The revised design corrects
the one blocking defect from v1 (B1): the "no 403 fallback — cap parity with
list" claim is gone, replaced by a `resolve_worker_id`-style 403 fallback that
is now correctly justified against the real, flat, no-implication capability
model. I re-verified the cap gating in the server directly: the reasoning is
sound and the fallback is required, not optional.

The design is otherwise correct as written. The superset invariant holds, the
no-server-change constraint is appropriate, the error-handling UX matches the
established `resolve_worker_id` model, and the new display helpers are genuinely
net-new (no duplication). Two items the task asked to confirm — the empty-prefix
semantics and the manage-only-by-short-prefix behavior — are benign and (for the
latter) documented; I flag the empty-prefix case as a one-line gap.

**Verdict: GO.** B1 is genuinely resolved (corrected reasoning + reinstated
fallback), not merely reworded. Remaining items are low severity and do not
block implementation.

---

## Verification performed

Read in full and cross-checked against the design's claims:
`cbc/src/periodic.rs`, `cbc/src/worker.rs` (the cited precedent),
`cbc/src/client.rs`, `cbc/src/error.rs`, `cbc/Cargo.toml`,
`cbsd-server/src/routes/periodic.rs`, `cbsd-server/src/db/periodic.rs`, and the
two v1 review documents.

Confirmed true in code:

- **403 surfaces as `Error::Api { status: 403, .. }`.** `client.rs::request`
  maps any non-success response to
  `Err(Error::Api { status: status.as_u16(), message })` (client.rs L236–250).
  `client.get` routes through `request` (L107–110). So the design's match arm
  `Err(Error::Api { status: 403, .. }) => return Ok(prefix.to_string())` is
  valid and will fire on a real 403. The arm `Err(e) => return Err(e)` and the
  matching `Error` variant shape (`error.rs` L15–21) are both correct.
- **Cap gating is exactly as the revised design states.**
  - `list_tasks` gates on `periodic:view` (routes L435).
  - `get_task` gates on `periodic:view` (routes L475).
  - `update_task` (L529), `delete_task` (L694), `enable_task` (L754),
    `disable_task` (L814) gate on `can_manage_task` and **never** check
    `periodic:view`.
  - `can_manage_task` = `periodic:manage:any` OR (`periodic:manage:own` AND
    owner) (routes L142–145). No `periodic:view` anywhere in that path.
  - The server's own tests construct manage-without-view shapes
    (`["periodic:manage:any"]`, `["periodic:manage:own"]`; routes L180–213), and
    `no_manage_cap_denies_even_for_own_task` confirms `periodic:view` alone does
    not confer mutation. The manage-without-view permission shape is real and
    supported.
- **Superset invariant.** `db::list_tasks` has no owner filter
  (`... ORDER BY created_at`, db L116–135); `get_task` is `WHERE id = ?` (db
  L89). Every task `get` can return is in `list`. The invariant the design
  relies on holds.
- **No new dependency.** `cbc/Cargo.toml` has no `uuid` dep; the helpers are
  pure string ops (`match_indices('-')`, `starts_with`). The non-goal is
  accurate.
- **No duplication.** `grep` for `resolve_periodic_id`, `match_periodic_prefix`,
  `min_unique_components`, `truncate_components` across `cbc/src/` returns
  nothing — all helpers are net-new; `worker list` shows names, so there is no
  existing min-unique _display_ logic to reuse. No D2.

---

## v1 finding disposition

| v1 finding    | Status             | Notes                                            |
| ------------- | ------------------ | ------------------------------------------------ |
| B1 (critical) | RESOLVED           | Fallback reinstated; reasoning corrected (below) |
| N1 (low)      | RESOLVED           | Display half no longer claims precedent          |
| N2 (low)      | RESOLVED           | `n >= 1` contract now documented                 |
| N5 (low)      | PARTIALLY RESOLVED | TOCTOU/extra-fetch still not noted in design     |

### B1 — RESOLVED

The "no 403 fallback / cap parity with list" text is gone. The "Prefix
resolution" section now carries an explicit **"403 fallback (required)"**
paragraph that:

- States the capability set is flat with no implication between caps — verified:
  `has_cap` is a plain membership test, no expansion logic exists.
- Correctly identifies that `update`/`delete`/`enable`/`disable` gate on
  `periodic:manage:*` (via `can_manage_task`) and never check `periodic:view`.
- Explains the regression the fallback prevents (manage-only caller loses
  mutation by **any** id, full UUID included) and that the fallback is uniform
  across all five commands — a harmless no-op for `get` (its subsequent
  `GET /api/periodic/{id}` would itself 403, which I confirmed: `get_task` gates
  on `periodic:view`), and the full-UUID path preserved for the four manage
  commands.

This is a genuine correction of the reasoning, not a rewording. The verdict
moves to GO on this basis.

### N1 — RESOLVED

The design now states the display helpers are net-new and that "only the
_resolution_ half mirrors `resolve_worker_id`" (design L86–88). Accurate.

### N2 — RESOLVED

The `truncate_components` `n >= 1` contract is documented in the plan's helper
doc-comment and referenced in the design (L89–90: "never invoked with fewer than
one component (its documented `n >= 1` contract)"). The byte-index slice remains
panic-safe regardless (`match_indices('-')` yields ASCII char boundaries).

### N5 — PARTIALLY RESOLVED

The per-command extra list fetch and the benign TOCTOU window are still not
acknowledged anywhere in the revised design. It is genuinely benign (list is
unpaginated and small; the resolved value is a stable full UUID; the server
re-checks existence/ownership so the worst case is a clean 404/403 on the op),
but v1 explicitly asked for one sentence and it is absent. Low severity, no
deduction beyond the original N5 weight.

---

## New findings (full independent pass)

### N6 — Empty-prefix semantics undocumented (low)

The task asked to assess empty-prefix behavior; the design does not address it.
`"".starts_with(_)` is true for every string, so `resolve_periodic_id("")`
filters to **all** tasks: it resolves uniquely (returns the sole task) when
exactly one task exists, and errors as "ambiguous" when ≥2 exist. This mirrors
`resolve_worker_id` exactly (same `starts_with` filter, same behavior), so it is
not a regression and is arguably acceptable, but it is surprising (an empty
argument silently targeting the only task). Clap requires the positional `id`,
so an empty value only arrives via an explicit `""`. Worth one sentence in the
error-handling section. Low severity.

### N7 — Manage-only short-prefix path is correct and documented (no deduction)

Confirmed the path the task flagged as a potential new risk is safe: a
`periodic:manage`-only caller (no view) running `delete <short-prefix>` gets a
list-403 → fallback returns the prefix verbatim →
`DELETE /api/periodic/{prefix}` → `get_task(WHERE id = prefix)` finds no row →
server **404**, not a silent no-op. The design documents this (L155–158). No new
silent-failure risk. Clean.

---

## Strengths

- B1 is fixed at the root: the reasoning now matches the real auth model, and
  the fallback is reinstated uniformly rather than bolted onto four commands ad
  hoc.
- The superset invariant is stated and (for `get`) tied to the actual SQL.
- `delete` echoes the resolved full UUID before the destructive call (design
  L144–146), so a prefix matching the wrong task is visible — good defensive UX,
  mirroring `worker deregister`.
- The "character-granularity vs component-granularity" alternative is reasoned
  out and the choice is consistent with `get` accepting any character prefix.

---

## Confidence scoring (design)

| Item                                                     | Points |
| -------------------------------------------------------- | ------ |
| Starting score                                           | 100    |
| D11: empty-prefix semantics undocumented (N6)            | -5     |
| D11: per-command extra-fetch / TOCTOU still unnoted (N5) | -5     |
| **Total**                                                | **90** |

90/100 — ready to implement. The single high-impact v1 finding (B1) is genuinely
resolved; the two residual deductions are documentation-only and do not affect
correctness.

---

## Findings ordered by severity

1. **N6 (low):** document empty-prefix resolution semantics (resolves to the
   sole task when one exists; ambiguous otherwise).
2. **N5 (low, carried):** add one sentence on the per-command list fetch and the
   benign TOCTOU window.
3. **N7 (verified clear):** manage-only short-prefix surfaces a server 404, not
   a silent no-op — correct and documented.

**Recommendation:** GO. Optionally fold N5/N6 into the design as one-line notes
before implementation; neither blocks.
