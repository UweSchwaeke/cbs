# cbscore-rs Implementation Plans

## Overview

Phased implementation plan for the Rust rewrite of `cbscore/`. The new member
crates land inside the existing `cbsd-rs/` Cargo workspace — there is no
separate `cbscore-rs/` workspace.

**Design documents:** `cbsd-rs/docs/cbscore-rs/design/`

## Implementation Status

| Phase                                                         | Description                                                                               | Commits | Status  |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ------- | ------- |
| [Phase 1](002-20260508T1558-01-types.md)                      | M0 — workspace scaffold + `cbscore-types` (zero-IO descriptor / config / error surface)   | 4–5     | Pending |
| [Phase 2](002-20260508T1558-02-subprocess-and-shell-tools.md) | M1.1 — subprocess + secret redaction + podman/buildah/skopeo + git wrappers               | 4–5     | Pending |
| [Phase 3](002-20260508T1558-03-storage-and-secrets.md)        | M1.2 — S3, Vault, config IO, secrets manager                                              | 3–4     | Pending |
| [Phase 4](002-20260508T1558-04-runner.md)                     | M1.3 — runner subsystem (state machine, mount layout, podman invocation, signal handling) | 2–3     | Pending |
| [Phase 5](002-20260508T1558-05-builder-and-releases.md)       | M1.4 — builder pipeline stages + `run_build` orchestrator + releases + image sign/sync    | 5–6     | Pending |
| [Phase 6](002-20260508T1558-06-cbsbuild-cli.md)               | M1.5 — `cbsbuild` clap CLI + logging + exit codes + end-to-end Ceph build acceptance      | 4–5     | Pending |
| [Phase 7](002-20260508T1558-07-worker-cutover.md)             | M2 — `cbsd-worker` switches from `cbscore-wrapper.py` to direct Cargo dep on `cbscore`    | 2–3     | Pending |

**Total estimate:** ~25–30 commits across 7 phases.

## Dependency Graph

```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7
                                              (M1 cut)    (M2 cut)
```

Strict linear ordering. Each phase's outputs are inputs to the next. Design 002
§Migration Strategy spells out the M0 / M1 / M2 milestone cuts and the
M1-internal subsystem order
(`subprocess → podman/buildah/skopeo → git → s3 → vault → secrets → config IO → runner → builder stages → releases → images`);
the phase breakdown above mirrors that order.

## Related plans

The following are tracked under their own design seq, not folded into this plan:

- **seq-001** — workspace scaffold (cbscore-types/, cbscore/, cbsbuild/
  Cargo.toml + bare `lib.rs` / `main.rs`). Folded into Phase 1 of this plan as
  Commit 1 for blast-radius reasons; design 001 remains the authoritative source
  for crate boundaries and dependency lists.
- **seq-003** — interactive `cbsbuild config init` (post-M1, lands as a 1.x.0
  minor add after Phase 6).
- **seq-004** — configurable `VersionDescriptor` location
  (`Config.paths.versions` + `--versions-dir`). M1-scope; lands alongside or
  interleaved with this plan's Phase 6. Tracked under its own seq.
- **seq-005** — optional positional VERSION on `cbsbuild versions create`
  (post-M1, lands as a 1.x.0 minor add).

## Deferred (post-M2)

- M3 — Python consumer migration or retirement (`cbc`, `crt`, `cbsdcore`,
  `cbsd`). Each is its own future effort; out of scope for this plan.

## Conventions

- **Commit style:** Ceph project conventions
- **Sign-off:** `-s` flag, no GPG signing (`-c commit.gpgsign=false`)
- **Co-Authored-By trailer:** required on autonomous commits per
  `cbsd-rs/docs/cbscore-rs/CLAUDE.md`; never stack multiples
- **Each commit must compile** (`cargo build --workspace`) and pass tests
  (`cargo test --workspace`)
- **Pre-commit checks** (in order): `cargo fmt --all`,
  `cargo clippy --workspace`, `cargo check --workspace`. All must pass with zero
  errors and zero warnings before staging.
- **Update the progress table** in this README and the affected phase file after
  each commit lands.
