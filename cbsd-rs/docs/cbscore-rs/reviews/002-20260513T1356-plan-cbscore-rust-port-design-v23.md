# Review v23 — cbscore-rs Plan + Design: Pre-Impl Audit Pass 3 Closure Confirmation

**Date:** 2026-05-13 **Commit reviewed:** `1a88722` **Reviewer:** Staff Engineer
(design-reviewer agent) **Corpus:** design 002 + plans 01–05 (cbscore-rs Phase
1–5)

---

## §Scope

Focused confirmation review of the 11 closures from pre-implementation audit
pass 3, delivered in a single commit (`1a88722`). Findings span C1-1, C1-2,
C1-3, C2-1, C4-1, C4-2, C4-3, C5-1, C6-1, C6-2, C6-3, C6-4, and C7-1 (13 IDs
across 7 categories, 11 discrete closures). Scope is strictly limited to:

1. Verifying each stated closure is actually present in the text.
2. Verifying no prior closure from v15–v22 / audit-pass-1 / audit-pass-2 was
   disturbed.
3. Checking that no new contradiction or hedge was introduced by the edits.

No re-examination of already-confirmed design decisions from prior review
rounds.

---

## §Method

All verification performed by direct `grep` and `sed` inspection of the six
files modified in `1a88722`:

- `cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-02-subprocess-and-shell-tools.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-04-runner.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-05-builder-and-releases.md`

Each check cites the line numbers observed at review time.

---

## §Closure Verification

### C1-1 — `run_build` missing `secrets` parameter

**Claim:** `run_build` in design 002 §Build Pipeline orchestrator sketch now
carries `secrets: &SecretsMgr` as the third parameter.

**Verified.** Design 002 lines 1069–1074:

```
pub async fn run_build(
    desc:    &VersionDescriptor,
    config:  &Config,
    secrets: &SecretsMgr,
    opts:    &BuildOptions,
) -> Result<BuildArtifactReport, BuilderError> {
```

Four parameters present; `secrets: &SecretsMgr` is the third. Closed.

---

### C1-2 — `signing::run` / `upload::run` call sites not updated

**Claim:** Both call sites in the orchestrator sketch pass `secrets` through.

**Verified.** Design 002 lines 1077–1078:

```
let signed   = signing::run(desc, config, secrets, &rpms).await?;
let uploaded = upload::run(desc, config, secrets, &signed).await?;
```

Both carry `secrets` as the third positional argument, matching the corrected
`run_build` signature. Closed.

---

### C1-3 — `Config::load` / `Config::store` sketches use sync `fn`

**Claim:** Both occurrences in design 002 (§Wire-Format Versioning and
§Configuration & Secrets Subsystem) now use `pub async fn`.

**Verified.** Four matching lines found; all are `pub async fn`:

- Line 415: `pub async fn load(path: &Path) -> Result<Config, ConfigError>`
- Line 421:
  `pub async fn store(cfg: &Config, path: &Path) -> Result<(), ConfigError>`
- Line 543: `pub async fn load(path: &Utf8Path) -> Result<Config, ConfigError>`
- Line 555:
  `pub async fn store(&self, path: &Utf8Path) -> Result<(), ConfigError>`

No sync `pub fn load` or `pub fn store` remains in design 002. Closed.

---

### C2-1 — `ComponentError` defined in Phase 5 rather than `cbscore-types`

**Claim:** Phase 1 Commit 2 §Files declares `ComponentError` in
`cbscore-types/src/core/component/errors.rs` with the three canonical variants;
Phase 5 Commit 2 removes the hedge and imports from `cbscore-types`.

**Verified — Phase 1.** Lines 216–222 of `plans/002-20260508T1558-01-types.md`:

```
- `cbsd-rs/cbscore-types/src/core/component/errors.rs` — `ComponentError` with
  variants: `Walk { source: io::Error }` (directory walk failure),
  `Parse { path: Utf8PathBuf, source: SchemaVersionError }` (per-file
  YAML/schema-version parse failure, used at WARN level by the loader),
  `DuplicateComponentName { name: String, first: Utf8PathBuf, second: Utf8PathBuf }`
  (two `cbs.component.yaml` files claim the same `name:`). Consumed by Phase 5
  Commit 2's `cbscore::core::component::load_components`; lives in
```

All three variants present with correct field types.

**Verified — Phase 5.** No `ComponentError` definition, no "add here if not"
hedge. Line 170 of `plans/002-20260508T1558-05-builder-and-releases.md`
explicitly states: "`ComponentError` is **declared in Phase 1 Commit 2** at …".
Lines 163, 187, and 207 reference `ComponentError` as a consumed type only.
Closed.

---

### C4-1 / C4-2 / C4-3 — logger.rs target enumeration incomplete / uses `…`

**Claim:** Phase 1 Commit 2 §Files logger.rs section enumerates all 22 tracing
targets explicitly; the `…` placeholder is removed.

**Verified.** Lines 162–191 of `plans/002-20260508T1558-01-types.md` carry the
full enumeration under the heading "**Target enumeration (pinned — no `…`
placeholder).**" All 22 spot-check targets confirmed present:

| Target                         | Present  |
| ------------------------------ | -------- |
| `"cbscore"`                    | line 165 |
| `"cbscore::config"`            | line 166 |
| `"cbscore::core::component"`   | line 167 |
| `"cbscore::secrets"`           | line 168 |
| `"cbscore::runner"`            | line 169 |
| `"cbscore::builder"`           | line 170 |
| `"cbscore::builder::prepare"`  | line 171 |
| `"cbscore::builder::rpmbuild"` | line 173 |
| `"cbscore::builder::signing"`  | line 174 |
| `"cbscore::builder::upload"`   | line 175 |
| `"cbscore::containers"`        | line 176 |
| `"cbscore::images::skopeo"`    | line 177 |
| `"cbscore::images::signing"`   | line 178 |
| `"cbscore::images::sync"`      | line 179 |
| `"cbscore::releases"`          | line 180 |
| `"cbscore::utils::buildah"`    | line 181 |
| `"cbscore::utils::git"`        | line 182 |
| `"cbscore::utils::podman"`     | line 184 |
| `"cbscore::utils::s3"`         | line 185 |
| `"cbscore::utils::subprocess"` | line 186 |
| `"cbscore::utils::vault"`      | line 188 |
| `"cbscore::versions"`          | line 189 |

The enumeration is in Phase 1 Commit 2 §Files. No `…` placeholder found anywhere
in the logger section. C4-1, C4-2, C4-3 all closed.

---

### C5-1 — `serde-value` missing from `cbscore-types/Cargo.toml`

**Claim:** Phase 1 Commit 1 §Files lists `serde-value = "0.7"` in
`cbscore-types/Cargo.toml` `[dependencies]` with an explanation of why it is
needed.

**Verified.** Lines 92–106 of `plans/002-20260508T1558-01-types.md` specify the
`cbscore-types/Cargo.toml` as including `serde-value = "0.7"` (consumed as
`serde_value` in the hand-rolled `Deserialize` fallback for integer-tag
schema-version dispatch — see design 002 §Wire-Format Versioning sketch). Lines
102–106 provide explicit rationale: required by the Commit 5 hand-rolled
`Deserialize` fallback that reads the schema-version marker as a
`serde_value::Value` before dispatching into the appropriate `VersionedX`
variant. Closed.

---

### C6-1 — Phase 5 Commit 5 `signing::run` citation used stale line number

**Claim:** Phase 5 Commit 5 §Design constraints no longer cites the old design
002 line number for `signing::run`; replaced with a section-name reference.

**Verified.** Lines 317–322 of
`plans/002-20260508T1558-05-builder-and-releases.md`:

```
- `signing::run`'s signature carries `secrets: &SecretsMgr` per design 002
  …
  closure (signature pinned in design 002 §Build Pipeline orchestrator sketch).
```

Section-name reference confirmed; no stale line 925 citation remains.
`grep "line 925"` across all five plan files returns zero hits. Closed.

---

### C6-2 — Phase 3 §Depends on Migration Strategy used stale line number

**Claim:** Phase 3 §Depends on citation for Migration Strategy now uses section
name only.

**Verified.** Line 47 of `plans/002-20260508T1558-03-storage-and-secrets.md`:

```
ordering in the README reflects design 002 §Migration Strategy
```

No line 1272 citation remains. `grep "line 1272"` across all five plan files
returns zero hits. Closed.

---

### C6-3 — Phase 4 Commit 3 `config.vault` citation used stale line number

**Claim:** Phase 4 Commit 3 `config.vault` reference now cites §Configuration &
Secrets Subsystem by section name only.

**Verified.** Line 213 of `plans/002-20260508T1558-04-runner.md`:

```
`config.vault: Option<Utf8PathBuf>` (per design 002 §Configuration &
```

No line 449 citation remains. `grep "line 449"` across all five plan files
returns zero hits. Closed.

---

### C6-4 — Phase 1 Commit 5 integer-tag fallback cited stale line range

**Claim:** Phase 1 Commit 5 §Design constraints integer-tag fallback now cites
§Wire-Format Versioning by section name and adds explicit implementation order
(default-serde first; hand-rolled fallback if round-trip drift surfaces).

**Verified.** Lines 392–403 of `plans/002-20260508T1558-01-types.md`:

```
- Serde's internal-tag dispatch on integer-valued tags may need a hand-rolled
  `Deserialize` if string-matching does not accept integer-valued tags directly
  (design 002 §Wire-Format Versioning, `VersionedConfig` doc-comment + sketch).
  Implementation order: try the default `#[serde(tag = "...", rename = "1")]`
  approach first; if the round-trip corpus surfaces quoted-string drift, drop to
  the hand-rolled `Deserialize` using `serde_value::Value::deserialize` per the
  design 002 sketch.
```

Section-name citation confirmed; implementation order is explicit. No line
343–345 citation remains. `grep "line 343\|line 344\|line 345"` across all five
plan files returns zero hits. Closed.

---

### C7-1 — Phase 2 Commit 5 test assertions inconsistently wrapped

**Claim:** Phase 2 Commit 5 §Testable assertions for `get_major_version` and
`get_minor_version` all use `Ok(...)` wrappers.

**Verified.** Lines 323–325 of
`plans/002-20260508T1558-02-subprocess-and-shell-tools.md`:

```
- `get_major_version("ces-v19.2.3-dev.1")` → `Ok("19".to_string())`.
- `get_minor_version("ces-v19.2.3-dev.1")` → `Ok(Some("19.2.3".to_string()))`;
  `get_minor_version("ces-v19.2")` → `Ok(None)` (patch missing — well-formed
```

All three assertions wrapped in `Ok(...)`. Consistent with declared return types
`Result<String, MalformedVersion>` and
`Result<Option<String>, MalformedVersion>` at lines 290–291. Closed.

---

## §No-Drift Check

Spot-checks of closures from prior rounds (v15–v22, audit-pass-1, audit-pass-2)
that are most likely to be disturbed by the edits in `1a88722`:

1. **Phase 5 has 7 commits with `core::component` as Commit 2.** Confirmed.
   README line 19: `| Phase 5 | … | 7 | Pending |`; Phase 5 plan line 16:
   `| 2 | cbscore: add core::component module (load_components IO) | ~200 | Pending |`.

2. **`HashMap<String, *Creds>` for the four secrets families in Phase 3
   Commit 3.** Confirmed. Phase 3 plan line 219:
   `Secrets { git: HashMap<String, GitCreds>, storage: HashMap<String, StorageCreds>, signing: HashMap<String, SigningCreds>, registry: HashMap<String, RegistryCreds> }`.

3. **Design 002 §Storage secrets subsection with full enum tree.** Confirmed.
   Design 002 lines 634, 641, 652: `enum StorageCreds`,
   `enum StoragePlainCreds`, `enum StorageVaultCreds` all present.

4. **Design 004 §Write site `write_descriptor` handles `mkdir -p` internally.**
   Confirmed. Design 004 lines 347–353: "`The create_dir_all` lives **inside
   `write_descriptor`** — the helper creates…".

5. **Design 005 patch walker uses `target: "cbscore::builder::prepare"`.**
   Confirmed. Design 005 line 503: `target: "cbscore::builder::prepare"`.

6. **README Phase 5 row: "7"; total estimate "~26–32".** Confirmed. README lines
   19 and 23.

7. **seq-004 §Status section reads "Approved — finalized".** Confirmed. Plan
   `004-20260513T0900-configurable-version-descriptor-location.md` line 5.

No prior closure disturbed.

---

## §Findings

No new findings. All 13 closure IDs verified. No new contradictions, hedges, or
stale citations introduced by the edits in `1a88722`. The 22 tracing targets in
the logger enumeration are internally consistent with every downstream target
reference found across the plan corpus. The `serde-value` dependency entry
includes sufficient rationale to survive a future Cargo.toml audit. The
section-name citations for C6-1 through C6-4 are stable across any future line
renumbering of design 002.

---

## §Verdict

> **Approve — C1-1+C1-2+C1-3+C2-1+C4-1+C4-2+C4-3+C5-1+C6-1+C6-2+C6-3+C6-4+C7-1
> closed; pre-impl audit pass 3 fully resolved; design corpus + plan corpus
> ready for Phase 1 implementation start.**
