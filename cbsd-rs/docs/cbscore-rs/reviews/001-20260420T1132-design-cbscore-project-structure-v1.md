# Design Review: cbscore Rust Port — Project Structure & Crate Layout

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/001-20260418T2045-cbscore-project-structure.md`

---

## Summary

**Verdict: approve-with-changes.**

The crate split is architecturally sound and mirrors the proven
`cbsd-rs` pattern well. The motivation for separating zero-IO types
from the IO-heavy library is clear. Three findings need attention
before implementation starts: one consumer import is wrong, the CLI
surface omits a flag that is actively exercised by callers, and the
entrypoint-bundling mechanism contains a code-level bug.

---

## Strengths

- **`cbscore-types` boundary is well-drawn.** Keeping all serde
  structs, error enums, and pure parse helpers in a zero-IO crate
  means `cbsd-rs/cbsd-worker` can gain compile-time wire-format
  agreement without dragging in tokio, aws-sdk-s3, or vaultrs. That
  is the primary architectural payoff and the design states it
  clearly.
- **Dependency sketches are actionable.** The provisional Cargo.toml
  blocks give implementors a starting point without over-committing;
  the caveat "exact versions pinned when the workspace is created" is
  the right posture.
- **Migration shapes are well-ordered and independently
  revertable.** The wire-format and CLI parity requirements are
  listed as unconditional, which correctly prevents the migration
  from becoming a flag day.

---

## Findings

### F1 — `cbsd` consumer import table is incomplete

**Section:** "Downstream Consumers" table (§ "Downstream Consumers")

The table shows `cbsd` imports only from `cbscore.errors`,
`cbscore.logger`, `cbscore.config`, `cbscore.runner`, and
`cbscore.versions`. Actual grep of the `cbsd/` tree reveals
additional import sites:

```
cbsd/cbslib/worker/types.py:
    from cbscore.versions.desc import VersionDescriptor

cbsd/cbslib/config/worker.py:
    from cbscore.config import ConfigError as CBSCoreConfigError

cbsd/cbslib/logger.py:
    from cbscore.logger import logger as root_logger
```

The `root_logger` object is not listed, and `ConfigError` appears in
the table for `cbsd-rs` (via the wrapper) but not for `cbsd` itself,
even though `cbsd/cbslib/config/worker.py` imports it directly.
`VersionDescriptor` is also absent from the `cbsd` row.

**Impact:** Any design that relies on this import table (e.g. the
Rust surface that must match what Python consumers can still import
from the existing Python `cbscore` package) will be incomplete and
miss symbols that `cbsd` actually uses.

**Action:** Audit every `from cbscore` import under `cbsd/` and add
the missing symbols to the `cbsd` row. At minimum add
`versions.desc.VersionDescriptor`, `config.ConfigError`, and
`logger.logger` (the module-level logger object, distinct from
`set_debug_logging`).

---

### F2 — CLI surface drops the `--cbscore-path` flag

**Section:** "cbsbuild" crate description, "Build & Run" section.

`cbscore/cmds/builds.py` defines `cbsbuild build` with a required
`--cbscore-path` argument (the path to the cbscore source tree that
the Python runner mounts at `/runner/cbscore`). The design states
"Binary mount instead of source mount" and the flag disappears in
the Rust CLI table.

This is correct *as an end-state* but the transition is not handled.
`cbsd-rs/scripts/cbscore-wrapper.py` calls the Python `runner()`
function directly (not via `cbsbuild` CLI), so it does not use this
flag. However, any operator or script that shells out to
`cbsbuild build --cbscore-path ...` will break the moment the Rust
binary ships, which violates CLI UX parity (Correctness Invariant 2
in CLAUDE.md).

**Action:** Either (a) retain `--cbscore-path` as a deprecated no-op
flag with a visible deprecation warning, or (b) add an explicit
note that this flag is being intentionally dropped — and document
that this constitutes a deliberate, design-approved UX break that
operators must be notified of before M1 ships. If (b), add it to the
design's "out of scope" section with the rationale.

---

### F3 — `write_entrypoint` mixes sync and async IO incorrectly

**Section:** "Runner Container" section, `write_entrypoint` code
snippet.

```rust
async fn write_entrypoint() -> io::Result<NamedTempFile> {
    let mut f = tempfile::Builder::new()
        .prefix("cbs-entry-").suffix(".sh").tempfile()?;
    f.write_all(ENTRYPOINT_SH.as_bytes()).await?;   // ← BUG
    ...
}
```

`tempfile::NamedTempFile` implements `std::io::Write`, not
`tokio::io::AsyncWrite`. Calling `.await` on `write_all` from
`std::io::Write` will not compile. The design snippet mixes the two
IO traits. Since the content is a small static `&str` (≤40 lines),
there is no reason to use async IO here at all.

**Action:** Replace with synchronous `std::io::Write::write_all` in
an `async fn` context (which is fine for a fast, in-memory write):

```rust
use std::io::Write as _;
f.write_all(ENTRYPOINT_SH.as_bytes())?;
```

Or, if the function must stay illustrative, add a note that this
is pseudo-code and the real implementation will use `std::io::Write`.

---

### F4 — `cbscore-types` pulls in YAML and JSON crates unnecessarily

**Section:** "Crate Dependencies — `cbscore-types`" Cargo.toml
sketch.

`cbscore-types` lists `serde_json = "1"` and
`serde_yaml_ng = "0.10"` as dependencies. The pure-types crate
should not need either. Types carry `#[derive(Serialize, Deserialize)]`
but do not perform IO. The YAML and JSON parsing belongs in the
`cbscore` library crate, which does the actual file reading.

Including both serde format crates in `cbscore-types` means every
downstream that depends on `cbscore-types` — including a future lean
`cbsd-rs` crate dep and any `no_std` consumer — transitively pulls
in `serde_yaml_ng`. That crate depends on `saphyr` and several other
crates, meaningfully inflating the dependency graph.

**Action:** Remove `serde_json` and `serde_yaml_ng` from
`cbscore-types/Cargo.toml`. Keep only `serde`, `thiserror`,
`tracing`, and `chrono`. Move `serde_json` and `serde_yaml_ng` to
the `cbscore` library crate where they are actually consumed.

---

### F5 — `anyhow` in a library crate is called out but then permitted

**Section:** "Crate Dependencies — `cbscore`" Cargo.toml sketch.

The comment reads:

```toml
anyhow = "1"  # only inside binaries; library is
              # allowed to use it in adapter glue
```

This contradicts the CLAUDE.md principle "Library code never uses
`anyhow`" (echoed in `002` § "Error Taxonomy"). The carve-out
"adapter glue" is undefined and will be stretched by implementors to
cover any inconvenient conversion.

**Action:** Remove `anyhow` from `cbscore`'s dependency list and
remove the comment. If a specific narrow case genuinely needs
`anyhow` (e.g. a `Box<dyn Error>` facade for an external crate's
opaque error type), add a note in the design explaining what that
case is. Otherwise, the rule from `002` should be the rule here too.

---

## Cross-document notes

See the `002` review for findings specific to Architecture &
Subsystem Design. The following points span both documents and are
noted here because they originate in this document's tables:

- The `--cbscore-path` CLI flag drop (F2) is a correctness invariant
  violation that `002` does not address; it needs an explicit
  decision recorded in one of the two design documents.

---

## Suggested follow-ups

- Rerun `grep -rn "from cbscore" cbsd/ cbsdcore/ cbc/ crt/` and
  reconcile the full list against the "Downstream Consumers" table.
  (`cbsd/cbslib/logger.py:26`, `cbsd/cbslib/worker/types.py:17`,
  `cbsd/cbslib/config/worker.py:19` are the three currently missing.)
- Decide explicitly on `--cbscore-path`: keep as no-op or drop with
  documented UX break notice. Record the decision in this doc.
- Move `serde_json` / `serde_yaml_ng` to `cbscore` crate deps;
  remove from `cbscore-types`.
- Fix the `write_entrypoint` snippet to use `std::io::Write` or mark
  it as pseudo-code.
- Remove `anyhow` from `cbscore` deps (or spell out the one narrow
  case where it is allowed).
