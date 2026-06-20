# Plan Review: 007 — Periodic Short IDs (v1)

**Document:** `docs/cbc/plans/007-20260619T2133-periodic-short-ids.md`
**Design:** `docs/cbc/design/007-20260619T2132-periodic-short-ids.md`

**Reviewer mandate:** adversarial; commit boundaries assessed with the
`git-commits` skill smell test.

---

## Summary

The two-commit plan is well-structured, confined to one file, and split by
**capability** (display vs. resolution) rather than by layer — it is not the
dead-code / layer-by-layer anti-pattern. Both commits pass the smell test and
land independent, testable value. The split is a defensible judgment call rather
than a requirement (both are sub-200 LOC on one theme, so a single combined
commit would be equally valid).

The plan inherits design B1 (the 403-fallback regression) and additionally
under-specifies the exhaustive set of `args.id` rewrite sites and the list
header width — both of which would produce a subtly wrong implementation if
followed literally.

**Verdict: NO-GO until design B1 is resolved and P1/P2 are addressed.** The
commit sizing/boundaries themselves are fine.

---

## Commit boundary assessment (git-commits smell test)

### Commit 1 — `cbc: minimum-viable short ids in periodic list`

| Smell test           | Result                                             |
| -------------------- | -------------------------------------------------- |
| One-sentence purpose | Yes — "list shows resolvable min-viable short ids" |
| Previous compiles    | Yes — no dependency on commit 2                    |
| Revertable           | Yes — isolated to `cmd_list` + two helpers         |
| Testable             | Yes — pure helpers unit-tested in-commit           |
| No dead code         | Yes — `cmd_list` consumes both helpers same commit |

Verified the helpers have a caller in the same commit: `cmd_list` calls
`min_unique_components` + `truncate_components`. No `#[allow(dead_code)]`
needed. Clean. **No D12.**

### Commit 2 — `cbc: accept periodic task id prefixes`

| Smell test           | Result                                        |
| -------------------- | --------------------------------------------- |
| One-sentence purpose | Yes — "id-taking subcommands accept a prefix" |
| Previous compiles    | Yes — commit 1 stands alone                   |
| Revertable           | Yes — adds `resolve_periodic_id` + call sites |
| Testable             | Yes — ambiguity branch unit-tested            |
| No dead code         | Yes — helper called by all five handlers      |

Clean. **No D12.**

### Over-fragmentation check

The plan itself admits "both are well under the 400-LOC floor" (~70 and ~90).
Per `git-commits`, sub-200 commits warrant the question "is this meaningful
alone?" Both _are_ (display is a visible, revertable improvement; resolution is
a separate capability), and the plan's stated rationale — independent
revertability, no cross-dependency, minimal blast radius — is legitimate. This
is a **minor judgment call, not a violation**: merging into one
`cbc: resolvable short periodic task ids` commit (~160 LOC, one file, one theme)
would be equally defensible and arguably simpler to review. Either choice is
acceptable; flag as preference, no deduction.

---

## Plan-specific findings

### P1 — Inherits design B1 (the 403-fallback regression)

Commit 2's content says: "No 403 fallback (cap parity with `list`)." This
encodes the design's incorrect premise directly into the implementation spec.
Four of the five wired commands gate on `periodic:manage`, not `periodic:view`,
so cap parity does not hold (see design review B1 for full evidence: `has_cap`
is flat, `can_manage_task` never checks `view`, manage-without-view is
server-tested and supported via custom roles in migration 008). If implemented
as written, a `periodic:manage`-only user loses the ability to mutate tasks by
any id. Must be corrected in lockstep with the design fix. High severity
(carried from B1).

### P2 — `args.id` rewrite sites are under-enumerated

The plan says "resolve once at the top, then use the resolved full UUID for
every URL, fetch, and printed message," but the prose does not enumerate every
site, and several are easy to miss. The exhaustive list in `periodic.rs` today:

- `cmd_get`: one site — the GET URL (L398).
- `cmd_update`: **three** — existing-task fetch (L586), the PUT URL (L721), and
  the success message `"periodic task {} updated"` (L726). The plan mentions the
  message but the implementer must not leave L726 on `args.id`.
- `cmd_delete`: **two** — the DELETE URL (L747) **and** the final
  `"periodic task {} deleted"` message (L748). The plan calls out the pre-delete
  echo and the delete itself but does not flag that L748 today prints the
  prefix; it must print the resolved id.
- `cmd_enable`: two — the enable URL (L765) and message (L767).
- `cmd_disable`: two — the disable URL (L784) and message (L786).

Recommend the plan include this table so "resolve once" is verifiably
exhaustive. Medium severity — a partial rewrite leaves user-visible output
echoing the prefix, contradicting the design's "visible resolved target" goal.
(Note `cmd_update` also issues a `whoami` GET at L591 when descriptor fields
change — unrelated to id, no rewrite needed.)

### P3 — `cmd_list` header width not addressed

Commit 1's content says it "derives the ID column width from the displayed
length so the table stays aligned when `n > 1`," but the header row is a
separate `println!` with a hard-coded `{:<10}` for `ID` (periodic.rs L357–360).
If the body width becomes dynamic but the header stays `{:<10}`, the table
misaligns for `n > 1`. The plan must state the header uses the same computed
width. Low severity (only triggers on a first-component collision, which is the
astronomically-rare path), but it is exactly the path commit 1 exists to make
correct, so it should not itself misrender.

### P4 — `delete` echo ordering vs `--yes-i-really-mean-it` is safe but unstated

`cmd_delete` checks `--yes-i-really-mean-it` and returns early **before**
constructing the client (periodic.rs L739–742). The plan's "resolve, echo the
resolved full UUID before deleting, then delete" is safe: resolution happens
after the confirmation gate, so a missing flag never triggers a network fetch,
and the echo lands between resolution and the destructive call. Worth one
sentence confirming resolve happens _after_ the confirmation check so the gate
is not bypassed. Verified correct; no deduction.

---

## Testing plan assessment

- Commit 1 unit tests (min-unique: distinct→1, forced collision→2, single→1,
  empty handled; truncate: `n==1`→8 chars, oversize-`n`→whole id) cover the pure
  logic adequately. Good.
- Commit 2 tests the `resolve_periodic_id` ambiguity branch (0/1/many). **Gap:**
  the plan does not test the no-fallback-vs-403 behavior, which is the exact
  locus of B1. Whatever fallback decision is made for P1 must have a test. Add a
  test asserting the chosen behavior on a list-403.
- "Manual end-to-end" is fine as a supplement but is not a substitute for the
  403 unit coverage above.

---

## Convention / hygiene notes

### H1 — seq numbering inconsistent with siblings (low)

The new design/plan use a **3-digit** seq (`007`) while every existing cbc
design/plan uses **2-digit** (`00`–`06`); the reviews directory likewise uses
2-digit (`04-…-design-v1-…`). Per the `seq-docs-convention` skill the 3-digit
form (`001`–`999`) is the documented rule, so the new docs are "more correct"
than their siblings — but they are now visually inconsistent within the same
directory. This is inherited from the docs under review, not introduced by this
review; flag for the author to decide whether to backfill siblings to 3-digit or
accept the mix. These review files follow the task's explicit directive: shared
seq `007`, legacy review filename order `<seq>-<ts>-<type>-<title>-v<N>.md`.

### H2 — commit message component prefix is correct

`cbc:` is the right prefix (file under `cbsd-rs/cbc/`), and both proposed
subjects are under 72 chars and descriptive. The plan's commit-convention
section (DCO `-s`, no autonomous GPG, single `Co-authored-by`, separate
`add`/`commit`) matches `cbsd-rs/CLAUDE.md`. Good.

### H3 — `detect_changes` / no-`.sqlx` claims verified plausible

The plan asserts no `Cargo.toml`/sqlx change. Confirmed: no `uuid` (or any) new
dependency is needed, and no `sqlx::query!` is touched (the change is
client-side only). The GitNexus pre-commit step is appropriate.

---

## Confidence scoring (plan)

| Item                                                           | Points |
| -------------------------------------------------------------- | ------ |
| Starting score                                                 | 100    |
| D7: encodes the B1 fallback regression as spec (P1)            | -20    |
| D1: `args.id` rewrite sites under-enumerated → partial rewrite | -5     |
| D5: no test for the 403/fallback behavior at locus of B1       | -15    |
| D10: header width not specified; misaligns at `n>1` (P3)       | -5     |
| D11: seq-numbering inconsistency unaddressed (H1)              | -5     |
| **Total**                                                      | **50** |

50/100 — significant issues; must address before proceeding. The boundary and
sizing of the commits are sound; the deductions are entirely about the inherited
B1 defect, the under-specified rewrite/header details, and the missing 403 test
— all fixable without re-architecting the plan.

---

## Findings ordered by severity

1. **P1 (critical, inherited B1):** the "no 403 fallback / cap parity"
   instruction regresses `periodic:manage`-only users. Fix in lockstep with the
   design; add a test for the chosen behavior.
2. **P2 (medium):** enumerate all `args.id` → resolved-id sites (`update` ×3,
   `delete` ×2 incl. L748 message, `enable`/`disable` ×2 each) so "resolve once"
   is exhaustive and no user-facing output echoes the prefix.
3. **P3 (low):** specify that `cmd_list`'s header uses the same computed width
   as the body, or the table misaligns on the `n>1` path.
4. **H1 (low):** 3-digit vs 2-digit seq inconsistency with siblings — author
   decision.
5. **P4, H2, H3 (verified clear):** delete-echo ordering is safe; commit
   conventions and no-dependency/no-sqlx claims hold.

**Recommendation:** the commit structure is good; revise the design (B1),
propagate the fallback fix and the rewrite-site/header/test details into the
plan, then implement.
