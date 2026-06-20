# Implementation Review: 007 — Periodic Short IDs (v1)

**Design:** `docs/cbc/design/007-20260619T2132-periodic-short-ids.md` **Plan:**
`docs/cbc/plans/007-20260619T2133-periodic-short-ids.md` **Commits in scope:**
`6fdcda44` (display) and `1ff3a271` (resolution). The third recent commit
`6dac2828` is docs-only and out of scope. **Reviewer mandate:** adversarial;
independent re-derivation against source. No trust in the implementer, the
design, or the three prior (design/plan) reviews. **Scope:** one file —
`cbsd-rs/cbc/src/periodic.rs`.

---

## Summary

The implementation matches design 007 and plan 007 exactly, with no deviations.
Both commits compile (verified at HEAD), `cargo test -p cbc` passes all 33 tests
(17 of them the new periodic tests), `cargo clippy --all-targets -D warnings` is
clean, and `cargo fmt --check` is clean. Every load-bearing decision carried
from the prior review rounds — the required 403 fallback for manage-without-view
callers, full-id display in ambiguity errors, and `delete` echoing the resolved
id before the destructive call — is honored in code, each re-verified by my own
reading rather than on the prior reviews' authority.

**Verdict: GO.** No functional defect, no spec deviation, no convention
violation, no untested critical path. The score is 100/100. The findings below
are explicitly dispositioned non-deductions, recorded to show the traps were
examined, not missed.

---

## Verification performed (re-derived in code)

Read in full and cross-checked against the diffs and the design/plan claims:
`cbc/src/periodic.rs`, `cbc/src/worker.rs` (the cited precedent),
`cbc/src/client.rs`, `cbc/src/error.rs`, `cbc/Cargo.toml`,
`cbsd-server/src/routes/periodic.rs`, `cbsd-server/src/db/periodic.rs`, and the
v3 design/plan reviews.

Empirically verified in this environment:

- **All 33 cbc tests pass** (`cargo test --manifest-path .../cbc/Cargo.toml`,
  `SQLX_OFFLINE=true`): the 8 display tests (`truncate_*`, `min_unique_*`) and
  the 9 resolution tests (`resolve_*`) all green.
- **Clippy clean** with `--all-targets -- -D warnings` (covers the test module):
  zero warnings. Backs a clean D4 (idiomatic) assessment.
- **Format clean** with `cargo fmt --check` (exit 0). Backs a clean D10.

Confirmed true in code (not taken from the design or prior reviews):

- **403 surfaces as `Error::Api { status: 403, .. }`.** `client.rs::request`
  maps any non-success response to
  `Err(Error::Api { status: status.as_u16(), message })` (L232–250); `get`
  routes through `request` (L107). The match arm
  `Err(Error::Api { status: 403, .. }) => return Ok(prefix.to_string())` fires
  on a real 403.
- **Cap gating is exactly as relied upon.** `list_tasks` (routes L435) and
  `get_task` (L475) gate on `periodic:view`. `update_task` (L529), `delete_task`
  (L694), `enable_task` (L754), `disable_task` (L814) gate on `can_manage_task`
  = `periodic:manage:any` OR (`periodic:manage:own` AND owner) (L142–145) — none
  of the four checks `periodic:view`. The manage-without-view shape is real and
  server-tested (`no_manage_cap_denies_even_for_own_task`). So the 403 fallback
  is **required**, not cosmetic: without it a manage-only caller would lose
  mutation by any id. Re-derived independently of the v3 B1 disposition.
- **Superset invariant holds.** `db::list_tasks` has no owner filter
  (`... ORDER BY created_at`, db L116–135); `get_task` is `WHERE id = ?` (db
  L70–113). Every task `get` can return is present in `list`, so display and
  resolution can use independent granularity and stay consistent.
- **No new dependency.** `cbc/Cargo.toml` has no `uuid` dep; the helpers are
  pure string ops. The design non-goal is accurate.
- **No raw `args.id` survives at any request/fetch/message site.**
  `grep args.id` returns exactly five hits — all of the form
  `resolve_periodic_id(&client, &args.id)`, i.e. the raw input feeding
  resolution. Every `periodic/{id}` URL, the update existing-task fetch, the
  PUT/DELETE, and all five success/echo messages read the resolved `id`.

---

## Plan coverage

| Plan item                                                | Status      |
| -------------------------------------------------------- | ----------- |
| C1: `truncate_components` (n>=1, byte-slice at hyphen)   | Implemented |
| C1: `min_unique_components` (1..=5, cap 5)               | Implemented |
| C1: `cmd_list` computes `n`, sizes ID column to width    | Implemented |
| C1: captured width on **both** header and data rows      | Implemented |
| C1: display unit tests (distinct/collision/single/empty) | Implemented |
| C2: `resolve_from_fetch` pure helper (all branches)      | Implemented |
| C2: `resolve_periodic_id` thin async wrapper             | Implemented |
| C2: `cmd_get` resolves before the GET                    | Implemented |
| C2: `cmd_update` resolves **after** `has_field` guard    | Implemented |
| C2: `cmd_delete` resolves **after** the `--yes` gate     | Implemented |
| C2: `cmd_delete` echoes resolved id before deleting      | Implemented |
| C2: `cmd_enable` / `cmd_disable` resolve before the PUT  | Implemented |
| C2: resolution unit tests (0/1/many, 403, non-403, "")   | Implemented |

Nothing deferred, nothing missing.

## Design fidelity

- Error wording matches the design: `no periodic task matching '<prefix>'`,
  `ambiguous task id '<prefix>' matches:` + full candidate ids, both wrapped by
  `Error::Other` whose `Display` prefixes `error: ` (error.rs L29).
- Ambiguity error lists **full** ids (periodic.rs L283), per the design's
  rationale that an ambiguous prefix shares its leading run so truncated
  candidates would render identically. (Note: this is a small, conscious
  divergence from `resolve_worker_id`, which truncates candidates to 12 chars —
  correct here precisely because periodic ambiguity implies a shared prefix.)
- 403 fallback returns the input verbatim (L267), uniform across all five
  commands; for `get` it is a harmless no-op (the subsequent GET would itself
  403), for the four manage commands it preserves the full-UUID path.
- `delete` prints `deleting periodic task {id}` before the destructive call
  (L834), then `periodic task {id} deleted` after — the design's defensive UX.

---

## Commit boundary assessment (git-commits smell test)

**Commit 1 — `cbc: minimum-viable short ids in periodic list`** (~114 added).

Applying the head-on question — _what can a user DO after this commit that they
could not before?_ — the honest answer is narrow: see short IDs that are
**guaranteed distinct**, fixing a first-8-hex collision that the design itself
calls astronomically unlikely. On its own that is a small, defensive change in a
sub-200-LOC commit, and most of its day-to-day value only lands once commit 2
makes the printed prefix copy-pasteable. It nonetheless clears the "meaningful
in isolation" bar for two concrete reasons: (a) it is a real correctness fix —
the old `&task.id[..8]` could print two identical, unresolvable rows, a latent
bug independent of commit 2; and (b) it is the _display_ feature, a clean seam
from the _resolution_ feature (the helpers are not shared — `cmd_list` uses
`truncate_components`/`min_unique_components`; resolution uses `starts_with`).
Split-by-feature, not split-by-layer. Smell test: one-sentence purpose ✓;
compiles ✓ (verified at HEAD, and the diff adds no reference to any commit-2
symbol); revertable without touching resolution ✓; testable (8 new tests) ✓;
**no dead code** — every helper is consumed by `cmd_list` or a test in the same
commit ✓.

> _Independence caveat (honest):_ `git worktree`/`checkout` are denied in this
> environment, so commit 1's standalone compile was **verified by diff
> analysis** (the added helpers are consumed in-commit by `cmd_list`; the test
> module is self-contained; no commit-2 symbol is referenced), **not** by
> building the parent in isolation. HEAD (both commits) builds and tests clean.

**Commit 2 — `cbc: accept periodic task id prefixes`** (~163 added).

One sentence: every id-taking subcommand accepts an unambiguous prefix. Compiles
✓; revertable (reverting restores full-UUID-only behavior, leaving commit 1's
display intact) ✓; testable (9 new tests covering every `resolve_from_fetch`
branch) ✓; no dead code — `resolve_from_fetch` is called by
`resolve_periodic_id`, which is called by all five handlers and exercised by
tests ✓. Depends on nothing from commit 1 (different helpers), and lands second
because the display change is what makes prefixes worth copying — correct
ordering.

Both messages follow Ceph style (`cbc:` prefix, imperative subject under 72
cols, body explaining the why), carry exactly one `Co-authored-by` and a DCO
`Signed-off-by`, and describe **intent** ("accept prefixes", "minimum-viable
short ids") rather than structure.

---

## Code-level traps examined (non-findings)

1. **`&id[..idx]` char-boundary safety (no defect).** `match_indices('-')`
   matches the ASCII byte `-`; the byte offset it yields is always at that
   single-byte hyphen, which is necessarily a UTF-8 char boundary. `&id[..idx]`
   therefore cannot panic on a non-boundary slice — even for a non-UUID-shaped
   id containing multibyte chars, since the cut is always at an ASCII hyphen.

2. **`n - 1` underflow in `truncate_components` (no defect).** In release mode
   `n == 0` would wrap `n - 1` to `usize::MAX`; `.nth(usize::MAX)` returns
   `None` → the whole id (wrong, but not a panic). It is guarded: the sole
   caller (`cmd_list`) always passes `min_unique_components(...) ∈ 1..=5`, and
   the `debug_assert!(n >= 1)` documents and enforces the contract in debug
   builds. The invariant holds via the single caller; no reachable underflow.

3. **`min_unique_components` edge cases (no defect).** Empty slice → the `all()`
   over an empty iterator is `true` at `n == 1`, returns 1 (tested). Identical
   ids never separate → the `1..=5` loop exhausts and the `5` fallback returns
   (tested). Single id → 1 (tested). All correct.

4. **Captured-width formatting (no defect).** `"{:<id_width$}"` is used on
   **both** the header (L446) and every data row (L462) with the same
   `id_width`, so the table stays aligned for `n > 1`. `id_width` derives from
   `max` of the rendered lengths with `unwrap_or(8)` — and the empty-task case
   returns before this code, so the `unwrap_or` is belt-and-suspenders. IDs are
   ASCII, so byte length equals display width; the padding is correct.

5. **D2 — resolution helpers resemble `resolve_worker_id` (no deduction).** Same
   fetch → 403-fallback → `starts_with` → 0/1/many shape, but different item
   types (`PeriodicListItem` vs `WorkerInfo`) and return shapes (`String` vs
   `(String, String)` with an `"unknown"` sentinel). Deduplicating would require
   generifying over a trait and editing out-of-scope `worker.rs`. The design
   scopes only the _resolution_ half as mirroring the precedent and the
   _display_ half as net-new (`worker list` shows names, not truncated ids,
   which I confirmed). Conscious non-defect, re-derived rather than inherited
   from the v3 disposition.

6. **N8 — empty-prefix no-match renders `matching ''` (no deduction).** An
   explicit `cbc periodic get ""` against an empty list yields
   `no periodic task matching ''`. Slightly odd but harmless, and matches the
   established `resolve_worker_id` behavior; clap requires the positional
   argument, so it only arises if `""` is passed explicitly.

---

## Confidence scoring

| Item                                                    | Points  |
| ------------------------------------------------------- | ------- |
| Starting score                                          | 100     |
| D1 deferred work (none — full plan coverage)            | 0       |
| D2 duplication (resolve helpers — conscious, see #5)    | 0       |
| D4 non-idiomatic (clippy `-D warnings` clean)           | 0       |
| D5 untested critical path (17 unit tests, all branches) | 0       |
| D6 dead code (every symbol consumed in-commit)          | 0       |
| D8 spec deviation (matches design 007 / plan 007)       | 0       |
| D10 convention (fmt clean, `cbc:` prefix, DCO, 72-col)  | 0       |
| D12 commit boundary (both pass the smell test)          | 0       |
| **Total**                                               | **100** |

100/100 — ready to merge. The display/resolution split is justified
feature-by-feature, every critical path is unit-tested, both commits build and
lint clean, and no functional, security, or convention defect was found on an
independent pass.

---

## Findings ordered by severity

1. **D2 (no deduction):** resolution helpers structurally resemble
   `resolve_worker_id`; consciously not deducted — different item/return types,
   dedup would touch out-of-scope `worker.rs`.
2. **N8 (no deduction):** empty-prefix no-match renders `matching ''`; harmless,
   matches the `resolve_worker_id` precedent.
3. **Process note (not a finding):** commit 1's standalone compile was verified
   by diff analysis, not an isolated build, because `git worktree`/`checkout`
   are denied here. HEAD builds and tests clean.

**Recommendation: GO.** The implementation is correct, complete, well-tested,
and faithful to the approved design and plan. No required actions before
proceeding.
