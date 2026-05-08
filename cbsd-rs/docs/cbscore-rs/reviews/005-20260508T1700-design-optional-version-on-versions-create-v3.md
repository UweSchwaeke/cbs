# Design Review v3: Optional VERSION on `cbsbuild versions create`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/005-20260504T1145-optional-version-on-versions-create.md`

**Prior reviews:**
`005-20260506T1000-design-optional-version-on-versions-create-v1.md`,
`005-20260506T1400-design-optional-version-on-versions-create-v2.md`

**Changes since v2:** None. Design 005 is unchanged on disk since the v2 review.

---

## Summary Assessment

One new MINOR finding: the patch walker sketch has a type mismatch between the
return type of `get_minor_version` (defined in design 002 as
`Result<Option<String>, MalformedVersion>`) and how the first `Ok(mv)` arm uses
`mv.as_str()`, which is not a method on `Option<String>`. The fix is a one-line
pattern correction.

The previously-identified open item from v2 remains open (§Design Sketch opening
paragraph, "Open from T1400 v2 — still unaddressed" below).

Both are MINOR documentation issues; the implementation guidance in the rest of
the document (§Patch walker prose, Migration Step 4) is correct and unambiguous.
The design is otherwise complete and sound. Once these two items are corrected,
design 005 is ready for approval.

---

## Open from T1400 v2 — still unaddressed

**MINOR-T1400-N1 — §Design Sketch opening paragraph contradicts Step 4**

The §Design Sketch opening paragraph (lines 368–371) still reads:

> The change consists of one CLI-shape edit, one resolver helper, one branch in
> the title generator, and one Cargo-feature add. No new config field, no new
> flag, no schema change, **no patch-walker code (item 1 is graceful degradation
> of the existing walker)**.

This contradicts Migration Step 4 (which is explicit Rust code — a new guard in
the patch walker — not a preservation of existing Python behaviour) and lists
four components when the migration table has five. The resolution was documented
in v2: update the sentence to name five components and replace the incorrect
parenthetical. Still unaddressed.

---

## New Finding

### MINOR-1 — Patch walker sketch has a type mismatch on `get_minor_version`

**What the design says (§Design Sketch › §Patch walker):**

```rust
match get_minor_version(filter_version) {
    Ok(mv) if path.file_name() == Some(mv.as_str()) => { /* match: descend */ }
    Ok(_) | Err(MalformedVersion) => { /* skip this subdirectory */ }
}
```

**Why this does not compile:**

Design 002 (§Version Descriptors & Creation) defines:

```rust
pub fn get_minor_version(v: &str) -> Result<Option<String>, MalformedVersion>;
```

The return type is `Result<Option<String>, MalformedVersion>`, not
`Result<String, MalformedVersion>`. In the `Ok(mv)` arm, `mv` therefore has type
`Option<String>`. `Option<String>` has no `.as_str()` method; the method exists
on `String` and `&str`, not on `Option<_>`. A Rust compiler will reject this
with something like:

```
error[E0599]: no method named `as_str` found for enum `Option<String>`
```

The correct pattern for the first arm is:

```rust
Ok(Some(mv)) if path.file_name() == Some(mv.as_str()) => { /* match */ }
```

With this correction, `Ok(None)` (patch component present, patch subdirectory
absent — i.e. minor is missing) naturally falls into the `Ok(_)` skip arm, which
is the correct behaviour: a valid but incomplete version string (major only, no
minor) cannot match a version-specific subdirectory.

**Why it matters:** An implementer copying the sketch verbatim will get a
compiler error. The fix is mechanical and has no semantic impact — the intent is
clear — but the sketch as written is not compilable.

**Direction for resolution:** Change the first arm to
`Ok(Some(mv)) if path.file_name() == Some(mv.as_str())`. No other change is
needed; the second arm (`Ok(_) | Err(MalformedVersion)`) correctly captures
`Ok(None)` as well as `Err`.

---

## Verification of Seven v1 Findings

All seven v1 findings were verified closed in v2 and remain closed. No
regression on any of them since the design is unchanged.

---

## Cross-Section Consistency Check

The following checks were performed fresh for this pass:

- **§Goals vs §Effects:** "Operators who continue passing an explicit VERSION
  see no behaviour change" (§Goals item 2) is consistent with §Consumer parsing,
  §Patch walker prose, and Migration Step 5 (gate `validate_version` on
  `args.version.is_some()`). Consistent.

- **§Goals vs §Migration table:** §Goals lists no new config field, no new flag,
  no schema change. Migration Steps 1–5 match: Step 1 is a Cargo feature add,
  Step 2 is a helper addition, Step 3 is a function branch, Step 4 is a guard,
  Step 5 is a CLI shape change. No step adds a config field or schema bump.
  Consistent.

- **OQ5–OQ8 vs §Effects subsections:** OQ7 (image tag) cross-references item 5
  of §Effects; item 5 (§Image tag) says the OCI tag fallback works as-is. OQ8
  (schema) cross-references item 7; item 7 (§Schema / wire format) confirms no
  bump. All four dissolved OQs have their resolution grounded in the correct
  §Effects subsection. Consistent.

- **§Design Sketch › §Resolver vs §Goals item 1:** `resolve_version` returns the
  UUIDv7 string when `cli` is `None`. The §Goals says descriptors land at
  `<root>/<type>/<UUIDv7>.json`. §Resolver returns the string; the call site in
  Migration Step 5 uses it as the `version` variable that eventually feeds
  `descriptor_path()` from design 004. The chain is complete. Consistent.

- **§Title generator return type vs callsite:** `do_version_title` returns
  `Result<String, VersionError>`. Callsite is
  `let title = do_version_title(&version, version_type)?;`. The `?` propagates
  `VersionError` to the handler's error type, which must accommodate
  `VersionError`. Migration Step 5 says the handler is in
  `cbsbuild/src/cmds/versions.rs`. `cbsbuild` uses `anyhow` at the binary
  boundary (design 001 §cbsbuild Cargo sketch), so `?` on `VersionError` is
  valid via `anyhow::Error`'s blanket `From<E: Error>`. Consistent.

---

## Verdict

**Two MINOR corrections required before final approval.**

1. **MINOR-T1400-N1** (open from v2, still unaddressed): §Design Sketch opening
   paragraph still says "no patch-walker code" and lists four components; the
   migration table has five. One-sentence fix.

2. **MINOR-1** (new in this pass): §Patch walker sketch uses `Ok(mv)` where
   `mv: Option<String>` and calls `.as_str()` on it, which does not compile.
   Change to `Ok(Some(mv))`. One-line fix.

Both are documentation issues only; the prose description and migration table
are correct in both cases. Once both are fixed, design 005 meets the bar for
approval.
