# Design Review v6: Optional VERSION on `cbsbuild versions create`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/005-20260504T1145-optional-version-on-versions-create.md`

**Prior reviews:**
`005-20260506T1000-design-optional-version-on-versions-create-v1.md` through
`005-20260508T1900-design-optional-version-on-versions-create-v5.md`

**Changes since v5:** Commit `339bf16` rewrote the `uuid_v7_timestamp()`
description paragraph in §Title generator, addressing MINOR-1 from the v5
review. The rewrite names `Timestamp::to_unix()` as the correct call path and
shows two construction routes to `chrono::DateTime<Utc>`.

---

## Summary Assessment

The v5 MINOR-1 fix landed correctly. Every API name in the revised paragraph —
`Uuid::get_timestamp()`, `Timestamp::to_unix()`,
`DateTime::<Utc>::from_timestamp()`, `DateTime::<Utc>::from_timestamp_millis()`
— exists in the locally-installed crate sources (uuid 1.22.0, chrono 0.4.44).
The argument types and return shapes match the prose. The arithmetic is correct
and overflow-safe for all plausible UUIDv7 timestamps. No new issues were found
on an independent sweep of the full document. Design 005 meets the bar for
approval.

---

## Verification of the v5 Fix

### MINOR-1 from v5 — `Timestamp::to_unix_millis()` does not exist in uuid 1.22

**Commit `339bf16` rewrites lines 455–460 of the design:**

```diff
-`uuid::Timestamp` for v6/v7/v1 inputs; convert with
-`Timestamp::to_unix_millis()` (returns `u64`) and feed the result to
-`chrono::DateTime::<Utc>::from_timestamp_millis()`. The alternative
-`Timestamp::to_unix()` returns `(seconds, nanoseconds)` and is also valid but
-requires more arithmetic.
+`uuid::Timestamp` for v6/v7/v1 inputs. In uuid 1.22, `Timestamp::to_unix()`
+returns `(u64 seconds, u32 subsec_nanos)`; construct the `chrono::DateTime<Utc>`
+via `chrono::DateTime::<Utc>::from_timestamp(secs as i64, nanos)`. If a millis
+form is preferred, derive it as `secs * 1_000 + u64::from(nanos) / 1_000_000`
+and pass it to `chrono::DateTime::<Utc>::from_timestamp_millis()`.
```

The following checks were performed against the locally-installed crate sources.
Each item is verified independently rather than trusting the prose.

#### 1. `Uuid::get_timestamp()` — uuid 1.22.0

Source: `uuid-1.22.0/src/lib.rs:892`

```rust
pub const fn get_timestamp(&self) -> Option<Timestamp>
```

Exists. Returns `Option<Timestamp>`. For a v7 UUID the `SortRand` branch fires
and returns `Some(Timestamp::from_unix_time(seconds, nanos, 0, 0))`. The
design's `uuid.get_timestamp().expect("v7 uuid has timestamp")` call is correct.
**Pass.**

#### 2. `Timestamp::to_unix()` — uuid 1.22.0

Source: `uuid-1.22.0/src/timestamp.rs:148`

```rust
pub const fn to_unix(&self) -> (u64, u32)
```

Exists. Returns `(self.seconds, self.subsec_nanos)`. The design describes this
as `(u64 seconds, u32 subsec_nanos)` — exactly right. **Pass.**

Note: `to_unix_millis()` (the method the prior text incorrectly named) does not
exist in the uuid crate at any version. `to_unix_nanos()` exists but is
deprecated since 1.2.0 and panics unconditionally. **The old text was wrong; the
replacement is right.**

#### 3. `DateTime::<Utc>::from_timestamp(secs as i64, nanos)` — chrono 0.4.44

Source: `chrono-0.4.44/src/datetime/mod.rs:803`

```rust
pub const fn from_timestamp(secs: i64, nsecs: u32) -> Option<Self>
```

Exists. Signature: `i64` for seconds, `u32` for nanoseconds. The design passes
`secs as i64` (a `u64 → i64` cast) and `nanos` (a `u32` already). The argument
types are correct. **Pass.**

The cast `secs as i64` is safe for all UUIDv7 timestamps: UUIDv7 stores 48 bits
of unsigned milliseconds since the Unix epoch, giving a maximum epoch of
approximately year 10,895. The maximum seconds value is
`2^48 / 1_000 ≈ 2.81 × 10^11`, which fits well within `i64::MAX`
(`≈ 9.22 × 10^18`). No truncation or sign-flip can occur. **Pass.**

The return type is `Option<Self>`. The design's pseudocode delegates the
`unwrap` / `.expect()` to the implementer, consistent with how every other code
block in the document elides error handling. This is acceptable at design-doc
level — the same convention applies to the resolver's `Uuid::now_v7()` and
`get_timestamp().expect(...)` calls throughout. **Pass.**

#### 4. `DateTime::<Utc>::from_timestamp_millis(millis)` — chrono 0.4.44

Source: `chrono-0.4.44/src/datetime/mod.rs:838`

```rust
pub const fn from_timestamp_millis(millis: i64) -> Option<Self>
```

Exists. Takes `i64`. The design derives the millis value as
`secs * 1_000 + u64::from(nanos) / 1_000_000` and casts it to `i64` in the
pseudocode's `millis as i64` call (as shown in the v5 review's code example).
The argument type is correct. **Pass.**

#### 5. Arithmetic correctness: `secs * 1_000 + u64::from(nanos) / 1_000_000`

With Rust's operator precedence (`*` and `/` bind tighter than `+`, both
left-to-right), this evaluates as:

```
(secs * 1_000) + (u64::from(nanos) / 1_000_000)
```

- `secs * 1_000`: converts whole seconds to milliseconds. **Correct.**
- `u64::from(nanos) / 1_000_000`: converts nanoseconds to the millisecond
  fraction (integer division, discards sub-millisecond precision). **Correct** —
  UUIDv7 itself only stores millisecond precision per RFC 9562 §5.7, so the
  truncation loses nothing.
- Sum: total Unix milliseconds. **Correct.**

**Overflow analysis:** `secs ≤ 2^48 / 1_000 ≈ 2.81 × 10^11`. Multiplied by
`1_000` gives `≈ 2.81 × 10^14`, which fits in `u64::MAX` (`≈ 1.84 × 10^19`) with
substantial headroom. Adding at most `999` (the millisecond fraction) does not
change this. The subsequent `as i64` cast is also safe: the maximum result
(`≈ 2.81 × 10^14`) is far below `i64::MAX` (`≈ 9.22 × 10^18`). **No overflow
concern. Pass.**

**Finding MINOR-1 (v5): CLOSED.** The replacement text names correct API names,
gives the correct return shape, and the arithmetic and type casts are sound.

---

## Independent Sweep

A full re-read of design 005 was performed with fresh eyes. No new issues were
found. For completeness, the items that have been clean across all prior passes
were re-confirmed:

- §Resolver pseudocode: `Uuid::now_v7()` exists in uuid 1.22.0 (`v7.rs:15`).
  `Uuid::new_v7(ts: Timestamp)` used in the unit test mention also exists
  (`v7.rs:63`). Both require the `v7` feature, which the §Cargo dep delta step
  correctly adds. **Pass.**
- §Patch walker: both match blocks remain structurally exhaustive (verified in
  v5). **Pass.**
- §Title generator return type (`Result<String, VersionError>`) consistent with
  callsite `?` propagation. **Pass.**
- OQ5–OQ8 dissolved correctly; no stale references. **Pass.**
- §Goals vs §Effects, §Migration table, §Non-Goals, §Resolved Decisions: all
  cross-section pointers consistent. **Pass.**

---

## Verdict

**No new findings. Zero open issues.**

Design 005 meets the bar for approval — **yes.**
