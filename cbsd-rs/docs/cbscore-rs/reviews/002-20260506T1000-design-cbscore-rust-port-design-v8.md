# Design Review v8: cbscore Rust Port — Architecture & Subsystem Design

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`

**Prior reviews:** `002-20260420T1132-design-cbscore-rust-port-design-v1.md`,
`002-20260420T1512-design-cbscore-rust-port-design-v2.md`,
`002-20260427T1330-design-cbscore-rust-port-design-v3.md`,
`002-20260428T1401-design-cbscore-rust-port-design-v4.md`,
`002-20260429T0929-design-cbscore-rust-port-design-v5.md`,
`002-20260429T1633-design-cbscore-rust-port-design-v6.md`,
`002-20260430T1208-design-cbscore-rust-port-design-v7.md`

**Changes since v7:** A cross-reference paragraph was added to §Version creation
describing design 005's resolved UUIDv7 shape and stating that no
`schema_version` bump is required.

---

## Summary

**Verdict: approve, no changes needed.**

The refreshed §Version creation paragraph accurately summarises design 005 and
correctly states that no `schema_version` bump is required. The phrasing is
consistent with design 005's own §Non-Goals and §Schema / wire format sections.
No contradictions were found.

---

## §Version Creation Paragraph Verification

The new paragraph reads (paraphrased; exact text at lines 727–739 of design
002):

> Design 005's resolved shape is: when the positional VERSION is omitted, the
> command generates a UUIDv7 string and uses it as the descriptor identifier.
> Operators who continue to pass an explicit VERSION see no behaviour change.
> Design 005 is intentionally post-M1 — its UUIDv7 path requires per-callsite
> graceful degradation in the title generator and patch walker (a UUIDv7 does
> not match `[prefix-]vM.m.p[-suffix]`), and bundling that work into M1 adds
> risk for limited gain. Once M1 is stable, design 005 lands as a 1.x.0 minor
> add. No `schema_version` bump is required — `desc.version` stays a string
> field and only its values change.

Three claims to verify:

**1. UUIDv7 when VERSION is omitted.** Design 005 OQ1 resolves: "generate a
UUIDv7 when no positional VERSION is supplied." The paragraph's summary matches.

**2. No schema_version bump.** Design 005 §Schema / wire format is explicit: no
bump on `VersionDescriptor`, `Config`, or any other wire format. `desc.version`
is and stays a `String` field; UUIDv7 produces different values, not a different
field shape. The "no `schema_version` bump" claim is accurate.

**3. Post-M1 as a 1.x.0 minor add.** Design 005 §Status and §Non-Goals both
confirm: "out of M1 scope", "ships post-M1 as a 1.x.0 minor add." The
paragraph's phrasing matches.

**4. Rationale (title generator + patch walker).** Design 005 §Effects of UUIDv7
VERSIONs covers exactly these two callsites. The paragraph's rationale for
deferring ("per-callsite graceful degradation in the title generator and patch
walker") is accurate in naming the affected sites, though see the note below re:
the patch walker.

**Note — patch walker correctness is a design 005 issue, not a design 002
issue.** Design 002 says the patch walker "requires graceful degradation";
design 005 claims to provide it. Whether design 005's claim about the walker is
correct is evaluated in the design 005 review (v1). Design 002's reference to
the requirement is accurate; it does not make the implementation claim itself.

---

## Summary of Action Items

None.
