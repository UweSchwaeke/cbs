# Design Review v5: Configurable VersionDescriptor Location

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/004-20260429T1319-configurable-version-descriptor-location.md`

**Prior reviews:**
`004-20260429T1633-design-configurable-version-descriptor-location-v1.md`,
`004-20260430T1208-design-configurable-version-descriptor-location-v2.md`,
`004-20260506T1000-design-configurable-version-descriptor-location-v3.md`,
`004-20260506T1400-design-configurable-version-descriptor-location-v4.md`

**Changes since v4:** None. Design 004 is unchanged on disk since the v4 review.

---

## Summary Assessment

**Verdict: approve, no changes needed.**

Design 004 is unchanged since v4. All prior findings remain closed. This pass
probed two angles not covered by earlier reviews: the `create_dir_all` ambiguity
note in the write-site sketch, and the `VersionError::NoDescriptorRoot` Display
contract. Both check out. No regression, no new coherence gap.

---

## Fresh Probe A: `create_dir_all` in the write-site sketch

The §Write site sketch ends with: "The `create_dir_all` already lives in
`Config::store` per design 002 F3 — but `desc.write` (a `VersionDescriptor`
method) needs the same behaviour, OR the call site does the `mkdir -p`
explicitly. Decide at implementation time; either is correct."

The ambiguity is intentional and appropriate: whether `mkdir -p` lives in
`VersionDescriptor::write` or at the call site is an implementation convenience
choice, not an architectural one. The write-site sketch shows the explicit
`create_dir_all_async` call, which is one valid choice. The design is not
under-specifying a correctness-relevant behaviour; it is correctly deferring a
code-placement detail to the implementer. No issue.

## Fresh Probe B: `VersionError::NoDescriptorRoot` Display contract

The design specifies a multi-line error message that names both `--versions-dir`
and `Config.paths.versions` as the two remedies (OQ5, §Resolver). The variant
carries a `cwd` field (captured best-effort), and the fallback to `<unknown>`
when `current_dir()` fails or the path is not UTF-8 is explicitly documented.
The Display contract is sufficiently specified for an implementer to write the
`thiserror` `#[error("...")]` attribute without ambiguity. No issue.

---

## Summary of Action Items

None.
