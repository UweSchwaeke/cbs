# Design Review v3: cbscore Rust Port — Project Structure & Crate Layout

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/001-20260418T2045-cbscore-project-structure.md`

**Prior reviews:**
`001-20260420T1132-design-cbscore-project-structure-v1.md`,
`001-20260427T1330-design-cbscore-project-structure-v2.md`

---

## Summary

**Verdict: approve with one important correction.**

All three v2 findings (F1 Python Coexistence rewrite, F2 Python-removal
phrasing, F3 missing Cargo deps) are confirmed closed. The versioning
wording improvement (F4) is also closed. One new issue requires attention
before implementation: the `§ HOME normalisation` description contains a
self-contradictory safety justification that will mislead the implementor
about when and how to call `std::env::set_var` safely.

---

## v2 Finding Verification

**F1 — Python Coexistence item 1:** CLOSED. `## Python Coexistence` item 1 now
reads "No cross-language file interchange" and correctly states that
cross-language byte-equality is not a requirement, matching design 002 and
`CLAUDE.md` Correctness Invariant 1.

**F2 — Runner Container Python removal phrasing:** CLOSED. The "Binary mount
instead of source mount" bullet now reads "This removes `uv`, the cbscore
Python wheel, and the venv from the build image. System `python3` stays —
Ceph's `do_cmake.sh` and several `python3-mgr-*` RPMs require it", with a
cross-reference to design 002 OQ7 for the full rationale.

**F3 — Missing `camino`, `camino-tempfile`, `regex` in Cargo sketches:**
CLOSED. All three now appear in the `cbscore` Cargo sketch.
`camino = { version = "1", features = ["serde1"] }` appears in the
`cbscore-types` sketch. The `cbscore-types` description mentions
`Utf8PathBuf` path fields.

**F4 — Versioning major-bump trigger wording:** CLOSED. The Major row now
mentions `schema_version` and includes the no-cross-language caveat in a
parenthetical.

---

## New Findings

### F1 — HOME normalisation SAFETY comment contradicts itself [IMPORTANT]

**Section:** `## Runner Container`, bullet "No shell entrypoint wrapper"
(lines 475-477)

The bullet cross-references design 002 `§ HOME normalisation` for the
implementation details. That section (design 002 lines 786-800) contains a
code sketch with this SAFETY comment:

```rust
// SAFETY: set_var is unsafe in current Rust because it mutates
// process-global env without synchronisation. We call it before
// tokio's runtime starts (i.e. before any other thread exists),
// which is the only thread-safe window. main.rs uses #[tokio::main]
// so this normalisation must run *before* the runtime takes over —
// structure the runner subcommand to do this in a sync prelude.
unsafe { std::env::set_var("HOME", RUNNER_PATH); }
```

The comment contains two claims that cannot both be true:

1. "We call it before tokio's runtime starts (i.e. before any other thread
   exists)."
2. "main.rs uses `#[tokio::main]` so this normalisation must run _before_ the
   runtime takes over — structure the runner subcommand to do this in a sync
   prelude."

Claim 1 is the safety justification. Claim 2 is an implementation instruction.
The problem: if the code is placed at the top of `cbscore::cmds::runner::build`
(as the sketch shows: `// Top of cbscore::cmds::runner::build`), then by the
time that function runs, `#[tokio::main]` has already launched the tokio
multi-thread runtime — which spawns OS threads immediately. Claim 1 is false
when the code is in the location claim 2 points at.

`std::env::set_var` is `unsafe` in Rust 2024 precisely because concurrent
`getenv(3)` calls from other threads can race on the environment table on POSIX
systems (glibc allocates a new table entry on write while readers may be
traversing the old one). Tokio worker threads themselves are unlikely to be
calling `getenv` at that instant, but the safety guarantee "no other thread
exists" cannot be made from inside an async fn running on a tokio executor —
and more importantly, a future implementor may add a background task or a lazy
initialisation call to a library that does read env, breaking the assumption
silently.

The design _intends_ the right thing: do the normalisation in a sync context
before any threads exist. The code placement description defeats the intention.

**Why it matters:** An implementor reading only the code sketch (which places
the call inside an async fn body) will do exactly that and reason the `unsafe`
is justified by the comment. The `unsafe` block compiles; there is no compiler
warning. The race is latent and will likely never trigger in practice, but the
SAFETY comment is a false claim and will not survive a careful `unsafe` audit.

**Resolution:** The implementation guidance in the SAFETY comment already
points at the correct fix. The sketch just needs to be moved to match it.
Two options:

_Option A (preferred):_ Move the normalisation to `main()`, before
`#[tokio::main]` takes over. Structure `main.rs` as:

```rust
fn main() {
    // HOME normalisation: must happen before tokio's thread pool starts.
    // set_var is unsafe due to concurrent-env races; this is safe only
    // because no other thread exists yet.
    if matches!(
        std::env::var_os("HOME").as_deref(),
        None | Some(s) if s.is_empty() || s == std::ffi::OsStr::new("/")
    ) {
        unsafe { std::env::set_var("HOME", "/runner"); }
    }

    // Now enter the async runtime.
    tokio_main();
}

#[tokio::main]
async fn tokio_main() {
    // parse CLI, dispatch ...
}
```

The guard (`None | Some("/") | Some("")`) is functionally identical to the
sketch. The key change is placement: `main()` runs before tokio, so the
SAFETY claim "before any other thread exists" is true.

_Option B (acceptable):_ If the guard must stay in the runner subcommand
(e.g., to avoid normalising on workstation invocations that don't use the
runner), document it honestly: "The `SAFETY` guarantee here is _probabilistic_,
not absolute: tokio worker threads exist but do not call `getenv` in this
window. Accept the risk and annotate accordingly." Then treat this as a known
deviation from the strict `unsafe` rules, and record it as a Correctness
Invariant.

Update the design text in the `## Runner Container` bullet and in
design 002 `§ HOME normalisation` to match whichever option is chosen.

---

## Cross-Document Notes

The HOME normalisation issue exists in both design 001 (cross-reference in
`## Runner Container`) and design 002 (`§ HOME normalisation`). The
companion design 002 v4 review carries the same finding. Fix both in the same
pass.

---

## Suggested Follow-ups

1. Update the `§ HOME normalisation` code sketch placement (design 002) and
   update the cross-reference in design 001 `## Runner Container` per F1.
2. No other changes needed.
