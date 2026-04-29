# Design Review v4: cbscore Rust Port — Project Structure & Crate Layout

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/001-20260418T2045-cbscore-project-structure.md`

**Prior reviews:** `001-20260420T1132-design-cbscore-project-structure-v1.md`,
`001-20260427T1330-design-cbscore-project-structure-v2.md`,
`001-20260428T1401-design-cbscore-project-structure-v3.md`

**Commits reviewed since v3:** `6064814` (LoC refresh + `report.rs` layout +
`BuildArtifactReport` rename), `970fb3a` (cbscommon lift-out invariants)

---

## Summary

**Verdict: approve with one important correction.**

The v3 IMPORTANT finding (HOME normalisation SAFETY comment) is closed cleanly —
the design now passes `-e HOME=/runner` on the `podman run` command line, which
requires no `unsafe` at all and is strictly superior to the `set_var` approach.
The LoC refresh, `report.rs` additions, and `BuildArtifactReport` rename are all
internally consistent. The new lift-out invariants section is well-reasoned and
the three constraints are correct — but the post-merge migration recipe contains
one imprecise step that will confuse the implementor who executes it.

---

## v3 Finding Verification

**F1 (HOME normalisation SAFETY comment):** CLOSED. The `§ Runner Container`
bullet no longer mentions `set_var` or a sync prelude at all. HOME normalisation
now reads: "preserved by passing `-e HOME=/runner` to `podman run` from the host
runner — the flag overrides whatever the image or host process exports". This is
strictly better than the previous in-process approach: no `unsafe`, no
thread-synchronisation constraint, and it handles all the enumerated edge cases
(`--user`-altered HOME, image without HOME, rootless podman with weird UID maps)
without any in-container code.

---

## New Findings

### F1 — Migration recipe step 3 will mislead the implementor

[IMPORTANT]

**Section:** `§ Crate Responsibilities → cbscore`,
`#### Lift-out invariants for utils::git and utils::subprocess`, migration
recipe step 3.

The recipe reads:

> 3\. Add `cbscommon-rs` to the workspace `Cargo.toml` and to cbscore's
> `[dependencies]`. Move the cargo deps listed above out of cbscore's
> `Cargo.toml`.

"The cargo deps listed above" refers to the allowlist from the preceding
constraint bullet: `tokio`, `tracing`, `thiserror`, `regex`, `camino`, and
`which`.

The problem: `tokio`, `tracing`, `thiserror`, and `camino` are used throughout
`cbscore` beyond `utils::git` and `utils::subprocess`. An implementor following
the recipe literally would remove them from `cbscore/Cargo.toml`, breaking every
other module in the library that depends on them. Only `regex` (used exclusively
by `_sanitize_cmd`) and `which` (used exclusively for binary discovery in the
subprocess module) are plausible candidates for removal from cbscore's own
`Cargo.toml` after the lift-out.

**Why it matters:** The recipe is intended to be executed mechanically
("near-mechanical move rather than a refactor"). A step with ambiguous scope
produces a broken workspace when followed literally.

**Resolution:** Replace step 3 with:

> 3\. Add `cbscommon-rs` to the workspace root `Cargo.toml` and as a dependency
> in cbscore's `[dependencies]`. Add the full allowlist deps (`tokio`,
> `tracing`, `thiserror`, `regex`, `camino`, `which`) to
> `cbscommon-rs/Cargo.toml`. Remove from `cbscore/Cargo.toml` only those
> allowlist deps that are no longer used by any other module in cbscore — in
> practice this is likely `regex` and `which` only. `tokio`, `tracing`,
> `thiserror`, and `camino` remain in `cbscore/Cargo.toml` because the rest of
> the library depends on them.

---

## Strengths

- The `-e HOME=/runner` solution for HOME normalisation is the right call —
  cleaner than `set_var`, no `unsafe`, handles all the edge cases the comment
  enumerated.
- The three lift-out constraints (no cbscore-internal types, own tracing
  targets, dep allowlist) are correctly scoped and sufficient to guarantee the
  future lift-out is mechanical, not a refactor. The constraint on tracing
  targets is particularly good: tracing target strings are the one thing that
  would otherwise silently survive a `git mv` and produce subtly wrong log
  output in `cbscommon-rs`.
- The re-evaluation trigger boundary ("before M0 begins") is the right call.
  Once crates exist in the workspace, switching from the lift-out invariant
  strategy to a cbscommon-rs-from-day-one approach requires undoing work that
  has already landed commits. The trigger correctly sets the boundary at the
  last moment where switching is cost-free.
- The `BuildArtifactReport` split between `cbscore-types/src/builder/ report.rs`
  (serde structs) and `cbscore/src/builder/report.rs` (assembly logic) is
  coherent with the type/IO boundary the crate split enforces throughout. The
  types crate holds the wire shape; the library crate holds the
  stage-output-to-report construction. `schema_version` versioning for
  `BuildArtifactReport` is handled in design 002 `§ Wire-Format Versioning` as
  called out in the `§ Source Package` note — the cross-reference is correct.

---

## Summary of Action Items

| ID  | Severity  | Action                                                                                                                                     |
| --- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| F1  | IMPORTANT | Clarify migration recipe step 3: add allowlist deps to cbscommon-rs, but remove from cbscore only those not used elsewhere (regex, which). |
