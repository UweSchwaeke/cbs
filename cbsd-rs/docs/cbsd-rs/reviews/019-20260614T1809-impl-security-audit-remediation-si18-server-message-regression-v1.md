# Impl Review — SI-18 ServerMessage `deny_unknown_fields` regression test (v1)

- **Review type**: implementation (single commit, test-only)
- **Commit**: `c8a323f5` —
  `cbsd-rs/proto: add SI-18 regression test for ServerMessage`
- **Closes**: audit-rem D13-T6, SI-18
- **Design**:
  `docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md` (SI-18 ≈
  L1894–1899; D13-T6 sketch ≈ L2222–2552)
- **Plan**: `docs/cbsd-rs/plans/019-20260516T1033-security-audit-remediation.md`
  (Commit 18 ≈ L816–842)
- **Scope**: `cbsd-proto` only — `Cargo.toml` dev-dependency, `src/ws.rs` test
  module, `Cargo.lock`. No production code or doc changes.

## Verdict: GO

The commit delivers genuine, complete protection for SI-18. Between the Rust
compiler and this runtime test, every realistic way of (re)introducing
`deny_unknown_fields` onto `ServerMessage` is caught. Tests pass (25/25), clippy
is clean under `-D warnings`, the commit is a clean single-purpose test-only
change with no dead code. The one defect is descriptive: the header comment,
assert message, and commit subject describe a regression (a variant-level
attribute) that is actually a **compile error**, so that specific wording can
never reach the runtime assertion it sits next to. That is a wording correction,
not a coverage gap.

## Evidence gathered (verified, not trusted)

- `cargo test -p cbsd-proto` → **25 passed; 0 failed**, including
  `no_deny_unknown_fields_on_server_message` and the pre-existing
  `server_message_unauthorized_build_action_round_trip`.
- `cargo clippy -p cbsd-proto --all-targets -- -D warnings` → **clean** (no
  unused-import warning; `VersionType` is consumed by the pre-existing
  `server_message_build_new_round_trip` test at `ws.rs:187`).
- serde/serde_json versions in the experiment match the workspace exactly
  (`serde 1.0.228`, `serde_json 1.0.149`).
- `ServerMessage` is **not** `#[non_exhaustive]`; all three match helpers
  (`from_message`, `as_wire`, `sentinel_for_tag`) are fully enumerated with **no
  `_` wildcard**, so the compile-time gates genuinely fire.

## Highest-priority probe — does the test protect anything?

`ServerMessage` is internally tagged
(`#[serde(tag = "type", rename_all = "snake_case")]`). I ran a throwaway
experiment (`/tmp/serde-experiment`, pinned to the workspace's exact serde
versions; not committed) covering the three realistic regression placements:

| #   | Regression a developer could introduce                                                              | serde behaviour                                                                                                                | Who catches it                                   |
| --- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------ |
| 1   | `#[serde(deny_unknown_fields)]` directly on a **struct variant** of the enum                        | **Compile error**: `unknown serde variant attribute deny_unknown_fields` (it is a _container_ attribute, illegal on a variant) | `rustc` — the build breaks before this test runs |
| 2   | `#[serde(deny_unknown_fields)]` on the **enum container**                                           | Compiles; **rejects** the unknown field at runtime (`Err`)                                                                     | **This test** (runtime assertion fires)          |
| 3   | Refactor a variant to a **newtype wrapping a standalone struct** that carries `deny_unknown_fields` | Compiles; **rejects** the unknown field at runtime (`Err`)                                                                     | **This test** (runtime assertion fires)          |

**Conclusion:** SI-18 is _fully_ enforced. The compiler prevents case 1; the
test catches cases 2 and 3. There is no enforcement gap. The test's runtime
assertion is live and meaningful — it fires for exactly the regressions that
actually compile.

The defect is descriptive only: the comment block, the assert message
(`"variant ... rejected an unknown field — likely caused by #[serde(deny_unknown_fields)] being added"`),
and the commit subject
(`if any ServerMessage variant gains the serde deny_unknown_fields attribute`)
all frame the protected scenario as the **variant-level** attribute. That exact
scenario is a compile error and can never reach this assert. The accurate
framing is "container-level attribute or a refactor to a deny-fields struct."
This is inherited from the design's own sketch wording, so it is authorized, but
it remains a minor inaccuracy worth a one-line correction.

## Other adversarial probes (all clear)

- **`cases()` → `HashMap` collision (duplicate/typo'd wire key).** Fully
  guarded. With exactly 5 entries, any duplicate or typo collapses the map below
  5 keys; Loop 1's per-tag `cases_map.contains_key(wire)` assertion then fires
  loudly for the missing tag. Clean non-issue.
- **Payloads reach the unknown-field path for the right reason.** Each payload
  carries the correct `"type"` discriminator and all required fields (verified
  field-by-field against `ServerMessage` at `ws.rs:24–63`), plus `future_field`.
  So `is_ok()` cannot pass for the wrong reason. The secondary assertion
  `from_message(&msg).as_wire() == wire` additionally catches a payload that
  deserialized to the wrong variant.
- **5th-variant additions correct.** `UnauthorizedBuildAction` sentinel uses
  `BuildId(0)`, `WorkerBuildAction::WorkerStatus`, and
  `UnauthorizedBuildReason::NotAssigned` — all valid; wire tag is exactly
  `unauthorized_build_action`; the case JSON uses snake_case `action`
  (`"worker_status"`) and `reason` (`"not_assigned"`), matching the
  `rename_all = "snake_case"` on both sub-enums (`ws.rs:70–88`).
- **Dropping `Hash` is sound.** `ServerMessageTag` is never used as a map/set
  key; `cases_map` is keyed by `&'static str`. The remaining derives
  (`EnumIter, Debug, Clone, Copy, PartialEq, Eq`) are each used. Authorized by
  plan Commit 18 (NF-2-v8).
- **Compile-time gates truly fire.** No `#[non_exhaustive]`, no `_` wildcard. A
  new `ServerMessage` variant fails `from_message`'s exhaustive match; a new
  `ServerMessageTag` variant fails `as_wire` / `sentinel_for_tag`; same-crate
  placement preserves this even if `#[non_exhaustive]` is added later.
- **Authorized deviations from the sketch.** (a) Extending to the 5th live
  variant `UnauthorizedBuildAction` — explicitly instructed by the design
  (L2252–2258). (b) Dropping `Hash` — explicitly instructed by the plan
  (L836–838). Both correct.

## Hygiene / deps

- `strum` is under `[dev-dependencies]` only — confirmed in `Cargo.toml`; not
  reachable from any production target. Good for a security context.
- `strum 0.26` (dev) **duplicates** the workspace's existing transitive
  `strum 0.27.2` (pulled via another crate). `Cargo.lock` now carries both
  `strum`/`strum_macros` 0.26 and 0.27. The implementer followed the design,
  which pinned 0.26; aligning the dev-dep to `0.27` would drop the duplicate and
  the extra proc-macro build. Minor hygiene note, not blocking.
- Commit message: the `Co-authored-by: Claude Opus 4.8 (1M context)` trailer's
  parenthetical deviates from the CLAUDE.md example trailer form
  (`Claude Sonnet 4.6 <noreply@anthropic.com>`). Cosmetic; non-blocking.

## git-commits smell test

1. One-sentence purpose: yes — "regression test that fails if a `ServerMessage`
   variant becomes forward-incompatible (SI-18)."
2. Parent compiles: yes — test-only addition.
3. Revertable: yes — isolated to `cbsd-proto` test module + dev-dep.
4. Testable: yes — the commit _is_ a passing test.
5. No dead code: yes — every helper (`test_descriptor`, `test_descriptor_json`,
   `from_message`, `as_wire`, `sentinel_for_tag`, `cases`) has a caller inside
   `no_deny_unknown_fields_on_server_message`.

Subject is `component: what changed` form and matches the plan title. Size is
well under the budget (auto-generated `Cargo.lock` excluded). Pass.

## Confidence score

| Item                                                                                                                           | Points | Description                                                                 |
| ------------------------------------------------------------------------------------------------------------------------------ | ------ | --------------------------------------------------------------------------- |
| Starting score                                                                                                                 | 100    |                                                                             |
| D11: comment / assert message / commit subject describe the variant-level attr (a compile error) as the protected runtime case | -5     | Inaccurate framing; the live protection is container-level + refactor cases |
| D3: `strum 0.26` dev-dep duplicates the workspace's transitive `strum 0.27.2`                                                  | -5     | Two strum/strum_macros copies in `Cargo.lock`; align to 0.27 to dedupe      |
| **Total**                                                                                                                      | **90** |                                                                             |

Range 90–100: ready to merge; minor issues only.

## Findings by severity

**Blocker** — none.

**Serious** — none.

**Minor**

- _(M1)_ The header comment, the runtime assert message, and the commit subject
  describe "a `ServerMessage` **variant** gaining
  `#[serde(deny_unknown_fields)]`." That exact placement is a **compile error**
  (serde rejects it as an unknown variant attribute), so it can never reach the
  runtime assertion. The test's actual live protection is against (a) the
  attribute on the **enum container** and (b) a refactor of a variant into a
  newtype wrapping a `deny_unknown_fields` struct — both verified to compile and
  to be rejected at runtime. Recommend a one-line wording correction so the
  rationale is not misleading; no code change. (Note: this wording originates in
  the design sketch, so it is authorized as-shipped.)
- _(M2)_ Dev-dependency `strum = "0.26"` duplicates the workspace's existing
  transitive `strum 0.27.2`. Pinning the dev-dep to `0.27` would avoid a second
  strum/strum_macros copy in `Cargo.lock` and an extra proc-macro build. The
  design specified 0.26, so the implementer is consistent with spec; flag for a
  follow-up alignment.

**Nit**

- _(N1)_ `Co-authored-by` trailer parenthetical `(1M context)` deviates from the
  CLAUDE.md example trailer form. Cosmetic.

## Recommendation

GO. Optionally fold M1 (wording) and M2 (dep alignment) as a fixup into this
commit, but neither blocks. SI-18 is fully enforced as shipped.
