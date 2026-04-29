# Design Review v1: Configurable VersionDescriptor Location

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/004-20260429T1319-configurable-version-descriptor-location.md`

**Prior reviews:** none — first review of design 004.

---

## Summary Assessment

**Verdict: approve with one important correction and two minor issues.**

The design is focused, internally consistent, and ready for M1 implementation.
All seven OQs are coherently resolved; the rationale is sound throughout; and
the cross-doc edits in designs 002 and 003 land cleanly. One important issue
requires a fix before the design can be called implementation-complete: the
`descriptor_path` helper is ascribed to `cbscore-types` in the prose, but it is
placed in a contradictory location in the §Design Sketch. A second minor issue
concerns a subtlety in how `resolve_root` converts the `current_dir()` error
into a `VersionError`. A third minor issue is a reference to `cbscore::versions`
where the correct module is `cbscore::versions::resolve_root`.

---

## Strengths

**OQ4 (read sites stay explicit-path) is well-evidenced.** The design
cross-checks four concrete Python call sites (`builds.py:135`, `builds.py:263`,
`runner.py:202`, `cbscore-wrapper.py`) and makes explicit that the wrapper
builds `VersionDescriptor` in-memory, never from disk. This is a strong
argument, not hand-waving, and it validates the decision to leave read sites
untouched.

**OQ2 fallback error message is specified to the right level of detail.** The
verbatim error text in OQ5 names both override surfaces. This is exactly the
kind of ergonomic detail that is easy to lose at implementation time and
important to operators who hit the fallback unexpectedly.

**The operator scenario table in §Migration is complete.** All four realistic
operator states are covered, including the previously-blocked case (worker host
with no git checkout) — which is the main motivation for the design. The table
makes the change's impact legible at a glance.

**OQ6 (no schema bump pre-M1) is correctly scoped.** The qualifier added to
design 002 §Wire-Format Versioning applies the standing rule cleanly: accumulate
into v1 during development, bump at the post-1.0 boundary. The rationale is
consistent with design 001 §Versioning.

---

## Important Corrections

### I1 — `descriptor_path` crate placement is contradictory

The OQ3 resolution prose and the §Design Sketch header both reference
`cbscore-types::versions::desc` (zero-IO types crate), which is the right home
for a pure path-building function: no IO, no async, no subprocess. The actual
code block in §Design Sketch says:

```
`cbscore::versions::desc::descriptor_path` in
`cbscore-types/src/versions/desc.rs`
```

That is two contradictory namespaces in one sentence. The file path
(`cbscore-types/src/versions/desc.rs`) is correct per design 001's crate-split
rules: the function is pure (`root.join(…).join(…)`), takes only primitives and
`VersionType`, and has no IO or async dependency — it belongs in
`cbscore-types`, not `cbscore`. The function path prefix
`cbscore::versions::desc::descriptor_path` must be corrected to
`cbscore_types::versions::desc::descriptor_path`.

This matters for implementation: the write site in
`cbsbuild/src/cmds/versions.rs` will import from `cbscore_types`, not from
`cbscore`, and the two callers (write site now, any future reader) depend on
finding it in the types crate's public API. Using the wrong import path will
produce a compile error.

**Resolution:** Change the function path in §Design Sketch §Path builder from
`cbscore::versions::desc::descriptor_path` to
`cbscore_types::versions::desc::descriptor_path`. The file path
`cbscore-types/src/versions/desc.rs` is already correct.

---

## Minor Issues

### M1 — `current_dir()` error silently dropped in `resolve_root`

In the §Design Sketch §Resolver, the fallback error arm is:

```rust
Err(_) => Err(VersionError::NoDescriptorRoot {
    cwd: std::env::current_dir()?.try_into()?,
}),
```

The `?` inside the `Err(_)` arm propagates a `std::io::Error` (from
`current_dir()`) or a `camino::FromPathBufError` (from `try_into()`). Both would
surface as a different error variant, bypassing the `NoDescriptorRoot` message.
In practice `current_dir()` fails only when the process's working directory has
been deleted out from under it — a rare but non-zero case on long-running
daemons. For a CLI tool like `cbsbuild` the risk is negligible, but the `?`
propagation is still surprising: the caller designed to get `NoDescriptorRoot`
with the OQ5-specified message instead gets a raw I/O error.

The simplest fix is to use `unwrap_or_else` with a fallback path (e.g.
`Utf8PathBuf::from("<unknown>")`) so the `NoDescriptorRoot` message always
fires:

```rust
let cwd = std::env::current_dir()
    .ok()
    .and_then(|p| p.try_into().ok())
    .unwrap_or_else(|| Utf8PathBuf::from("<unknown>"));
Err(VersionError::NoDescriptorRoot { cwd })
```

Decide at implementation time. The current sketch is misleading about the actual
error type the operator will see in the degenerate case.

### M2 — Write-site sketch references unqualified `cbscore::versions`

The §Design Sketch §Write site calls `cbscore::versions::resolve_root(...)`.
Given that `resolve_root` is defined in `cbscore/src/versions/mod.rs`, the Rust
path is correct as written — but the immediately preceding §Path builder used
the wrong prefix for `descriptor_path` (see I1). After fixing I1, the contrast
between `cbscore_types::versions::desc::descriptor_path` (types crate) and
`cbscore::versions::resolve_root` (library crate) will be visible and correct.
This is a note for the implementer: these are two different crates and two
different import statements, which is intentional per the design.

No text change is needed once I1 is fixed; this is a heads-up for the
implementation commit.

---

## Suggestions

**S1 — Nail down `create_dir_all` ownership in §Write site.** The current text
says "Decide at implementation time; either is correct." This is fine for a
design document, but the two options have different visibility consequences: if
`desc.write()` internally does `mkdir -p`, every future call site that calls
`desc.write()` gets the mkdir silently, which may or may not be desirable. A
design-level nudge ("prefer explicit `create_dir_all` at the call site to keep
`VersionDescriptor::write` a pure serialise-and-write") would prevent an API
decision from being made ad-hoc at implementation time. Non-blocking; document
as is if the ambiguity is intentional.

**S2 — The operator scenario for `--for-systemd-install` says "re-run
`cbsbuild config init --for-systemd-install`".** Until design 003 ships
(post-M1), that interactive flag does not produce the new `paths.versions` field
because the Step 6 prompt is not implemented yet. The M1 bypass mode is
flag-driven: operators will need to manually add
`paths.versions: /cbs/_versions` to their `cbs-build.config.yaml` rather than
regenerate it until design 003 lands. Worth a note in the operator scenario
table to avoid confusion during M1 rollout. Non-blocking.

---

## OQ Internal Consistency — Full Walkthrough

All seven OQs verify as internally consistent:

- **OQ1 × OQ2 × OQ3:** The CLI flag overrides config overrides
  `git-root/_versions`. The layout under any root is
  `<root>/<type>/<VERSION>.json` regardless of how the root was supplied.
  Coherent.
- **OQ4 × OQ3:** Read sites take an explicit `desc_path` from the caller, so
  they never touch the configured root. `descriptor_path()` is only called at
  the write site. The claim "read sites stay explicit-path" is verified against
  the Python source at `builds.py:135`, `builds.py:263`, `runner.py:202`, and
  `cbscore-wrapper.py` (wrapper builds `VersionDescriptor` in-memory). It holds.
- **OQ5 × OQ2:** Default fallback is byte-identical to Python, so "no migration
  tooling" is correct for the majority case. Operators who relocate are making
  an informed choice. Coherent.
- **OQ6 × OQ1:** Adding `versions: Option<Utf8PathBuf>` with `#[serde(default)]`
  to `PathsConfig` is an additive, backwards- compatible (absent = `None`)
  change. Not bumping `schema_version` pre-M1 is consistent with the rule as
  qualified in design 002. The `#[serde(default)]` ensures files written before
  the field existed deserialise without error. Coherent.
- **OQ7 × OQ4:** Pre-filling `/cbs/_versions` in bypass mode is internally
  consistent with OQ4's note that bypass-mode deployments don't necessarily run
  `versions create`. The pre-fill is forward- looking, not load-bearing on M1.
  The design is clear about this. Coherent.

---

## Open Questions for the Author

None blocking the design. One clarification worth recording before
implementation:

- **`VersionError::NoDescriptorRoot` location:** `VersionError` lives in
  `cbscore-types::versions::errors` (per design 002 §Error Taxonomy), but the
  `cwd: Utf8PathBuf` field is populated by `std::env::current_dir()` — an IO
  call. The error _type_ is correct in the types crate; the error _construction_
  happens at the call site in `cbscore::versions::resolve_root`. This is the
  right split and no change is needed, but note it explicitly so the implementer
  does not accidentally move the error type into `cbscore` alongside the
  constructor.

---

## Summary of Action Items

| ID  | Severity     | Action                                                                                                                                |
| --- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| I1  | IMPORTANT    | Fix `cbscore::versions::desc::descriptor_path` → `cbscore_types::versions::desc::descriptor_path` in §Design Sketch §Path builder.    |
| M1  | MINOR        | Consider handling `current_dir()` / `try_into()` failures inside the `Err(_)` fallback arm to guarantee `NoDescriptorRoot` fires.     |
| M2  | MINOR        | Note (no text change): after I1, the two different import crates in the write-site sketch (`cbscore_types` vs `cbscore`) are correct. |
| S1  | NICE-TO-HAVE | Nudge toward explicit `create_dir_all` at the write-site call site rather than inside `VersionDescriptor::write`.                     |
| S2  | NICE-TO-HAVE | Add a note that M1 bypass-mode operators must manually add `paths.versions` until design 003 ships.                                   |
