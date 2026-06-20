# Design Review: 007 — Periodic Short IDs (v1)

**Document:** `docs/cbc/design/007-20260619T2132-periodic-short-ids.md`
**Reviewer mandate:** adversarial; every claim verified against source.
**Scope:** client-side change to `cbc periodic` (`cbsd-rs/cbc/src/periodic.rs`).

---

## Summary

The design solves a real, correctly-diagnosed problem: `list` prints an 8-char
truncation that no other subcommand can consume, and the truncation has a latent
(astronomically unlikely) collision bug. The proposed direction — minimum-viable
unique display plus client-side prefix resolution mirroring `resolve_worker_id`
— is sound and the no-server-change constraint is appropriate.

One blocker keeps this from a clean go: the "no 403 fallback" justification is
**factually wrong for four of the five ID-taking commands**, and taken literally
it regresses a constructible, server-tested permission shape (`periodic:manage`
without `periodic:view`). The reasoning must be corrected and the fallback
decision re-made. Everything else is either correct as written or a low-severity
note.

**Verdict: NO-GO until B1 is resolved.** B1 is a reasoning/spec defect in the
design, cheap to fix; the rest of the design can proceed unchanged.

---

## Verification performed

Read in full and cross-checked: `cbc/src/periodic.rs`, `cbc/src/worker.rs` (the
cited precedent), `cbc/src/client.rs`, `cbc/src/error.rs`, `cbc/Cargo.toml`,
`cbsd-server/src/routes/periodic.rs`, `cbsd-server/src/db/periodic.rs`,
`cbsd-server/src/auth/extractors.rs` (`has_cap`),
`cbsd-server/src/routes/permissions.rs`, migration
`008_periodic_manage_split.sql`, and design/review `04`.

Confirmed correct in the design:

- `cmd_list` truncation is exactly `&task.id[..8]` (periodic.rs L363–367).
- IDs are server-generated `uuid::Uuid::new_v4().to_string()` (routes L362) —
  standard `8-4-4-4-12`.
- `db::periodic::list_tasks` has **no** owner filter (`... ORDER BY created_at`,
  db L116–135); `get_task` is `WHERE id = ?` (db L70–89). The list ⊇ get
  superset claim holds.
- `cbc` has **no** `uuid` dependency (Cargo.toml) — the "no new dependency"
  non-goal is accurate.
- No existing `min_unique_components` / `truncate_components` helper exists in
  `cbc/src/` — the helpers are genuinely new, so no duplication (no D2).

---

## Blockers

### B1 — "No 403 fallback" justification is wrong and regresses manage-without-view

The design (Prefix resolution section) drops the worker-style 403 fallback with
this justification:

> Unlike `resolve_worker_id`, there is **no 403 fallback**: `list` and `get`
> require the identical `periodic:view` capability, so a 403 on the list implies
> `get` would also 403 — the fallback would be dead code.

The get-parity is true: `get_task` gates on `periodic:view` (routes L475). But
resolution is wired into **five** commands, and four of them do **not** gate on
`periodic:view`:

- `update_task`, `delete_task`, `enable_task`, `disable_task` gate on
  `can_manage_task` → `periodic:manage:own`/`:any` (routes L529, L694, L754,
  L814; helper L142–145). They never check `periodic:view`.

Capabilities are a **flat membership set with no implication**:

```rust
// auth/extractors.rs L99
pub fn has_cap(&self, cap: &str) -> bool {
    self.caps.iter().any(|c| c == "*" || c == cap)
}
```

There is no expansion logic anywhere (`grep` for `implies`/`expand` is empty).
So `periodic:manage:any` does **not** confer `periodic:view`. The
manage-without-view shape is real and supported:

- The server's own tests construct `["periodic:manage:own"]` with no view cap
  (routes L197, L209), and `["periodic:manage:any"]` alone (L185).
- Migration `008` explicitly contemplates **custom roles** carrying
  periodic-manage caps (008 L8–9), and `periodic:view` / `periodic:manage:*` are
  independent entries in the permission catalog (permissions.rs L42–45).

Consequence: a user holding `periodic:manage:any` but not `periodic:view` can
**today** `delete`/`enable`/`disable`/`update` by full UUID. Under this design,
`resolve_periodic_id` first calls `GET /api/periodic` (needs `periodic:view`),
gets 403, and — with no fallback — fails the whole command. They can no longer
perform those operations by **any** id, full UUID included. That is precisely
the regression the worker fallback exists to prevent.

**Required fix (pick one, and correct the stated reasoning either way):**

1. Keep a `resolve_worker_id`-style 403 fallback for the four manage commands:
   on list-403, treat the argument as a full id and proceed. (`get` may
   legitimately keep no fallback, since its 403 is genuine.) Or
2. Explicitly document "manage without view is unsupported for prefix
   resolution; full UUID still works only via …" — but note option 2 as written
   still breaks them, because resolution runs before the op, so it is not
   actually a viable fallback without code. Option 1 is the real fix.

The current text's premise ("identical `periodic:view`") is false for the
mutation path; even if the chosen resolution were "no fallback," the
justification must cite the real reason, not get-parity. This is the one finding
that moves the verdict.

---

## Major concerns

None beyond B1.

---

## Minor issues / notes

### N1 — "mirrors `resolve_worker_id`" overstates the precedent for display

The resolution helper does mirror `resolve_worker_id` (worker.rs L134–169)
closely and correctly. But the design implies the whole approach is precedented;
`worker list` shows **names**, not truncated IDs (worker.rs L186–212), so there
is **no** existing min-unique _display_ precedent. The display helpers are
net-new. This is accurate scoping for the reviewer, not a defect — but the
design should not lean on "established precedent" for the display half. Low
severity.

### N2 — `truncate_components` `n == 0` underflow is a latent footgun

`id.match_indices('-').nth(n - 1)` underflows on `n == 0`. The only caller
(`min_unique_components`) iterates `1..=5`, so it is unreachable in-commit and
both fns are private. The byte-index slice `&id[..idx]` is **safe**:
`match_indices('-')` returns the byte offset of an ASCII `-`, always a char
boundary, so no panic even on non-UUID input. Recommend a `debug_assert!` on the
`n >= 1` precondition, or a doc-comment contract; not a blocker. Low severity.

### N3 — `min_unique_components` on empty / single input is benign

Empty slice → the `all()` is vacuously true at `n == 1` → returns 1. Single
element → one insert succeeds → returns 1. Both are harmless, and `cmd_list`
early-returns on `tasks.is_empty()` (periodic.rs L352) so the empty case is
never reached in production. Correct as designed; the plan's "empty slice
handled" test is still worth keeping as a guard. No deduction.

### N4 — "supersedes design 04" does not violate the snapshot rule

The design annotates supersession without editing `04`; `04` remains the record
of original behavior. This is compatible with seq-docs-convention's "designs are
snapshots in time." Verified `04` (`design/04-20260318T1804-periodic-builds.md`)
is untouched. No issue.

### N5 — extra round-trip / TOCTOU is acceptable but undocumented in risk terms

Each id-taking command now fetches the full list before acting. The list
endpoint is unpaginated (`list_tasks` returns all rows), so for the handful of
periodic tasks this is fine. There is a benign TOCTOU window (task could change
between resolve and op) but the resolved value is a stable full UUID and the
server re-checks existence and ownership, so the worst case is a clean 404/403
on the op. Worth one sentence in the design; not a defect.

---

## Strengths

- Problem statement is precise and the latent-collision bug is correctly
  identified and tied to the un-resolvable short ID.
- Superset invariant is stated and (for `get`) verified against the actual SQL —
  good rigor.
- Error UX (no-match / ambiguous-with-candidates / full-UUID-not-found → clearer
  no-match) matches the `resolve_worker_id` model and the existing
  `Error::Other`/`Error::Api` shapes in `error.rs`.
- No server/DB/wire change keeps blast radius minimal and avoids changing the
  REST contract for the web UI — the rejected "server-side resolution"
  alternative is correctly reasoned.

---

## Confidence scoring (design)

| Item                                                               | Points |
| ------------------------------------------------------------------ | ------ |
| Starting score                                                     | 100    |
| D8: cap-parity spec claim false for 4/5 commands (B1)              | -5     |
| D7: dropping fallback regresses manage-without-view auth path (B1) | -20    |
| D11: precedent overstated for display half (N1)                    | -5     |
| D11: `n==0` underflow contract undocumented (N2)                   | -5     |
| **Total**                                                          | **65** |

65/100 — significant issue; must address B1 before proceeding. The single
high-impact finding (B1) accounts for the bulk; remove it and the design sits
comfortably in the 85+ band.

---

## Findings ordered by severity

1. **B1 (critical):** "no 403 fallback" justification is false for
   `update`/`delete`/`enable`/`disable`; dropping the fallback regresses
   `periodic:manage` users who lack `periodic:view`. Fix the fallback and the
   reasoning.
2. **N1 (low):** display helpers are net-new; do not claim `resolve_worker_id`
   precedent for the display path.
3. **N2 (low):** document/assert `truncate_components` `n >= 1` contract.
4. **N5 (low):** note the per-command extra list fetch + benign TOCTOU.
5. **N3, N4:** verified clear — no change required.

**Recommendation:** revise the design to correct B1, then implement.
