# Design Review v1: Optional VERSION on `cbsbuild versions create`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/005-20260504T1145-optional-version-on-versions-create.md`

**Prior reviews:** None — this is the first review.

---

## Summary Assessment

The design's goals are clear and the UUIDv7 choice is sound. The high-level
structure — optional positional, resolver, title-generator branch, no schema
change — is correct. However, two findings require attention before approval.
The first (CRITICAL) is that the patch-walker graceful-degradation claim is
factually wrong: the Python source does **not** suppress `MalformedVersionError`
from `get_major_version` / `get_minor_version`, so the Rust port cannot
"preserve this behaviour" without adding an explicit guard that the Python code
lacks. The second (IMPORTANT) is that the `do_version_title` sketch has an
uncompilable `?` operator in a function declared to return `String`.

The remaining checks — OCI tag compatibility, UUIDv7 collision safety, cross-
language interchange non-goal, clap positional unambiguity, `uuid` v7 feature
availability, `schema_version` correctness — all pass. The Python consumer
analysis for `cbc` and `crt` reaches the right conclusion but for partially
wrong reasons (see §Major Concerns for the nuance).

---

## Strengths

- **UUIDv7 choice is well-justified.** Chronological sort order (high-order
  timestamp bits per RFC 9562), 74 bits of randomness making collision
  practically impossible at CLI invocation rates, and zero server-side state are
  all correct.
- **OQ3 (CLI shape) is unambiguous.** `cbsbuild versions create` has one
  positional slot; clap's `Option<String>` on a positional is unambiguous when
  absent. The existing flags (`--type`, `--image-tag`, `--versions-dir`) are all
  named, so there is no parsing ambiguity between an absent positional and a
  flag argument. The design correctly notes this.
- **No schema change.** `desc.version` is a plain `String` field today and
  remains so. Producing a UUIDv7 value does not alter the field shape or
  serialisation. The "no `schema_version` bump" conclusion is correct.
- **OCI tag compatibility.** The OCI tag character set admits hyphens and the
  36-char hyphenated UUIDv7 form is well within the 128-char limit. The fallback
  to `VERSION` when `--image-tag` is not supplied works without any code change.
- **Cargo dep delta is correct.**
  `uuid = { version = "1", features = ["v4", "v7"] }`. The `v7` feature has been
  stable in `uuid` 1.x since 1.6.0 (released 2023). `Uuid::now_v7()` is gated
  behind the `v7` feature, which is what the design adds. The workspace already
  resolves uuid at 1.22.0. No new crate is introduced.
- **`resolve_version` is simple and correct.** Sync, infallible, one match arm.
  No IO, no errors to propagate.
- **Post-M1 scoping rationale is sound.** The two callsites that need changes
  (title generator and patch walker) require thought; deferring until M1 is
  stable reduces risk with no feature loss, since the positional is being
  _added_, not removed.

---

## Blockers

### B1 — Patch-walker graceful-degradation claim is factually wrong (CRITICAL)

**What the design says (§Patch walker, §Effects item 1):**

> The existing walker (`cbscore/builder/prepare.py:_get_patches_by_prio`)... For
> a UUIDv7 input, `parse_version()` returns `MalformedVersionError`; the walker
> treats that as "no major/minor/patch known" and applies only the top- level
> `patches/*.patch` files — exactly the desired graceful degradation.

**What the source actually does:**

Reading `cbscore/src/cbscore/builder/prepare.py:135–166`, the inner function
`_get_patches_by_prio(path, cur_prio, filter_version)` contains, at
`cur_prio > 0`:

```python
elif path.name == get_minor_version(filter_version):
    ...
elif path.name == get_major_version(filter_version):
    ...
```

`get_minor_version` and `get_major_version` in
`cbscore/src/cbscore/versions/utils.py:73–104` both call `parse_version(v)` and
**re-raise** `MalformedVersionError` on failure — they do not return `None` or a
sentinel. There is no `try/except` around these calls inside
`_get_patches_by_prio`. When `filter_version` is a UUIDv7 string,
`parse_version` raises `MalformedVersionError`, which propagates uncaught
through `_get_patches_by_prio`, through `_get_patch_list`, through
`_apply_patches`, and causes the build to fail with an unhandled exception.

**Why it matters:** The design's entire §Patch walker analysis rests on the
claim of pre-existing graceful degradation. If that is false, the Rust port
cannot "preserve this behaviour" — it needs to actively implement the guard.
More concretely: the migration table (Step 2, `cbscore/src/versions/mod.rs`)
does not list any patch-walker change; if the walker is deployed with a UUIDv7
version it will fail at build time, not at `versions create` time, giving a
confusing error.

**Direction for resolution:**

1. Correct the §Patch walker section: the Python walker does **not** currently
   handle `MalformedVersionError` gracefully; this is behaviour the Rust port
   needs to **add**, not preserve.
2. Update the migration table (Step 2 or a new Step) to explicitly mention
   adding a `MalformedVersion` guard around the `get_major_version` /
   `get_minor_version` calls in the Rust walker port.
3. The desired end-state (top-level patches only for UUIDv7 builds) is the right
   design decision; only the framing is wrong.

---

## Major Concerns

### M1 — `do_version_title` sketch is uncompilable (IMPORTANT)

**What the design shows:**

```rust
pub fn do_version_title(version: &str, version_type: VersionType) -> String {
    ...
    // Supplied-VERSION path: existing parse_version + format.
    let parsed = parse_version(version)?;  // ← ?-operator here
    format!("Release {type_desc} {}", parsed.title())
}
```

The `?` operator on the last `parse_version` call desugars to early-return with
the error value. For `?` to be valid, the function's return type must implement
`FromResidual<Result<_, MalformedVersion>>`. A bare `String` return type does
not. This sketch will not compile.

**Why it matters:** The sketch is a design-phase signal for the implementer; a
broken sketch misleads. The actual intent is clear, but the implementer will
discover the type error and may choose a solution (`.unwrap_or_else`, `expect`,
changing the return type to `Result`) that the design did not consider.

**Direction for resolution:** Change the function return type in the sketch to
`Result<String, MalformedVersion>` (or `Result<String, VersionError>`) and
update the callsite accordingly, or rewrite the last arm as an explicit `match`
with the error mapped to a fallback string. Either is acceptable; the design
should commit to one so the implementer has a clear target.

---

## Minor Issues

- **§Consumer parsing — `cbc` and `crt` analysis is mostly correct but partially
  imprecise.** The design says "external Python consumers (`cbc`, `crt`) call
  `parse_version()` against descriptor values." In practice: `crt` calls
  `parse_version(manifest.name)` at `crtlib/manifest.py:251` — but
  `manifest.name` is a `ReleaseManifest` name, not a `VersionDescriptor.version`
  field. `cbc`'s `_shared.py` does not call `parse_version` at all; it receives
  a `version` string as a parameter and passes it directly into
  `BuildDescriptor.version`. The conclusion ("UUIDv7 descriptors are not
  portable to Python consumers") is correct per design 002's
  no-cross-language-interchange policy, but the evidence cited is not fully
  accurate. No design change needed — the non-goal stands on policy, not on
  parse-call proximity — but the framing could be tightened in a future pass.

- **§Title generator — `ts.format("%Y-%m-%dT%H:%M:%SZ")` precision.** The design
  says the timestamp is extracted as "milliseconds since the Unix epoch" and the
  example title reads `2026-05-04T11:45:00Z`. The UUIDv7 timestamp has
  millisecond precision; the format string `%H:%M:%SZ` drops the sub-second
  component. This is an intentional trade-off (readable title), but the design
  does not acknowledge it. A note clarifying "seconds precision in the displayed
  title, milliseconds in the stored UUID" would prevent implementer confusion.

- **`uuid::Timestamp::to_unix_millis()` vs `to_unix()`.** The design says
  `Uuid::get_timestamp()` returns a `uuid::Timestamp` and describes it as
  providing the leading 48 bits as milliseconds. In `uuid` 1.x,
  `Timestamp::to_unix()` returns `(u64_seconds, u32_nanoseconds)` and
  `Timestamp::to_unix_millis()` returns `u64`. The sketch does not show the
  specific API call used inside `uuid_v7_timestamp()`, only that it returns a
  `chrono::DateTime<Utc>`. Since the helper is named but not expanded, this is
  not a blocker — but the implementer should use `to_unix_millis()` and
  construct a `chrono::DateTime<Utc>` from `DateTime::from_timestamp_millis(ms)`
  (or the equivalent). This is worth a one-line note.

---

## Suggestions

- **S1 — Name the `uuid_v7_timestamp` helper as a test target.** The timestamp-
  extraction helper is called out in §Design Sketch but not in §Testing. A
  direct unit test that constructs a known UUIDv7 (using `Uuid::new_v7` with a
  fixed `Timestamp`) and asserts the helper returns the expected `DateTime<Utc>`
  is cheap to write and prevents future regressions in the title format. Worth a
  line in the testing section.

- **S2 — Add a `versions list` note to §Operator UX.** §Sortability notes that
  UUIDv7 filenames sort chronologically in the filesystem. The operator UX
  section focuses on the printed `-> written to <path>` echo. A brief note that
  `ls -1 <root>/<type>/` provides chronological ordering for free would make the
  value of UUIDv7 concrete for operators who have accumulated multiple
  auto-derived descriptors.

---

## Open Questions

None from the eight declared OQs — all are resolved. The two issues above (B1,
M1) are new findings arising from source-level verification, not from the design
discussion.

---

## Verdict

**Changes required.**

B1 (patch walker claim is factually wrong) is CRITICAL and must be corrected
before approval. M1 (uncompilable `?` in a `-> String` function) is IMPORTANT
and should be corrected at the same time. Neither requires a design rethink —
only correction of the affected sections and the migration table.
