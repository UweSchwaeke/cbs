# seq-005 — Optional VERSION on `cbsbuild versions create` (UUIDv7 default)

## Status

**Implemented.** Lands on top of the M2 release as a backwards-compatible
additive change against the M1-1.0.0 baseline — operators who keep passing an
explicit VERSION see no behaviour change, operators who drop the positional get
a UUIDv7 descriptor at `<root>/<type>/<UUIDv7>.json` with a
`created at <timestamp>` title. cbscore-rs stays at 1.0.0 (no crate-version bump
for additive features; same posture as seq-004). Implements design 005
(`design/005-20260504T1145-optional-version-on-versions-create.md`); the two
design–code mismatches recorded as Open Questions below were resolved during the
planning phase and are now reflected in both this plan and design 005's
§Resolver / §Patch walker / §Migration table.

**Review trail:**

- Plan drafted 2026-05-21 against design 005 v1.
- OQ-A resolved 2026-05-21: match Python regex behaviour on the supplied-VERSION
  path, accept UUIDv7 as an explicit exception by adding `validate_version` to
  `cbscore::versions::utils`.
- OQ-B resolved 2026-05-22: keep the existing per-walk patch-walker warn
  implementation (already in `cbscore/src/builder/prepare.rs:258–266` since
  seq-002 Phase 5 Commit 1); amend design 005 §Patch walker to match the shipped
  shape. seq-005 has no patch-walker work.
- v2 design-review pass (2026-05-22) — design-reviewer agent verdict "approve
  with MINOR cleanups". Findings closed: MAJOR-1 (design called the title
  function `do_version_title` but the live code is `make_title` — design
  rewritten to match the existing private/infallible shape), MINOR-1 (wrong Unix
  timestamp constant in §Commit 2 test fixture — `1714822500` → `1777895100` for
  2026-05-04T11:45:00Z), MINOR-2 (`"Release Dev …"` → `"Release Development …"`
  in §Commit 2 §Testable; matches the existing `make_title` body's type-desc
  mapping), MINOR-3 (`uuid_v7_timestamp` doc section ambiguity — `# Returns`,
  not `# Errors`, since it's `Option`-valued), plus SUGGESTION-1
  (`Timestamp::from_unix_time` const ctor), SUGGESTION-2 (doctest reject arm
  uses `is_err()` not pattern match), SUGGESTION-3 (uppercase UUIDv7 row added
  to the accept/reject table).
- v3 design-review pass (2026-05-22) — verdict "approve with MINOR cleanups".
  Findings closed: MINOR-1 (SUGGESTION-1 from v2 was applied to Commit 2
  §Testable but missed Commit 1 §Testable line 351 — both now name
  `Timestamp::from_unix_time`), MINOR-2 (design §Resolver said `resolve_version`
  lives in `cbscore/src/versions/mod.rs` while the actual location and the rest
  of the design + plan say `resolve.rs` — design line 412 corrected),
  SUGGESTION-1 (§Open Questions section opener reworded from forward-instruction
  "These must be resolved" to "Both have been resolved as documented below").
- v4 design-review pass (2026-05-22) — **verdict "approve — clean"**. Zero
  findings; the review chain converged. Plan + design ready for implementation.
- Implementation landed 2026-05-22 in three commits on `feature/cbscore-rs`:
  `058f3a9` (Commit 1 — cbscore helpers + uuid v7 feature), `97f6f2b` (Commit 2
  — make_title UUIDv7 branch), `3b3b177` (Commit 3 — cbsbuild CLI cutover).
  Workspace gate green at each commit boundary; cbscore lib tests grew 175 →
  196, cbsbuild cmds::versions tests grew 8 → 12.

## Progress

| #   | Commit                                                                                  | ~LOC | Status |
| --- | --------------------------------------------------------------------------------------- | ---- | ------ |
| 1   | `cbscore: add uuid v7 feature + resolve_version + uuid_v7_timestamp + validate_version` | ~280 | Done   |
| 2   | `cbscore: branch make_title on UUIDv7 (created-at form)`                                | ~60  | Done   |
| 3   | `cbsbuild: optional positional VERSION + resolver wire-up + validate call`              | ~210 | Done   |

**Estimate:** ~400 LOC, 3 commits.

## Goal

Replace the required positional `VERSION: String` on `cbsbuild versions create`
with an `Option<String>`. When the operator omits it, the command generates a
UUIDv7 string and uses it as the descriptor identifier — the file lands at
`<root>/<type>/<UUIDv7>.json` (where `<root>` resolves per seq-004's
`resolve_root`) and `desc.version` carries the UUIDv7 verbatim. Supplied-VERSION
invocations are byte-identical to today's behaviour.

Per design 005 §Goals, the UUIDv7 path also yields:

- Chronologically-sortable filenames in `<root>/<type>/` (UUIDv7 strings sort
  lexicographically by their leading 48-bit timestamp per RFC 9562).
- A self-explanatory `Release <type-desc> version created at <ISO 8601 UTC>`
  title in `desc.title`, replacing the parseable-version title's body.

## Depends on

- **seq-002 Phase 6 Commit 2** — `cbsbuild versions create` exists with the
  `CreateArgs.version: String` positional that Commit 3 of this plan changes to
  `Option<String>`.
- **seq-002 Phase 5 Commit 1** — `cbscore::builder::prepare::get_patch_list`
  exists and already handles `Err(MalformedVersion)` from `get_major_version` /
  `get_minor_version` as **warn-and-skip** (see OQ-B).
- **seq-002 Phase 2 Commit 5** —
  `cbscore::versions::utils::{get_major_version, get_minor_version}` exist with
  the `Result<_, CbsError>` shape design 005 §Effects of UUIDv7 VERSIONs
  §Patches relies on.
- **seq-004 Commit 3** — `cbsbuild versions create` uses
  `cbscore::versions::resolve_root` + `descriptor_path` for its write path,
  which is the integration point seq-005 keeps unchanged.
- **uuid crate** — already a dependency of `cbscore` with the `v4` feature
  (Phase 2 / `runner::gen_run_name`). seq-005 adds the `v7` feature.
- **chrono crate** — already a workspace dependency of `cbscore-types`. The
  UUIDv7 timestamp formatter renders `DateTime<Utc>` via chrono's
  `format("%Y-%m-%dT%H:%M:%SZ")`.

Design references: design 005 (this plan implements its §Migration table) and
design 004 §OQ4 (read sites stay explicit-path; no auto-discovery affordance
added in seq-005).

## Open Questions

Both have been resolved as documented below; they surfaced as design–code
mismatches when the plan was drafted against the current `feature/cbscore-rs`
HEAD.

### OQ-A — `validate_version` equivalent in the Rust port — **RESOLVED**

**Resolution: match Python regex behaviour, accept UUIDv7 as an exception.**

Add
`cbscore::versions::utils::validate_version(v: &str) -> Result<(), CbsError>`
that accepts:

- Any string matching the Python regex (`[prefix-]vM.m.p[-suffix]`) with both
  minor AND patch present — same shape `cbscore/versions/create.py:37–42`'s
  `_validate_version` enforces (regex match plus
  `minor is not None and patch is not None`).
- Any UUIDv7 string, as identified by
  `uuid::Uuid::parse_str(v).ok().filter(|u| u.get_version() == Some(uuid::Version::SortRand))`.
  UUIDv7 is accepted whether the resolver generated it (no-VERSION path) or an
  operator typed it deliberately — same `desc.version` shape either way.

Rejects: `19`, `19.2`, `foobar`, UUIDv4 strings, anything else.

Concrete accept / reject table after this commit lands (Python parity with the
UUIDv7 carve-out):

| Input                                             | Verdict | Reason                                       |
| ------------------------------------------------- | ------- | -------------------------------------------- |
| `19.2.3`                                          | accept  | full M.m.p                                   |
| `v19.2.3`                                         | accept  | optional `v`                                 |
| `ces-v19.2.3-dev.1`                               | accept  | prefix + v + M.m.p + suffix                  |
| `19.2.3-rc1`                                      | accept  | M.m.p + suffix                               |
| `0193e1a8-7c2e-7000-89ab-...` (UUIDv7, lowercase) | accept  | UUIDv7 carve-out                             |
| `0193E1A8-7C2E-7000-89AB-...` (UUIDv7, uppercase) | accept  | `Uuid::parse_str` is case-insensitive        |
| `19`                                              | reject  | no minor/patch                               |
| `19.2`                                            | reject  | no patch                                     |
| `foobar`                                          | reject  | regex doesn't match                          |
| `12345678-1234-4abc-89ab-...` (UUIDv4)            | reject  | not v7; falls through to regex which rejects |

**Background.** Design 005 §Resolver claimed `validate_version` already existed
in the Rust port and that seq-005 should "gate the call on
`args.version.is_some()`". The plan-drafting sweep found:

- The Rust-port `version_create_helper`
  (`cbscore/src/versions/create.rs:125–172`) accepts `input.version` verbatim
  and copies it into `desc.version` without any regex validation. A bare `99` or
  `foobar` descriptor would write successfully on the current
  `feature/cbscore-rs` HEAD — i.e., the Rust port has been **behaviourally
  divergent from Python** since M1 cut.

- Design 005 §Resolver's "keep the existing regex validation" instruction
  therefore had no `existing` to keep; the instruction needed to be re-grounded
  as "add `validate_version` to close the gap, with a UUIDv7 exception for the
  no-VERSION path".

**Implementation shape.** The validator is uniformly called by
`cbsbuild versions create` (no `is_some()` gate); UUIDv7 passes by the UUIDv7
carve-out, operator-supplied strings get the regex check, malformed strings exit
non-zero with `MalformedVersion(<the-string>)`. The uniform call is simpler than
gating on `is_some()` and the carve-out also lets operators type a UUIDv7
explicitly (uncommon but not actively rejected).

### OQ-B — Patch walker warn-and-skip is already implemented — **RESOLVED**

**Resolution: keep the existing per-walk warn implementation; amend design 005
§Patch walker and §Migration to match the shipped shape.**

The Rust port's `cbscore::builder::prepare::get_patch_list` (at
`cbscore/src/builder/prepare.rs:258–266`, landed in seq-002 Phase 5 Commit 1)
already implements warn-and-skip when VERSION doesn't parse — one
`tracing::warn!` per walk with a generic "VERSION does not parse as
`[prefix-]vM.m[.p][-suffix]`; only top-level patches apply" message. The build
still terminates cleanly via top-level patches only. UUIDv7 builds will exercise
this code path unchanged from the M2 cut.

**seq-005 implication: no patch-walker work in this seq.** Commits 1–3 stay as
planned (helpers + title branch + CLI cutover); design 005 §Migration table's
step 4 ("patch walker") collapses to a note that the work was completed
pre-seq-005.

**Why not OQ-B.2 (per-subdir warn).** Adding one warn per skipped version-keyed
subdir would produce N near-identical warns for N subdirs — noisy without
diagnostic value, because when VERSION is malformed _all_ subdirs miss
uniformly. The per-subdir warn would only help if some subdirs matched and
others didn't, which never happens on the malformed- VERSION path.

**Why not OQ-B.3 (rewrite wording to name UUIDv7).** The malformed case isn't
always UUIDv7; a supplied VERSION like `19` or `foobar` lands in the same code
path (per OQ-A, those are now rejected upfront by `validate_version`, but the
warn still has to handle the wider set of malformed shapes for robustness). The
current generic wording "VERSION does not parse as `[prefix-]vM.m[.p][-suffix]`"
is accurate for every case that reaches it.

**Background.** Design 005's §Patch walker schematic showed a per-subdir warn
loop with UUIDv7-specific wording. The plan-drafting sweep found the Rust port
had already implemented a per-walk warn with generic wording — sufficient for
the design's _intent_ but different from the literal schematic. OQ-B was the
choice between aligning the design to reality (B.1) or implementing the design
literally (B.2 / B.3); B.1 wins because the existing implementation is correct
and complete, and the design's schematic was overspec for the operator-UX
problem it solves.

## Sequencing

Three commits, ordered. Each is individually compilable + testable + does not
require subsequent commits to make sense:

1. **Commit 1** (cbscore helpers) — adds `resolve_version`, `uuid_v7_timestamp`,
   and `validate_version` to `cbscore::versions`, plus the `v7` feature on the
   `uuid` crate. No callers yet; helpers are unit-tested in isolation (~200
   LOC).
2. **Commit 2** (title branch) — `make_title` gains a UUIDv7 detection branch
   that emits the created-at form. The supplied-VERSION path is unchanged (a
   parseable VERSION won't pass `Uuid::parse_str`, so the branch falls through
   to the existing format string).
3. **Commit 3** (CLI cutover) — `cbsbuild versions create`'s
   `CreateArgs.version` becomes `Option<String>`; the handler calls
   `resolve_version`, then `validate_version` unconditionally, then feeds the
   result through `version_create_helper`. After this commit the no-VERSION path
   is operator-facing, and the supplied-VERSION path matches Python's
   `[prefix-]vM.m.p[-suffix]` regex.

Splitting into three commits **does not** create broken intermediates, because:

- Commit 1's helpers are dead code until Commit 3 wires them up. Dead code with
  `#[must_use]` and unit tests compiles fine.
- Commit 2's branch is gated on `Uuid::parse_str(version).is_ok()` plus a
  v7-version check. A supplied VERSION like `19.2.3` fails the parse and falls
  through; behaviour is unchanged on the live path until Commit 3 lets a UUIDv7
  reach the branch.
- Commit 3 changes the CLI shape and wires the resolver in. After Commit 3 the
  no-VERSION path produces UUIDv7 descriptors.

This contradicts the design's framing ("all five steps tightly coupled,
splitting would create broken intermediates") — the design was written without
knowing the Rust port's current state. With OQ-A.2 + OQ-B.1 resolved, only three
of the design's five migration steps remain (steps 1, 2, 3, 5; step 4 is already
done), and they split naturally.

Visibility decisions for the new symbols (per CLAUDE.md §Visibility,
`pub(crate)` until a cross-crate caller exists, `pub` otherwise):

- `resolve_version` — `pub` because `cbsbuild::cmds::versions` is the
  cross-crate caller.
- `uuid_v7_timestamp` — `pub(crate)` initially. The only caller is the Commit 2
  `make_title` branch in the same crate. Promote to `pub` only if a future
  caller (e.g., `cbsbuild versions show` rendering UUIDv7 timestamps) needs it.
- The `make_title` change is internal; `make_title` itself stays `pub(crate)`
  (current visibility).

## Out of scope

- **Read-side auto-discovery.** `cbsbuild build VERSION --type dev` resolving
  `<root>/dev/<UUIDv7>.json` is rejected in design 004 §OQ4 and re-confirmed in
  design 005 §Non-Goals. Operators feed the printed `-> written to <path>` echo
  into the subsequent `cbsbuild build` invocation.
- **Type-specific behaviour.** Per design 005 §OQ2, all four types (`release` /
  `dev` / `test` / `ci`) behave uniformly. No `-t release`-only validation
  refusing the no-VERSION shortcut.
- **Cross-language interchange.** Per design 002 §Python Coexistence, UUIDv7
  descriptors are not portable to Python `cbc` / `crt`; this is expected and
  unchanged.
- **Schema / wire-format changes.** No `schema_version` bump on
  `VersionDescriptor`, `Config`, or any other wire format. `desc.version` stays
  a `String` field; UUIDv7 just produces different _values_. (Same posture as
  seq-004 §OQ6 — bump policy is deferred across every design in the corpus.)
- **Patch-walker changes.** Per OQ-B, the existing per-walk warn-and-skip
  implementation at `cbscore/src/builder/prepare.rs:258–266` is sufficient for
  UUIDv7 builds. seq-005 does not touch `prepare.rs`.
- **Operator-facing CLI tutorials / man pages.** No man-page work; the `--help`
  output is the surface that updates.

## Commit 1 — `cbscore`: uuid v7 feature + resolve_version + uuid_v7_timestamp + validate_version

Pure-helper commit. No callers wired in; the new functions stand alone with unit
tests.

**Files:**

- `cbsd-rs/cbscore/Cargo.toml` — extend the existing
  `uuid = { version = "1", features = ["v4"] }` line to
  `features = ["v4", "v7"]`. No new dependency; the v4 feature stays for
  `runner::gen_run_name`'s existing UUID v4 usage.
- `cbsd-rs/cbscore/src/versions.rs` — re-export `resolve_version` and
  `validate_version` at the module root for ergonomic cross-crate access from
  `cbsbuild`.
- `cbsd-rs/cbscore/src/versions/resolve.rs` (extended) — add:
  - `pub fn resolve_version(cli: Option<&str>) -> String` — sync, infallible.
    Returns
    `cli.map(str::to_owned).unwrap_or_else(|| uuid::Uuid::now_v7().to_string())`.
    Documents the UUIDv7 path's canonical hyphenated 36-char form.
  - `pub(crate) fn uuid_v7_timestamp(uuid: &uuid::Uuid) -> Option<chrono::DateTime<chrono::Utc>>`
    — extracts the leading 48 bits via `uuid.get_timestamp()` (per RFC 9562
    §5.7). Returns `None` for non-v7 UUIDs (defensive — caller should already
    have checked `uuid.get_version() == Some(Version::SortRand)`, but the helper
    handles the wrong-version case rather than panicking). The constructor of
    `chrono::DateTime<Utc>` is
    `DateTime::<Utc>::from_timestamp(secs as i64, nanos)`; the helper propagates
    `None` if the timestamp is out of range.
- `cbsd-rs/cbscore/src/versions/utils.rs` (extended) — add:
  - `pub fn validate_version(v: &str) -> Result<(), CbsError>` — see §OQ-A above
    for the full accept/reject table. Shape:
    ```rust
    pub fn validate_version(v: &str) -> Result<(), CbsError> {
        if let Ok(uuid) = uuid::Uuid::parse_str(v)
            && uuid.get_version() == Some(uuid::Version::SortRand)
        {
            return Ok(());
        }
        let parsed = parse_version(v)?;
        if parsed.minor.is_some() && parsed.patch.is_some() {
            Ok(())
        } else {
            Err(CbsError::MalformedVersion(v.to_owned()))
        }
    }
    ```
    Sync; no IO. The UUIDv7 fast path runs first so `validate_version` on a
    resolver-generated UUIDv7 is a constant-time `parse_str` + version-check
    rather than a regex run. The fallthrough returns the same
    `CbsError::MalformedVersion(<the-string>)` Python's `_validate_version`
    raises via `MalformedVersionError`.

**Design constraints:**

- **`resolve_version` is infallible.** `Uuid::now_v7()` reads the system clock
  and never errors out on a sane host. The function returns `String`, not
  `Result<String, _>`.
- **`uuid_v7_timestamp` returns `Option`, not `Result`.** Defensive programming
  for the case where a caller passes a non-v7 UUID; per the design 005 §Title
  generator schematic the caller already checks
  `get_version() == Some(Version::SortRand)`, so `Some(_)` is the live path.
  Document the `None` arm (non-v7 input, plus the
  `chrono::DateTime::<Utc>::from_timestamp` out-of-range case) under
  `# Returns`, consistent with the codebase convention for `Option`-returning
  functions — the rest of `cbscore` reserves `# Errors` for `Result`-returning
  functions only.
- **Module placement.** `resolve.rs` already houses `resolve_root` from seq-004;
  co-locating `resolve_version` keeps the "CLI-input resolution" surface in one
  module. `uuid_v7_timestamp` is a private helper for Commit 2's title branch —
  `pub(crate)` initially.
- **No new crate dependencies.** `uuid` is already in `cbscore`'s
  `[dependencies]`; only the feature list changes. `chrono` is re-exported via
  `cbscore_types` and accessible at `cbscore_types::chrono` or directly via the
  workspace dependency.

**Testable:**

- Unit test (`#[test]`): `resolve_version(Some("19.2.3"))` returns `"19.2.3"`
  byte-identically (passes the explicit value through).
- Unit test: `resolve_version(None)` returns a string that parses as a UUIDv7
  (`Uuid::parse_str(&s).unwrap().get_version() == Some(Version::SortRand)`).
- Unit test: two consecutive `resolve_version(None)` calls return two distinct
  UUIDv7 strings (collision check; the 74 random bits make a collision
  astronomically unlikely, but the test pins the determinism-free contract).
- Unit test: `uuid_v7_timestamp` on a UUIDv7 minted from a fixed
  `uuid::Timestamp` returns a `DateTime<Utc>` matching the timestamp to the
  second. Use `Uuid::new_v7(Timestamp::from_unix_time(secs, 0, 0, 0))` to build
  the fixture (the `const` constructor avoids the clock-sequence context arg
  that `from_unix` requires); assert against the expected `DateTime<Utc>`.
- Unit test: `uuid_v7_timestamp` on a UUIDv4 (built via `Uuid::new_v4`) returns
  `None`. Defensive-programming arm.
- Doctest on `resolve_version` showing the CLI-passthrough and the no-input
  path.
- Unit tests on `validate_version` covering the full accept/reject table from
  §OQ-A. At minimum: one accept per row in the accept group (`19.2.3`,
  `v19.2.3`, `ces-v19.2.3-dev.1`, `19.2.3-rc1`, a UUIDv7 minted with
  `Uuid::new_v7(...)`) and one reject per row in the reject group (`19`, `19.2`,
  `foobar`, a UUIDv4 minted with `Uuid::new_v4()`). Each reject asserts the
  error is `CbsError::MalformedVersion(<offending-string>)`.
- Doctest on `validate_version` showing one accept and one reject — the doctest
  is the operator-facing summary of what shapes the validator enforces. Use
  `assert!(result.is_ok())` and `assert!(result.is_err())` rather than
  pattern-matching the `CbsError::MalformedVersion` variant in the doctest body,
  so a future error-type field addition (extra payload fields, variant renames)
  doesn't break the doctest.

## Commit 2 — `cbscore`: branch `make_title` on UUIDv7 (created-at form)

`cbscore::versions::create::make_title` (currently at `create.rs:209`) returns
`"Release {type_desc} version {version}"`. Add a UUIDv7 detection arm that
returns `"Release {type_desc} version created at {timestamp}"` instead.

**Files:**

- `cbsd-rs/cbscore/src/versions/create.rs` — update `make_title` to:

  ```rust
  fn make_title(version: &str, version_type: VersionType) -> String {
      let type_desc = match version_type {
          VersionType::Dev => "Development",
          VersionType::Test => "Test",
          VersionType::Ci => "CI",
          VersionType::Release => "Release",
      };
      if let Ok(uuid) = uuid::Uuid::parse_str(version)
          && uuid.get_version() == Some(uuid::Version::SortRand)
          && let Some(ts) = crate::versions::resolve::uuid_v7_timestamp(&uuid)
      {
          let formatted = ts.format("%Y-%m-%dT%H:%M:%SZ");
          return format!("Release {type_desc} version created at {formatted}");
      }
      format!("Release {type_desc} version {version}")
  }
  ```

  The `parse_str` + `get_version() == Some(Version::SortRand)` chain is the
  design 005 §Title generator §Robustness check: UUIDv4 strings return
  `Some(Version::Random)` and fall through, non-UUID strings fail `parse_str`
  and fall through. No false positives.

**Design constraints:**

- **Signature unchanged.** `make_title(&str, VersionType) -> String` stays
  `fn`-private (not `pub`); the only caller is `version_create_helper` in the
  same module. The function does **not** become fallible; the title is always
  producible — either the v7 form or the existing passthrough.
- **`validate_version` runs before `make_title`** (in Commit 3). By the time
  `make_title` is reached, `version` is either a parseable
  `[prefix-]vM.m.p[-suffix]` string (which falls through to the passthrough
  format) or a UUIDv7 (which hits the new branch). Malformed strings are
  rejected earlier by `validate_version`; `make_title` does not see them on the
  live path. The `parse_str` fallthrough still handles the off-path case (e.g.,
  a test that calls `make_title` directly with `"foobar"`).
- **Display format.** ISO 8601 in UTC at seconds precision
  (`%Y-%m-%dT%H:%M:%SZ`) per design 005 §Title. The displayed seconds match the
  UUIDv7's millisecond truncated to seconds; the random bits already
  disambiguate same-second mints.

**Testable:**

- Unit test: `make_title("19.2.3", VersionType::Dev)` returns
  `"Release Development version 19.2.3"` (existing behaviour, regression guard).
- Unit test: `make_title` on a UUIDv7 minted at a fixed timestamp (e.g.,
  `2026-05-04T11:45:00Z`) returns
  `"Release Development version created at 2026-05-04T11:45:00Z"`. Use
  `Uuid::new_v7(Timestamp::from_unix_time(1777895100, 0, 0, 0))` for the fixture
  (1777895100 = 2026-05-04T11:45:00Z UTC; verify with
  `chrono::DateTime::<Utc>::from_timestamp(1777895100, 0)`).
- Unit test: `make_title` on a UUIDv4 string (`Uuid::new_v4()`) falls through to
  the passthrough format with the literal UUID string. Asserts no false positive
  on the v7 detection.
- Unit test: `make_title` on a non-UUID malformed string (`"foobar"`) falls
  through to `"Release Development version foobar"` (`VersionType::Dev` maps to
  `"Development"` per the existing `make_title` body). The `parse_str` rejects
  the input cleanly.

## Commit 3 — `cbsbuild`: optional positional VERSION + resolver wire-up + validate call

The CLI cutover. After this commit the no-VERSION path is operator-facing and
produces UUIDv7 descriptors end-to-end, and the supplied-VERSION path matches
Python's regex behaviour (per OQ-A).

**Files:**

- `cbsd-rs/cbsbuild/src/cmds/versions.rs`:
  - Change `CreateArgs.version` from `pub version: String` to
    `pub version: Option<String>`. The clap derive recognises the
    `Option<String>` shape and parses an absent positional as `None`. No
    `#[arg(...)]` annotation change is needed (positional inference still
    applies).
  - In `handle_create`, resolve and validate before any other work:
    ```rust
    let version = cbscore::versions::resolve_version(args.version.as_deref());
    cbscore::versions::validate_version(&version)
        .with_context(|| format!("invalid version '{version}'"))?;
    ```
    The call is unconditional — UUIDv7 passes by the UUIDv7 carve-out;
    operator-supplied strings get the regex check. Use `version` thereafter
    everywhere `args.version` was previously used.
  - Update the `--help` text on the positional (clap derive picks up the doc
    comment) to: "Optional version string matching `[prefix-]vM.m.p[-suffix]`.
    When omitted, a UUIDv7 is generated and used as the descriptor identifier."
  - Module doc (`//!` block, currently at `versions.rs:4–16`) — the
    `versions create <VERSION>` bullet becomes `versions create [VERSION]` and
    the prose updates to reflect that the positional is optional plus the regex
    shape requirement on supplied values.

**Design constraints:**

- **CLI UX parity on the well-shaped supplied-VERSION path** (CLAUDE.md
  correctness invariant 2). An operator who passes
  `cbsbuild versions create 19.2.3 -c ceph@main` sees the same descriptor
  written to the same path with the same content as today.
- **Behaviour change on the malformed-supplied-VERSION path** (per OQ-A).
  Operators who previously got away with `cbsbuild versions create 19`,
  `cbsbuild versions create foobar`, etc. now see a non-zero exit with
  `MalformedVersion(<their-string>)` — closing the gap between the Rust port and
  Python's behaviour. This is the intentional consequence of OQ-A's resolution;
  the operator-facing CHANGELOG entry for this minor add names it explicitly.
- **No new flag.** Per design 005 §OQ3, VERSION stays positional; `--version`
  flag was rejected.
- **No `--print-path-only` or `derived-version=…` lines.** Per design 005 §OQ4;
  the existing `version:` and `-> written to ` echoes are the script-consumable
  handles.
- **Threading `version` everywhere.** Audit `handle_create` for every reference
  to `args.version` and replace with the resolved `version` binding. Currently
  four sites: `VersionCreateInput.version` (clone), the `with_context` error
  message, `write_resolved_descriptor`'s `version` arg, and a logging emit (if
  any).
- **`validate_version` runs early.** Call sequence inside `handle_create` is
  resolve → validate → load config → load components → … . Validation failure
  exits before any IO (no config load, no component dir read), so a malformed
  VERSION fails fast with a clean error message.
- **No descriptor-overwrite policy change.** The existing
  `VersionError::AlreadyExists` check in `write_resolved_descriptor` remains. A
  UUIDv7 collision is astronomically unlikely, but if it somehow occurs the
  EEXIST refusal still fires.

**Testable:**

- Unit test (clap-level):
  `Cli::try_parse_from(["cbsbuild", "versions", "create", "-c", "ceph@main"])`
  succeeds with `CreateArgs.version == None`.
- Unit test (clap-level):
  `Cli::try_parse_from(["cbsbuild", "versions", "create", "19.2.3", "-c", "ceph@main"])`
  succeeds with `CreateArgs.version == Some("19.2.3".into())`.
- Integration test (the existing `tests` module pattern in `cmds/versions.rs`):
  a `write_resolved_descriptor`-driven test asserting that when
  `resolve_version(None)` is fed through the write path, the resulting `dst`
  matches the `<root>/<type>/<UUIDv7>.json` shape. Assert `dst.file_name()`
  parses as a UUIDv7 via `Uuid::parse_str`.
- Integration test: `cbsbuild versions create 19 -c ceph@main` exits non-zero
  with `MalformedVersion("19")` (Python-parity regression guard for OQ-A).
  Exercise via the same `handle_create`-bypassing test pattern used in seq-004 —
  either drive `validate_version` directly or invoke `handle_create` with a
  minimal config + components fixture.
- Integration test: the existing `cli_versions_dir_wins_and_writes_under_it`
  test stays green (regression guard for the supplied-VERSION + `--versions-dir`
  interaction landed in seq-004).

## End-of-feature acceptance

After all three commits land:

- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets`, `cargo fmt --all --check` all pass
  with zero warnings.
- `cbsbuild versions create -t dev -c ceph@main` (no VERSION, inside a git
  checkout) writes `<git-root>/_versions/dev/<UUIDv7>.json`, prints
  `-> written to <that-path>`, and exits 0. The descriptor's `version` field is
  the UUIDv7 string, `title` is
  `"Release Development version created at <timestamp>"`.
- `cbsbuild versions create 19.2.3 -t dev -c ceph@main` (explicit VERSION)
  writes `<git-root>/_versions/dev/19.2.3.json` with `version: "19.2.3"`,
  `title: "Release Development version 19.2.3"` — byte-identical to the
  pre-seq-005 supplied-VERSION behaviour.
- `cbsbuild versions create -t dev -c ceph@main --versions-dir /tmp/x` (no
  VERSION + `--versions-dir`) writes `/tmp/x/dev/<UUIDv7>.json` — the seq-004
  `--versions-dir` surface interleaves cleanly with seq-005's auto-derived
  VERSION.
- `cbsbuild versions create 19 -c ceph@main` exits non-zero with
  `MalformedVersion("19")` — Python-parity check (OQ-A). Same for
  `cbsbuild versions create foobar -c ceph@main`. No descriptor file is written;
  the error surfaces before any config / components IO.
- `ls -1 <root>/<type>/ | tail -1` returns the most recently-minted UUIDv7
  descriptor in chronological order — the §Operator UX affordance from
  design 005.
- Plans README progress table updates: §"Related plans › seq-005" bullet flips
  from "Pending" to "Done" (same commit boundary as Commit 3 so the README state
  matches the on-disk reality).
- Plan progress table flips all three rows to `Done`.

## Verification history

The pre-implementation verification ran as follows; recorded here for the
historical record (now superseded by the implementation entry in the §Status
review trail above).

1. **OQ resolutions confirmed.** OQ-A (`validate_version`, accept UUIDv7) and
   OQ-B (keep current per-walk warn) were resolved in their respective §Open
   Questions subsections.
2. **Design 005 amended to match.** OQ-A's resolution updated §Resolver (dropped
   the "gate on `is_some()`" instruction; described the unconditional
   `validate_version` call with the UUIDv7 carve-out) and §Migration table (step
   2 split into 2a/2b; step 5 mentions `validate_version` unconditionally).
   OQ-B's resolution simplified §Patch walker to match the existing per-walk
   implementation and noted that §Migration table step 4's work was already
   complete pre-seq-005. Design 005's status flipped from "Complete — ready for
   review" to "Approved for seq-005 implementation".
3. **Design-review chain ran v2 → v3 → v4.** Same cadence as seq-004's v3 → v4 →
   v5 chain. Verdicts (in order): "approve with MINOR cleanups", "approve with
   MINOR cleanups", "approve — clean". All findings closed inline before
   implementation began.
4. **Implementation landed in three commits** as listed in the §Status review
   trail. cbscore lib tests grew 175 → 196, cbsbuild cmds::versions tests grew 8
   → 12, workspace gate green at each commit boundary.
