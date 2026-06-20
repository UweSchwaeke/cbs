# Plan Review: 007 — Periodic Short IDs (v2)

**Document:** `docs/cbc/plans/007-20260619T2133-periodic-short-ids.md`
**Design:** `docs/cbc/design/007-20260619T2132-periodic-short-ids.md` **Reviewer
mandate:** adversarial; commit boundaries assessed with the `git-commits` smell
test; the v1 review distrusted alongside the revision.

---

## Summary

Full independent pass. The two-commit plan is well-structured, confined to one
file, and split by **capability** (display vs. resolution), not by layer — it is
not the dead-code / layer-by-layer anti-pattern. Both commits pass the smell
test. The v1 findings P2 (rewrite-site enumeration) and P3 (header width) are
resolved, and the inherited B1/P1 fallback regression is corrected in lockstep
with the design.

One finding keeps this from a clean go: **D5 is only PARTIALLY resolved.** The
plan extracts and unit-tests `match_periodic_prefix` (good), but then leaves the
403-fallback branch — the **exact locus of the critical B1 regression** — as
"manual only," justified by an "untestable I/O" claim that is false. The
fallback _decision_ is pure logic and is trivially unit-testable without any
HTTP mock. v1 explicitly required a test for the chosen fallback behavior; the
plan papers over that gap rather than closing it.

**Verdict: NO-GO until P5 is addressed** (add a pure unit test for the
403→fallback decision, or have the user explicitly accept manual-only coverage
of the very branch that prevents the regression). The commit sizing and
boundaries are sound; this is a one-helper fix, not a re-architecture.

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

Verified `cmd_list` calls both `min_unique_components` and `truncate_components`
in the same commit; no `#[allow(dead_code)]` needed. **No D12.**

### Commit 2 — `cbc: accept periodic task id prefixes` (~110 LOC)

| Smell test           | Result                                               |
| -------------------- | ---------------------------------------------------- |
| One-sentence purpose | Yes — "id-taking subcommands accept a prefix"        |
| Previous compiles    | Yes — commit 1 stands alone                          |
| Revertable           | Yes — adds `resolve_periodic_id` + `match_*` + sites |
| Testable             | Yes — `match_periodic_prefix` unit-tested in-commit  |
| No dead code         | Yes — helper called by all five handlers             |

**No D12.**

### Over-fragmentation check

Both commits are sub-200 LOC on one theme in one file. Per `git-commits`, that
warrants the "meaningful alone?" question. Both _are_ (display is a visible,
revertable improvement; resolution is a distinct capability), and the plan's
rationale (independent revertability, no cross-dependency, minimal blast radius)
is legitimate. Merging into one `cbc: resolvable short periodic task ids` (~180
LOC, one file, one theme) would be equally defensible. **Preference, not a
violation; no deduction.**

---

## v1 finding disposition

| v1 finding         | Status                 | Notes                                            |
| ------------------ | ---------------------- | ------------------------------------------------ |
| P1 (critical, =B1) | RESOLVED               | Fallback now required + justified in commit 2    |
| P2 (medium)        | RESOLVED               | All `args.id` sites enumerated with line numbers |
| D5 (medium)        | PARTIALLY RESOLVED     | Matching tested; fallback branch left manual     |
| P3 (low)           | RESOLVED               | Header uses computed width (commit 1)            |
| P4 (verified)      | RESOLVED               | resolve-after-`--yes` ordering stated for delete |
| H1 (low)           | UNCHANGED BY DIRECTIVE | 3-digit seq among 2-digit siblings persists      |

### P1 / B1 — RESOLVED

Commit 2's content (plan L108–115) now states the 403 fallback is "**required**,
not optional," with the correct reason: "`update`, `delete`, `enable`, and
`disable` gate on `periodic:manage:*` (not `periodic:view`), so a manage-only
caller must keep its existing ability to act by full UUID even though it cannot
list." Verified against the server (routes L529/L694/L754/L814 →
`can_manage_task`, never `periodic:view`). Correct.

### P2 — RESOLVED

Commit 2 (plan L121–133) now enumerates every `args.id` rewrite site with line
numbers. I verified each against the current `periodic.rs`:

- `cmd_get` GET — L398 ✓
- `cmd_update` existing-task fetch — L586 ✓; PUT — L721 ✓; success message —
  L726 ✓ (today prints `args.id`)
- `cmd_delete` DELETE — L747 ✓; success message — L748 ✓ (today prints
  `args.id`); plus the pre-delete echo of the resolved id ✓
- `cmd_enable` PUT — L765 ✓; message — L767 ✓
- `cmd_disable` PUT — L784 ✓; message — L786 ✓

All line numbers are accurate. The `whoami` GET at L591 is correctly excluded
(not id-bearing). Enumeration is now exhaustive.

### P3 — RESOLVED

Commit 1 (plan L70–73) now states the ID column width is "applied to **both**
the header and the data rows (replacing the hard-coded `{:<10}`) so the table
stays aligned when `n > 1`." Verified the current header is a separate
`println!` with `{:<10}` (periodic.rs L357–360); the plan now covers it.

### D5 — PARTIALLY RESOLVED — see P5 below

---

## Plan-specific findings

### P5 — 403-fallback branch left untested on a false "untestable I/O" premise (medium)

This is the strongest remaining finding and the reason for the NO-GO.

The plan (L139–147) tests the pure `match_periodic_prefix` (0/1/many/full-UUID —
good) but explicitly **excludes** the 403 fallback from unit coverage:

> The 403 fallback is an I/O branch (no HTTP mock exists in `cbc`, and the
> `resolve_worker_id` precedent is likewise not unit-tested here); verify it
> manually...

The premise is wrong. The 403→fallback **decision** is not I/O — it is a pure
match on a `Result` that the caller already holds. It is unit-testable with no
`reqwest` mock by factoring the decision into a `Result`-taking helper:

```rust
fn resolve_from_fetch(
    fetched: Result<Vec<PeriodicListItem>, Error>,
    prefix: &str,
) -> Result<String, Error> {
    match fetched {
        Ok(list) => match_periodic_prefix(&list, prefix),
        Err(Error::Api { status: 403, .. }) => Ok(prefix.to_string()),
        Err(e) => Err(e),
    }
}
```

`Error::Api { status, message }` is a public, bare-field variant in the same
crate (`error.rs` L15–21), so a test constructs
`Err(Error::Api { status: 403, message: String::new() })` and asserts
`Ok(prefix)`, then `status: 500` and asserts the error propagates. The async
`resolve_periodic_id` then becomes a thin wrapper: `do the fetch`, pass the
`Result` to `resolve_from_fetch`. This single helper subsumes both the matching
and the fallback, so v1's explicit requirement — "whatever fallback decision is
made for P1 must have a test" — is met without any mock.

The `resolve_worker_id`-is-also-untested argument is not a justification: that
precedent predates this review and is itself a coverage gap, not a standard to
match. Given the fallback is the exact branch that prevents the critical
manage-without-view regression, leaving it manual-only is precisely the gap v1
flagged. **D5 -15.** Fix: add the pure `resolve_from_fetch`-style test, or get
explicit user sign-off on manual-only coverage of the regression-prevention
branch.

### P6 — `cmd_update` resolve ordering unspecified → wasted fetch on no-field update (low)

The plan states resolution runs after `cmd_delete`'s `--yes-i-really-mean-it`
gate (plan L135–136, good — verified the gate returns before any client is built
at periodic.rs L739–742). It is **silent on `cmd_update`'s ordering** relative
to the `has_field` check (periodic.rs L518–537), which returns
`"at least one option must be provided"` before any network call today. If
resolution is wired in "at the top" (as the plan says for every handler,
L121–122) ahead of that check, a no-field `cbc periodic update <prefix>` will
perform a wasted list fetch and resolution before failing on the missing-field
error. Specify that `cmd_update` resolves **after** the `has_field` guard. Low
severity (wasted round-trip + the resolution error could mask the clearer
"provide an option" message).

### P7 — empty-prefix behavior not in the test matrix (low)

`match_periodic_prefix(tasks, "")` matches every task (`"".starts_with` is
universally true): unique-resolves with one task, ambiguous with ≥2. This is
inside the pure helper the plan already unit-tests, so it costs one extra case.
Add an empty-prefix case to the `match_periodic_prefix` matrix to pin the
behavior. Low severity; folds into existing test work.

---

## Testing plan assessment

- Commit 1 unit tests (min-unique: distinct→1, forced collision→2, single→1,
  empty handled; truncate: `n==1`→8 chars, oversize-`n`→whole id) cover the pure
  logic adequately. Good.
- Commit 2 tests `match_periodic_prefix` (0/1/many/full-UUID) — adequate for the
  matching half. **Gap (P5):** the 403-fallback decision, the locus of B1, is
  excluded from unit coverage on a false "untestable I/O" premise. Add P7's
  empty-prefix case to the same matrix.

---

## Convention / hygiene notes

### H1 — seq numbering: UNCHANGED BY DIRECTIVE (low)

The design/plan use a **3-digit** seq (`007`) while every existing cbc
design/plan/review uses **2-digit** (`00`–`06`). Per `seq-docs-convention` the
3-digit form is the documented rule, so the new docs are "more correct" than
their siblings but visually inconsistent within the directory. This persists by
the task's explicit directive (shared seq `007`, legacy review filename order).
It is not resolved and is not expected to be — flagged honestly as accepted, not
fixed. These v2 review files follow the same directed pattern
(`007-<ts>-<type>-periodic-short-ids-v2.md`).

### H2 — commit conventions correct

`cbc:` prefix is right (file under `cbsd-rs/cbc/`); both subjects are < 72 chars
and descriptive. DCO `-s`, no autonomous GPG, single `Co-authored-by`, separate
`add`/`commit` (plan L176–181) match `cbsd-rs/CLAUDE.md`. Good.

### H3 — no-`Cargo.toml` / no-`.sqlx` claims verified

Confirmed: no new dependency needed (no `uuid`; pure string ops) and no
`sqlx::query!` is touched (change is client-side only). The `detect_changes`
pre-commit step (plan L172–174) is appropriate; "no `.sqlx/` regeneration" is
correct.

---

## Confidence scoring (plan)

| Item                                                                 | Points |
| -------------------------------------------------------------------- | ------ |
| Starting score                                                       | 100    |
| D5: 403-fallback branch (locus of B1) untested on false premise (P5) | -15    |
| D11: `cmd_update` resolve-ordering unspecified (P6)                  | -5     |
| D11: empty-prefix case missing from test matrix (P7)                 | -5     |
| **Total**                                                            | **75** |

75/100 — acceptable with the noted fix; address P5 before the plan is executed.
The commit boundaries, sizing, rewrite-site enumeration, header fix, and
fallback correction are all sound; the deductions are the residual test gap on
the regression-prevention branch plus two low-severity specification omissions.

---

## Findings ordered by severity

1. **P5 (medium):** the 403-fallback decision — the exact branch that prevents
   the critical manage-without-view regression — is left manual-only on a false
   "untestable I/O" premise. Factor the decision into a `Result`-taking pure
   helper and unit-test it (no mock needed), or obtain explicit sign-off on
   manual-only coverage. (D5 PARTIALLY RESOLVED.)
2. **P6 (low):** specify that `cmd_update` resolves **after** the `has_field`
   guard so a no-field update does not trigger a wasted list fetch.
3. **P7 (low):** add an empty-prefix case to the `match_periodic_prefix` test
   matrix.
4. **H1 (low, by directive):** 3-digit/2-digit seq inconsistency persists by the
   task's directive — accepted, not fixed.

**Recommendation:** the commit structure and the B1/P2/P3 corrections are good;
close P5 (one pure helper + two assertions) and fold P6/P7 into the plan, then
implement.
