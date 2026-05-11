# Plan Review — cbscore Rust Port: Phase 2 (M1.1) — v4

**Plans reviewed:**

- [`002-20260508T1558-02-subprocess-and-shell-tools.md`](../plans/002-20260508T1558-02-subprocess-and-shell-tools.md)
  — **Phase 2, primary focus (first review)**
- [`002-20260508T1558-01-types.md`](../plans/002-20260508T1558-01-types.md) —
  Phase 1, sanity re-check after Phase 2 draft
- [`plans/README.md`](../plans/README.md) — sanity re-check

**Prior reviews (Phase 1, all findings closed):**

- v1:
  [`002-20260511T1002-plan-cbscore-rust-port-design-v1.md`](./002-20260511T1002-plan-cbscore-rust-port-design-v1.md)
  — 9 findings (C1, I1, I2, M1–M4, S1–S2)
- v2:
  [`002-20260511T1130-plan-cbscore-rust-port-design-v2.md`](./002-20260511T1130-plan-cbscore-rust-port-design-v2.md)
  — NI1 raised
- v3:
  [`002-20260511T1240-plan-cbscore-rust-port-design-v3.md`](./002-20260511T1240-plan-cbscore-rust-port-design-v3.md)
  — NI1 closed, NF1 (minor, non-blocking), Phase 1 declared ready for
  implementation

**Designs referenced:** 001 (project structure), 002 (Rust port architecture).

**Reviewer:** Staff review, 2026-05-11.

---

## Summary Assessment

Phase 2 has the right scope, the right commit sequence, and the right design
citations. The five-commit structure cleanly mirrors the M1.1 subsystem order
from design 002. Three issues require attention before Phase 2 can drive
implementation: one **IMPORTANT** (a missing function from the `versions::utils`
public surface that is explicitly named in design 002 and in design 001's
downstream-consumers table), one **IMPORTANT** (the lift-out invariant
verification claim is technically unsound — `cargo tree` cannot surface
module-level isolation), and one **MINOR** (Commit 3 and Commit 5 fall below the
200-line floor individually and the rationale for keeping them separate is not
given). Two minor items and two suggestions complete the findings. Phase 1
re-check passes clean: no regression introduced by the Phase 2 draft. README
re-check passes clean.

---

## Phase 1 Re-check

### §Out of scope cross-reference

Phase 1 §Out of scope states the parse-version family "lands in Phase 2." Phase
2 Commit 5 delivers it. The cross-reference is accurate and unambiguous.

### Path disambiguation

Phase 1 Commit 4 creates `cbsd-rs/cbscore-types/src/versions/utils.rs` (carrying
only `VersionType`). Phase 2 Commit 5 creates
`cbsd-rs/cbscore/src/versions/utils.rs` (carrying `ParsedVersion` + parse
family). Both paths are stated explicitly in their respective plans; the
`cbscore-types` vs `cbscore` distinction is unambiguous. No implementer
confusion risk.

### Logger reuse

Phase 1 Commit 2 places `logger.rs` in `cbscore-types`, with
`tracing-subscriber` in `[dependencies]` (the NI1 fix confirmed in v3). Phase
2's lift-out invariant bullet (§End-of-phase acceptance) says
`utils::subprocess` and `utils::git` depend on "`cbscore-types::errors` +
`cbscore::logger`". The phrasing "`cbscore::logger` re-export" in Commit 4's
design constraints is the right formulation: `cbscore` re-exports the logger
from `cbscore-types`, so the module uses `use crate::logger::…` without an
awkward cross-crate path. The intent is clear.

### Phase 1 findings — regression check

All nine v1 findings plus NI1 remain closed. The Phase 2 draft does not touch
any of the plan-01 text. The NF1 minor (positive assertion for
`tracing-subscriber`) remains open but was declared non-blocking in v3.

---

## README Re-check

The dependency graph line is:

```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7
                                                        (M1 cut)    (M2 cut)
```

Both cut markers are in the correct positions (confirmed in v2/v3). Phase 2's
entry in the table and its link to
`002-20260508T1558-02-subprocess-and-shell-tools.md` are correct. The commit
count column ("4–5") matches Phase 2's five-commit plan. Status is "Pending" —
correct. No regression.

---

## Phase 2 — Primary Review

### Strengths

- **Design-section citations are precise.** Every commit names its authoritative
  design 002 section and line range. The subprocess section cites lines
  939–1071, skopeo cites lines 1077–1083, version-utils cites lines 672–696. All
  verified against the design text.

- **Timeout / cancellation contract is correctly captured.** Commit 1 accurately
  states the two-path contract from design 002 lines 1018–1031: internal timeout
  fires → `Child::start_kill()` + `wait().await` + `CommandError::Timeout`;
  outer future dropped → RAII guard's `Drop` calls `Child::start_kill()`. The
  distinction between the internal-timeout-only contract and the outer
  cancellation path is the subtlest piece of the subprocess design, and it is
  stated correctly.

- **`reset_python_env` exclusion is correctly applied.** Commit 1 explicitly
  states it is not ported, cites design 002 §Open Questions resolution lines
  1386–1396. Consistent with design and memory.

- **`cidfile` threading is correct.** Commit 2 notes `podman_run` accepts a
  `cidfile: Utf8PathBuf` option matching design 002 lines 754, 1033–1049. The
  note that the runner — Phase 4, not Phase 2 — orchestrates the cidfile + stop
  dance is correct.

- **`buildah` independence from `podman` is preserved.** Commit 2 explicitly
  states `utils::buildah` does not depend on `utils::podman`. This matches
  design 001's layout (independent wrappers) and keeps the lift-out boundary
  clean.

- **Drift closure is properly scoped.** Commit 5 explicitly names the Phase 1
  §Out of scope drift and closes it. The rationale (`regex` dep forbidden in
  `cbscore-types`) is accurate. The `ParsedVersion` placement decision is argued
  correctly (no external Python consumer forces `cbscore-types` placement).

- **Testable sections are substantive.** Each commit lists concrete, runnable
  test scenarios. The subprocess tests cover both pipe streams concurrently, the
  timeout path, and the out_cb path. The git tests cover command construction, a
  stub parse, and the redaction invariant. These are the right test targets.

- **`GitError` placement is consistent with v1 review.** Commit 4 places
  `GitError` in `cbscore/src/utils/git/errors.rs`, not in `cbscore-types`. The
  v1 review §Strengths noted this as correct (lift-out invariant: git error type
  travels with the module). The Phase 2 plan maintains this and cites the design
  001 lift-out invariants section.

---

## Important Concerns

### I1 — `get_version_type` is absent from Commit 5's function list

**Where:** Phase 2, Commit 5 §Design constraints (plan lines 229–234) and §Files
description (plan lines 217–223).

**What the plan says:** Commit 5 lists five functions: `parse_version`,
`get_major_version`, `get_minor_version`, `normalize_version`,
`parse_component_refs`.

**What the design says:** Design 002 §VersionType and parsing (lines 672–700)
lists six signatures in `cbscore-types::versions::utils`:

```rust
pub fn parse_version(s: &str) -> Result<ParsedVersion, MalformedVersion>;
pub fn get_version_type(name: &str) -> Result<VersionType, VersionError>;
pub fn parse_component_refs(components: &[String])
    -> Result<HashMap<String, String>, VersionError>;
pub fn get_major_version(v: &str) -> Result<String, MalformedVersion>;
pub fn get_minor_version(v: &str) -> Result<Option<String>, MalformedVersion>;
pub fn normalize_version(v: &str) -> Result<String, MalformedVersion>;
```

`get_version_type` is the sixth function. It is listed in design 001 §Crate
Responsibilities (lines 213–215) as part of the `cbscore-types` public API for
the narrow consumers: "`VersionType` enum + pure parse helpers (`parse_version`,
`parse_component_refs`, `get_version_type`, …)". Design 001 §Downstream
Consumers (line 65) shows that `cbc` imports `versions.utils.get_version_type`
today. Since the whole phase-1/phase-2 split was predicated on the `regex` dep
exclusion from `cbscore-types`, the key question is: does `get_version_type`
require `regex`?

Looking at the Python source (`cbscore/versions/utils.py`):
`get_version_type(name)` calls `parse_version(name)` and maps the parsed suffix
to a `VersionType` variant. If `parse_version` requires `regex` (it does — it
uses the verbose multi-line pattern), then `get_version_type` depends on
`parse_version` and also requires `regex`. This means `get_version_type` belongs
in `cbscore::versions::utils` alongside `parse_version`, not in `cbscore-types`.

However, design 002 places all six functions in `cbscore-types::versions::utils`
(at line 672 the section header says exactly that). This contradicts the Phase 1
/Phase 2 split rationale. The plan correctly moved five of those six to
`cbscore` and left only `VersionType` in `cbscore-types` — but it silently
dropped `get_version_type`.

**Failure mode:** If `get_version_type` is not implemented anywhere, `cbc`'s
import of `versions.utils.get_version_type` has no Rust equivalent when `cbc` is
eventually rewritten. More immediately: an implementer reading the plan has no
signal to add the function; the omission will be noticed only at Phase 6
integration (or at `cbc` rewrite time, too late).

**Resolution:** Add
`get_version_type(name: &str) -> Result<VersionType, VersionError>` to Commit
5's function list and signatures (alongside `parse_version`). It depends on
`parse_version` internally (same `regex` constraint applies). Add a test case:
`get_version_type("ces-v19.2.3-dev.1")` → `Ok(VersionType::Dev)`. Note the drift
from design 002 in a §Design constraints bullet matching the existing note for
the five other functions, so the author of the design-002 edit (already flagged
as a follow-up) knows to include it.

---

### I2 — Lift-out invariant check via `cargo tree` is not achievable as stated

**Where:** Phase 2, §End-of-phase acceptance, last bullet (plan lines 269–274).

**What the plan says:**

> Lift-out invariants (design 001): `utils::subprocess` and `utils::git` depend
> only on `cbscore-types::errors` + `cbscore::logger` + (for `utils::git`)
> `cbscore::utils::subprocess`. Verified by `cargo tree -p cbscore --depth 3`
> listing — no deps from these modules into
> `cbscore::{config, runner, builder, releases, images::{sign, sync}}`.

**Why this doesn't work:** `cargo tree` shows **crate-level** transitive
dependencies, not module-level dependency boundaries within a single crate. Once
`cbscore` depends on `aws-sdk-s3` (Phase 3) and `vaultrs` (Phase 3), those
crates will appear in `cargo tree -p cbscore --depth 3` regardless of whether
`utils::subprocess` or `utils::git` _import_ them. The check as written would
produce a false positive as soon as Phase 3 adds a single import to any other
module in `cbscore`.

**Failure mode:** An implementer runs the stated check, sees `aws-sdk-s3` in the
tree (legitimately present for Phase 3's S3 module), and has no idea whether the
invariant is actually satisfied for `subprocess` and `git` modules specifically.
The verification step is unexecutable as written, which means the invariant has
no enforcement path.

**Resolution:** Replace the `cargo tree` check with a verifiable method. Two
options:

1. **Code-review checklist (simplest):** Change the acceptance criterion to:
   "Code review confirms `cbscore/src/utils/subprocess.rs` and
   `cbscore/src/utils/git.rs` (and `git/errors.rs`) have no `use` or `mod`
   statements importing from
   `cbscore::{config, runner, builder, releases, images}`. A
   `grep -n 'use crate::\(config\|runner\|builder\|releases\|images\)' cbscore/src/utils/{subprocess,git}.rs`
   returns zero matches."

2. **`cargo-modules` (more mechanical):** If `cargo-modules` is available in the
   dev environment, `cargo modules structure --package cbscore` produces a
   module-level dependency graph that can be read to confirm the constraint.
   Note this requires installing an additional tool and is optional.

The grep-based check is sufficient for enforcement; option 1 is the recommended
resolution. The prose describing _what_ the invariant is (no deps into config /
runner / builder / releases / images) is correct and should be preserved.

---

## Minor Issues

### M1 — Commit 3 and Commit 5 are below the 200-line floor with no bundling rationale

**Where:** Phase 2 progress table (plan lines 5–11); Commit 3 (~150 LOC), Commit
5 (~200 LOC).

**What the CLAUDE.md says:** "Below 200, consider whether the commit is
meaningful alone." Commit 3 is below 200; Commit 5 is at the floor boundary.

**Assessment:** Both commits are independently meaningful — `images::skopeo`
introduces a new top-level module (`images/`) that Phase 5 will extend; the
parse-version family closes a named drift from Phase 1. Keeping them separate is
defensible. However, the plan does not explain _why_ they are kept separate
given the size concern. Commit 5 in particular (versions::utils) has a strong
dependency link to Commit 4 (utils::git calls `parse_version` in
`version_create_helper`, Phase 6) and both touch related `versions` semantics.

**Resolution:** Either add a one-sentence justification in each small commit's
description explaining why the scope cannot be widened (e.g., "Kept separate
because `images/mod.rs` introduces the module tree that Phase 5 extends;
bundling with a different subsystem would conflate unrelated concerns") — or
bundle Commit 5 into Commit 4, noting that Commit 4's LOC estimate (~500) +
Commit 5's (~200) = ~700, within the 400–800 sweet spot. Either resolution is
acceptable; the gap is minor.

---

### M2 — `SkopeoOpts` struct is defined in the plan but not in design 002

**Where:** Phase 2, Commit 3 §Design constraints (plan lines 158–161).

**What the plan says:**

> TLS / auth flags from `SkopeoOpts` (a small struct in this same module):
> `tls_verify: bool`, `creds: Option<RegistryCreds>`.

**What design 002 says:** §Skopeo driver (lines 1077–1083) mentions
`skopeo_image_exists` and `skopeo_copy` as free async functions and refers to
"optional TLS / auth flags" but does not define `SkopeoOpts` or name its fields.

**Assessment:** The plan is adding detail beyond the design, which is normal for
a plan. The chosen fields (`tls_verify`, `creds`) are clearly correct given the
Python source's `--src-tls-verify` / `--dest-tls-verify` / `--src-creds` /
`--dest-creds` flags. However, there is an asymmetry: `skopeo copy` has
_separate_ src and dst TLS verify flags and separate src / dst creds. A single
`tls_verify: bool` applies to both sides, which may not match the Python
wrapper's per-side semantics. The plan should either confirm the Python wrapper
collapses src/dst flags into one boolean, or use separate `src_tls_verify` and
`dst_tls_verify` fields.

**Resolution:** Check `cbscore/images/skopeo.py` at implementation time and add
a note to the Commit 3 §Design constraints confirming whether TLS verify is
per-side or unified. If per-side, the struct should be:

```rust
pub struct SkopeoOpts {
    pub src_tls_verify:  bool,
    pub dst_tls_verify:  bool,
    pub src_creds:       Option<RegistryCreds>,
    pub dst_creds:       Option<RegistryCreds>,
}
```

This is minor because the plan correctly defers `SkopeoOpts` finalisation to the
module itself ("a small struct in this same module") — but the ambiguity should
be resolved before the commit lands, and the testable section should assert the
per-side flag mapping.

---

## Suggestions

### S1 — `utils::git` §Out of scope: `ls_remote` component-ref resolution not flagged

**Where:** Phase 2, §Out of scope (plan lines 42–53).

`git_ls_remote` is one of the functions listed in Commit 4. Design 002 lines
706–711 describe `version_create_helper` (Phase 6 / versions::create) as using
`git ls-remote` to resolve component refs. Phase 2 adds the raw `git_ls_remote`
wrapper; Phase 6 adds the resolution logic on top. This split is architecturally
clean but could confuse an implementer who reads Commit 4 and wonders whether
`git_ls_remote` should already parse the `components` list into a map.

The §Out of scope section mentions skopeo sign/sync and the actual cbscommon
lift-out, but does not mention that `ls_remote`'s **consumer** (component-ref
resolution) is deferred to Phase 6. A one-line note would prevent the question
from being raised during Commit 4 implementation.

**Resolution:** Add to §Out of scope: "Component-ref resolution via
`git_ls_remote` (the `version_create_helper` caller in
`cbscore::versions::create`) — deferred to Phase 6 alongside
`cbsbuild versions create`."

---

### S2 — Commit 1 testable: no positive assertion that the RAII drop guard is

present

**Where:** Phase 2, Commit 1 §Testable (plan lines 89–103).

The RAII drop guard (outer-cancellation path) is cited in §Design constraints as
a requirement. The §Testable block covers the internal-timeout path
(`CommandError::Timeout` from a `sleep 5` child with 100ms budget) and the
out_cb path, but it does not include a test for the guard: e.g., spawning a
child then dropping the future mid-flight and asserting the child process is no
longer alive. This is harder to test than the timeout path, but the guard is the
correctness property that prevents zombie processes in `tokio::select!`
scenarios — the most common consumer pattern in the runner.

**Resolution:** Add an optional test hint: "RAII guard smoke test: spawn
`sleep 60`, wrap the `async_run_cmd` call in a `tokio::select!` with a 50ms
timer, let the select cancel the subprocess branch, then verify `kill(pid, 0)`
returns `ESRCH` (process gone)." Mark as `#[ignore]` to avoid non-determinism in
CI if needed.

---

## Open Questions

### OQ1 — Does `SkopeoOpts` need separate per-side TLS flags?

Covered in M2 above. Resolution is "check `cbscore/images/skopeo.py` at
implementation time." Named here for completeness as an open question rather
than a blocker.

### OQ2 — Is `get_version_type` pure enough to land in `cbscore-types`?

Covered in I1 above. The answer is no: it calls `parse_version`, which uses
`regex`. But the design 002 header places it in `cbscore-types`. This should be
resolved when the design-002 follow-up edit is made (already flagged in Phase 1
§Out of scope as a non-blocking pending update). Phase 2 should add the function
to `cbscore::versions::utils`, and the design-002 update should move all six
signatures from `cbscore-types::versions::utils` to `cbscore::versions::utils`.

---

## Verdict

**Phase 2 requires two important fixes before it can drive implementation.** I1
(missing `get_version_type`) and I2 (unachievable lift-out invariant
verification) must be resolved. Two minor items (M1, M2) and two suggestions are
non-blocking. Phase 1 is confirmed free of regression; the README is accurate.

**New findings by severity:** 0 blockers (CRITICAL), 2 important (I1, I2), 2
minor (M1, M2), 2 suggestions (S1, S2).

**Phase 2 implementation start:** Not yet — resolve I1 and I2 first, then
re-confirm.
