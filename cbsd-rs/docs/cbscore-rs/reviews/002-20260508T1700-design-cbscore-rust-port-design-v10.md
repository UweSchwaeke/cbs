# Design Review v10: cbscore Rust Port — Architecture & Subsystem Design

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`

**Prior reviews:** `002-20260420T1132-design-cbscore-rust-port-design-v1.md`,
`002-20260420T1512-design-cbscore-rust-port-design-v2.md`,
`002-20260427T1330-design-cbscore-rust-port-design-v3.md`,
`002-20260428T1401-design-cbscore-rust-port-design-v4.md`,
`002-20260429T0929-design-cbscore-rust-port-design-v5.md`,
`002-20260429T1633-design-cbscore-rust-port-design-v6.md`,
`002-20260430T1208-design-cbscore-rust-port-design-v7.md`,
`002-20260506T1000-design-cbscore-rust-port-design-v8.md`,
`002-20260506T1400-design-cbscore-rust-port-design-v9.md`

**Changes since v9:** None. Design 002 is unchanged on disk since the v9 review.

---

## Summary Assessment

**Verdict: approve, no changes needed.**

Design 002 is unchanged since v9. All prior findings remain closed. This pass
probed two angles not emphasised in earlier reviews: the `schema_version`
integer-tag serde dispatch caveat, and the `async_run_cmd` RAII-guard drop
contract. Both check out. No regression, no new coherence gap.

---

## Fresh Probe A: `schema_version` integer-tag serde dispatch

The design acknowledges that serde's `#[serde(tag = "schema_version")]` with
integer values may require a hand-rolled `Deserialize` implementation ("a
hand-rolled `Deserialize` may be needed if serde's default string-matching for
internal tags does not accept integer tags directly"), then brackets it as an
implementation detail. This is the right call: the design correctly identifies
the risk, defers the exact mechanism, and states the goal (`schema_version: 1`
on disk, not `"schema_version": "1"`). An implementer reading this has enough to
act; they will encounter the serde limitation at the time they write M0 and can
choose between `#[serde(rename = "1")]` + string-coercion or a custom visitor.
The design does not over-commit to a specific workaround that might not compile.
No issue.

## Fresh Probe B: RAII-guard drop contract in `async_run_cmd`

The design states: "Child::start_kill() runs in the Drop impl of an internal
RAII guard, so the child is killed even if the future is not polled to
completion. Reaping happens in the guard's Drop, best-effort."

The "best-effort" qualifier on reaping is correct and honest: calling an
`.await` inside `Drop` is not possible in stable Rust. The design is not
claiming guaranteed reap — only kill. The child process may linger as a zombie
until the parent's next `waitpid` sweep or until the process itself exits. For a
build process running inside a podman container (PID 1 reaping is handled by
podman's init shim), a brief zombie period is operationally acceptable. The
design's position is sound. No issue.

---

## Summary of Action Items

None.
