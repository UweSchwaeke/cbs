# Design Review v6: cbscore Rust Port — Project Structure & Crate Layout

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/001-20260418T2045-cbscore-project-structure.md`

**Prior reviews:** `001-20260420T1132-design-cbscore-project-structure-v1.md`,
`001-20260427T1330-design-cbscore-project-structure-v2.md`,
`001-20260428T1401-design-cbscore-project-structure-v3.md`,
`001-20260429T0929-design-cbscore-project-structure-v4.md`,
`001-20260430T1208-design-cbscore-project-structure-v5.md`

**Changes since v5:** No functional changes. Cross-document coherence check
against design 005 (new). The `uuid` dep sketch lists `features = ["v4"]`;
design 005 adds `"v7"` to that list — this lands in `cbscore/Cargo.toml`, not
`cbscore-types/Cargo.toml`, which is correct per the split.

---

## Summary

**Verdict: approve, no changes needed.**

Design 001 is unchanged since v5. The single NICE-TO-HAVE finding from v5
(reqwest pin drift) was already closed before that review. The cross-document
coherence check against design 005 finds no contradictions: the `uuid` feature
addition from design 005 (`["v4", "v7"]`) lands in `cbscore/Cargo.toml`, which
is the correct crate per design 001's split. No new issues are introduced by
design 005 in this crate.

---

## Cross-Document Coherence with Design 005

Design 005 §Cargo dep delta specifies:

```toml
uuid = { version = "1", features = ["v4", "v7"] }
```

Design 001 §Crate Dependencies / `cbscore` already carries `uuid` with
`features = ["v4"]`. The addition of `"v7"` is mechanical and lands in
`cbscore/Cargo.toml` — consistent with design 001's rule that IO-bound and
computation-heavy logic (including UUID generation via the system clock) lives
in `cbscore`, not in `cbscore-types`. No change to `cbscore-types` deps is
needed because UUIDv7 generation does not happen in the types crate.

---

## Summary of Action Items

None.
