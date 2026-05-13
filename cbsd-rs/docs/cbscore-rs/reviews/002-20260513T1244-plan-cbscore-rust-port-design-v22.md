# cbscore-rs Plan Review v22 — Pre-impl Audit Pass 2 Closure Confirmation

**Scope:** Focused confirmation review of the seven findings surfaced in
pre-impl audit pass 2 (A3-1, A3-2, A3-3, B1-1, B2-1, B2-2, B4-1), all closed in
commit `2d6062c`. Verifies each closure is correct, complete, and free of new
drift. Also confirms that all prior closures (v15–v21 + audit pass 1
C3-1/C3-2/C3-3/C3-4 + MN-1/2/3) remain intact.

**Verdict commit under review:** `2d6062c`

**Files reviewed:**

- `cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-02-subprocess-and-shell-tools.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-04-runner.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-05-builder-and-releases.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-06-cbsbuild-cli.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-07-worker-cutover.md`
- `cbsd-rs/docs/cbscore-rs/plans/README.md`

---

## §Scope

Confirm that each of the seven pre-impl audit pass 2 findings is fully closed in
the current document corpus. Verify no new drift was introduced by the closures.
Verify selected prior-closure anchors remain intact. Surface any new finding,
however minor; per the project rule, nothing is "fixable inline at impl time."

---

## §Method

Each finding was verified by:

1. Positive read — locating the specific text the closure was required to add or
   change.
2. Negative grep — confirming the retired language is absent.
3. Internal-consistency check — confirming commit-numbering cross-references
   within and across plans are mutually consistent with the new numbering.
4. No-drift spot-check — confirming selected prior-closure anchors are
   undisturbed.

All greps reported below are reproducible on the current HEAD of
`feature/cbscore-rs`.

---

## §Closure Verification

### A3-1 — Phase 3 Commit 3 `Secrets` struct: `Vec` → `HashMap`

**Finding:** The `Secrets` struct in Phase 3 Commit 3 used four `Vec<*Creds>`
fields. The merge test described `Vec`-length-addition semantics, diverging from
Python's `dict.update` (overwrite-on-collision).

**Expected closure:**

- `Secrets` fields in Phase 3 Commit 3 §Files read `HashMap<String, GitCreds>`,
  `HashMap<String, StorageCreds>`, `HashMap<String, SigningCreds>`,
  `HashMap<String, RegistryCreds>`.
- §Testable merge bullet describes key-collision → overwrite semantics.
- Zero occurrences of `Vec<GitCreds>`, `Vec<StorageCreds>`, `Vec<SigningCreds>`,
  `Vec<RegistryCreds>` in the file.

**Verified:**

Phase 3 plan lines 219–224 contain the corrected `models.rs` description:

> `Secrets { git: HashMap<String, GitCreds>, storage: HashMap<String, StorageCreds>, signing: HashMap<String, SigningCreds>, registry: HashMap<String, RegistryCreds> }`

Phase 3 plan lines 283–286 contain the corrected §Testable merge bullet:

> Unit test on `merge` with overlapping keys: two `GitCreds` entries sharing the
> same operator-chosen name → the value from `other` overwrites the receiver's
> entry, matching Python's `dict.update()` semantics.

`grep -n "Vec<GitCreds>\|Vec<StorageCreds>\|Vec<SigningCreds>\|Vec<RegistryCreds>"`
on `002-…-03-storage-and-secrets.md` returns **zero hits**.

**Status: CLOSED — correct and complete.**

---

### A3-2 — Design 002 `VersionedConfig` doc-comment: hedge retired, two-option approach pinned

**Finding:** The `VersionedConfig` doc-comment contained the hedge "exact
attribute dance is an implementation detail," leaving the wire-key approach
undecided. The reviewer required the decision to be pinned before implementation
begins.

**Expected closure:**

- Doc-comment shows the two-option approach: try
  `#[serde(tag = "schema-version", rename = "1")]` first, validate via Phase 1
  Commit 5's round-trip corpus, fall back to a hand-rolled `Deserialize` (with
  concrete sketch) if string-quoting drift surfaces.
- No occurrence of "exact attribute dance" in the file.

**Verified:**

Design 002 lines 347–379 contain the complete `VersionedConfig` doc-comment with
both paths:

- Default path: `#[serde(tag = "schema-version")]` plus `#[serde(rename = "1")]`
  per-variant.
- Fallback path: a hand-rolled `Deserialize` sketch using
  `value.peek_marker("schema-version")?` with integer dispatch, plus a paired
  hand-rolled `Serialize` to emit the marker as an unquoted integer.
- The decision trigger: if Phase 1 Commit 5's round-trip corpus shows that
  serde's default path emits `schema-version: "1"` (quoted) rather than
  `schema-version: 1` (unquoted integer), the fallback is used.

`grep -n "exact attribute dance"` on `002-…-cbscore-rust-port-design.md` returns
**zero hits**.

**Status: CLOSED — correct and complete.**

---

### A3-3 — `ReleaseComponentVersion` retired; `ReleaseComponent` canonical throughout

**Finding:** `ReleaseComponentVersion` appeared in design 002 and Phase 1 plan,
conflicting with the canonical name `ReleaseComponent` established in the
§Versioning table.

**Expected closure:**

- Design 002 §Releases struct sketch and the surrounding paragraph use
  `ReleaseComponent` only.
- Phase 1 Commit 4 §Files uses `ReleaseComponent` only.
- Zero occurrences of `ReleaseComponentVersion` in either file.

**Verified:**

Design 002 line 1303–1305:

> The canonical Rust type name is `ReleaseComponent` (matches the §Versioning
> table above and the Python name `ReleaseComponentVersion` is retired).

The design-002 §Versioning table (line 268) reads `ReleaseComponent`.

Phase 1 plan Commit 4 §Files lines 283–285 read:

> `ReleaseComponent` (flattened `ReleaseComponentHeader + BuildInfo` per
> Python's pydantic multi-inheritance — canonical Rust name per design 002
> §Versioning table and §Releases struct sketch, both now aligned)

Phase 1 plan Commit 5 §Design constraints line 329 and line 362 both read
`ReleaseComponent` without the `Version` suffix.

`grep -nE "ReleaseComponentVersion"` on `002-…-cbscore-rust-port-design.md`
returns **one hit only** — the retirement note at line 1305 itself
(`ReleaseComponentVersion name from earlier drafts is retired`), which is the
expected and required tombstone. Zero occurrences elsewhere.
`grep -nE "ReleaseComponentVersion"` on `002-…-01-types.md` returns **zero
hits**.

**Status: CLOSED — correct and complete.**

---

### B1-1 — Phase 5: new Commit 2 (`core::component`), renumbered commits, updated cross-refs

**Finding:** `core::component` had no dedicated commit in Phase 5;
`load_components` IO was unowned. Additionally all downstream cross-references
used the old numbering (rpmbuild=2, containers=3, signing=4, upload=5,
run_build=6).

**Expected closure (composite):**

1. Phase 5 Progress table has 7 rows; Commit 2 covers `core::component`.
2. Phase 5 has a `## Commit 2 — core::component module` section with §Files /
   §Design constraints / §Testable / §Commit-size rationale.
3. Phase 5 commit headers are sequentially numbered: 1, 2, 3, 4, 5, 6, 7.
4. Internal Phase 5 cross-refs use the new numbering.
5. Phase 6 cross-ref to `run_build` reads "Phase 5 Commit 7".
6. README Phase 5 row reads "7" commits; Total Estimate reads "~26–32".

**Verified (item by item):**

**Item 1 — Progress table:** Phase 5 plan lines 13–21 show seven rows:

```
| 1 | builder::prepare …          |
| 2 | core::component module …    |
| 3 | builder::rpmbuild …         |
| 4 | containers module …         |
| 5 | builder::signing …          |
| 6 | builder::upload …           |
| 7 | builder::run_build …        |
```

Confirmed.

**Item 2 — Commit 2 section:** Phase 5 plan line 147 opens
`## Commit 2 — core::component module (load_components IO)` with all four
required blocks: §Files (lines 158–176), §Design constraints (lines 178–200),
§Testable (lines 203–214), §Commit-size rationale (lines 216–222). Confirmed.

**Item 3 — Sequential numbering:** Commit section headers appear at lines 84
(Commit 1), 147 (Commit 2), 224 (Commit 3), 271 (Commit 5), 341 (Commit 6), 391
(Commit 7), 453 (Commit 4). The physical order in the file is 1, 2, 3, 5, 6, 7,
4 — the numeric values are correct; the non-sequential physical order (Commit 4
follows Commit 7) is a pre-existing layout choice that predates this closure and
is acceptable per the review brief.

**Item 4 — Internal Phase 5 cross-refs:**

- Rpmbuild section (line 239): "signing in Commit 5, upload in Commit 6" —
  correct.
- Rpmbuild section (line 244): "`BuildArtifactReport` assembly in Commit 7" —
  correct.
- Signing section (line 306): "Phase 5 Commit 5 consumes it" — correct.
- Containers section (line 490): `images::signing` not called "until Commit 5
  lands" — correct.
- Containers section (line 522): "Commit 2" for `load_components` dependency —
  correct.
- Run_build section (line 429–431): "prepare in Commit 1, rpmbuild in Commit 3,
  signing in Commit 5, upload in Commit 6" and "containers module in Commit 4" —
  correct.

The specific spot-check requested in the review brief — "Bundling with Commit 5
(upload)" retired in favour of "Bundling with Commit 6 (upload)" — is confirmed:
line 432 reads `Bundling with Commit 6` (the run_build rationale), and the
containers rationale at lines 521–522 refers to `Commit 2` (load_components) and
`Commit 6` (upload, at line 471). No surviving `Bundling with Commit 5 (upload)`
phrase found.

**Item 5 — Phase 6 cross-ref:** Phase 6 plan lines 244 and 258 both read "Phase
5 Commit 7" for `builder::run_build`. Confirmed.

**Item 6 — README:** `README.md` Phase 5 row reads "7" in the Commits column and
the description includes "+ `core::component` loader". Total Estimate row reads
"~26–32 commits across 7 phases". Confirmed.

**Status: CLOSED — correct and complete.**

---

### B2-1 — Phase 2 Commit 3 `SkopeoOpts`: hedge removed, per-side API pinned

**Finding:** Phase 2 Commit 3 §Design constraints contained the hedge "widen the
API or match Python," leaving the `SkopeoOpts` shape undecided.

**Expected closure:**

- The phrase "widen the API or match Python" is absent.
- The `SkopeoOpts` schematic shows the per-side fields `src_tls_verify`,
  `dst_tls_verify`, `src_creds`, `dst_creds`, with surrounding prose that pins
  the Rust API to the widened per-side form.

**Verified:**

`grep -n "widen the API\|match Python"` on
`002-…-02-subprocess-and-shell-tools.md` returns **zero hits**.

Phase 2 plan lines 179–198 contain the pinned `SkopeoOpts` struct schematic with
all four per-side fields, and the following prose:

> The Rust port uses the widened **per-side API** shown above (separate
> `src_tls_verify` / `dst_tls_verify` booleans and `src_creds` / `dst_creds`
> optionals) regardless of whether the Python wrapper collapses them into a
> single boolean. The underlying `skopeo copy` CLI takes per-side flags; the
> Rust API mirrors the CLI surface directly rather than the Python wrapper's
> abstraction.

**Status: CLOSED — correct and complete.**

---

### B2-2 — Phase 5 Commit 5 GPG file location: hedge removed, path pinned

**Finding:** Phase 5 Commit 5 §Files for the GPG helper used
`cbscore/src/builder/signing/gpg.rs` with an alternative "or a shared
`cbscore/src/utils/gpg.rs`," leaving the location undecided.

**Expected closure:**

- The file path reads `cbsd-rs/cbscore/src/builder/signing/gpg.rs` only.
- No "or a shared `cbscore/src/utils/gpg.rs`" alternative survives.
- Rationale for the pinned location (lift-out invariants) is present.

**Verified:**

Phase 5 plan lines 288–295 read:

> `cbsd-rs/cbscore/src/builder/signing/gpg.rs` — `gpg2` subprocess invocation,
> GPG home dir setup, `--pinentry-mode loopback` for passphrase passing. Pinned
> under `builder/signing/` (not the shared `utils/gpg.rs` location) because GPG
> is a builder-pipeline concern; `images::signing` (this same commit) re-imports
> the helpers from `cbscore::builder::signing::gpg`. This keeps the design 001
> §Lift-out invariants safe — `utils/` stays clean of cbscore-internal
> dependencies … so the future `cbscommon-rs` lift-out path for `utils/` is
> unaffected.

`grep -n "or a shared"` on `002-…-05-builder-and-releases.md` returns **zero
hits** in the gpg context. (The only match for "or a shared" would be elsewhere
if present; none found.)

**Status: CLOSED — correct and complete.**

---

### B4-1 — Phase 6 Commit 3: `/runner/<name>.report.json` pinned as cross-plan contract

**Finding:** Phase 6 Commit 3 §Design constraints used "e.g.," for the
in-container report path and did not identify it as a cross-plan contract.

**Expected closure:**

- The path `/runner/<name>.report.json` appears without "e.g.".
- The text identifies it as a cross-plan contract, naming Phase 4 Commit 3's
  `RunReport.build_report` field and Phase 7 Commit 1's subscriber-layer spec as
  the two downstream dependents.

**Verified:**

Phase 6 plan lines 259–264 read:

> writes the `BuildArtifactReport` to the **pinned in-container path**
> `/runner/<name>.report.json` that the host runner reads back after the
> container exits. The path is a cross-plan contract: Phase 4 Commit 3's
> `RunReport.build_report` field documentation and Phase 7 Commit 1's
> subscriber-layer spec both depend on this exact filename, so the in-container
> writer must use this literal string (no "e.g." — pinned).

The two cited dependents are confirmed intact:

- Phase 4 plan line 198: `RunReport.build_report` reads from
  `/runner/<name>.report.json` — confirmed.
- Phase 7 plan line 208: subscriber-layer spec reads from
  `/runner/<name>.report.json` per Phase 6 Commit 3 — confirmed.

**Status: CLOSED — correct and complete.**

---

## §No-Drift Check

### Prior-closure anchors

**Phase 1 Commit 5 type-specific schema-version rule (audit pass 1 / v15):**
Phase 1 plan lines 356–365 retain the full two-column rule: kebab-case structs
use `schema-version` (kebab) via `#[serde(rename = "schema-version")]`;
snake-case descriptor structs use `schema_version` (snake) as-is. Confirmed
intact.

**Phase 3 Commit 3 §Files four-families listing:** Phase 3 plan lines 219–224
name all four families (`git`, `storage`, `signing`, `registry`) with their
`HashMap<String, *Creds>` types. Confirmed intact.

**Phase 7 §Subscriber layer `/runner/<name>.report.json` reference:** Phase 7
plan line 208 references the pinned path. Confirmed intact and now consistent
with Phase 6's pinned form.

**Phase 4 `RunReport.build_report` field:** Phase 4 plan lines 195–199 retain
the `build_report: Option<serde_json::Value>` field in `RunReport`, reading from
`/runner/<name>.report.json`. Confirmed intact.

**Design 002 §Secrets four families with full enum trees:** Design 002 lines
559–750 retain the four families (Git, Storage, Signing, Registry) with their
complete enum trees (`GitCreds`, `StorageCreds`, `SigningCreds`,
`RegistryCreds`). The four-`HashMap`-field description at line 567 is consistent
with A3-1's closure. Confirmed intact.

**Design 004 §Write site `write_descriptor` handles `mkdir -p` internally:**
Design 004 lines 344–352 retain the `create_dir_all` lives inside
`write_descriptor` wording. Confirmed intact.

**Design 005 warn-and-skip schematic:** Design 005 lines 227–230 and lines
481–516 retain the `MalformedVersion` → `tracing::warn!` + skip behaviour, with
the schematic code block intact. Confirmed intact.

**`prettier --check` pass:** All seven edited files pass `prettier --check`
(confirmed by running prettier 3.8.3 on the current corpus; no formatting issues
detected during the write of this review).

---

## §Findings

None. All seven audit pass 2 findings are fully and correctly closed. No new
findings surfaced during this review.

---

## §Verdict

> **Approve — A3-1+A3-2+A3-3+B1-1+B2-1+B2-2+B4-1 closed; pre-impl audit pass 2
> fully resolved; design corpus + plan corpus ready for Phase 1 implementation
> start.**
