# Review — Plan: Security Audit Remediation (Unified), v3

| Field        | Value                                                                                                                                                                                      |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Review       | 019 — plan-security-audit-remediation, v3                                                                                                                                                  |
| Reviewer     | Claude Sonnet 4.6 (adversarial, source-validated)                                                                                                                                          |
| Date         | 2026-05-18                                                                                                                                                                                 |
| Plan         | `cbsd-rs/docs/cbsd-rs/plans/019-20260516T1033-security-audit-remediation.md`                                                                                                               |
| Designs      | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` (WCP v11) + `cbsd-rs/docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md` (audit-rem v8) |
| Prior review | `cbsd-rs/docs/cbsd-rs/reviews/019-20260517T0838-plan-security-audit-remediation-v2.md` (v2, score 90/100)                                                                                  |
| Scope        | Full plan v2 review: verify v2 findings resolved; find anything v2 missed.                                                                                                                 |

---

## Executive Summary

The v2 plan has addressed all three v2 findings (N1, N2, N3) substantively and
correctly. One new finding (N4) is a documentation gap in commit 21's pitfalls:
the `sentinel_for_tag` function added by commit 18 constructs
`ServerMessage::BuildRevoke { build_id: BuildId(0) }` as a named-field
struct-literal, making it a fifth compile-break site when commit 21 adds the
`reason` field — a site that commit 21's pitfalls enumerate as "four sites
verified against source on this branch" but miss because commit 18 postdates
that verification. The plan ordering is sound; the gap is documentation only
with zero runtime risk. Final score: **95/100**.

**Recommendation: Go — with one recommended plan improvement (N4).**

---

## V2 Findings: Status

### N1 — `ActiveAssignmentReceipt` placed in `cbsd-proto` (was: Major)

**Status: RESOLVED.**

Commit 2's packages list now reads:

> `cbsd-server` (`queue/mod.rs` declares new `ActiveAssignmentReceipt` enum
> alongside `ActiveBuild`, and `ActiveBuild` gains a
> `receipt: ActiveAssignmentReceipt` field; ...)

Source validation confirms: `ActiveBuild` lives in
`cbsd-server/src/queue/mod.rs` (read at lines 27-37). The type does not exist
anywhere in the codebase yet. There is no mention of `ActiveAssignmentReceipt`
in `cbsd-proto`'s packages list. The crate-boundary violation is eliminated.

### N2 — Commit 21 pitfalls omit server-side compile-break sites (was: Minor)

**Status: RESOLVED.**

Commit 21's pitfalls now enumerates all four sites with exact file and line
references:

- `cbsd-server/src/ws/dispatch.rs:500`:
  `let msg = ServerMessage::BuildRevoke { build_id: BuildId(build_id) };`
- `cbsd-server/src/main.rs:422`:
  `let msg = cbsd_proto::ws::ServerMessage::BuildRevoke { build_id: cbsd_proto::BuildId(*build_id) };`
- `cbsd-server/src/ws/handler.rs:698`:
  `let msg = ServerMessage::BuildRevoke { build_id };`
- `cbsd-worker/src/ws/handler.rs:389`:
  `ServerMessage::BuildRevoke { build_id } => { … }`

All four citations were independently verified against source:

- `dispatch.rs:500`: confirmed by reading the file (grep and direct read).
- `main.rs:422`: confirmed in `run_drain_shutdown` function.
- `ws/handler.rs:698`: confirmed in the `"not_found"` reconnect arm.
- Worker `handler.rs:389`: confirmed as a named-field pattern with no `..`.

The `cbc` crate does not match on `ServerMessage`; its `BuildRevoke` references
are CLI command names only (confirmed by reading `cbc/src/builds.rs`). N2 is
fully resolved.

### N3 — Phase independence framing misleading (was: Minor)

**Status: RESOLVED.**

The plan overview now reads (lines 79-86):

> Phase 2 (audit-remediation cross-cutting) has no _compile_ or _functional_
> dependency on Phase 1 or Phase 3, but SHOULD land after Phase 1 is complete to
> avoid a window in which message-level size bounds (commit 9) are in force
> without the build-scoped ownership check (commit 2) that makes D6's trust
> argument sound — see commit 9's pitfalls and the audit-rem design's note on
> the Phase C / WCP ownership-rule interaction.

The qualification "_compile_ or _functional_" accurately limits the independence
claim. The security ordering preference is now explicit and cross-referenced to
commit 9's pitfalls. N3 is fully resolved.

---

## New Findings

### N4 — Commit 21 misses a fifth `BuildRevoke` compile-break site (Minor)

**Finding:** Commit 21's pitfalls section states:

> Sites verified against source on this branch:
>
> - Server-side struct-literal constructions (three sites): ...
> - Worker-side destructure (one site): ...

This enumeration was correct at the time it was written. However, commit 18
(`cbsd-rs/proto: add SI-18 regression test for ServerMessage`) adds a
`sentinel_for_tag` function inside `cbsd-proto/src/ws.rs::tests`. The design
(audit-rem v8 lines 2361-2363) specifies that `sentinel_for_tag`'s `BuildRevoke`
arm constructs the variant as a named-field struct-literal:

```rust
ServerMessageTag::BuildRevoke => ServerMessage::BuildRevoke {
    build_id: BuildId(0),
},
```

This is the same named-field struct-literal pattern as the three server-side
construction sites. When commit 21 adds `reason: Option<BuildRevokeReason>` to
`BuildRevoke`, this construction in `sentinel_for_tag` becomes a compile break:
Rust requires all fields to be present in a struct-literal expression, even
`Option<>` fields (unlike serde's `#[serde(default)]`, which is a
deserialization concern). The implementer must add `reason: None` to this arm in
the same commit.

The site is:

- `cbsd-proto/src/ws.rs::tests::sentinel_for_tag` (added by commit 18,
  `ServerMessageTag::BuildRevoke` arm):
  `ServerMessage::BuildRevoke { build_id: BuildId(0) }`

**Why this was missed:** The commit 21 pitfalls were verified against the branch
source as it existed before commit 18 was planned. Commit 18 creates a new
instance of the pattern _after_ that verification was recorded. The pitfalls
section's claim of "verified against source on this branch" is accurate for the
four pre-existing sites; the fifth is a consequence of the plan's own ordering
(commit 18 introduces the site; commit 21 breaks it).

**Severity assessment:** Minor. The compiler catches this immediately — the
`cbsd-proto` crate is in commit 21's packages list, so the implementer will see
the error. Runtime risk is zero. The gap is that commit 21's pitfalls text will
tell the implementer "four sites" while the compiler surfaces five.

**Fix:** Add the fifth site to commit 21's pitfalls:

> - `cbsd-proto/src/ws.rs::tests::sentinel_for_tag` (added by commit 18,
>   `BuildRevoke` arm): add `reason: None` to the struct-literal.

A two-sentence note is sufficient. The fix is a plan-text edit only.

---

## Per-Commit Assessment

All 21 commits were reviewed against source code, design documents, and the
`git-commits` skill. The findings below reflect only items not already covered
in the v2 per-commit assessment (which passes through unchanged where no new
issues were found).

**Commits 1-17:** No new findings. The v2 assessment stands.

**Commit 18 (SI-18 regression test):** Clean design. The `sentinel_for_tag`
addition is verified against the design sketch (audit-rem v8 lines 2352-2375).
The `ServerMessageTag` companion enum uses `#[derive(strum::EnumIter)]` to
eliminate the "witness updated, case forgotten" gap. The commit is correctly
self-contained: it adds no feature, only a regression guard, and the test is the
entire deliverable. The `strum` dev-dependency addition is additive with no
effect on non-test builds. However, see N4 — this commit's test code creates a
compile-break site that commit 21's pitfalls do not document.

**Commit 19 (accepted-phase reconnect):** No new findings.

**Commit 20 (dead worker resolution):** No new findings.

**Commit 21 (migration revoke + drain):** Sound overall. Pitfalls for the
`BuildRevoke` field addition are thorough and all four pre-existing sites are
correctly identified with source-verified line numbers. The one gap is N4:
`sentinel_for_tag` in `cbsd-proto/src/ws.rs::tests` (added by commit 18) is a
fifth named-field construction that also needs `reason: None` added. The test
count (7 SM-W migration tests + 5 serde-compatibility tests + D13-T7 boundary
sub-cases) is proportionate to the commit's complexity. The ~700-LOC size
exception is correctly justified — the commit cannot be split without creating a
`BuildRevokeReason` enum with no callers.

---

## Confidence Score

| Item                                                             | Points | Description                                                                                |
| ---------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------ |
| Starting score                                                   | 100    |                                                                                            |
| N4: Commit 21 pitfalls miss 5th `BuildRevoke` compile-break site | -5     | D11 — `sentinel_for_tag` in commit 18 test creates a new site commit 21 does not enumerate |
| **Total**                                                        | **95** |                                                                                            |

N1, N2, N3 from v2 are resolved — no deductions carried forward.

---

## Summary

**Score: 95/100 — Ready to proceed.**

### Recommended plan improvement (non-blocking)

**N4**: Add the fifth `BuildRevoke` struct-literal site to commit 21's pitfalls:

> - `cbsd-proto/src/ws.rs::tests::sentinel_for_tag` (added by commit 18): add
>   `reason: None` to the `ServerMessageTag::BuildRevoke` arm.

This is a two-sentence addition to the pitfalls text. No other plan changes are
needed.

### Top findings by severity

1. **N4 (Minor):** Commit 21's pitfalls enumerate four `BuildRevoke`
   compile-break sites "verified against source." Commit 18 introduces a fifth —
   the `sentinel_for_tag` named-field construction in
   `cbsd-proto/src/ws.rs::tests`. The compiler will surface it immediately; the
   only cost is implementer confusion when "four sites" in the pitfalls becomes
   five compiler errors.

2. **N1 (RESOLVED):** `ActiveAssignmentReceipt` crate-boundary violation was the
   blocking finding in v2. Correctly resolved: the type is now declared in
   `cbsd-server/src/queue/mod.rs`.

3. **N2 (RESOLVED):** Commit 21's server-side `BuildRevoke` construction sites
   were missing from pitfalls in v2. All four sites are now enumerated with
   source-verified line numbers.
