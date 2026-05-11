# Plan Review — cbscore Rust Port (v11)

**Scope:** Comprehensive confirmation pass over Phases 1–4 (regression check
only) plus first-review of Phase 5 (M1.4 — builder pipeline + releases + image
sign/sync). Phase 5 is the primary focus. README sanity check included.

**Artefacts reviewed:**

- `002-20260508T1558-05-builder-and-releases.md` — Phase 5 (M1.4, **new**)
- `002-20260508T1558-01-types.md` — Phase 1 (M0, unchanged since v10)
- `002-20260508T1558-02-subprocess-and-shell-tools.md` — Phase 2 (M1.1,
  unchanged since v10)
- `002-20260508T1558-03-storage-and-secrets.md` — Phase 3 (M1.2, unchanged since
  v10)
- `002-20260508T1558-04-runner.md` — Phase 4 (M1.3, unchanged since v10)
- `plans/README.md` — index (updated to include Phase 5 link)
- Design 001 — §Workspace Layout lines 133–137 (containers/ subtree)
- Design 002 — §Build Pipeline lines 866–938, §Image Sign & Sync lines
  1073–1104, §Releases & S3 lines 1106–1170

---

## 1. Summary Assessment

Phase 5 is coherently structured and well-reasoned. The six-commit decomposition
covers the right subsystem boundaries, LOC estimates are plausible, and the
design-alignment is strong across five of the six commit areas. Three issues
require resolution before implementation can start: (1) `transit_sign` has no
enumerated declaration site — Phase 3 doesn't specify it and Phase 5 says "if
not previously" without nominating a concrete owner; (2) three intermediate
types (`BuildOptions`, `RpmArtifact`, `ContainerImageReport`) are used in
public-function signatures across multiple commits but are never declared in any
phase plan; (3) the `upload::run` signature omits `&SecretsMgr`, which is needed
for registry credentials at image-push time. These are concrete compilation
blockers if left unresolved. The remaining findings are minor or suggestions.
Phases 1–4 are free of regression.

---

## 2. Strengths

- **Phase-4-is-a-peer, not-a-dependency** is correctly argued and carried
  through. The call graph (host-side runner spawns container → in-container
  `cbsbuild` CLI → `run_build`) means Phase 5 genuinely has no import-time
  dependency on Phase 4, and the plan makes this explicit with a concrete
  rationale paragraph. This is the right level of precision.

- **"Sign before push" is named and enforced by design.** Commit 6's
  `images::sync` section states the order constraint explicitly (`sign_manifest`
  before `skopeo_copy`), names it as a "tested precondition of the downstream
  registry tooling", and backs it with a call-order assertion test. That is
  exactly the right treatment for an invariant this load-bearing.

- **Optional-signing carve-out is preserved on both sides.** Both
  `builder::signing` and `images::signing` gate on `config.signing` being
  `Some`, per design 002 lines 1094–1096. The language "no-ops returning empty
  reports" is precise enough to implement correctly.

- **Commit-size rationale for the two below-sweet-spot commits (C5, C6) is
  well-reasoned.** C5's "integration boundary" argument is convincing; C6's
  bundling argument (containers::build + images::sync co-produce the same
  artifact) is sound.

- **Cross-phase dependency enumeration is complete.** The §Depends on section
  lists every Phase 1/2/3 symbol by module path. A future implementer can grep
  for each one and know exactly what they are building on.

- **S3 upload gate is explicit.** "Upload is gated on `config.storage`" with a
  no-op `UploadReport` when storage is unset mirrors the design exactly and is
  backed by a dedicated negative test.

- **Patch walker alignment with design 005 is correct.** The `MalformedVersion`
  guard ("no major/minor/patch known → skip subdirectory, only top-level patches
  apply") matches design 005 §Effects of UUIDv7 VERSIONs §Patch walker exactly.

- **README Phase 5 link resolves.** The link in the README table to
  `002-20260508T1558-05-builder-and-releases.md` corresponds to the actual file.
  Status "Pending" is correct.

---

## 3. Blockers

### V11-B1 — `transit_sign` has no enumerated declaration site

**What.** Phase 5 Commit 3 says: "Vault transit signing via
`utils::vault::transit_sign` (Phase 3 Commit 2 ships KV read; transit signing is
the parallel HTTP API — implementation lands here in Phase 5 if not
previously)." Phase 3 Commit 2's §Files list specifies only `kv_read`, auth
helpers, and `VaultError`; it does not enumerate `transit_sign` in the public
surface. The "if not previously" hedge means neither phase explicitly owns the
function's declaration.

**Why it matters.** `utils::vault::transit_sign` is consumed by both
`builder::signing` and `images::signing` in Commit 3. If it is not in the Phase
3 plan, the Phase 3 implementer will not write it, and the Phase 5 implementer
will encounter a missing function. The hedge creates ambiguity about which
commit adds it to `utils/vault.rs`, which could lead to either (a) Phase 5
adding a new function to a Phase 3 module mid-build (widening blast radius), or
(b) the function simply not existing when Phase 5 Commit 3 needs it.

**Fix.** One of two resolutions:

1. **Preferred — update Phase 3 Commit 2** to add
   `transit_sign(config: &VaultConfig, key_name: &str, input: &str) -> Result<String, VaultError>`
   to its §Files public surface. The function is a straightforward `vaultrs`
   call parallel to `kv_read`; landing it in Phase 3 keeps the vault module
   cohesive and avoids Phase 5 touching a prior-phase module.
2. **Acceptable — update Phase 5 Commit 3** to explicitly state "adds
   `utils::vault::transit_sign` to the existing `utils/vault.rs` file" in its
   §Files list, removing the hedge entirely.

Either way, the hedge "if not previously" must be replaced by an explicit
commitment in exactly one phase.

---

### V11-B2 — `BuildOptions`, `RpmArtifact`, and `ContainerImageReport` are undeclared

**What.** Three types appear in Phase 5 public-function signatures but are not
declared in any phase plan:

1. **`BuildOptions`** — appears in `prepare::run`, `rpmbuild::run`, and
   `run_build` signatures (Commits 1, 2, 5). Design 002 §Stage contracts says
   `skip_build` and `force` "become fields on `BuildOptions`" but names no
   declaration site.
2. **`RpmArtifact`** — appears in
   `upload_release(desc, rpms: &[RpmArtifact], config)` (Commit 4). Neither
   Phase 1's descriptor types nor Phase 2's report types enumerate this type.
3. **`ContainerImageReport`** — return type of `build_image` (Commit 6). Not in
   Phase 1's type list.

**Why it matters.** All three are part of public function signatures. Without an
explicit declaration site (module path + field layout), the implementer must
invent the type at implementation time without a review-approved spec. For
`BuildOptions` in particular, the field layout (`skip_build`, `force`) is
present in the design but the crate placement is not: it could land in
`cbscore-types::builder` (zero-IO, shareable) or in `cbscore::builder::mod`
(library-internal). These are different API-stability decisions. `RpmArtifact`
and `ContainerImageReport` have the same ambiguity.

**Fix.** For each type, add a declaration entry to the phase that introduces it:

- **`BuildOptions`** — add to Phase 5 Commit 1 §Files (or Commit 5 if
  preferred), specifying:
  - Module path: `cbscore::builder::BuildOptions` (recommend `cbscore`, not
    `cbscore-types` — it is only consumed inside the builder pipeline and by
    Phase 6's CLI; no external consumer needs it at `cbscore-types` import
    time).
  - Fields: `pub skip_build: bool`, `pub force: bool` (per design 002 line
    930–931).
- **`RpmArtifact`** — add to Phase 5 Commit 2 (produced by the rpmbuild stage)
  or Commit 4 (first consumption site). Specify module path and fields (path to
  the `.rpm` file, component name, arch, etc.).
- **`ContainerImageReport`** — add to Phase 5 Commit 6. Specify module path and
  fields (image ID, tag, digest, or similar).

---

### V11-B3 — `upload::run` signature omits `&SecretsMgr` (image-push registry credentials)

**What.** Phase 5 Commit 4 specifies:

```
pub async fn run(
    desc: &VersionDescriptor,
    config: &Config,
    signed: &SigningReport,
) -> Result<UploadReport, BuilderError>
```

The §Design constraints text says "Image push goes through
`utils::buildah::buildah_push` (or the equivalent skopeo-copy from Phase 2
Commit 3) per design 002 §Image Sign & Sync; the in-container image push uses
the registry creds resolved from `SecretsMgr`." The registry credentials come
from `SecretsMgr`, but `SecretsMgr` is absent from the function signature.

**Why it matters.** `buildah_push` (or `skopeo_copy` with `--dest-creds`) must
supply registry credentials. Without `&SecretsMgr` in the signature, the
function has no mechanism to pass credentials to the image-push subprocess. The
design's own orchestrator sketch (design 002 line 918) omits `secrets` from
`run_build` too — but that omission is a known sketch incompleteness (the plan
correctly adds `&SecretsMgr` to `run_build` in Commit 5). The upload stage is
the place where registry credentials are consumed; they must be in scope.

**Fix.** Add `secrets: &SecretsMgr` to `upload::run`'s signature and thread it
from `run_build` → `upload::run`. Propagate the same to `releases::s3` functions
that push image manifests, if they also consume credentials. Update the Commit 4
§Files §Testable accordingly.

---

## 4. Major Concerns

### V11-M1 — `upload::run` image-push vs RPM-upload conflation

**What.** Commit 4's §Design constraints says the stage "uploads signed RPMs to
S3, pushes the built container image to the registry, writes the release
descriptor." This conflates two distinct operations: RPM → S3 (storage-gated)
and image → registry (credential-gated). The `UploadReport` that flows into
`BuildArtifactReport::new` must carry both outcomes. However, image push depends
on `build_image` (Commit 6's `containers::build`) — a module that does not exist
until Commit 6. If `upload::run` calls `containers::build_image`, it creates a
forward dependency from Commit 4 to Commit 6 within Phase 5.

**Why it matters.** The commit ordering in Phase 5 is C1 → C2 → C3 → C4 → C5 →
C6. If Commit 4 calls into `containers::build_image`, Commit 4 cannot compile
until after Commit 6 lands — breaking the "each commit must compile" invariant
from the CLAUDE.md §Commit Granularity.

Examining the design more carefully: design 002's pipeline diagram shows "image
→ reg" as part of `upload`, but image _building_ is separate from image
_pushing_. The Python `builder/upload.py` likely calls buildah/skopeo to push an
already-built image. If the image is built during the rpmbuild or a separate
containers step that currently lives in Commit 6, the dependency direction needs
clarification.

**Fix options:**

1. **Move `containers::build_image` to an earlier commit** (e.g., split from
   Commit 6 into a Commit 4.5 between C4 and C5) so the dependency is satisfied
   before `upload::run` uses it.
2. **Clarify that `upload::run` receives a pre-built image handle** (e.g., an
   `Option<ImageRef>` from a prior step) rather than calling `build_image`
   itself. The image-build step would then need to be explicitly placed earlier
   in the pipeline or in a stage not currently named.
3. **Explicitly state that image push in `upload::run` is stubbed to a no-op**
   until Commit 6 adds the `containers` module, and that the integration test
   for the full path is an `#[ignore]`-able test gated on Commit 6 existing.

The plan needs a sentence resolving which model applies.

---

## 5. Minor Issues

- **V11-N1 — `signing::run` parameter order vs design 002 sketch.** Design 002
  line 925 shows `signing::run(desc, config, &rpms)` (no `secrets`). Phase 5
  Commit 3 correctly adds `secrets: &SecretsMgr` to `signing::run`'s signature
  because GPG passphrases and transit key names come from the secrets manager.
  This is a correct divergence from the design sketch. However, the plan should
  note it explicitly ("the design sketch omits `secrets`; the plan adds it
  because signing requires resolved GPG passphrases from `SecretsMgr`") so the
  implementer does not revert to the design sketch signature when reconciling
  design vs plan. One sentence in Commit 3 §Design constraints is sufficient.

- **V11-N2 — `releases::s3` read operations and Phase 5 scope.** The §Out of
  scope section says: "Phase 6 owns the consumer; Phase 5 lands the underlying
  S3 read operations in `releases::s3`." Design 002 §S3 operations names
  `check_release_exists` and `check_released_components` as read operations.
  Phase 5 Commit 4's §Files list for `releases/s3.rs` enumerates only
  `upload_release`. If `check_release_exists` and `check_released_components`
  are intended to land in Phase 5, they need to appear in Commit 4's §Files
  list. If they land in Phase 6 (alongside the consumer), the §Out of scope
  clause is misleading — it says "Phase 5 lands the underlying S3 read
  operations" but Commit 4 doesn't list them. Clarify either way.

- **V11-N3 — `images::signing` module name inconsistency.** Phase 2 Commit 3
  specifies `pub mod skopeo;` inside `cbscore/src/images/mod.rs`. Phase 5 Commit
  3 says "add `pub mod signing;` alongside the existing `pub mod skopeo;`".
  Phase 5 Commit 6 says "add `pub mod sync;` alongside the existing
  `pub mod skopeo;` (Phase 2) and `pub mod signing;` (Phase 5 Commit 3)". This
  is consistent — no issue with the accumulation. However, the design 001 layout
  (line 139) names the module `images/signing.rs` while Commit 3's public
  surface calls the function `sign_manifest`. This is consistent (module name vs
  function name) and is not an issue; noting explicitly to confirm no confusion.

- **V11-N4 — `builder::signing`'s optional-signing is implicit.** The
  optional-signing carve-out is stated cleanly for `images::signing` in Commit
  3's §Design constraints: "when `config.signing` is `None`, both
  `builder::signing` and `images::signing` become no-ops returning empty
  reports." The `images::signing` side is clear. The `builder::signing` side is
  named in the same sentence but not given a dedicated bullet, which means the
  implementer sees the rule for `images::signing` but has to infer the
  `builder::signing` no-op path. Add one explicit sentence to `builder::signing`
  §Design constraints: "When `config.signing` is `None`, `builder::signing::run`
  returns `Ok(SigningReport::empty())` without invoking `rpm --addsign` or the
  GPG subprocess."

- **V11-N5 — `BuildArtifactReport::new` not named in Phase 1.** Phase 5 Commit 5
  says "this file holds the constructor that gathers per-stage reports into a
  single artifact summary" in `cbscore/src/builder/report.rs`. Phase 1 Commit 4
  specifies `cbscore-types::builder::report::BuildArtifactReport` as the type
  declaration site. The plan is clear that Phase 1 owns the type and Phase 5's
  `report.rs` in `cbscore` (not `cbscore-types`) owns the constructor. This is
  correct. Noting it as confirmed-correct to avoid future confusion.

- **V11-N6 — README description omits `containers/`.** The README's Phase 5
  description reads "M1.4 — builder pipeline stages + `run_build` orchestrator
  - releases + image sign/sync — 5–6 commits". The `containers/` module
    (`containers::build`, `containers::component`, `containers::repos`) is a
    significant deliverable that is omitted. Since `containers/` feeds
    `images::sync` (Commit 6 bundles them), its absence from the description
    could mislead a reader scanning the index. Suggest expanding to: "M1.4 —
    builder pipeline stages + `run_build` orchestrator + releases + containers
    module + image sign/sync — 6 commits". Not blocking, but the inaccuracy will
    accumulate as readers use the README as a map.

---

## 6. Suggestions

### V11-S1 — Consider whether `RpmbuildReport` carries `RpmArtifact` paths or bare `Utf8PathBuf`s

Phase 5 Commit 2 produces `RpmbuildReport` but does not enumerate its fields.
Commit 4 passes `rpms: &[RpmArtifact]` to `upload_release`. If `RpmbuildReport`
carries `Vec<RpmArtifact>`, that is the natural shape. If `RpmbuildReport`
carries `Vec<Utf8PathBuf>`, then `RpmArtifact` must be constructed from those
paths in Commit 4 with additional metadata (arch, component, etc.) that is not
available post-rpmbuild without re-parsing the artifact path. Deciding this at
plan time rather than implementation time prevents a structural mismatch between
C2 and C4.

Recommendation: define `RpmArtifact` in Commit 2 alongside `RpmbuildReport`
(both are rpmbuild-stage outputs), and have `RpmbuildReport` carry
`Vec<RpmArtifact>`. Then Commit 4's `upload_release` receives the natural slice
from `RpmbuildReport::rpms` without a conversion step.

### V11-S2 — Commit 5 stub-stage test technique warrants a brief note on test double approach

Commit 5 §Testable proposes a "stub-stage integration test: replace each stage's
`run` with a test double". Since the stages are free async functions (not
trait-object-behind-an-interface), the stub approach requires either (a)
feature-flag substitution, (b) function-pointer injection via an explicit
`StageRunner` trait, or (c) recording the actual call order through side effects
(e.g., a shared `Arc<Mutex<Vec<String>>>` that each stage pushes to). Option (c)
is the simplest and requires no design change. A sentence noting "side-channel
counter via `Arc<Mutex<Vec<String>>>` passed via `RunContext` or equivalent"
avoids the implementer discovering at test-writing time that free async
functions cannot be stubbed the way a method call can.

### V11-S3 — The `containers::repos` variant list warrants a note on the `unknown` variant

Commit 6 names `copr`, `file`, and `url` as repo variants from the descriptor.
The Python `containers/repos.py` likely handles an `unknown` or `unsupported`
variant with an error. A sentence noting the expected error variant for an
unsupported repo type (`ContainerError::UnknownRepoType` or similar) keeps the
test surface explicit and prevents a silent `unreachable!()` macro in the
implementation.

---

## 7. Open Questions

### V11-OQ1 — Does `upload::run` build the container image, or does it push a pre-built image?

The §Design constraints text says "pushes the built container image to the
registry" and references `buildah_push`. But `containers::build_image` (which
builds the image) lands in Commit 6. If `upload::run` in Commit 4 depends on
`containers::build_image`, the commit-ordering invariant is violated. If
`upload::run` receives a pre-built image reference (e.g., a local image tag
produced by an earlier step not yet named in the pipeline), that step needs to
be identified. The Python `builder/upload.py` is the reference; the plan should
state explicitly whether the container image is built before or within this
stage.

---

## 8. Phases 1–4 Regression Check

Phases 1, 2, 3, and 4 are byte-for-byte unchanged since v10. The v10 open minor
(V10-N1 — Phase 1 C2 `RunnerError::Timeout` parenthetical says "internal" where
it should say "outer runner-level") carries forward as previously noted and
remains a minor; Phase 5's introduction does not affect it.

Cross-phase regression check for new Phase 5 references:

- **`cbscore-types::releases::desc::ReleaseDesc`** — Phase 5 Commit 4 cites this
  type. Phase 1 Commit 4's §Files lists `cbscore-types/src/releases/desc.rs`
  with `ReleaseDesc` explicitly. Names match. No regression.
- **Patch walker cross-reference** — Phase 5 Commit 1 references "Phase 2 Commit
  5 spelled this out" for the `MalformedVersion` guard. Phase 2 Commit 5 §Design
  constraints names `get_major_version` / `get_minor_version` as returning
  `Result<…, MalformedVersion>`; Phase 5's guard is exactly the
  `Err(MalformedVersion)` branch of those calls. The cross-reference is
  accurate.
- **Phase 4 `versions::desc` IO** — Phase 5 does not call
  `versions::desc::read_descriptor` or `write_descriptor`. The descriptor
  arrives pre-deserialized via the CLI (Phase 6). No Phase 4 IO dependency. No
  regression.
- **`ContainerError` for `containers::build_image`** — Phase 5 Commit 6
  specifies `-> Result<ContainerImageReport, ContainerError>`. Phase 1 Commit 2
  declares `ContainerError` in `cbscore-types::containers::errors`. The error
  type reference is correct.

**Summary:** Phases 1–4 are free of regression introduced by Phase 5.

---

## Verdict

**Phase 5 does not yet meet the bar for implementation start.** Three blockers
require resolution: V11-B1 (`transit_sign` declaration gap), V11-B2
(`BuildOptions`/`RpmArtifact`/`ContainerImageReport` undeclared), and V11-B3
(`upload::run` missing `&SecretsMgr` for image-push credentials). One major
concern (V11-M1) must also be resolved to preserve the per-commit compilation
invariant.

All four are documentation/specification gaps — none require architectural
rethinking, and all have clear resolution paths. Once addressed in a v12 pass,
Phase 5 should be ready to proceed.

- **New findings by severity:** 3 blockers, 1 major, 6 minor, 3 suggestions, 1
  open question.
- **Phases 1–4:** Free of regression. V10-N1 carries forward unchanged.
- **Phase 5:** Not yet ready for implementation start.
