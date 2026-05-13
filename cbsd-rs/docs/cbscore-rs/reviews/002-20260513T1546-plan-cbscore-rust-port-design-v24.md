# cbscore-rs Plan Review v24 — Pre-impl Audit Pass 4 Closure Confirmation

## Scope

Focused confirmation review of the 9 pre-implementation audit pass-4 findings
(D1-1, D2-1, D2-2, D3-1, D3-2, D3-3, D6-1, D7-1, D8-1) closed in commit
`a1b7895`. The review confirms each closure, verifies the no-drift checklist,
and checks prettier formatting on every edited file. Prior-review findings
(v1–v23) are out of scope.

## Method

1. Read the diff stat for `a1b7895` to enumerate the 11 edited files.
2. Read every file named in the diff in full.
3. Execute each grep specified in the verification checklist.
4. Run `prettier --check` on all 11 edited files.
5. Spot-check the five no-drift items from the checklist.

## Closure Verification

### D1-1 — parse-family placement

**Status: CLOSED — with one residual minor.**

The primary targets are correct:

- `grep -n "cbscore-types::versions::utils" design/001-*.md design/002-*.md`
  returns a single hit: design 002 line 844,
  `### VersionType (in cbscore-types::versions::utils)`. No parse helpers are
  attributed to `cbscore-types::versions::utils`. Pass.
- Design 002 now has two distinct subsections —
  `### VersionType (in cbscore-types::versions::utils)` (line 844) and
  `### Parsing (in cbscore::versions::utils)` (line 855) — with all six
  parse-helper signatures in the latter. Pass.
- Design 001 §Downstream Consumers table cites
  `cbscore::versions::utils::{get_version_type, parse_component_refs}` for `cbc`
  and `cbscore::versions::utils::parse_version` for `crt`. Pass.
- Design 001 §Crate Responsibilities now reads: "`VersionType` enum only. The
  six parse helpers … live in **`cbscore::versions::utils`** (the library
  crate), not here." Pass.
- Phase 1 §Out of scope bullet updated: "Designs 001 and 002 now reflect this
  placement (audit pass 4 / D1-1 closure)." Pass.

**Residual (MINOR — N1):** Phase 5 Commit 2 §Design constraints, line 195,
references `Err(ComponentError::Parse)` as the return value when no components
load successfully. `ComponentError::Parse` is not one of the five flat variants
defined by the D3-1 closure in Phase 1 Commit 2 (the variants are `Walk`,
`Yaml`, `MissingSchemaVersion`, `UnknownSchemaVersion`,
`DuplicateComponentName`). The sentence is a carry-forward from before D3-1
restructured the error taxonomy; the D3-1 closure updated the variant
_declaration_ in Phase 1 and the _conversion_ prose in Phase 5, but missed this
§Design constraints sentence. The §Testable for the same commit contradicts it
(empty-tree case → `Ok(HashMap::new())`, not `Err`). An implementer following
the Phase 5 text verbatim would reference a variant that does not exist and fail
to compile. One-sentence fix required: replace `Err(ComponentError::Parse)` with
the correct variant — most likely `Err(ComponentError::Yaml { … })` for the
all-files-fail case (the accumulated last-seen parse error), or a policy
decision to return `Ok(HashMap::new())` for any non-fatal-walk outcome.

### D2-1 — invariant 6 removed

**Status: CLOSED — fully correct.**

- `grep -n "Python-side compatibility" cbsd-rs/docs/cbscore-rs/CLAUDE.md`
  returns zero hits. Pass.
- §Correctness Invariants now has exactly 6 numbered items (1–6). Pass.
- Item 6 is "Runner container reproducibility" (the former item 7). Pass.
- A new `## Never touch Python code` section appears at line 335 of CLAUDE.md,
  stating the boundary explicitly. Pass.
- No plan file references "Correctness Invariants item 6" by number (no grep
  hits for "invariant 6" in plans/). Pass.

### D2-2 — log-file-location not tested

**Status: CLOSED — deliberate non-action confirmed.**

Phase 6 Commit 1 §Testable names four test cases (help output, unexpected-flag
rejection, placeholder-handler exit code, and exit-code mapping). No
log-file-location test bullet appears. The operator decision to skip this test
is preserved exactly as recorded in the audit finding. Pass.

### D3-1 — ComponentError 5-variant restructure

**Status: CLOSED in primary targets; residual surfaced (N1 above).**

- Phase 1 Commit 2 §Files enumerates the 5 flat variants: `Walk`,
  `Yaml { path, message: String }`, `MissingSchemaVersion`,
  `UnknownSchemaVersion`, `DuplicateComponentName`. No `SchemaVersionError`
  aggregate. Pass.
- `grep -n "SchemaVersionError" plans/*.md design/*.md` returns zero hits across
  designs and plans. The single hit is in Phase 1 at line 219, inside the
  `ComponentError` spec itself — as a negative statement: "(no reference to any
  `SchemaVersionError` aggregate — the variants are flat)". Correctly worded.
  Pass.
- `Yaml` variant carries `message: String`, not a `serde_saphyr::Error` type.
  Pass.
- Phase 5 Commit 2 §Files documents `ComponentError` as declared in Phase 1
  Commit 2, imports without redefining, and names the conversion
  `|e| ComponentError::Yaml { path, message: e.to_string() }` at the call site.
  Pass.
- **Exception (N1):** Phase 5 Commit 2 §Design constraints line 195 references
  `ComponentError::Parse` — a variant that does not exist in the 5-variant spec.
  Full analysis under D1-1 / N1 above.

### D3-2 — uuid get_version

**Status: CLOSED — fully correct.**

- `grep -n "get_version_num" design/005-*.md` returns zero hits. Pass.
- Two call sites in design 005 use
  `uuid.get_version() == Some(uuid::Version::SortRand)`: line 435 (title
  generator sketch) and line 472 (post-sketch prose). Pass.

### D3-3 — serde_value sketch

**Status: CLOSED — fully correct.**

- `grep -n "peek_marker" design/002-*.md` returns zero hits. Pass.
- The §Wire-Format Versioning hand-rolled `Deserialize` sketch (design 002 lines
  356–393) uses `Value::deserialize(d)?` (line 363), matches on `Value::Map(m)`
  (line 366), looks up the marker via `map.get(&key)` (line 370), dispatches via
  `value.into_deserializer()` (line 382). All three operations are valid API
  calls against `serde-value = "0.7"`. Pass.

### D6-1 — §Status v23

**Status: CLOSED — fully correct.**

All seven seq-002 phase plans (Phases 1–7) read:

> **Approved — finalized and ready for implementation.** Last audited at the v23
> corpus pass (`reviews/002-20260513T1356-plan-cbscore-rust-port-design-v23.md`,
> verdict commit `cd22cb8`); zero findings across CRITICAL / MAJOR / MINOR /
> SUGGESTION / OPEN QUESTION on the seq-002 phase plans.

- `grep -n "verdict commit \`a806158\`" plans/002-\*.md` returns zero hits.
  Pass.
- seq-004 §Status is unchanged: "Approved — finalized and ready for M1
  implementation. Audited at v2 … verdict `49d6f78`". Pass.

### D7-1 — Vault env-var contract

**Status: CLOSED — fully correct.**

Phase 3 Commit 2 §Testable (lines 217–224) names the env-var contract
explicitly: `CBSCORE_TEST_VAULT_ADDR` (defaults to `http://127.0.0.1:8200` when
unset) and `CBSCORE_TEST_VAULT_TOKEN` (the root token printed by
`vault server -dev`), with the "set to enable / skip with clear message when
missing" pattern. Both names carry the `CBSCORE_TEST_*` prefix consistent with
the Phase 3 Commit 1 AWS-credential env vars and the Phase 6 / Phase 7 patterns.
Pass.

### D8-1 — VaultError variants

**Status: CLOSED — fully correct.**

Phase 3 Commit 2 §Files names four variants explicitly:

- `PathNotFound { mount: String, path: String }` — 404 / missing-secret.
- `AuthFailed { method: &'static str, source: vaultrs::error::ClientError }`.
- `RequestFailed { source: vaultrs::error::ClientError }`.
- `BadResponse { message: String }`.

§Testable references `Err(VaultError::PathNotFound)` (negative test for missing
path) and `Err(VaultError::AuthFailed { method: "approle", .. })` (negative test
for invalid role_id). Variant names are consistent between the declaration and
the test bullets. Pass.

## No-Drift Check

1. **Phase 5 commit structure:** 7 commits; `core::component` is Commit 2. Pass.
2. **Phase 1 Commit 2 logger enumeration:** 22 named targets (`cbscore`,
   `cbscore::config`, `cbscore::core::component`, `cbscore::secrets`,
   `cbscore::runner`, `cbscore::builder`, `cbscore::builder::prepare`,
   `cbscore::builder::rpmbuild`, `cbscore::builder::signing`,
   `cbscore::builder::upload`, `cbscore::containers`, `cbscore::images::skopeo`,
   `cbscore::images::signing`, `cbscore::images::sync`, `cbscore::releases`,
   `cbscore::utils::buildah`, `cbscore::utils::git`, `cbscore::utils::podman`,
   `cbscore::utils::s3`, `cbscore::utils::subprocess`, `cbscore::utils::vault`,
   `cbscore::versions`). Pass.
3. **Phase 3 Commit 3 `Secrets` struct:** `HashMap<String, GitCreds>`,
   `HashMap<String, StorageCreds>`, `HashMap<String, SigningCreds>`,
   `HashMap<String, RegistryCreds>` — all four families present. Pass.
4. **Design 002 §Secrets:** all four families (`GitCreds`, `StorageCreds`,
   `SigningCreds`, `RegistryCreds`) present. Pass.
5. **Design 004 `write_descriptor` mkdir-p:** `create_dir_all` lives inside
   `write_descriptor`; call site does not repeat it. Pass.
6. **Prettier:** `prettier --check` passes on all 11 edited files (CLAUDE.md,
   design 001, design 002, design 005, plans Phase 1–7). Pass.
7. **No new contradictions across designs:** D1-1, D2-1, D3-2, D3-3, D6-1, D7-1,
   D8-1 — no new contradictions introduced by any closure. D3-1 closure
   introduced one residual (N1 / `ComponentError::Parse`).

## Findings

### N1 — `ComponentError::Parse` stale reference in Phase 5 Commit 2

**Severity: MINOR**

**Location:** Phase 5 plan (`002-20260508T1558-05-builder-and-releases.md`),
Commit 2 §Design constraints, line 195.

**Problem:** The sentence reads: "The function returns
`Err(ComponentError::Parse)` only if **no** components were successfully
loaded." `ComponentError::Parse` does not exist in the 5-variant shape defined
by D3-1 in Phase 1 Commit 2. An implementer following this text would reference
a missing variant and receive a compile error.

**Additional contradiction:** The §Testable immediately below (line 219) says
the empty-tree case returns `Ok(HashMap::new())`, not `Err(…)` — which conflicts
with the §Design constraints sentence on its own terms regardless of the variant
name.

**Resolution:** One-sentence fix. Decide and state the intended behaviour when
every `cbs.component.yaml` file in the tree fails to parse:

- Option A (lenient): return `Ok(HashMap::new())` — same as the empty-tree case.
  Consistent with the §Testable text and the Python `try / except continue`
  semantics (which never raises when the loop exhausts). Remove the sentence
  entirely, or replace with "the function returns `Ok(HashMap::new())` when
  every file fails to parse (all errors logged at WARN)."
- Option B (strict): add a `NoneLoaded` (or `AllFailed`) variant to
  `ComponentError` in Phase 1 Commit 2 and update Phase 5 to reference it.
  Requires a one-line addition to the Phase 1 spec.

The existing §Testable text for the empty-tree case (`Ok(HashMap::new())`)
suggests Option A is the intended behaviour; Option B would require a
corresponding testable bullet for the all-files-fail scenario that is currently
absent.

## Verdict

**CONDITIONAL — D1-1+D2-1+D2-2+D3-1+D3-2+D3-3+D6-1+D7-1+D8-1 closures confirmed;
1 residual minor (N1) must close before Phase 1 implementation start.**

N1 (`ComponentError::Parse` stale reference in Phase 5 Commit 2 §Design
constraints) is a direct compile-blocker for Phase 5, not Phase 1. However, a
single-sentence fix applied to the plan now costs nothing, while discovering it
during Phase 5 Commit 2 implementation creates a loop back to plan revision at
the worst possible moment. Per the project rule that all findings close before
implementation begins, this must be resolved before the implementation gate
opens.

Once N1 is closed:

> **Approve — D1-1+D2-1+D2-2+D3-1+D3-2+D3-3+D6-1+D7-1+D8-1 closed; pre-impl
> audit pass 4 fully resolved; design corpus + plan corpus ready for Phase 1
> implementation start.**
