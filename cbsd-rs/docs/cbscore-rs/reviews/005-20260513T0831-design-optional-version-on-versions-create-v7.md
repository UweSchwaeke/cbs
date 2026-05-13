# Design Review v7: Optional VERSION on `cbsbuild versions create`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/005-20260504T1145-optional-version-on-versions-create.md`

**Prior reviews:**
`005-20260506T1000-design-optional-version-on-versions-create-v1.md` through
`005-20260508T2000-design-optional-version-on-versions-create-v6.md`

**Changes since v6:** Commit `345083f` amended the patch-walker behaviour from
silent-skip to warn-and-skip when VERSION is a UUIDv7. Three subsections were
touched:

- **§Patches: only top-level apply** — added warn behaviour and pinned message
  format.
- **§Consumer parsing** — closing paragraph changed "treats the malformed case
  as 'skip'" to "'warn-and-skip'".
- **§Design Sketch › §Patch walker** — rewrote schematic Rust: hoisted the
  malformed-version signal out of the per-subdir loop
  (`let version_is_malformed = minor.is_err() && major.is_err();`) and added the
  `tracing::warn!` branch with `TARGET_BUILDER_PATCHES`.

---

## Scope

This is a focused confirmation pass on the warn-and-skip amendment only. The
prior v6 review approved the full document; only the delta introduced by commit
`345083f` is under scrutiny. The six verification tasks are:

1. Schematic correctness of the §Patch walker rewrite.
2. Warn message string format.
3. `TARGET_BUILDER_PATCHES` constant provenance.
4. Cross-reference consistency across the three amended subsections.
5. No drift elsewhere in the doc (regression check for silent-skip residue).
6. Existing design 005 invariants untouched (§Status, schema, Non-Goals, OQ3 CLI
   shape).

---

## Method

- Read the amended design 005 in full.
- Read design 001 §Crate Responsibilities (lines 194–219) and plan
  `002-20260508T1558-01-types.md` (Commit 2 §Files, lines 143–145) for
  `TARGET_BUILDER_PATCHES` provenance.
- Read the Python source `cbscore/src/cbscore/builder/prepare.py` for the logger
  hierarchy used by the Python patch walker.
- Read design 002 for the tracing-target naming convention
  (`cbscore::builder::prepare`).
- Diffed all silent-skip / warn-and-skip occurrences in the design against the
  amended §Patch walker semantics.

---

## Closed Findings Confirmed

All findings from v1 through v6 remain closed. MINOR-1 (v5, uuid API name) was
verified closed in v6 and is unchanged by the amendment.

---

## Findings

### MINOR-1 — `TARGET_BUILDER_PATCHES` is not named in design 001 or plan 002

**Severity:** MINOR

**Where:** §Design Sketch › §Patch walker, lines 503 and 520–521.

```
target: TARGET_BUILDER_PATCHES,
…
The `TARGET_BUILDER_PATCHES` constant comes from `cbscore-types::logger`
(Phase 1 of the cbscore-rs port)…
```

**What design 001 actually says:** Design 001 §Crate Responsibilities (line 218)
lists the tracing target hierarchy as
`"cbscore", "cbscore::runner", "cbscore::builder", …` — an illustrative,
open-ended list terminated with `…`. The file `cbscore-types/src/logger.rs` is
specified to carry `pub const TARGET_*: &str` constants (plan
`002-…-01-types.md`, Commit 2, line 144), but the exact set of constant names,
including `TARGET_BUILDER_PATCHES`, is not enumerated anywhere in either
document.

Design 002 names the per-stage tracing targets textually as
`cbscore::builder::prepare`, `cbscore::builder::rpmbuild`, etc. (§Build
Pipeline, line 879), but does not name Rust constant identifiers for them.

**Why it matters:** The schematic references a specific Rust identifier
(`TARGET_BUILDER_PATCHES`) that is not yet defined in any Phase 1 plan or design
document. An implementer reading the schematic could reasonably understand the
intent as `cbscore::builder::prepare` (per design 002 §Build Pipeline) or as a
coarser `cbscore::builder` constant. The schematic does not make this choice,
leaving the identifier opaque without a cross-reference.

Additionally, the tracing macro in the schematic passes `subdir = %name` as a
structured field and also interpolates `name` positionally in the format string.
This logs the subdirectory name twice per event, which is redundant in
structured logging output.

**Direction:** Two equally acceptable resolutions — design 005 should choose
one:

1. **Replace the opaque identifier with the string literal** the implementation
   will use, e.g. `target: "cbscore::builder::prepare"`, and note that a
   constant for this string will live in `cbscore-types::logger`. This is
   consistent with how design 002 names the targets throughout.
2. **Remove the `target:` field from the schematic entirely** and add a prose
   note: "the warn is emitted under the `cbscore::builder::prepare` tracing
   target (the existing builder/patches target per design 002 §Build Pipeline),
   using whatever constant Phase 1 defines for that string in
   `cbscore-types::logger`." This decouples the design sketch from an
   implementation detail that no prior document has fixed.

The redundant double-logging of `name` should also be cleaned up: either keep
the structured field (`subdir = %name`) and drop the positional interpolation,
or vice versa.

---

### MINOR-2 — Migration table step 4 still says "skip this subdirectory" without mentioning warn

**Severity:** MINOR

**Where:** §Migration › §Code, table row 4 (`cbscore/src/builder/prepare.rs`):

> treat `Err(MalformedVersion)` from `get_minor_version` / `get_major_version`
> as "skip this subdirectory" rather than propagating.

The amendment added warn-and-skip language to §Patches and §Consumer parsing and
rewrote §Patch walker to match. The Migration table was not updated. As written,
an implementer reading only the Migration table would implement silent-skip —
the exact behaviour the amendment was intended to replace.

**Why it matters:** The Migration table is a specification artefact as much as
the prose sections; it is the concrete "what to code" checklist. Divergence
between the table and the prose creates ambiguity about what must be
implemented.

**Direction:** Update table row 4 to read:

> treat `Err(MalformedVersion)` from `get_minor_version` / `get_major_version`
> as "warn-and-skip" rather than propagating — emit a `tracing::warn!` per
> skipped version-keyed subdir (per §Patch walker).

---

## Verification Results

### Task 1 — Schematic correctness

**One warn per skipped subdir, no double-warn.** Correct. The precondition
`version_is_malformed` is hoisted before the loop and evaluated once. The
`tracing::warn!` fires only inside the `else if version_is_malformed` branch,
which is exclusive with the `matches_minor || matches_major` branch. A single
version-keyed subdir produces exactly one warn. **Pass.**

**Parseable VERSION + name mismatch is silently skipped.** Correct. When
`version_is_malformed` is `false` (both extractors returned `Ok`), the `else if`
branch is unreachable; the final `else` is the only remaining path, and the
schematic explicitly labels it "parseable VERSION + name mismatch — silently
skip (existing behaviour)". **Pass.**

**`version_is_malformed = minor.is_err() && major.is_err()` precondition.**
Correct. `get_minor_version` and `get_major_version` both delegate to
`parse_version` (Python: `cbscore/versions/utils.py`), which applies a single
regex. Either both fail on the same input (any non-parseable string, including
UUIDv7) or both succeed. The `&&` conjunction is sound: it correctly
distinguishes the "neither extractor can produce a result" case from every other
case. A UUIDv7 fails both extractors uniformly; an `Ok(None)` from
`get_minor_version` (patch-level-only version with no minor component) is a
success, not an error. **Pass.**

### Task 2 — Warn message string format

The format
`"skipping version-keyed patch subdir '<name>' — VERSION is a UUIDv7, no version match possible"`
is specific enough for operator log scanning: it names the subdir, names the
root cause (UUIDv7 VERSION), and states the consequence (no match possible). The
`<name>` substitution is `subdir.file_name()`, which is a plain filename
component, not a full path. Subdir names are matched against parsed version
components (major / major-minor strings) and so are constrained in practice to
version-like strings (e.g. `19`, `19.2`); shell-special characters are not
expected. The format is adequate. **Pass** (modulo the redundant double-log
noted under MINOR-1).

### Task 3 — `TARGET_BUILDER_PATCHES` constant provenance

Not named in design 001 or plan 002. See MINOR-1 above.

### Task 4 — Cross-reference consistency across the three amended subsections

**§Patches (item 1) ↔ §Consumer parsing (item 2).** §Consumer parsing closes
with "each downstream site (currently just the patch walker, item 1) gains a
guard that treats the malformed case as 'warn-and-skip' rather than
propagating." This matches §Patches exactly. **Pass.**

**§Patches ↔ §Patch walker.** §Patches says: "emits a `tracing::warn!` once per
skipped version-keyed subdirectory … See §Design Sketch › §Patch walker for the
precise change." §Patch walker delivers that precisely. **Pass.**

**§Consumer parsing ↔ §Patch walker.** §Consumer parsing says the guard "treats
the malformed case as 'warn-and-skip'". §Patch walker implements warn-and-skip.
**Pass.**

All three subsections describe the same behaviour without contradiction.

### Task 5 — No drift elsewhere (silent-skip residue)

A full-document grep for "silent" and "silently" found two occurrences:

1. Line 511 of the schematic code comment:
   `// parseable VERSION + name mismatch — silently skip (existing behaviour)`.
   This is correct — this branch is the supplied-VERSION / name-mismatch path,
   which does not involve a UUIDv7 and was never changed by the amendment. **Not
   a drift issue.**
2. Line 528: "so the operator sees the skip in the log rather than being
   silently denied a patch they put on disk." This is in §Patch walker's
   explanatory prose and is a positive statement about the warn — correct in
   context. **Not a drift issue.**

The §Title generator subsection (lines 309–314) references patches with
"subdirectory matches fail by definition … per-major and per-minor-patch
subdirectories are **unreachable** for UUIDv7 builds … see §Design Sketch ›
§Patch walker." "Unreachable" here means "will not be descended into", which is
compatible with warn-and-skip. The sentence cross-refers to §Patch walker for
the precise mechanism. **Not a drift issue.**

No remaining silent-skip implication was found outside the Migration table (see
MINOR-2).

### Task 6 — Existing design 005 invariants (regression check)

- **§Status (post-M1 scope).** Unchanged: "This design is intentionally out of
  M1 scope." **Pass.**
- **§Non-Goals (no Python-side changes).** Unchanged. **Pass.**
- **§Non-Goals (no schema_version bump).** Unchanged. **Pass.**
- **OQ3 (CLI shape — optional positional).** Unchanged.
  `version: Option<String>`, `cbsbuild versions create [OPTIONS] [VERSION]`.
  **Pass.**
- **OQ8 / §Schema (no wire-format change).** Unchanged. **Pass.**

---

## Verdict

**Approve with revisions.**

Two MINOR findings require a targeted fix before implementation:

- **MINOR-1:** Replace the undefined `TARGET_BUILDER_PATCHES` identifier in the
  §Patch walker schematic with either the string literal
  `"cbscore::builder::prepare"` (the target design 002 names for this module)
  plus a note that a `logger.rs` constant will hold it, or remove the `target:`
  field from the schematic and describe the target in prose. Also remove the
  redundant double-log of `name` (structured field `subdir = %name` and
  positional `{}` both present).
- **MINOR-2:** Update Migration table row 4 to say "warn-and-skip" to match the
  §Patches and §Patch walker prose.

The warn-and-skip semantics themselves are correctly and consistently specified
in the amended subsections. The schematic logic (one warn per subdir, no
double-warn, silent-skip preserved for parseable VERSION) is sound. No blocking
issues were found.
