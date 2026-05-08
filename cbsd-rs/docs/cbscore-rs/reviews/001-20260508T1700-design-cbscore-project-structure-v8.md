# Design Review v8: cbscore Rust Port — Project Structure & Crate Layout

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/001-20260418T2045-cbscore-project-structure.md`

**Prior reviews:** `001-20260420T1132-design-cbscore-project-structure-v1.md`,
`001-20260427T1330-design-cbscore-project-structure-v2.md`,
`001-20260428T1401-design-cbscore-project-structure-v3.md`,
`001-20260429T0929-design-cbscore-project-structure-v4.md`,
`001-20260430T1208-design-cbscore-project-structure-v5.md`,
`001-20260506T1000-design-cbscore-project-structure-v6.md`,
`001-20260506T1400-design-cbscore-project-structure-v7.md`

**Changes since v7:** None. Design 001 is unchanged on disk since the v7 review.

---

## Summary Assessment

**Verdict: approve, no changes needed.**

Design 001 is unchanged since v7. All prior findings remain closed. No design in
this review cycle (designs 002–005) introduces any change that touches the crate
split, workspace layout, lift-out invariants, Cargo dependency sketches, or
versioning policy described here. No regression, no new cross-document coherence
gap.

---

## Fresh Probe: lift-out invariants vs. design 005 uuid dep

This pass probed an angle not covered by earlier reviews: the `uuid` crate added
by design 005 (`cbscore/Cargo.toml` —
`uuid = { version = "1", features = ["v4", "v7"] }`). The v4 feature was already
present (design 001 §cbscore Cargo sketch); v7 is the design 005 addition.

Design 001's lift-out invariants for `cbscore::utils::subprocess` and
`cbscore::utils::git` explicitly enumerate the allowed deps for those two
modules (`tokio`, `tracing`, `thiserror`, `regex`, `camino`, `which`). The
`uuid` crate is not on that list — which is correct, because `Uuid::now_v7()` is
called from `cbscore::versions::create`, not from either lift-out candidate. The
placement is consistent with the invariant. No issue.

---

## Summary of Action Items

None.
