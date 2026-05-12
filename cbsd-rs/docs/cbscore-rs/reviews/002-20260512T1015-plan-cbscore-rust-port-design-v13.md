# Plan Review v13 — cbscore Rust port (plan 002)

**Scope:** Comprehensive pass covering all six drafted Phase plans plus README.
Primary focus is Phase 6 (first review). Sanity recheck of Phases 1–5 for
regressions introduced by Phase 6's addition.

**Files reviewed:**

- `cbsd-rs/docs/cbscore-rs/plans/README.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md` (Phase 1)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-02-subprocess-and-shell-tools.md`
  (Phase 2)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md`
  (Phase 3)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-04-runner.md` (Phase 4)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-05-builder-and-releases.md`
  (Phase 5)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-06-cbsbuild-cli.md` (Phase 6
  — new)
- Design corpus: 001, 002, 003, 004, 005 (all at current revision)

---

## 1. Summary Assessment

Phase 6 is a well-structured first draft that correctly closes the loop from the
library surface (Phases 1–5) to a runnable `cbsbuild` binary. The clap tree
matches the design 002 §CLI Surface, the exit codes are right, the dual-mode
(host / in-container) invocation of the same binary is clearly separated across
Commit 1 and Commit 3, and the seq-004 coordination paragraph in §Out of scope
is honest about the interleaving. Two minor issues need resolution before
implementation starts; neither requires re-review. No blockers. Phases 1–5 are
free of regression.

---

## 2. Phases 1–5 Regression Recheck

All thirteen v11 findings and the one v12 finding remain closed. No text in
Phase 6 contradicts or invalidates any prior decision. Specific cross-phase
touchpoints verified:

- **Phase 4 §Out of scope** — "cbsbuild runner stop --all CLI integration …
  Phase 6" → Commit 3 adds it. CONSISTENT.
- **Phase 5 §Out of scope** — `cbsbuild build`, `cbsbuild runner build`,
  `cbsbuild advanced …` clap wiring deferred to Phase 6 → Commits 3, 3, 4
  respectively cover all three. CONSISTENT.
- **Phase 5 §Out of scope** — `releases::s3` read operations
  (`check_release_exists`, `check_released_components`) defer to Phase 6
  alongside their consumer → Commit 2 adds them. CONSISTENT.
- **Phase 1 §Out of scope** — IO list says `read/write_descriptor` lands in
  Phase 4, descriptor-store walks in seq-004. Phase 6 adds
  `versions::create::version_create_helper` with git IO and descriptor write.
  Phase 1's IO bullet does not enumerate `versions::create` IO, but this
  omission is acceptable: Phase 1 §Out of scope covers _file IO_, not subprocess
  IO, and `version_create_helper`'s git subprocess call goes through the Phase 2
  `utils::git` wrapper that Phase 1 already defers. The descriptor write reuses
  Phase 4 Commit 1's `write_descriptor`. No new IO category is introduced that
  Phase 1 failed to anticipate. NOT A REGRESSION.

**Phases 1–5 free of regression.**

---

## 3. Blockers

None.

---

## 4. Major Concerns

None.

---

## 5. Minor Issues

### N1 — `version_create_helper` signature uses `type` as a parameter name (reserved keyword)

**What:** Commit 2 §Files defines the public surface of `version_create_helper`
as:

```rust
pub async fn version_create_helper(
    component_refs: &[(String, String)],
    type: VersionType,
    signed_off_by: VersionSignedOffBy,
) -> Result<VersionDescriptor, VersionError>
```

`type` is a reserved keyword in Rust. This code will not compile; the parameter
must be renamed (e.g., `version_type: VersionType`).

**Why it matters:** The plan is the implementer's contract. A signature that
does not compile will produce a minor stumble on the first `cargo check` run.
The issue is cosmetic — a one-word rename — but an implementer who copies the
signature literally will hit an immediate compile error. Plans in this corpus
have held to the standard that function signatures in §Files blocks are
copy-paste-ready.

**Resolution:** Rename the parameter to `version_type` (or `vtype`) in Commit
2's function signature block. No other changes needed; the prose description of
the parameter is correct.

### N2 — Commit 5 §Design constraints lists only three named env vars but implies a fourth

**What:** §Design constraints states: "The test reads
`CBSCORE_TEST_CEPH_DESCRIPTOR` (descriptor path), `CBSCORE_TEST_BUILDER_IMAGE`
(image ref), `CBSCORE_TEST_PYTHON_CBSCORE` (path to the Python cbscore for
comparison). Missing env vars → test is `#[ignore]`-skipped with a clear 'set
<var> to enable' message."

The sentence immediately before refers to the "local rpmbuild - GPG + S3 +
podman sidecars" all being available, and the §Design constraints item 4
references the "semantic content" comparison needing "the same descriptor" from
Python. Three named vars are documented; the compound infrastructure requirement
(rpmbuild, GPG, S3, podman available) is documented in prose but no env var
gates it. This is not a fourth env var gap per se — the test can gate on
endpoint availability via the three named vars — but the prose says "plus a
fourth implied", which the reviewee prompt also notes. The plan never names that
fourth var.

**Why it matters:** The M1 acceptance test is the milestone gate. The §Design
constraints block is the specification for what makes it runnable. If there is a
fourth required env var (e.g., `CBSCORE_TEST_S3_ENDPOINT` or
`CBSCORE_TEST_SCRATCH_DIR`), it should be named so the CI configuration task is
complete at plan time, not at implementation time. If there is no fourth var
(the three named ones are sufficient to gate on), the "fourth implied" language
in §Goal's preamble comment should be removed to avoid confusion.

**Resolution:** Either (a) name the fourth env var explicitly in §Design
constraints if a fourth gate is genuinely required, or (b) remove the
parenthetical "(plus a fourth implied)" from the Commit 5 preamble block if the
three named vars are sufficient. No re-review needed; this is editorial.

---

## 6. Suggestions

### S1 — Consider naming the in-container config override path explicitly in Commit 3

Commit 3 §Design constraints explains that `cbsbuild runner build` loads the
"mounted config + secrets" and references Phase 4's mount table. However,
neither Commit 1 (scaffold) nor Commit 3 (runner build entry point) states that
the in-container invocation hardcodes `--config /runner/cbs-build.config.yaml`
rather than using whatever `-c` the operator might pass. This is implicit from
the Phase 4 mount table, but an implementer new to Phase 4 might wonder whether
the in-container invocation should honour a user-supplied `-c` or not.

**Suggestion:** Add one sentence to Commit 3 §Design constraints: "The
`runner build` handler ignores the global `-c/--config` flag and loads
`/runner/cbs-build.config.yaml` directly — that path is the contract established
by Phase 4's mount table and is not operator-overridable from inside the
container." This mirrors the level of explicitness that Phase 4's SIGTERM, HOME,
and CBS_DEBUG bullets achieve for their respective in-container behaviours.

### S2 — seq-004 interleaving: clarify which commit Phase 6 Commit 2 must precede or follow

§Out of scope is correctly scoped and the interleaving language is good.
However, the sentence "The two plans interleave at implementation time" leaves
the exact ordering to the implementer's judgement. Given that seq-004 is
approved for M1 scope and adds `Config.paths.versions`, an implementer could
ship Commit 2 first (hardcoded path, no config field) and then land seq-004
before Commit 5 — or ship them in the opposite order. Either works, but the
first ordering means the M1 acceptance gate in Commit 5 exercises the
configurable path (because seq-004 landed before Commit 5), which is strictly
better than the converse.

**Suggestion:** Add one line to §Out of scope: "Recommended interleaving:
seq-004 should land before Phase 6 Commit 5 so that the M1 acceptance test
exercises the configurable `--versions-dir` path rather than the hardcoded
fallback. The acceptance test's correctness is not affected either way, but
exercising the configurable path at the milestone gate gives stronger end-to-end
coverage." Non-blocking; the current text is not wrong.

---

## 7. Open Questions

None.

---

## 8. Phase 6 Specific Checklist

Each item from the review scope verified:

**Commit 1 (CLI scaffold)**

- Clap tree names (`build`, `runner`, `versions`, `config`, `advanced`) match
  design 002 §CLI Surface lines 1212–1242. PASS.
- Global flags `-c/--config` and `-d/--debug` with `env = "CBS_DEBUG"` match
  design 002 lines 1244–1245. PASS.
- Exit codes 131 (`ENOTRECOVERABLE`) and 22 (`EINVAL`) match design 002 lines
  1246–1248. PASS.
- `--cbscore-path` drop documented in §Out of scope with design 002 lines
  1249–1255 citation. PASS.

**Commit 2 (versions subcommand group)**

- Four subcommands (`create`, `list`, `show`, `validate`) match design 002 lines
  1228–1232. PASS.
- `version_create_helper` placed at `cbscore::versions::create` per design 001
  §Workspace Layout. PASS.
- `versions::mod.rs` updated to add `pub mod create` alongside existing
  `pub mod desc` (Phase 4) and `pub mod utils` (Phase 2). PASS.
- `releases::s3` read operations (`check_release_exists`,
  `check_released_components`) added here, matching the Phase 5 V11-N2 deferral.
  PASS.
- `type` parameter name conflict: FAIL — see N1 above.

**Commit 3 (build + runner)**

- `cbsbuild build` aliases `cbsbuild runner run` per design 002 lines
  1219, 1224. PASS.
- In-container `cbsbuild runner build` entry point matches Phase 4's
  `--entrypoint /runner/cbsbuild` contract. PASS.
- SIGTERM cooperative cancellation chain documented correctly: podman receives
  SIGTERM from host runner, forwards to container PID 1 (`cbsbuild`), which
  drops the `run_build` future. PASS.
- `cbsbuild runner stop --all` present per Phase 4 §Out of scope deferral. PASS.

**Commit 4 (config + advanced)**

- `config init` bypass-mode flags match design 002 §Open Questions resolution
  lines 1424–1432; no-flags path emits an error. PASS.
- seq-003 cross-reference accurate (design 003 link confirmed in §Out of scope).
  PASS.
- `--for-containerized-run` pre-fill reference to design 004 §Bypass-mode
  (line 358) cited. PASS.

**Commit 5 (M1 acceptance)**

- Four acceptance criteria map cleanly onto design 002 lines 1276–1281.
  Criterion 1 (exit 0), criterion 2 (byte-identical RPMs), criterion 3
  (round-trip), criterion 4 (semantic equivalence). PASS.
- Three named env vars documented (`CBSCORE_TEST_CEPH_DESCRIPTOR`,
  `CBSCORE_TEST_BUILDER_IMAGE`, `CBSCORE_TEST_PYTHON_CBSCORE`). Fourth var
  ambiguity: see N2 above.
- `#[ignore]` default with `--include-ignored` unlock pattern is consistent with
  every prior integration test in Phases 2–4. PASS.
- README and plan progress table updated in same commit. PASS.

**M1 cut framing**

- The "byte-identical RPMs" criterion is realistic for a first-run acceptance
  test only if the build environment is deterministic (same rpmbuild version,
  same spec files, same timestamps/seeds). The plan's "reproducibility" bullet
  in §Design constraints acknowledges the random-seed issue for container run
  names (matching Python's `random.choices`) but does not address RPM build
  timestamp determinism. RPM spec files that embed `%{_buildtime}` will produce
  non-identical binaries across runs even with the same inputs. This is not a
  new concern (it predates this plan), and the plan wisely matches what design
  002 §Migration Strategy says — so it is not a finding against the plan. Worth
  noting as a known operational caveat for whoever runs the acceptance test.
- The `1.0.0` version claim at M1 cut is a strong commitment. The plan correctly
  anchors it to design 002 §Migration Strategy lines 1283–1291. PASS.

**Cross-phase consistency**

- Phase 6 §Depends on lists Phase 1–5 deps with correct module paths:
  `cbscore::utils::git::git_ls_remote` (Phase 2), `versions::utils::*` (Phase
  2), `config::Config::{load, store}` (Phase 3), `utils::s3` (Phase 3),
  `secrets::SecretsMgr` (Phase 3), `runner::{run, stop, gen_run_name}` (Phase
  4), `versions::desc::{read_descriptor, write_descriptor}` (Phase 4),
  `builder::run_build` (Phase 5), `releases::s3` write path (Phase 5). All
  citations verified against their source plans. PASS.
- `BuildOptions`, `RpmArtifact`, `ContainerImageReport` from Phase 5 are not
  directly referenced by name in Phase 6 prose (the CLI passes `BuildOptions`
  into the `runner::run` call chain implicitly). This is acceptable — the CLI's
  direct dependency is on `runner::run` which consumes `BuildOptions`
  internally. No gap.

**Commit granularity**

- Commits 1–4: 450, 600, 400, 500 LOC respectively — all in or above the
  400-line sweet spot. PASS.
- Commit 5 (~300 LOC): below sweet spot; rationale paragraph present and
  convincing (M1 milestone gate is a distinct review concern from routine CLI
  plumbing). PASS.

**README**

- Phase 6 filename `002-20260508T1558-06-cbsbuild-cli.md` resolves in the
  `plans/` directory. PASS.
- README description "M1.5 — `cbsbuild` clap CLI + logging + exit codes +
  end-to-end Ceph build acceptance — 4–5 commits" matches Phase 6's 5-commit
  plan and its concerns. PASS.
- Running commit total: Phase 1 (5) + Phase 2 (5) + Phase 3 (4) + Phase 4 (3)
  - Phase 5 (6) + Phase 6 (5) = 28 commits across 6 phases. README bound is
    "~25–30 commits across 7 phases"; with Phase 7 (2–3 commits) the total is
    30–31. CONSISTENT.

---

## Verdict

Phase 6 is **ready for implementation start** pending the two minor fixes
described in N1 and N2. Both are editorial; neither requires re-review before
the implementer proceeds. N1 (reserved keyword) must be corrected before
`cargo check` passes on Commit 2; N2 (fourth env var) should be resolved while
writing the acceptance test in Commit 5. Phases 1–5 are free of regression.

**New findings by severity:** 0 blockers, 0 majors, 2 minors, 2 suggestions.
