# Design Review v6: cbscore Rust Port — Architecture & Subsystem Design

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`

**Prior reviews:** `002-20260420T1132-design-cbscore-rust-port-design-v1.md`,
`002-20260420T1512-design-cbscore-rust-port-design-v2.md`,
`002-20260427T1330-design-cbscore-rust-port-design-v3.md`,
`002-20260428T1401-design-cbscore-rust-port-design-v4.md`,
`002-20260429T0929-design-cbscore-rust-port-design-v5.md`

**Commits reviewed since v5:** `1aaf7a4` (§Version Descriptors & Creation
cross-reference to design 004), `bdd0eff` (§Wire-Format Versioning "every change
bumps" qualifier for pre-M1).

---

## Summary

**Verdict: approve, no changes needed.**

Both edits are correct and coherent. The §Version Descriptors & Creation
paragraph accurately describes the current state (hardcoded path until design
004 lands, now approved). The §Wire-Format Versioning qualifier is precisely
placed and consistent with design 001 §Versioning. No new issues introduced.

---

## Commit Verification

### `1aaf7a4` — §Version Descriptors & Creation cross-reference

The added paragraph reads (in the current document):

> The Rust port treats the descriptor-store location as configurable; the design
> lives in design 004 and is currently in the discussion phase. Until that
> lands, the Rust port preserves the Python behaviour (hardcoded
> `<git-root>/_versions/<type>` path).

Design 004 is now **approved**, not in the discussion phase. The "currently in
the discussion phase" description is stale. This is a minor doc drift, not a
design flaw — the implementation behaviour described ("until that lands,
preserve Python behaviour") is still correct for any M0 work predating the
design 004 implementation commit. The stale status phrase is worth updating in
the next editorial pass but does not block anything.

### `bdd0eff` — §Wire-Format Versioning qualifier

The qualifier text added to the "Every change bumps" rule:

> **This rule applies from the M1 release onward.** During M0–M1 the schema is
> still being defined and per-format `schema_version: 1` accumulates every
> change up to the M1 1.0.0 cut; the first post-1.0 change to any format is the
> first bump. Pre-M1 cbscore-rs is a 0.x release with no stability promise (see
> design 001 § Versioning).

This is correctly positioned inside the "Rules" bullet list, immediately
following the "Every change bumps" rule it qualifies. The qualifier is
consistent with:

- Design 001 §Versioning ("The M1 release is cbscore-rs 1.0.0 — version 0.x is
  reserved for in-progress pre-release builds and carries no stability
  promise").
- Design 004 OQ6 ("no bump — design 004 is a pre-M1 change").
- The §Implementation pattern (the `VersionedConfig::V1` enum arm covers the
  entire M0–M1 accumulated shape; V2 is the first post-1.0 arm).

The qualifier does not contradict the adjacent rules ("absent is an error", "no
migration tool", "unknown-version handling") because those rules describe Rust
runtime behaviour, which applies whenever cbscore-rs reads a file regardless of
which phase produced it. The pre-M1 qualifier applies only to the decision of
_when to bump_ the integer, not to how the runtime handles the integer it finds.

---

## Previously Open Finding

**v5 N1 (reqwest version pin):** The v5 review flagged a discrepancy — design
001 Cargo sketch used `reqwest = { version = "0.12", ... }` while design 002
capability table said `reqwest 0.13`. Commit `af5a24f` (landed before design 004
work began) updated the design 001 Cargo sketch to `0.13`. Both documents now
agree. Closed.

---

## Minor Doc Drift (no action required)

The "currently in the discussion phase" phrasing in §Version Descriptors &
Creation is stale now that design 004 is approved. Worth one word change ("the
design lives in design 004, now approved") in a future editorial pass. This is
cosmetic; it does not affect implementation correctness.
