# Design Review v7: cbscore Rust Port — Architecture & Subsystem Design

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`

**Prior reviews:** `002-20260420T1132-design-cbscore-rust-port-design-v1.md`,
`002-20260420T1512-design-cbscore-rust-port-design-v2.md`,
`002-20260427T1330-design-cbscore-rust-port-design-v3.md`,
`002-20260428T1401-design-cbscore-rust-port-design-v4.md`,
`002-20260429T0929-design-cbscore-rust-port-design-v5.md`,
`002-20260429T1633-design-cbscore-rust-port-design-v6.md`

**Commits reviewed since v6:** `f0069f2` (cosmetic update to §Version
Descriptors & Creation — "currently in the discussion phase" → "approved for M1
implementation").

---

## Summary

**Verdict: approve, no changes needed.**

The cosmetic fix is correct and complete. The §Version Descriptors & Creation
paragraph now accurately describes the current state of design 004. The
`PathsConfig` sketch in §Configuration & Secrets does not include the `versions`
field added by design 004; this is expected — design 004 is the authoritative
add, not a retrofit of design 002. No new issues introduced.

---

## f0069f2 Verification

### Cosmetic update to §Version Descriptors & Creation

The updated paragraph reads:

> The Rust port treats the descriptor-store location as configurable; the design
> lives in design 004 and is approved for M1 implementation. The default
> fallback (no flag, no config) preserves the Python behaviour at runtime by
> resolving to `<git-root>/_versions/<type>`.

Three things to verify:

1. **Status phrase is accurate.** Design 004 carries the status header
   "Approved, ready for M1 implementation." The phrase "approved for M1
   implementation" is a faithful summary. ✓

2. **Fallback description is accurate.** Design 004 OQ2 resolves to
   `<git-rev-parse --show-toplevel>/_versions` at runtime when neither
   `--versions-dir` nor `Config.paths.versions` is set. The phrase "resolving to
   `<git-root>/_versions/<type>`" is correct — the `<type>` subdirectory
   (`release/`, `dev/`, etc.) is still part of the runtime path per OQ3. ✓

3. **The v6 "no action required" note:** The v6 review called this "minor doc
   drift, not a design flaw" and listed it as needing a word change. The word
   change has landed. ✓

---

## PathsConfig Sketch — Expected Drift

The §Configuration & Secrets `PathsConfig` sketch (lines 454–460) shows:

```rust
pub struct PathsConfig {
    pub components:         Vec<Utf8PathBuf>,
    pub scratch:            Utf8PathBuf,
    pub scratch_containers: Utf8PathBuf,
    #[serde(default)]
    pub ccache:             Option<Utf8PathBuf>,
}
```

Design 004 adds `versions: Option<Utf8PathBuf>` to this struct. The field is not
present in design 002's sketch. This is intentional: design 002 is the base
architecture document; design 004 is the authoritative source for the `versions`
addition and carries the definitive `PathsConfig` snapshot in its §Design Sketch
/ §Config schema. Retroactively updating design 002's sketch every time a
follow-up design adds a field would make design 002 a redundant,
perpetually-stale mirror. The correct mental model is: design 002 shows the
shape at the time of writing; follow-up designs (004, and any future ones)
extend it.

This is not a finding — it is documentation of expected drift. No text change to
design 002 is warranted.

---

## Summary of Action Items

None.
