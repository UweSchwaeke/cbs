# Design Review v2: Configurable VersionDescriptor Location

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/004-20260429T1319-configurable-version-descriptor-location.md`

**Prior reviews:**
`004-20260429T1633-design-configurable-version-descriptor-location-v1.md`

**Commits reviewed since v1:** `f0069f2` — addresses I1 IMPORTANT, M1 MINOR, and
the cosmetic update on design 002.

---

## Summary

**Verdict: approve, no changes needed.**

All three `f0069f2` fixes are correct. The `cbscore_types::` namespace is
consistent at every occurrence. The resolver sketch is type-correct and the
`<unknown>` sentinel is the right fallback. The design 002 cosmetic reads
cleanly. The suite is coherent end-to-end.

---

## f0069f2 Fix Verification

### I1 — `descriptor_path` namespace (IMPORTANT)

The review required three sites to change from
`cbscore::versions::desc::descriptor_path` to
`cbscore_types::versions::desc::descriptor_path`.

Checking the current document:

| Location                                   | Text                                                                                           | Correct? |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------- | -------- |
| §OQ3 rationale, "Single read/write" bullet | `cbscore_types::versions::desc::descriptor_path(root, type, version) -> Utf8PathBuf`           | ✓        |
| §Design Sketch / §Path builder heading     | `` `cbscore_types::versions::desc::descriptor_path` in `cbscore-types/src/versions/desc.rs` `` | ✓        |
| §Design Sketch / §Write site               | `let path = cbscore_types::versions::desc::descriptor_path(`                                   | ✓        |

All three occurrences use `cbscore_types::`. The added explanatory sentence
("The helper is pure (a chain of `Utf8Path::join` calls), has no IO or async,
and lives in `cbscore-types` per the cbscore-types-vs-cbscore split in design
001 (types in `cbscore-types`, IO in `cbscore`).") is accurate and useful.
**CLOSED.**

### M1 — `resolve_root` fallback type propagation (MINOR)

The new resolver sketch:

```rust
Err(_) => {
    // Best-effort cwd capture for the error context; never propagate
    // a std::io::Error here, that would bypass the OQ5 friendly text.
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| Utf8PathBuf::try_from(p).ok())
        .unwrap_or_else(|| Utf8PathBuf::from("<unknown>"));
    Err(VersionError::NoDescriptorRoot { cwd })
}
```

Type chain:

- `std::env::current_dir()` → `Result<PathBuf, io::Error>`
- `.ok()` → `Option<PathBuf>`
- `.and_then(|p| Utf8PathBuf::try_from(p).ok())`:
  `Utf8PathBuf::try_from(PathBuf)` returns
  `Result<Utf8PathBuf, FromPathBufError>`; `.ok()` converts to
  `Option<Utf8PathBuf>`; the overall chain is
  `Option<PathBuf>.and_then(...) -> Option<Utf8PathBuf>` ✓
- `.unwrap_or_else(|| Utf8PathBuf::from("<unknown>"))` → `Utf8PathBuf` ✓
- `Err(VersionError::NoDescriptorRoot { cwd })` — `cwd: Utf8PathBuf` ✓

The chain is type-correct. In the degenerate case (cwd deleted, or non-UTF-8
path) the sentinel `<unknown>` is produced and `NoDescriptorRoot` fires with the
OQ5-specified message. No `io::Error` can escape. The accompanying prose ("The
`cwd` is captured best-effort: if `std::env::current_dir()` itself fails … the
error renders with `<unknown>` rather than propagating a `std::io::Error` that
would mask the intended message.") correctly explains the intent. **CLOSED.**

### Cosmetic on design 002

The §Version Descriptors & Creation paragraph in design 002 now reads "is
approved for M1 implementation. The default fallback (no flag, no config)
preserves the Python behaviour at runtime by resolving to
`<git-root>/_versions/<type>`." Both the status and the fallback description are
accurate. **CLOSED.**

---

## Cross-Doc Coherence

### `cbscore-types` / `cbscore` split — design 001 ↔ 004

Design 001 rule: types (including pure helpers) in `cbscore-types`; IO and async
in `cbscore`. Design 004:

- `descriptor_path` — pure `Utf8Path::join` chain, no IO, no async →
  `cbscore-types::versions::desc`. ✓
- `resolve_root` — async, calls `cbscore::utils::git::repo_root()` →
  `cbscore::versions`. ✓
- `VersionError::NoDescriptorRoot` — error type, no IO → stays in
  `cbscore-types::versions::errors`. Constructor lives in
  `cbscore::versions::resolve_root`. Split is correct. ✓

### `PathsConfig` field ↔ design 001 crate responsibilities

Design 004 §Design Sketch adds `versions: Option<Utf8PathBuf>` with
`#[serde(default)]` to `cbscore-types/src/config/paths.rs`. This is consistent
with design 001's description of `cbscore-types` responsibilities: "Config
structs (`Config`, `PathsConfig`, …)." ✓

### OQ7 ↔ design 003 bypass list

Design 004 OQ7 specifies the bypass pre-fill value as `/cbs/_versions`. Design
003 §Bypass Behaviour lists `/cbs/_versions` for `Config.paths.versions`. The
values match. ✓

### Migration Step 5 deferral ↔ design 003 status

Design 004 §Migration Step 5 defers the interactive prompt to design 003
(post-M1). Design 003 is marked "Deferred from M1." ✓

---

## v1 Suggestions — Status

**S1 (explicit `create_dir_all` at write-site):** Remains open as a suggestion
for the implementation commit. No text change required.

**S2 (M1 bypass-mode note in operator scenario table):** The table row for
`--for-systemd-install` still says "Re-run
`cbsbuild config init --for-systemd-install`". Until design 003 ships (post-M1),
the interactive path is not implemented and the regenerated config will _not_
include `paths.versions` from that flag alone. The M1 flow is purely
flag-driven; operators need to add `paths.versions: /cbs/_versions` by hand
until design 003 lands. The note from v1 S2 stands as a suggestion for a future
editorial pass. Non-blocking.

---

## Summary of Action Items

None required before implementation.
