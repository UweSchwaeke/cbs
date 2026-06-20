# Plan Review: 007 — Periodic Short IDs (v3)

**Document:** `docs/cbc/plans/007-20260619T2133-periodic-short-ids.md`
**Design:** `docs/cbc/design/007-20260619T2132-periodic-short-ids.md` **Reviewer
mandate:** adversarial; full independent pass; commit boundaries assessed with
the `git-commits` smell test; the v1/v2 reviews distrusted alongside the
revision.

---

## Summary

Full independent pass. The two-commit plan is confined to one file
(`cbsd-rs/cbc/src/periodic.rs`), split by **capability** (display vs.
resolution), and both commits pass the smell test. The single finding that held
v2 at NO-GO — **P5**, the 403-fallback branch left manual-only on a false
"untestable I/O" premise — is now genuinely resolved: the plan factors the
decision into the pure
`resolve_from_fetch(Result<Vec<PeriodicListItem>, Error>, prefix)` helper and
unit-tests every branch, including the 403 arm, from constructed values. I
verified in code that the 403 arm is constructible (`Error::Api` is a bare-field
`pub enum` variant, error.rs L15–21) and that the helper is genuinely pure. The
two low-severity items (P6 update-ordering, P7 empty-prefix test) are also
addressed.

I checked the most likely defect a folding revision leaves behind — an orphaned
helper name. The v2 plan tested `match_periodic_prefix`; v3 folds it into
`resolve_from_fetch`. A grep of both v3 docs finds **no** surviving
`match_periodic_prefix` reference: design and plan name only
`resolve_from_fetch` and `resolve_periodic_id`, consistently. No doc-internal
contradiction.

**Verdict: GO.** P5 is resolved (pure helper + 403-branch unit test, not a
rewording); P6 and P7 are closed; no previously-resolved finding regressed.

---

## Commit boundary assessment (git-commits smell test)

### Commit 1 — `cbc: minimum-viable short ids in periodic list` (~70 LOC)

| Smell test           | Result                                             |
| -------------------- | -------------------------------------------------- |
| One-sentence purpose | Yes — "list shows resolvable min-viable short ids" |
| Previous compiles    | Yes — no dependency on commit 2                    |
| Revertable           | Yes — isolated to `cmd_list` + two helpers         |
| Testable             | Yes — pure helpers unit-tested in-commit           |
| No dead code         | Yes — `cmd_list` consumes both helpers same commit |

Verified `cmd_list` (periodic.rs L346–384) is the in-commit caller of both
`min_unique_components` and `truncate_components`; no `#[allow(dead_code)]`
needed. **No D12.**

### Commit 2 — `cbc: accept periodic task id prefixes` (~110 LOC)

| Smell test           | Result                                                |
| -------------------- | ----------------------------------------------------- |
| One-sentence purpose | Yes — "id-taking subcommands accept a prefix"         |
| Previous compiles    | Yes — commit 1 stands alone                           |
| Revertable           | Yes — adds `resolve_from_fetch`/`resolve_periodic_id` |
| Testable             | Yes — `resolve_from_fetch` unit-tested in-commit      |
| No dead code         | Yes — helper called by all five handlers              |

**No D12.**

### Over-fragmentation check

Both commits are sub-200 LOC on one theme in one file, which per `git-commits`
warrants the "meaningful alone?" question. Both are: commit 1 is a visible,
revertable display improvement that also closes the latent duplicate-short-ID
bug; commit 2 is a distinct capability. Merging into one
`cbc: resolvable short periodic task ids` (~180 LOC) would be equally
defensible. Preference, not a violation; no deduction.

---

## v2 finding disposition

| v2 finding  | Status   | Notes                                                 |
| ----------- | -------- | ----------------------------------------------------- |
| P5 (medium) | RESOLVED | Pure `resolve_from_fetch` + 403-branch unit test      |
| P6 (low)    | RESOLVED | `cmd_update` resolves **after** the `has_field` guard |
| P7 (low)    | RESOLVED | Empty-prefix case added to the test matrix            |
| P1/B1       | INTACT   | 403 fallback still required + correctly justified     |
| P2 (medium) | INTACT   | All `args.id` rewrite sites still enumerated          |
| P3 (low)    | INTACT   | Header uses computed width                            |
| H1 (low)    | ACCEPTED | 3-digit seq `007` by directive — not a defect         |

### P5 — RESOLVED (the v2 NO-GO is cleared)

v2's NO-GO rested on the 403-fallback decision — the exact branch that prevents
the manage-without-view regression — being left "manual only" on a false
"untestable I/O" premise. v3 closes this exactly as v2 prescribed:

- The plan (L117–123) factors **all** decision logic — the 403 fallback **and**
  the zero/one/many matching — into the pure helper
  `resolve_from_fetch(Result<Vec<PeriodicListItem>, Error>, prefix) -> Result<String, Error>`,
  with `resolve_periodic_id` a thin async wrapper
  (`resolve_from_fetch(client.get("periodic").await, prefix)`).
- The test section (L146–159) unit-tests **all** branches from constructed
  values, explicitly including the 403 fallback:
  `Err(Error::Api { status: 403, .. })` → input returned verbatim, called out as
  "the branch that prevents the manage-without-view regression; it is pure
  logic, not untestable I/O."

Verified the premise is now true in code:
`Error::Api { status: u16, message: String }` is a public bare-field variant
(error.rs L19), so `Err(Error::Api { status: 403, message: String::new() })`
constructs directly in a `#[cfg(test)]` module with no HTTP mock. `client.get`
is `get<T: DeserializeOwned>(&self, &str) -> Result<T, Error>` (client.rs L107),
so the wrapper's `T` infers to `Vec<PeriodicListItem>` from the helper parameter
and type-checks. The 403 path really fires: `request` returns
`Error::Api { status: status.as_u16(), .. }` on any non-success (client.rs
L246–250). P5 genuinely resolved.

### P6 — RESOLVED

The plan (L130–132) now states `cmd_update` resolves **after** the existing
`has_field` guard (periodic.rs L533), "so a no-field update still fails fast
without a wasted list fetch." Verified the guard at periodic.rs L518–537 returns
`"at least one option must be provided for update"` before any client is built —
resolving after it preserves fail-fast and avoids the wasted fetch. Correct.

### P7 — RESOLVED

The test matrix (L158–159) now includes the empty-prefix case: "matches all →
resolves with a single-task list, ambiguous with multiple, 'no task' with an
empty list." This pins the `"".starts_with` behavior inside the pure helper that
is already unit-tested. Correct.

### P1/B1, P2, P3 — INTACT (re-verified, no regression)

- **P1/B1:** Commit 2 (plan L108–115) keeps the 403 fallback "**required**, not
  optional," with the correct manage-vs-view reasoning. Re-verified against the
  server (routes L529 `can_manage_task`, never `periodic:view`). Intact.
- **P2:** Every `args.id` rewrite site is still enumerated with line numbers
  (plan L127–139). Re-verified each against the **current** periodic.rs:
  - `cmd_get` GET — L398 ✓
  - `cmd_update` existing-task fetch — L586 ✓; PUT — L721 ✓; success message —
    L726 ✓ (currently prints `args.id`); `has_field` guard — L533 ✓
  - `cmd_delete` DELETE — L747 ✓; success message — L748 ✓; pre-delete echo ✓
  - `cmd_enable` PUT — L765 ✓; message — L767 ✓
  - `cmd_disable` PUT — L784 ✓; message — L786 ✓
  - `--yes-i-really-mean-it` gate at L739–742, resolve runs after it (plan
    L141–143) ✓. The `whoami` GET at L591 is correctly excluded (not
    id-bearing). All line numbers are accurate against current periodic.rs. See
    **N9** for a locator caveat.
- **P3:** Commit 1 (plan L70–73) applies the computed width to both header and
  data rows, replacing the hard-coded `{:<10}` (periodic.rs L357–360). Intact.

### H1 — ACCEPTED (by directive)

The 3-digit seq `007` among 2-digit siblings persists by explicit task directive
and is not a defect. These v3 review files follow the directed legacy pattern
`007-<ts>-<type>-periodic-short-ids-v3.md`.

---

## New findings (full independent pass)

### N9 — Commit 2 line numbers are pre-Commit-1 locators (low, no deduction)

Every Commit 2 site (398, 533, 586, 721, 726, 747/748, 765/767, 784/786) is
accurate against the **current** `periodic.rs`, but Commit 1 inserts the two
helpers and expands `cmd_list` (~25–30 lines) **above** `cmd_get` (L390). Once
Commit 1 lands, every Commit 2 line number shifts down by that delta. The cited
numbers are correct locators for the pre-Commit-1 file, not post-Commit-1
guarantees. The plan lists commit 1 first, so an implementer working in order
will see the shift; the enumeration is by site (GET, PUT, message, guard), which
remains unambiguous regardless of exact line. No deduction — locators, not spec
— but worth noting so an implementer does not blind-trust the integers after
commit 1.

### N10 — `resolve_from_fetch` near-duplicates `resolve_worker_id` (no deduction)

Same disposition as the design review's D2 note: the resolution helper shares
the shape of `resolve_worker_id` but over a different item type and return
shape, and deduping would touch out-of-scope `worker.rs`. Consciously not a D2
deduction.

---

## Testing plan assessment

- Commit 1 (plan L78–83): min-unique (distinct→1, forced collision→2, single→1,
  empty handled) and truncate (`n==1`→8 chars, oversize-`n`→whole id) cover the
  pure logic. Adequate.
- Commit 2 (plan L146–159): `resolve_from_fetch` across **all** branches —
  matching (0/1/many/full-UUID), 403 fallback (the regression-prevention
  branch), other-error propagation, and empty prefix — all from constructed
  values, no mock. This is the coverage v2 required. Adequate.

---

## Convention / hygiene notes

- **H2 — commit conventions correct.** `cbc:` prefix is right (file under
  `cbsd-rs/cbc/`); both subjects are < 72 chars. DCO `-s`, no autonomous GPG,
  single `Co-authored-by`, separate `add`/`commit` (plan L190–196) match
  `cbsd-rs/CLAUDE.md`. Good.
- **H3 — no-`Cargo.toml` / no-`.sqlx` claims verified.** No new dependency (no
  `uuid`; pure string ops, `cbc/Cargo.toml` unchanged) and no `sqlx::query!`
  touched (client-side only). The `detect_changes` pre-commit step (plan
  L186–188) and "no `.sqlx/` regeneration" are appropriate.

---

## Confidence scoring (plan)

| Item                                     | Points   |
| ---------------------------------------- | -------- |
| Starting score                           | 100      |
| (v2 D5: 403-fallback untested — P5)      | resolved |
| (v2 D11: update resolve-ordering — P6)   | resolved |
| (v2 D11: empty-prefix test missing — P7) | resolved |
| **Total**                                | **100**  |

100/100 — ready to implement. The v2 NO-GO driver (P5) is genuinely closed with
a pure, unit-tested helper, verified constructible in code; the two low-severity
specification gaps are filled; the commit boundaries, rewrite-site enumeration,
header fix, and fallback reasoning all remain sound. N9 and N10 are explicitly
dispositioned as non-deductions.

---

## Findings ordered by severity

1. **N9 (low, no deduction):** Commit 2's cited line numbers are pre-Commit-1
   locators; they shift down once Commit 1 inserts the display helpers. The
   per-site enumeration stays unambiguous; do not blind-trust the integers after
   commit 1.
2. **N10 (no deduction):** `resolve_from_fetch` near-duplicates
   `resolve_worker_id`; consciously not deducted (different types/returns; dedup
   would touch out-of-scope `worker.rs`).
3. **H1 (low, by directive):** 3-digit/2-digit seq inconsistency persists by the
   task's directive — accepted, not fixed.

**Recommendation:** GO. P5/P6/P7 are resolved and the prior corrections are
intact; the plan is implementable as written.
