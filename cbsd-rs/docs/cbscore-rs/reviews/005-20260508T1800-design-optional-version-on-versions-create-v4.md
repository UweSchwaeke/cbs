# Design Review v4: Optional VERSION on `cbsbuild versions create`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/005-20260504T1145-optional-version-on-versions-create.md`

**Prior reviews:**
`005-20260506T1000-design-optional-version-on-versions-create-v1.md`,
`005-20260506T1400-design-optional-version-on-versions-create-v2.md`,
`005-20260508T1700-design-optional-version-on-versions-create-v3.md`

**Changes since v3:** Commit `1c179e5` addresses both open items from the v2 and
v3 reviews:

1. **§Design Sketch opening paragraph (MINOR-T1400-N1):** updated to name five
   components and remove the "no patch-walker code" phrase.
2. **§Patch walker pseudocode (MINOR-1 from v3):** the minor-version match now
   uses `Ok(Some(mv))` and the two matches are structurally separated with
   type-naming comments.

---

## Summary Assessment

Both targeted fixes landed correctly. The preamble now accurately counts five
components and is consistent with Migration Step 4 and the §Patch walker
subsection. The minor-version match is now type-correct against the design 002
signature.

However, one new MINOR was introduced by the same fix: the major-version match
was restructured but is still non-exhaustive — the `Ok(mv)` arm carries a guard
and `Err(MalformedVersion)` is the only other arm, leaving `Ok(mv)` with a
non-matching filename unhandled. This is the same structural issue the v3 fix
corrected in the minor-version match, applied incompletely. The fix is one line.

The design is otherwise complete and sound.

---

## Verification of the Two Targeted Fixes

### Fix 1 — §Design Sketch opening paragraph (MINOR-T1400-N1)

**Current text (lines 367–372):**

> The change consists of one CLI-shape edit, one resolver helper, one branch in
> the title generator, one patch-walker guard, and one Cargo-feature add. No new
> config field, no new flag, no schema change.

**Checks:**

- Five components named: CLI-shape edit, resolver helper, title-generator
  branch, patch-walker guard, Cargo-feature add. **Pass.**
- "No patch-walker code" phrase: absent. **Pass.**
- Consistent with Migration Step 4 ("In the Rust port of `_get_patches_by_prio`,
  treat `Err(MalformedVersion)` … as 'skip …'"): **Pass.**
- Consistent with §Patch walker subsection (which describes the guard in
  detail): **Pass.**

**Finding MINOR-T1400-N1: CLOSED.**

---

### Fix 2 — §Patch walker pseudocode (MINOR-1 from v3)

**Current code (lines 478–489):**

```rust
// get_minor_version returns Result<Option<String>, MalformedVersion>.
match get_minor_version(filter_version) {
    Ok(Some(mv)) if path.file_name() == Some(mv.as_str()) => { /* descend */ }
    Ok(_) | Err(MalformedVersion) => { /* skip this subdirectory */ }
}
// get_major_version returns Result<String, MalformedVersion>.
match get_major_version(filter_version) {
    Ok(mv) if path.file_name() == Some(mv.as_str()) => { /* descend */ }
    Err(MalformedVersion) => { /* skip this subdirectory */ }
}
```

**Checks against the five required criteria:**

(a) Minor-version match uses `Ok(Some(mv))`: **Pass.** `mv` now has type
`String`, so `.as_str()` is valid.

(b) Catch-all `Ok(_) | Err(MalformedVersion)` handles both `Ok(None)` and the
malformed case: **Pass.** `Ok(None)` is a `Ok(_)` match; `Err` is explicit.

(c) Major-version match is structurally distinct (separate block, no `Option`
layer): **Pass.** `get_major_version` returns
`Result<String, MalformedVersion>`; the major-version match is a separate
`match` expression with no `Option` wrapper, and a preceding comment names the
return type.

(d) Both matches type-check against design 002:686 and design 002:690:
**Minor-version — Pass.** Comment says
`Result<Option<String>, MalformedVersion>`, matching design 002:690.
**Major-version — Pass for the comment**, but the match body itself is
non-exhaustive (see MINOR-1 below).

(e) Comments preceding each match name the return types accurately: **Pass** for
both. The comments are present and correct.

**Finding MINOR-1 (v3): CLOSED** on criterion (a). A new MINOR is raised on
criterion (d) / exhaustiveness below.

---

## New Finding

### MINOR-1 — Major-version match is non-exhaustive

**What the design says (lines 484–488):**

```rust
// get_major_version returns Result<String, MalformedVersion>.
match get_major_version(filter_version) {
    Ok(mv) if path.file_name() == Some(mv.as_str()) => { /* descend */ }
    Err(MalformedVersion) => { /* skip this subdirectory */ }
}
```

**Why this does not compile:**

`get_major_version` returns `Result<String, MalformedVersion>`. The possible
values are:

- `Ok(mv)` where the filename guard holds → descend (arm 1)
- `Ok(mv)` where the filename guard does **not** hold → neither arm covers this
- `Err(MalformedVersion)` → skip (arm 2)

The second case — a well-formed VERSION whose major component does not match the
current subdirectory name (the common case when walking a patch tree with
multiple per-major subdirectories) — is unhandled. Rust's exhaustiveness checker
will reject this with:

```
error[E0004]: non-exhaustive patterns: `Ok(_)` not covered
```

The minor-version match was correctly fixed in this commit with
`Ok(_) | Err(MalformedVersion)` as the skip arm. The same correction was not
applied to the major-version match.

**Direction for resolution:** Add `Ok(_)` to the skip arm:

```rust
match get_major_version(filter_version) {
    Ok(mv) if path.file_name() == Some(mv.as_str()) => { /* descend */ }
    Ok(_) | Err(MalformedVersion) => { /* skip this subdirectory */ }
}
```

This is a one-line fix, identical in structure to what was correctly done for
the minor-version match.

---

## Cross-Section Consistency Sweep

All cross-section checks from the v3 review were re-run and remain clean:

- **§Goals vs §Effects:** consistent (no regression).
- **§Goals vs §Migration table:** five steps, no new config field, no schema
  change — consistent.
- **OQ5–OQ8 vs §Effects subsections:** all four dissolved OQs grounded in the
  correct §Effects entries — consistent.
- **§Resolver vs §Goals item 1:** chain from `resolve_version` →
  `descriptor_path` (design 004) is intact — consistent.
- **§Title generator return type vs callsite:** `Result<String, VersionError>`,
  `?` via `anyhow` at the binary boundary — consistent.
- **§Patches prose vs §Patch walker pseudocode:** both say
  `Err(MalformedVersion)` triggers skip and only top-level patches apply for
  UUIDv7 — consistent.
- **Preamble vs Migration Step 4:** five components, patch-walker guard named —
  consistent (fix 1 verified above).

No stale references to the old "no patch-walker code" language were found
elsewhere in the document.

---

## Verdict

**One minor correction required before final approval.**

MINOR-1 (major-version match non-exhaustive): the `Ok(_)` skip arm is missing
from the major-version `match` block, leaving `Ok(mv)` with a non-matching guard
unhandled. The fix is one line, structurally identical to the correct
minor-version match fix landed in this same commit. Once corrected, design 005
meets the bar for approval.
