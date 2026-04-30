# Design Review v5: cbscore Rust Port — Project Structure & Crate Layout

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/001-20260418T2045-cbscore-project-structure.md`

**Prior reviews:** `001-20260420T1132-design-cbscore-project-structure-v1.md`,
`001-20260427T1330-design-cbscore-project-structure-v2.md`,
`001-20260428T1401-design-cbscore-project-structure-v3.md`,
`001-20260429T0929-design-cbscore-project-structure-v4.md`

**Commits reviewed since v4:** `f0069f2` (design 004 v1 review fixes — no
changes to design 001 itself; reviewed for cross-doc coherence with design 004's
`Config.paths.versions` addition and the lift-out invariants).

---

## Summary

**Verdict: approve, no changes needed.**

The v4 IMPORTANT finding (migration recipe step 3 ambiguity) is closed exactly
as specified in the review. No changes were made to design 001 in `f0069f2`; the
cross-document coherence check against design 004's new `Config.paths.versions`
field finds no contradictions.

---

## v4 Finding Verification

**F1 (migration recipe step 3 — IMPORTANT):** CLOSED. The recipe now reads:

> 3\. Add `cbscommon-rs` to the workspace root `Cargo.toml` and as a dependency
> in cbscore's `[dependencies]`. Add the full allowlist deps (`tokio`,
> `tracing`, `thiserror`, `regex`, `camino`, `which`) to
> `cbscommon-rs/Cargo.toml`. From cbscore's `Cargo.toml`, remove only the deps
> that are now exclusively used by `cbscommon-rs` — in practice `regex` (used
> solely by the `_sanitize_cmd` redaction logic) and `which` (used solely by the
> git binary lookup). The other four (`tokio`, `tracing`, `thiserror`, `camino`)
> stay in cbscore because they are used throughout the rest of the library.

This matches the resolution text from the v4 review verbatim. The ambiguity that
would have caused a broken workspace if followed literally is gone.

---

## Cross-Document Coherence with Design 004

Design 004 adds `versions: Option<Utf8PathBuf>` to `PathsConfig`, landing in
`cbscore-types/src/config/paths.rs`. The design 001 rule — "types that cross
process and file boundaries go in `cbscore-types`; IO goes in `cbscore`" —
applies cleanly. `PathsConfig` is a pure config struct with no IO; adding a
field to it is consistent with that rule.

The lift-out invariants for `utils::git` and `utils::subprocess` constrain only
those two modules. They say nothing about the `versions` module or about
`PathsConfig`. Design 004's additions are in a different part of the crate graph
and the lift-out constraints do not touch them.

No design 001 text requires updating.

---

## Summary of Action Items

None.
