# Design Review v5: cbscore Rust Port — Architecture & Subsystem Design

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`

**Prior reviews:** `002-20260420T1132-design-cbscore-rust-port-design-v1.md`,
`002-20260420T1512-design-cbscore-rust-port-design-v2.md`,
`002-20260427T1330-design-cbscore-rust-port-design-v3.md`,
`002-20260428T1401-design-cbscore-rust-port-design-v4.md`

**Commits reviewed since v4:** `6064814` (`BuildArtifactReport` rename in
`run_build()` sketch), `970fb3a` (lift-out notes in `§ Capability Mapping`)

---

## Summary

**Verdict: approve with one minor correction.**

The v4 IMPORTANT finding (HOME normalisation SAFETY comment) is closed cleanly —
`§ Container entry point` now passes `-e HOME=/runner` on the `podman run`
command line with no `unsafe set_var` anywhere. The `BuildArtifactReport` rename
is consistent across all occurrences; no orphan `BuildReport` remains. The
lift-out notes in `§ Capability Mapping` are correctly positioned and point at
design 001. One minor cross-document inconsistency in the `reqwest` version pin
deserves a one-line fix.

---

## v4 Finding Verification

**N1 (HOME normalisation SAFETY comment):** CLOSED. The
`§ Container entry point` section now contains `.arg("-e").arg("HOME=/runner")`
on the `podman_run()` call, with an explanatory paragraph confirming that
`-e HOME=/runner` on the podman command line handles the same edge cases the
previous `set_var` sketch was trying to handle. No `unsafe` block, no SAFETY
comment, no thread-synchronisation constraint. The fix is better than both
proposed options in the v4 review.

---

## New Findings

### N1 — `reqwest` version pin differs between design 001 and 002

[NICE-TO-HAVE]

**Sections:** design 001 `§ Crate Dependencies / cbscore` Cargo sketch
(`reqwest = { version = "0.12", ... }`); design 002 `§ Capability Mapping` table
row ("`reqwest` 0.13").

Design 001's Cargo sketch pins
`reqwest = { version = "0.12", features = ["rustls-tls", "json"] }`. Design
002's capability table lists `reqwest 0.13`. Both documents refer to the same
dependency in the same `cbscore` crate.

This is not load-bearing — the Cargo sketches in both designs are explicitly
provisional ("exact versions pinned when the workspace is created") — but a
stale version number in the capability table is exactly the kind of drift that
accumulates silently and causes a reviewer at M0 to chase a phantom discrepancy.

**Resolution:** Pick one version as the recorded intent and update the other
reference to match. `reqwest 0.13` was released after 0.12 and is the current
stable release; updating the design 001 Cargo sketch to
`reqwest = { version = "0.13", ... }` is the minimal fix. Alternatively, align
both to whichever version is current at implementation time and add a note that
both sketches use provisional pins.

---

## Cross-Document Consistency

**`BuildArtifactReport` — all occurrences consistent.** Seven occurrences in
design 002: versioned files table (`build-report.json` → `BuildArtifactReport`),
the `report_version` rename note, the `VersionedX` implementation list, the
`build-report.json` key name change bullet, the descriptor snake_case bullet,
the `run_build()` sketch return type (two occurrences). No orphan `BuildReport`
remains. The rename is complete and internally consistent.

**Lift-out notes in `§ Capability Mapping`:** The two added notes ("lift- out
candidate to a future `cbscommon-rs` (see design 001 § Lift-out invariants)") in
the Subprocess + redaction and git rows are correctly positioned and
cross-reference the right section. They add useful forward pointers without
duplicating the invariant detail.

**HOME normalisation:** The `§ Container entry point` sketch and the surrounding
explanation are now aligned with design 001 `§ Runner Container`. Both documents
describe the same `-e HOME=/runner` solution. No inconsistency.

---

## Summary of Action Items

| ID  | Severity     | Action                                                                                                       |
| --- | ------------ | ------------------------------------------------------------------------------------------------------------ |
| N1  | NICE-TO-HAVE | Align `reqwest` version: design 001 Cargo sketch says 0.12, design 002 capability table says 0.13. Pick one. |
