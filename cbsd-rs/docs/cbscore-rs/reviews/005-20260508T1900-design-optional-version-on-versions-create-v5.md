# Design Review v5: Optional VERSION on `cbsbuild versions create`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/005-20260504T1145-optional-version-on-versions-create.md`

**Prior reviews:**
`005-20260506T1000-design-optional-version-on-versions-create-v1.md`,
`005-20260506T1400-design-optional-version-on-versions-create-v2.md`,
`005-20260508T1700-design-optional-version-on-versions-create-v3.md`,
`005-20260508T1800-design-optional-version-on-versions-create-v4.md`

**Changes since v4:** Commit `d120e47` adds `Ok(_) |` to the major-version
match's catch-all arm, addressing MINOR-1 from the v4 review.

---

## Summary Assessment

The v4 MINOR-1 fix landed exactly as directed. Both patch-walker match blocks
now have the same exhaustive structure. One new MINOR was found on this pass —
`Timestamp::to_unix_millis()` is cited in the `uuid_v7_timestamp()` description
but does not exist in uuid 1.22.0 (the workspace-locked version). The correct
method is `Timestamp::to_unix()`, which returns `(u64, u32)`. Everything else
remains clean. The design is very close to the bar for approval; the one-line
prose correction below is all that stands in the way.

---

## Verification of the v4 Fix

### MINOR-1 from v4 — Major-version match exhaustiveness

**Commit `d120e47` diff (single line, design line 487):**

```diff
-    Err(MalformedVersion) => { /* skip this subdirectory */ }
+    Ok(_) | Err(MalformedVersion) => { /* skip this subdirectory */ }
```

**Current §Patch walker pseudocode (lines 478–489):**

```rust
// get_minor_version returns Result<Option<String>, MalformedVersion>.
match get_minor_version(filter_version) {
    Ok(Some(mv)) if path.file_name() == Some(mv.as_str()) => { /* descend */ }
    Ok(_) | Err(MalformedVersion) => { /* skip this subdirectory */ }
}
// get_major_version returns Result<String, MalformedVersion>.
match get_major_version(filter_version) {
    Ok(mv) if path.file_name() == Some(mv.as_str()) => { /* descend */ }
    Ok(_) | Err(MalformedVersion) => { /* skip this subdirectory */ }
}
```

**Structural verification:**

- Minor-version match: guarded `Ok(Some(mv))` arm +
  `Ok(_) | Err(MalformedVersion)` catch-all. `Ok(None)` falls into `Ok(_)`.
  Exhaustive. **Pass.**
- Major-version match: guarded `Ok(mv)` arm + `Ok(_) | Err(MalformedVersion)`
  catch-all. A well-formed VERSION whose major component does not match the
  directory name falls into `Ok(_)`. Exhaustive. **Pass.**
- Both catch-alls are structurally identical (`Ok(_) | Err(MalformedVersion)`).
  The only intentional difference between the two blocks is the `Option` layer
  in the minor-version guarded arm — `Ok(Some(mv))` vs plain `Ok(mv)`. **Pass.**

**Type-check against design 002:**

- `get_major_version` declared at design 002:686 as
  `Result<String, MalformedVersion>`. The major-version match's `Ok(mv)` binds a
  `String` directly, consistent with the return type. `.as_str()` is valid.
  **Pass.**
- `get_minor_version` declared at design 002:690 as
  `Result<Option<String>, MalformedVersion>`. The minor-version match's
  `Ok(Some(mv))` unwraps one `Option` layer before binding `mv: String`.
  `.as_str()` is valid. **Pass.**

**Finding MINOR-1 (v4): CLOSED.**

---

## Independent Sweep

### MINOR-1 — `Timestamp::to_unix_millis()` does not exist in uuid 1.22.0

**What the design says (lines 455–460):**

> The `uuid` crate exposes the timestamp via `Uuid::get_timestamp()` returning a
> `uuid::Timestamp` for v6/v7/v1 inputs; convert with
> `Timestamp::to_unix_millis()` (returns `u64`) and feed the result to
> `chrono::DateTime::<Utc>::from_timestamp_millis()`.

**What the API actually provides:**

In uuid 1.22.0 (the workspace-locked version in `cbsd-rs/Cargo.lock`),
`uuid::Timestamp` exposes:

- `to_unix() -> (u64, u32)` — seconds and subsecond nanoseconds
- `to_unix_nanos() -> u32` — deprecated since 1.10.0
- `to_gregorian() -> (u64, u16)` — 100-nanosecond ticks since UUID epoch

`to_unix_millis()` is not present. An implementer who follows the design
literally will hit a compilation error:

```
error[E0599]: no method named `to_unix_millis` found for struct
`uuid::Timestamp`
```

**Correct approach** using `to_unix()`:

```rust
fn uuid_v7_timestamp(uuid: &Uuid) -> chrono::DateTime<Utc> {
    let ts = uuid.get_timestamp().expect("v7 uuid has timestamp");
    let (secs, nanos) = ts.to_unix();
    let millis = secs * 1_000 + u64::from(nanos) / 1_000_000;
    chrono::DateTime::<Utc>::from_timestamp_millis(millis as i64)
        .expect("valid uuid timestamp")
}
```

Or more directly without manual millis arithmetic, using
`DateTime::from_timestamp(secs as i64, nanos)`.

**Why this passed four prior passes:** The v1 review (MINOR-3) flagged the
`uuid_v7_timestamp` internals as unspecified and suggested naming the API. The
v2 fix added the specific method name `to_unix_millis()`, which the v2 reviewer
accepted as naming "correct signatures." The actual existence of the method
against the locked crate version was not independently verified in v2, v3, or
v4.

**Direction for resolution:** Replace the `to_unix_millis()` sentence with the
correct `to_unix()` call path. For example:

> convert with `Timestamp::to_unix()` (returns `(u64_seconds, u32_nanos)`),
> compute milliseconds as `secs * 1_000 + u64::from(nanos) / 1_000_000`, and
> feed the result to `chrono::DateTime::<Utc>::from_timestamp_millis()`.

The alternative note on `Timestamp::to_unix()` already in the text ("also valid
but requires more arithmetic") should be updated to reflect that this _is_ the
primary path, not the alternative.

---

## Cross-Section Consistency Sweep

All checks from the v4 cross-section sweep re-run and remain clean:

- **§Goals vs §Effects:** consistent.
- **§Goals vs §Migration table:** five steps, no new config field, no schema
  change — consistent.
- **OQ5–OQ8 vs §Effects subsections:** all four dissolved OQs grounded correctly
  — consistent.
- **§Resolver vs §Goals item 1:** chain intact — consistent.
- **§Title generator return type vs callsite:** `Result<String, VersionError>`,
  `?` at the binary boundary — consistent.
- **§Patches prose vs §Patch walker pseudocode:** both say
  `Err(MalformedVersion)` triggers skip, only top-level patches apply for UUIDv7
  — consistent.
- **Preamble (five components) vs Migration Step 4:** consistent.
- **No stale references** to the old non-exhaustive major-version match shape or
  to "no patch-walker code" language anywhere in the document.

---

## Verdict

**One minor correction required before final approval.**

MINOR-1 (`Timestamp::to_unix_millis()` does not exist in uuid 1.22.0): the
referenced method name is wrong; the implementer will hit a compile error. The
fix is a one-sentence prose update naming `to_unix()` as the correct path. Once
corrected, design 005 meets the bar for approval.
