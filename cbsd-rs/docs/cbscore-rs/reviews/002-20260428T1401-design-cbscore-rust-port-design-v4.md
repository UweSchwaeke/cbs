# Design Review v4: cbscore Rust Port — Architecture & Subsystem Design

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`

**Prior reviews:**
`002-20260420T1132-design-cbscore-rust-port-design-v1.md`,
`002-20260420T1512-design-cbscore-rust-port-design-v2.md`,
`002-20260427T1330-design-cbscore-rust-port-design-v3.md`

---

## Summary

**Verdict: approve with one important correction.**

All six v3 findings (N3–N8) and the C4 advisory are confirmed closed. The
document now ends cleanly at line 1408 with the resolved Open Questions
section; the merge-conflict debris is gone. Config struct sketches use
`Utf8PathBuf` throughout. The `gen_run_name` sketch uses `rand::rng()`. The
rolling-deployment migration note is present.

One new issue requires attention: the `§ HOME normalisation` SAFETY comment
contains a self-contradictory justification that will mislead implementors.

---

## v3 Finding Verification

**N3 — Unresolved `diff3` merge conflict:** CLOSED. The file ends at line
1408 (`shipped`/`cbsbuild config init` OQ8 resolution). No conflict markers,
no duplicate sections.

**N4 — `PathBuf` in Config/PathsConfig sketches:** CLOSED. The
`§ Configuration & Secrets Subsystem — Types` sketch now uses `Utf8PathBuf`
for all path fields in `Config`, `PathsConfig`, and `VaultConfig`. The
`Config::load`/`store` signatures use `&Utf8Path`. The note about
`camino = { version = "1", features = ["serde1"] }` appearing in
`cbscore-types/Cargo.toml` is present.

**N5 — Stale "see Open Questions" forward reference:** CLOSED. The
`§ Subprocess & Secret Redaction` preamble now reads "the Python
`reset_python_env` flag is intentionally **not ported** — the Rust `cbsbuild`
binary has no venv-shadowing problem to solve", with no dead pointer to an
open question.

**N6 — `/bin/cbsbuild` path inconsistency:** CLOSED. Auto-resolved when N3
was fixed; the OQ7 text no longer exists in the document and the mount table
consistently shows `/runner/cbsbuild`.

**N7 — `crt` does not import `ReleaseDesc`:** CLOSED. The OQ6 resolution no
longer contains a claim about `crt` being a consumer of `ReleaseDesc`.

**N8 — `thread_rng()` → `rand::rng()`:** CLOSED. The `gen_run_name` sketch
at line 815 now uses `rand::rng()`. Note: the redundant `.into_iter()` on the
`choose_multiple` result remains (line 816:
`...choose_multiple(&mut rng, 10).into_iter().collect()`). This is cosmetic
in a sketch, but `choose_multiple` in `rand` 0.9 returns a `Vec<char>` whose
`IntoIterator` yields `char` directly; `.into_iter()` is a no-op call.

**C4 — Rolling-deployment migration note:** CLOSED. The `§ Rollout
considerations` subsection (lines 1288-1312) documents the tag-first-then-
upgrade sequencing, including the pydantic `extra = "ignore"` one-way
compatibility guarantee.

---

## New Findings

### N1 — HOME normalisation SAFETY comment is internally
        contradictory [IMPORTANT]

**Section:** `§ HOME normalisation` (lines 785-805)

The code sketch reads:

```rust
// Top of cbscore::cmds::runner::build, before any subprocess spawn.
const RUNNER_PATH: &str = "/runner";

let needs_fix = std::env::var_os("HOME")
    .map(|s| s.is_empty() || s == "/")
    .unwrap_or(true);
if needs_fix {
    // SAFETY: set_var is unsafe in current Rust because it mutates
    // process-global env without synchronisation. We call it before
    // tokio's runtime starts (i.e. before any other thread exists),
    // which is the only thread-safe window. main.rs uses #[tokio::main]
    // so this normalisation must run *before* the runtime takes over —
    // structure the runner subcommand to do this in a sync prelude.
    unsafe { std::env::set_var("HOME", RUNNER_PATH); }
}
```

The comment's safety claim is "we call it before tokio's runtime starts (i.e.
before any other thread exists)". But the comment also says the call lives at
"the top of `cbscore::cmds::runner::build`" — which is an async fn invoked
from within the running tokio executor. By the time any async fn executes,
`#[tokio::main]` has already constructed and launched the multi-thread runtime
(which spawns N OS threads in its thread pool). The "no other thread exists"
claim is false at that call site.

`std::env::set_var` is `unsafe` in Rust 2024 because concurrent reads of the
environment table from other threads can cause memory unsafety on POSIX
systems (glibc reallocates the table on write; concurrent `getenv(3)` readers
can observe a dangling pointer). Tokio worker threads are unlikely to be
calling `getenv` at that precise instant in this application, but:

1. The SAFETY justification provided is demonstrably false at the described
   call site — a reviewer auditing `unsafe` blocks will reject it.
2. Any background task or lazy-init from a third-party library (e.g., a
   tracing subscriber opening a file, `aws-sdk-s3` reading proxy config,
   `reqwest` reading TLS cert paths) could call `getenv` concurrently.

The design _intends_ the correct behaviour: the normalisation runs in a sync
prelude before threads exist. The code placement description defeats that
intention, and an implementor will follow the placement description (it is
more concrete than the guidance in the comment).

**Why it matters:** The `unsafe` block will compile with the incorrect comment,
will likely pass code review on the first pass, and will silently contain a
false safety invariant. The risk is low in practice but the invariant is load-
bearing — it is one of the few explicit `unsafe` blocks in the codebase and
will be scrutinised.

**Resolution:** Move the normalisation to `main()`, before `#[tokio::main]`
starts the runtime. Proposed sketch for `cbsbuild/src/main.rs`:

```rust
fn main() {
    // Normalise HOME before tokio's thread pool starts.
    // set_var is unsafe (concurrent env mutation). Placing this
    // call here, before #[tokio::main], guarantees no other thread
    // exists yet — the only valid window for set_var.
    //
    // Scope: any in-container invocation where $HOME is absent or is
    // "/". Workstation invocations are unaffected because $HOME is
    // always set to a real path on any sane workstation OS.
    match std::env::var_os("HOME").as_deref() {
        None | Some(s) if s.is_empty() || s == std::ffi::OsStr::new("/") => {
            // SAFETY: no other thread exists; called before the tokio
            // runtime is constructed.
            unsafe { std::env::set_var("HOME", "/runner"); }
        }
        _ => {}
    }
    tokio_main();
}

#[tokio::main]
async fn tokio_main() { /* parse CLI, dispatch */ }
```

The guard logic is identical to the design's existing sketch. The only change
is placement: in `main()` before the runtime is constructed.

If you deliberately want the guard scoped only to `runner build` invocations
(to avoid the guard firing on workstation `cbsbuild versions list` calls), one
option is to check `argv` for the `runner build` subcommand in `main()` before
entering `#[tokio::main]`. This keeps the scope narrow while keeping the call
in a single-threaded context.

Update the sketch in `§ HOME normalisation` to match.

---

## Cross-Document Notes

The same finding applies to the design 001 `## Runner Container` section, which
cross-references `§ HOME normalisation` for implementation details. See the
companion design 001 v3 review. Both documents should be updated in the same
editing pass.

---

## Summary of Action Items

| ID | Severity  | Action                                                                                                                              |
| -- | --------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| N1 | IMPORTANT | Move HOME normalisation to `main()` before `#[tokio::main]`; update sketch and SAFETY comment in `§ HOME normalisation`            |
