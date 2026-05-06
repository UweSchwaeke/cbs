# Design Review v3: Configurable VersionDescriptor Location

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/004-20260429T1319-configurable-version-descriptor-location.md`

**Prior reviews:**
`004-20260429T1633-design-configurable-version-descriptor-location-v1.md`,
`004-20260430T1208-design-configurable-version-descriptor-location-v2.md`

**Changes since v2:** No functional changes to design 004 itself. Cross-document
coherence check against design 005 (new).

---

## Summary

**Verdict: approve, no changes needed.**

Design 004 is unchanged since v2. All v1 and v2 findings remain closed. Design
005 builds directly on top of design 004: it uses the `resolve_root` helper from
design 004 and the `--versions-dir` flag unchanged. The write path
`<root>/<type>/<UUIDv7>.json` is the standard design 004 path applied to a
UUIDv7 string as the VERSION. No contradictions; no new issues introduced.

---

## Cross-Document Coherence with Design 005

Design 005 §Context explicitly defers to "design 004 OQ1+OQ2" for root
resolution and states that write path uses the standard
`<root>/<type>/<UUIDv7>.json` shape. Design 005 §Design Sketch / §Resolver
confirms `resolve_version` lives _alongside_ `resolve_root` in
`cbscore/src/versions/mod.rs`, and that the UUIDv7 write path continues to use
`resolve_root` from design 004. The two designs are compositional, not
conflicting.

Design 005 §Migration table lists `cbscore/Cargo.toml` (Step 1),
`cbscore/src/versions/mod.rs` (Step 2), `cbscore/src/versions/create.rs` (Step
3), and `cbsbuild/src/cmds/versions.rs` (Step 4). None of these steps modify
`cbscore-types/src/config/paths.rs` or the resolver in
`cbscore/src/versions/mod.rs` that design 004 owns — they add to it. No
collision.

---

## Summary of Action Items

None.
