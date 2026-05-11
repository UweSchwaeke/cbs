# Plan Review v12 — cbscore Rust port (plan 002)

**Scope:** Confirmation pass after v11 fixes. Verify all v11 findings closed;
fresh-eyes sweep of Phase 5 post-restructure.

**Files reviewed:**

- `cbsd-rs/docs/cbscore-rs/plans/README.md` (Phase 5 description update)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md`
  (`transit_sign` addition)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-05-builder-and-releases.md`
  (substantial Phase 5 restructure)

---

## 1. Summary Assessment

All thirteen v11 findings (3 blockers, 1 major, 6 minors, 3 suggestions, 1 open
question) are confirmed closed. The Phase 5 restructure landed coherently:
commit ordering now matches the data-flow DAG, the three previously-undeclared
types are fully specified with module paths and field layouts, and
`upload::run`'s signature carries all required inputs. The corpus is ready for
Phase 5 implementation start subject to one new minor finding introduced by the
restructure.

---

## 2. V11 Closure Confirmations

All items below confirmed by direct text inspection of the modified files.

- **V11-B1 (BLOCKER) — `transit_sign` declaration.** Phase 3 Commit 2 §Design
  constraints now enumerates
  `transit_sign(config: &VaultConfig, key_name: &str, input: &str) -> Result<String, VaultError>`
  alongside `kv_read`, with per-call auth rationale and design 002 §Image Sign &
  Sync citation. Phase 5 Commit 4 says "declared in Phase 3 Commit 2 … no Phase
  5 changes to `utils/vault.rs`". **CLOSED.**

- **V11-B2 (BLOCKER) — `BuildOptions`, `RpmArtifact`, `ContainerImageReport`.**
  `BuildOptions` declared in Commit 1 `builder/mod.rs` with fields and design
  citations. `RpmArtifact` declared in Commit 2 `builder/rpmbuild.rs` with four
  named fields. `ContainerImageReport` declared in Commit 3
  `containers/build.rs` with three named fields. All three carry module paths
  and placement rationale. **CLOSED.**

- **V11-B3 (BLOCKER) — `upload::run` signature.** Commit 5 signature is
  `pub async fn run(desc, config, secrets: &SecretsMgr, signed: &SigningReport, image: &ContainerImageReport)`.
  §Design constraints cites both V11-B3 (registry creds) and V11-M1
  (forward-dependency elimination). **CLOSED.**

- **V11-M1 (MAJOR) — Commit reordering.** Progress table re-ordered 1–6:
  prepare, rpmbuild, containers+images::sync, signing+images::signing,
  upload+releases, orchestrator. Section headers match. Each commit's inputs
  come from earlier commits only. §Goal end-state chain:
  `prepare → rpmbuild → containers::build → signing → upload`. Commit 6 §Design
  constraints chain matches exactly. **CLOSED.**

- **V11-N1 — `signing::run` signature note.** Commit 4 §Design constraints
  explicitly names the deliberate divergence from design 002 line 925.
  **CLOSED.**

- **V11-N2 — `releases::s3` scope.** §Out of scope states that Phase 5's
  `releases::s3` lands only `upload_release`; `check_release_exists` and
  `check_released_components` defer to Phase 6. **CLOSED.**

- **V11-N3 — already-correct (no edit).** Confirmed still correct. **CLOSED.**

- **V11-N4 — `builder::signing` explicit no-op.** Commit 4 §Design constraints
  states `builder::signing::run` returns `Ok(SigningReport::empty())` without
  invoking subprocesses when `config.signing` is `None`. **CLOSED.**

- **V11-N5 — already-correct (no edit).** Confirmed still correct. **CLOSED.**

- **V11-N6 — README description.** README Phase 5 description row reads "builder
  pipeline stages + `run_build` orchestrator + releases + containers + image
  sign/sync". **CLOSED.**

- **V11-S1 — `RpmbuildReport` carries `Vec<RpmArtifact>`.** Commit 2 declares
  `pub struct RpmbuildReport { pub rpms: Vec<RpmArtifact>, pub component_builds: Vec<ComponentBuild> }`.
  **CLOSED.**

- **V11-S2 — Stub-stage test approach.** Commit 6 §Testable specifies the
  `Arc<Mutex<Vec<String>>>` side-channel technique with `#[cfg(test)]`-only
  parameter or `RunContext` extension. **CLOSED.**

- **V11-S3 — Unsupported repo variant error.** Commit 3 `containers::repos`
  names `ContainerError::UnsupportedRepoType { value }`. **CLOSED.**

- **V11-OQ1 — Forward dependency.** Resolved by M1 reordering; `upload::run`
  consumes `&ContainerImageReport` from the earlier-running Commit 3.
  **CLOSED.**

---

## 3. Blockers

None.

---

## 4. Major Concerns

None.

---

## 5. Minor Issues

### N1 — Commit 3 §Design constraints contradicts commit-ordering reality for `images::sync` signing

**What:** Commit 3 §Design constraints states: "images::sync orchestrates per
design 002 line 1098–1104: **sign before push, not after** — the order is
enforced by chaining `images::signing::sign_manifest` before `skopeo_copy`
rather than after." This is written as a present-tense constraint on _this
commit's_ `sync_image` implementation. But `images::signing` does not exist
until Commit 4, so `sync_image` in Commit 3 cannot actually call
`images::signing::sign_manifest` — it uses the optional-signing skip path
instead. The commit-size rationale block acknowledges this correctly, but
§Design constraints contradicts it.

A directly parallel problem appears in §Testable: the "order test — assert
`sign_manifest` is called before `skopeo_copy`" is listed under Commit 3's
testable items, but this test cannot be written (let alone pass) until
`images::signing` is implemented in Commit 4.

**Why it matters:** An implementer working from §Design constraints and
§Testable in isolation will either (a) try to call
`images::signing::sign_manifest` and hit a compile error, or (b) write a Commit
3 test for an ordering guarantee that the Commit 3 code does not yet establish.
The rationale block is well-written but it is supplementary reading; §Design
constraints and §Testable are the implementation contracts.

**Resolution:** Two targeted edits:

1. Revise §Design constraints for Commit 3 to read: "The invariant that signing
   precedes push is established in Commit 4 when `images::signing` lands; the
   Commit 3 implementation of `sync_image` uses the optional-signing skip path
   (no-op when `config.signing` is `None`), which compiles and passes tests
   without `images::signing` existing."

2. Move the "`images::sync` order test" from Commit 3 §Testable to Commit 4
   §Testable, where it can be implemented against the real `sign_manifest`
   function.

---

## 6. Suggestions

None new. All prior suggestions closed or carried forward through v11.

---

## 7. Open Questions

None.

---

## Verdict

Phase 5 is **ready for implementation start** pending the one-sentence §Design
constraints correction and §Testable relocation described in N1 above. N1 is
editorial and does not require re-review before implementing — the implementer
can apply the fix inline while landing Commit 3. Phases 1–4 remain free of
regression.

**New findings by severity:** 0 blockers, 0 majors, 1 minor, 0 suggestions.
