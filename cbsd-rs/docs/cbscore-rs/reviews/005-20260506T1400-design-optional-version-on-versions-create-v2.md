# Design Review v2: Optional VERSION on `cbsbuild versions create`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/005-20260504T1145-optional-version-on-versions-create.md`

**Prior reviews:**
`005-20260506T1000-design-optional-version-on-versions-create-v1.md`

**Changes since v1:** Seven commits addressed all v1 findings — d83509f (B1),
c8088d3 (M1), 163461f (MINOR-1), f3d9a76 (MINOR-2), e502473 (MINOR-3), 87bc884
(S1), 08e0dc1 (S2).

---

## Summary Assessment

Six of the seven v1 findings are cleanly and correctly closed. One new MINOR
issue was introduced by the B1 fix: the §Design Sketch opening paragraph was not
updated to reflect that the patch-walker guard is now an explicit fifth
implementation component, and the phrase "no patch-walker code" is now
incorrect. This is a documentation inconsistency only — the migration table
(Step 4) is unambiguous and correct. No rethinking required; a one-sentence
update to §Design Sketch closes it.

The design is otherwise ready for approval.

---

## Strengths

Everything from v1 that passed continues to pass: UUIDv7 choice, no schema
change, OCI tag compatibility, clap positional unambiguity, Cargo dep delta,
sync-and-infallible `resolve_version`. The seven fixes are well-executed.

---

## Verification of Seven v1 Findings

### B1 — Patch-walker graceful-degradation claim (CRITICAL → CLOSED)

**Read:** §Patches (lines 211–233), §Title closing paragraph (lines 304–309),
§Consumer parsing closing paragraph (lines 252–258), §Design Sketch › §Patch
walker (lines 473–499), Migration Step 4 (table row 4).

**§Patches** now opens with "The Python walker does **not** currently degrade
gracefully on a malformed VERSION" and states that "The Rust port therefore
**adds a guard**." This directly corrects the v1 claim that the Python walker
silently degraded.

**§Title closing paragraph** reads "This requires a small change in the Rust
port relative to the Python source — see §Design Sketch › §Patch walker."
Correctly frames the guard as new Rust behaviour.

**§Consumer parsing closing paragraph** ends: "each downstream site (currently
just the patch walker, item 1) gains a guard that treats the malformed case as
'skip' rather than propagating." Consistent.

**§Patch walker pseudocode** shows both arms: `Ok(mv) if ... => { /* match */ }`
and `Ok(_) | Err(MalformedVersion) => { /* skip */ }`. Both Ok-no-match and Err
are handled in a single arm; the semantics are correct.

**Migration Step 4** explicitly labels the patch-walker change as "**New
behaviour relative to the Python source**, which propagates the error through
`_apply_patches`."

**Finding: CLOSED.** The framing is now accurate throughout.

---

### M1 — Uncompilable `do_version_title` sketch (IMPORTANT → CLOSED)

**Read:** §Title generator code block (lines 424–452) and surrounding prose.

The function signature is now `-> Result<String, VersionError>`. The UUIDv7
branch returns `Ok(format!(...))`. The supplied-VERSION branch calls
`parse_version(version)?` (valid because `?` in a `Result<_, VersionError>`
context desugars correctly) and returns `Ok(format!(...))`. The prose notes the
error is unreachable in practice but says "the type signature stays honest." The
callsite is shown as `let title = do_version_title(&version, version_type)?;`.

No return path is missing a wrapping `Ok(...)`. No orphaned `?` operator.
Mentally compiles.

**Finding: CLOSED.**

---

### MINOR-1 — `cbc`/`crt` evidence imprecise (CLOSED)

**Read:** §Consumer parsing (lines 235–258) and §Non-Goals (lines 78–83).

The §Consumer parsing paragraph now reads: "External Python consumers (`cbc`,
`crt`) are not part of cbscore-rs's compatibility surface. Per design 002
§Python Coexistence — 'no cross-language file interchange' — operators run one
implementation per deployment; mixing UUIDv7 descriptors with Python `cbc`/`crt`
is not supported, **regardless of whether those tools call `parse_version()`
directly or pass `desc.version` through to other layers**."

The old "calls parse_version against descriptor values" claim is gone. The
paragraph correctly grounds the non-portability on the design 002 policy rather
than on parse-call proximity. The §Non-Goals item reads: "Per design 002 §Python
Coexistence, mixing Python and Rust against the same on-disk files is not
supported." Also correct.

**Finding: CLOSED.**

---

### MINOR-2 — Seconds-precision trade-off unacknowledged (CLOSED)

**Read:** §Title generator prose, lines 297–302.

The new paragraph reads: "The displayed timestamp is rendered at seconds
precision (`%H:%M:%SZ`), even though UUIDv7 stores millisecond precision. This
is a readability choice for the title — the full millisecond timestamp remains
in the UUID itself for any consumer that needs it, and chronological ordering is
unaffected (the tie-break for two UUIDv7s minted in the same second lives in the
random bits, not in the displayed seconds)."

The trade-off is stated honestly. The claim about the tie-break is correct: the
74 random bits following the 48-bit timestamp in the UUIDv7 layout per RFC 9562
§5.7 provide collision resistance and ordering within the same millisecond
window.

**Finding: CLOSED.**

---

### MINOR-3 — `uuid::Timestamp` API unspecified (CLOSED)

**Read:** §Title generator prose describing `uuid_v7_timestamp()`, lines
454–461.

The text now reads: "convert with `Timestamp::to_unix_millis()` (returns `u64`)
and feed the result to `chrono::DateTime::<Utc>::from_timestamp_millis()`. The
alternative `Timestamp::to_unix()` returns `(seconds, nanoseconds)` and is also
valid but requires more arithmetic."

Both the recommended path (`to_unix_millis()` → `from_timestamp_millis()`) and
the alternative (`to_unix()`) are named with correct signatures. The implementer
has an unambiguous call path.

**Finding: CLOSED.**

---

### S1 — `uuid_v7_timestamp` not named as a test target (CLOSED)

**Read:** §Title generator prose, lines 463–466.

"A unit test for `uuid_v7_timestamp` constructs a UUIDv7 from a fixed
`uuid::Timestamp` via `Uuid::new_v7(...)` and asserts the round-tripped
`chrono::DateTime<Utc>` matches — cheap to write and pins the title format
against future regressions."

Clear, accurate, and located at the point of the helper's description. The test
construction technique (`Uuid::new_v7` with a fixed timestamp) is the correct
way to build a deterministic UUIDv7 for a round-trip test.

**Finding: CLOSED.**

---

### S2 — No `ls -1` note in §Operator UX (CLOSED)

**Read:** §Operator UX (lines 326–348).

The new paragraph at lines 344–348 reads: "A practical side benefit:
`ls -1 <root>/<type>/` returns descriptors in chronological creation order
without needing `-t` (mtime) or any custom sort, because UUIDv7 strings sort
lexicographically by their leading 48-bit timestamp (per §Sortability).
Operators accumulating multiple auto-derived descriptors can pick the most
recent one with a plain `ls | tail -1`."

The §Sortability cross-reference is explicit and the claim is correct.

**Finding: CLOSED.**

---

## New Finding

### MINOR-N1 — §Design Sketch opening paragraph contradicts Step 4

**What the design says (lines 366–371):**

> The change consists of one CLI-shape edit, one resolver helper, one branch in
> the title generator, and one Cargo-feature add. No new config field, no new
> flag, no schema change, **no patch-walker code (item 1 is graceful degradation
> of the existing walker)**.

**Why this is now wrong:**

The B1 fix (d83509f) correctly added Step 4 to the migration table:

> In the Rust port of `_get_patches_by_prio`, treat `Err(MalformedVersion)` from
> `get_minor_version` / `get_major_version` as "skip this subdirectory" rather
> than propagating. **New behaviour relative to the Python source.**

Step 4 is explicit Rust code — a guard added to the walker — not a preservation
of existing Python behaviour. The phrase "no patch-walker code" is therefore
factually incorrect, and the parenthetical rationalisation "item 1 is graceful
degradation of the existing walker" is precisely what B1 corrected elsewhere in
the document.

The sentence also lists four implementation components ("one CLI-shape edit, one
resolver helper, one branch in the title generator, and one Cargo-feature add"),
but the migration table now has five steps. Step 4 is a fifth component.

**Why it matters:** The §Design Sketch opening paragraph is the first sentence
most implementers read before the subsections. An implementer who stops there
would incorrectly believe no patch-walker change is needed, contradicting
Step 4.

**Direction for resolution:** Update the opening sentence to name five
components and replace "no patch-walker code (item 1 is graceful degradation of
the existing walker)" with something accurate, for example:

> The change consists of one CLI-shape edit, one resolver helper, one branch in
> the title generator, one patch-walker guard, and one Cargo-feature add. No new
> config field, no new flag, no schema change.

This is a one-sentence edit with no design impact; the implementation guidance
in Step 4 and §Patch walker is unambiguous and correct.

---

## Minor Issues

None beyond MINOR-N1 above.

---

## Suggestions

None.

---

## Open Questions

None. All eight declared OQs remain resolved.

---

## Verdict

**One minor correction required before final approval.**

MINOR-N1 (§Design Sketch opening paragraph contradicts the patch-walker guard
added by the B1 fix) must be corrected. It is a one-sentence fix with no
design-level impact. Once landed, this design is ready to approve.
