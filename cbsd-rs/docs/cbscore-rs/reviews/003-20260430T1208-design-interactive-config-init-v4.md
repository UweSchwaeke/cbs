# Design Review v4: Interactive `config init` for `cbsbuild`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/003-20260427T1255-interactive-config-init.md`

**Prior reviews:** `003-20260428T1401-design-interactive-config-init-v1.md`,
`003-20260429T0929-design-interactive-config-init-v2.md`,
`003-20260429T1633-design-interactive-config-init-v3.md`

**Commits reviewed since v3:** `f0069f2` — no changes to design 003 in this
commit. Reviewed for cross-doc coherence with design 004's `f0069f2` fixes and
the end-to-end suite status.

---

## Summary

**Verdict: approve, no changes needed.**

Design 003 was not touched by `f0069f2`. All five v1 findings remain closed
(verified in v2 and v3). The design's references to design 004 OQ2, OQ7, and the
`--versions-dir` flag are consistent with the current (post-`f0069f2`) state of
design 004. No coherence gaps introduced.

---

## Cross-Doc Coherence Check

### Step 6 in §config_init_paths ↔ design 004 OQ7 (bypass pre-fill)

Design 003 §Bypass Behaviour lists:

> `--for-systemd-install`: pre-fill paths for the systemd worker layout
> (`/cbs/components`, `/cbs/scratch`, `/cbs/_versions` for
> `Config.paths.versions` per design 004 OQ7, etc.)

Design 004 OQ7 specifies:

> `init.paths.versions = Some(Utf8PathBuf::from("/cbs/_versions"))`

The value `/cbs/_versions` matches. The OQ7 cross-reference is accurate. ✓

### Step 6 in §config_init_paths ↔ design 004 OQ2 (fallback)

Design 003 Step 6 reads:

> The field is `Config.paths.versions` (added by design 004); when unset,
> cbscore-rs falls back at runtime to `<git-root>/_versions` (per design 004
> OQ2).

Design 004 OQ2 specifies exactly this fallback. ✓

### `--versions-dir` flag name ↔ design 004 §CLI flag

Design 003 per-field flags list includes `--versions-dir (design 004)`. Design
004 §CLI flag defines:

```rust
#[arg(long, value_name = "PATH")]
versions_dir: Option<Utf8PathBuf>,
```

clap renders this as `--versions-dir`. ✓

### Migration table Step 5 (design 004) ↔ design 003 scope

Design 004 §Migration Step 5 correctly marks the interactive prompt
(`cbsbuild/src/cmds/config/init.rs`) as `post-M1, design 003`. Design 003 is
marked "Deferred from M1." The two documents agree on deferral timing. ✓

---

## Summary of Action Items

None.
