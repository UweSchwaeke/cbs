# Design Review v8: Optional VERSION on `cbsbuild versions create`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/005-20260504T1145-optional-version-on-versions-create.md`

**Prior reviews:**
`005-20260506T1000-design-optional-version-on-versions-create-v1.md` through
`005-20260513T0831-design-optional-version-on-versions-create-v7.md`

**Changes since v7:** Commit `51b02c0` closed both v7 MINOR findings. Changes
are confined to one file — the design doc itself. MINOR-1 closure: the schematic
in §Design Sketch › §Patch walker now uses the string literal
`target: "cbscore::builder::prepare"` in place of the undefined
`TARGET_BUILDER_PATCHES` identifier; the redundant `subdir = %name` structured
field has been removed; and a prose paragraph was added explaining that the
literal matches design 002 §Build Pipeline's naming and that Phase 1's
`cbscore-types::logger` will define the matching `pub const TARGET_*: &str`.
MINOR-2 closure: Migration table row 4 now says **warn-and-skip** and includes a
pointer to §Design Sketch › §Patch walker.

---

## Scope

This is a focused confirmation pass on the two v7 MINOR closures. The v7 review
approved the full document; only the delta introduced by commit `51b02c0` is
under scrutiny. The five verification tasks are:

1. **MINOR-1 — target literal.** No `TARGET_BUILDER_PATCHES` residue; literal
   matches design 002 §Build Pipeline; prose explains the decoupling.
2. **MINOR-1 — double-log.** `subdir = %name` structured field is gone; name is
   now logged once via positional interpolation only.
3. **MINOR-2 — Migration table row 4.** Row text says "warn-and-skip" and
   includes a §Patch walker pointer.
4. **No drift.** No remaining "silently skip" / "silent skip" implication that
   conflicts with warn-and-skip. Design 005 invariants from v7 untouched.
5. **No regression.** All findings from v1 through v7 remain closed.

---

## Method

- Read the amended design 005 in full.
- Grep for `TARGET_BUILDER_PATCHES`, all `silent` occurrences, all
  `warn-and-skip` occurrences, and the `target:` line in the schematic.
- Verified the string literal `"cbscore::builder::prepare"` against design 002
  §Build Pipeline line 879.
- Read Migration table row 4 directly for the updated language.

---

## Closed Findings Confirmed

All findings from v1 through v7 remain closed. No finding reopened by the
amendment.

---

## Findings

None.

---

## Verification Results

### Task 1 — MINOR-1: target literal

**No `TARGET_BUILDER_PATCHES` residue.** A full-file grep for
`TARGET_BUILDER_PATCHES` returns zero matches. **Pass.**

**Literal is correct.** The schematic at line 503 reads:

```
target: "cbscore::builder::prepare",
```

Design 002 §Build Pipeline (line 879) lists this module's tracing target as
`cbscore::builder::prepare`. The literal matches exactly. **Pass.**

**Prose explanation is adequate.** Lines 519–524 of the design state:

> The target string `"cbscore::builder::prepare"` matches the naming design 002
> §Build Pipeline uses for this module; Phase 1 of the cbscore-rs port defines
> the matching `pub const TARGET_*: &str` in `cbscore-types::logger`. The
> schematic uses the string literal directly to stay decoupled from Phase 1's
> exact constant identifier; the warn line carries the existing target hierarchy
> so `CBS_DEBUG` filtering keeps working unchanged.

This adequately explains why the schematic uses a literal rather than a constant
reference, and points the implementer to where the constant will be defined.
**Pass.**

### Task 2 — MINOR-1: double-log removed

A full-file grep for `subdir = %name` and `subdir=%name` returns zero matches.
The `tracing::warn!` call in the schematic now logs the subdir name via the
positional `{}` interpolation in the message string only — once per event.
**Pass.**

### Task 3 — MINOR-2: Migration table row 4

Row 4 of the Migration §Code table now reads (line 557):

> In the Rust port of `_get_patches_by_prio`, treat `Err(MalformedVersion)` from
> `get_minor_version` / `get_major_version` as **warn-and-skip** rather than
> propagating — emit a `tracing::warn!` per skipped version-keyed subdir (see
> §Design Sketch › §Patch walker for the schematic). **New behaviour relative to
> the Python source**, which propagates the error through `_apply_patches`.
> Required for UUIDv7 builds to terminate in `_apply_patches` cleanly.

The row explicitly says "warn-and-skip" and includes a pointer to §Design Sketch
› §Patch walker. The remaining four rows are unchanged and do not contradict the
warn-and-skip semantics. **Pass.**

### Task 4 — No drift

All four `silent`/`silently` occurrences in the file are benign:

- Line 232: "rather than a silent drop" — positive statement in §Patches
  explaining that the warn-and-skip makes the omission visible. Not a
  contradiction.
- Line 501:
  `// apply is not a silent omission. One warn per skipped subdir per walk.` —
  code comment in the schematic reinforcing the warn behaviour. Not a
  contradiction.
- Line 510:
  `// parseable VERSION + name mismatch — silently skip (existing behaviour)` —
  this is the _other_ branch of the walker (supplied, parseable VERSION + subdir
  name mismatch), which was never changed and is correctly described as silent.
  Not a drift issue.
- Line 530: "so the operator sees the skip in the log rather than being silently
  denied a patch they put on disk" — explanatory prose in §Patch walker; again a
  positive statement about the warn. Not a contradiction.

No occurrence of "silently skip" or "silent skip" applies to the UUIDv7 path.
**Pass.**

Design 005 invariants from v7 are all intact:

- **§Status (post-M1 scope).** Unchanged: "This design is intentionally out of
  M1 scope." **Pass.**
- **§Non-Goals (no Python-side changes).** Unchanged. **Pass.**
- **§Non-Goals (no schema_version bump).** Unchanged. **Pass.**
- **OQ3 (CLI shape — optional positional).** Unchanged:
  `version: Option<String>`, `cbsbuild versions create [OPTIONS] [VERSION]`.
  **Pass.**
- **OQ8 / §Schema (no wire-format change).** Unchanged: `desc.version` stays a
  plain string field; no `schema_version` bump on any wire format. **Pass.**

### Task 5 — No regression in earlier findings

All findings from v1 through v7 remain closed. The amendment touches only the
three areas identified in the v7 MINOR findings; no other section of the
document was modified.

---

## Verdict

**Approve — v7 MINOR-1 + MINOR-2 closed; no new findings.**

Both v7 MINOR findings are cleanly resolved:

- `TARGET_BUILDER_PATCHES` is gone; the schematic uses the string literal
  `"cbscore::builder::prepare"` with adequate prose explaining the decoupling
  from the Phase 1 constant.
- The double-log of the subdir name is gone.
- Migration table row 4 says "warn-and-skip" and points to §Patch walker.

The document is consistent, internally complete, and ready for implementation.
