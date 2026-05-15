# Plan Review v29 — Pre-Implementation Audit Pass 9 Closure Confirmation

**Review target:** seq-002 plan corpus (Phase 7) + seq-004 plan + Phase 3 plan\
**Commit under review:** `dacc242`\
**Reviewer:** Staff Engineer (design-reviewer agent)\
**Date:** 2026-05-15

---

## §Scope

Focused confirmation review of the 2 pre-implementation audit pass-9 findings
(I1, I2) claimed closed in commit `dacc242`. Also confirms no-drift on three
structural invariants established by passes 1–8. A `prettier --check` pass on
all four edited files is included.

## §Method

For each finding, the closure text was located directly in the current plan file
at the relevant commit section. Quoted phrases are verified verbatim; line
references are recorded where the text lands. The no-drift checks read the live
plan corpus state — not git diff — and compare against the known-good baselines
recorded in the v28 review and project memory.

---

## §Closure Verification

### I1 — Phase 5 C1 `prepare::run` signature gains `secrets: &SecretsMgr` third parameter + design-002-alignment rationale; §Depends on calls out git credentials

**Claimed change:** Phase 5 Commit 1 `prepare::run` public surface gains
`secrets: &SecretsMgr` as a third parameter with a design-002-alignment
rationale paragraph. §Depends on Phase 3 bullet calls out git credentials as the
reason.

**Verified.** Phase 5 Commit 1 §Files `prepare.rs` public surface lists:

> `pub async fn run(desc: &VersionDescriptor, config: &Config, secrets: &SecretsMgr, opts: &BuildOptions) -> Result<PrepareReport, BuilderError>`
> — the stage entry point. Returns a `PrepareReport` carrying the per-component
> `BuildComponentInfo` records that downstream stages consume. `PrepareReport`
> is declared inline in this file. The `secrets: &SecretsMgr` arg threads
> through to the underlying `utils::git` calls so that source fetches against
> private SSH/HTTPS repos can resolve the matching `GitCreds` entry by host
> (matches design 002's §Build Pipeline orchestrator sketch, which threads
> `secrets` into every stage uniformly). Even on M1 deployments with public-only
> repos, the param is present so the orchestrator's
> `prepare::run(desc, config, secrets, opts).await?` line matches the other
> stages 1:1.

And §Depends on Phase 3 bullet reads:

> Phase 3 — `utils::s3` for RPM + release-descriptor uploads; `utils::vault` for
> transit signing; `config::Config` for `paths.scratch` / `signing.gpg` /
> `signing.transit` / `storage.s3.bucket` settings; `secrets::SecretsMgr` for
> resolved git credentials (consumed by `prepare::run` for private-repo source
> fetches), GPG passphrases (signing), and registry creds (upload).

The `secrets: &SecretsMgr` third parameter, the design-002-alignment rationale
("matches design 002's §Build Pipeline orchestrator sketch"), and the explicit
call-out of git credentials in §Depends on are all present. **Closed.**

---

### I2 — Phase 7 C3 references updated: `Phase 6 Commit 5` → `Phase 6 Commit 6`; `CBSCORE_TEST_CEPH_DESCRIPTOR` → `CBSCORE_TEST_SMOKE_DESCRIPTOR`; structural-equivalence criterion 3 rewritten so M2 gate stands alone; same drift fixed in seq-004 and Phase 3

**Claimed change:** Phase 7 Commit 3 updates three things — (a) "Phase 6 Commit
5" references updated to "Phase 6 Commit 6", (b) `CBSCORE_TEST_CEPH_DESCRIPTOR`
renamed to `CBSCORE_TEST_SMOKE_DESCRIPTOR`, (c) criterion 3 of the M2
structural-equivalence test rewritten to stand on its own without claiming
parallel to the M1 acceptance gate. Same rename and numbering corrections
applied to seq-004 and Phase 3.

**Sub-check (a): "Phase 6 Commit 6" reference in Phase 7 C3.**

Phase 7 Commit 3 §Files reads:

> Reads env vars naming the test smoke descriptor + builder image (same vars as
> the Phase 6 Commit 6 M1 smoke test — `CBSCORE_TEST_SMOKE_DESCRIPTOR`,
> `CBSCORE_TEST_BUILDER_IMAGE`).

And §Design constraints §Test environment documented reads:

> Same env-var contract as the Phase 6 Commit 6 M1 smoke test
> (`CBSCORE_TEST_SMOKE_DESCRIPTOR`, `CBSCORE_TEST_BUILDER_IMAGE`).

Both occurrences reference "Phase 6 Commit 6". No residual "Phase 6 Commit 5"
text is present in the Phase 7 plan. **Sub-check (a): Closed.**

**Sub-check (b): `CBSCORE_TEST_SMOKE_DESCRIPTOR` throughout Phase 7 C3.**

Both Phase 7 C3 occurrences (`§Files` and `§Design constraints`) use
`CBSCORE_TEST_SMOKE_DESCRIPTOR`. A grep across all active plans confirms zero
occurrences of `CBSCORE_TEST_CEPH_DESCRIPTOR` in any file under
`cbsd-rs/docs/cbscore-rs/plans/`. **Sub-check (b): Closed.**

**Sub-check (c): M2 structural-equivalence criterion 3 stands alone.**

Phase 7 Commit 3 §Design constraints acceptance criterion 3 reads:

> The produced RPM set is **structurally equivalent** to the pre-M2 reference
> (same cardinality, same NEVRA per RPM, same file list per RPM, same
> dependencies). This is the M2-specific property: a pre-M2 reference build
> (subprocess-driven) and an M2 candidate build (direct-dep) against the same
> builder image must produce the same artifacts. Byte-for-byte equality is not
> required: even running both builds back-to-back in the same test process, RPM
> headers carry `BUILDTIME` at second resolution and the two `rpmbuild`
> invocations will typically differ by at least one second. The M1 smoke gate
> (Phase 6 Commit 6) does NOT assert structural equivalence — it asserts exit 0
>
> - non-empty RPM set only. The M2 gate stands on its own as the
>   cutover-correctness property.

The criterion defines the M2 gate positively ("same cardinality, NEVRA, file
list, dependencies") and explicitly states what M1 asserts ("exit 0 + non-empty
RPM set only") without claiming the two gates are parallel. The self-standing
rationale is present. **Sub-check (c): Closed.**

**Sub-check (d): seq-004 env-var name.**

A grep of `004-20260513T0900-configurable-version-descriptor-location.md`
confirms `CBSCORE_TEST_SMOKE_DESCRIPTOR` is used at the integration test slot
reference (Commit 3 §Testable) and zero occurrences of
`CBSCORE_TEST_CEPH_DESCRIPTOR`. **Sub-check (d): Closed.**

**Sub-check (e): Phase 3 plan.**

A grep of `002-20260508T1558-03-storage-and-secrets.md` returns zero occurrences
of `CBSCORE_TEST_CEPH_DESCRIPTOR`. The plan does not reference the smoke test
env var at all (correct — Phase 3 predates the M1 gate; the reference lives in
Phase 6 and Phase 7 only). **Sub-check (e): Closed.**

**Finding I2: Closed.**

---

## §No-Drift Spot Checks

Three structural invariants from passes 1–8 were spot-checked against the live
plan corpus state.

| Invariant                                                       | Expected                                                                                                                                                             | Observed                                                                                                                                                                                                                                                                | Status |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| Phase 6 commit count                                            | 6 commits (H14.1 added Commit 5; smoke gate → C6)                                                                                                                    | Progress table has 6 rows: Commits 1–6                                                                                                                                                                                                                                  | PASS   |
| Phase 6 Commit 6 "No Python comparison" wording (v28-rewritten) | Opens with "**No Python comparison.** The earlier design 002 framing called for Rust-vs-Python structural equivalence…That comparison is dropped from M1 acceptance" | Phase 6 Commit 6 §Design constraints: "**No Python comparison.** The earlier design 002 framing called for Rust-vs-Python structural equivalence (cardinality, NEVRA, file list, dependencies). That comparison is dropped from M1 acceptance:" — present and unchanged | PASS   |
| Stale `CBSCORE_TEST_CEPH_DESCRIPTOR` in active plans            | Absent from all files under `cbsd-rs/docs/cbscore-rs/plans/`; may appear only in historical review documents                                                         | `grep -rn CBSCORE_TEST_CEPH_DESCRIPTOR cbsd-rs/docs/cbscore-rs/plans/` returns no output; occurrence-free                                                                                                                                                               | PASS   |

---

## §Formatting

`prettier --check` on all four files modified in commit `dacc242`:

```
prettier --check \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-05-builder-and-releases.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-07-worker-cutover.md \
  cbsd-rs/docs/cbscore-rs/plans/004-20260513T0900-configurable-version-descriptor-location.md

All matched files use Prettier code style!
```

Exit code: 0.

---

## §Verdict

> **Approve — I1+I2 (2 findings) closed; pre-impl audit pass 9 fully resolved;
> plan corpus ready for Phase 1 implementation start.**
