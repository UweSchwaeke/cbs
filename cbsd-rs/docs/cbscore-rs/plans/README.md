# cbscore-rs Implementation Plans

## Overview

Phased implementation plan for the Rust rewrite of `cbscore/`. The new member
crates land inside the existing `cbsd-rs/` Cargo workspace — there is no
separate `cbscore-rs/` workspace.

**Design documents:** `cbsd-rs/docs/cbscore-rs/design/`

## Implementation Status

| Phase                                                         | Description                                                                                                                    | Commits | Status |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | ------- | ------ |
| [Phase 1](002-20260508T1558-01-types.md)                      | M0 — workspace scaffold + `cbscore-types` (zero-IO descriptor / config / error surface)                                        | 5       | Done   |
| [Phase 2](002-20260508T1558-02-subprocess-and-shell-tools.md) | M1.1 — subprocess + secret redaction + podman/buildah/skopeo + git wrappers                                                    | 5       | Done   |
| [Phase 3](002-20260508T1558-03-storage-and-secrets.md)        | M1.2 — S3, Vault, secrets manager, config IO                                                                                   | 4       | Done   |
| [Phase 4](002-20260508T1558-04-runner.md)                     | M1.3 — runner subsystem (state machine, mount layout, podman invocation, signal handling)                                      | 3       | Done   |
| [Phase 5](002-20260508T1558-05-builder-and-releases.md)       | M1.4 — builder pipeline stages + `run_build` orchestrator + releases + containers + image sign/sync + `core::component` loader | 7       | Done   |
| [Phase 6](002-20260508T1558-06-cbsbuild-cli.md)               | M1.5 — `cbsbuild` clap CLI + logging + exit codes + visibility audit + end-to-end Rust-only smoke build (M1 cut gate)          | 6       | Done   |
| [Phase 7](002-20260508T1558-07-worker-cutover.md)             | M2 — `cbsd-worker` switches from `cbscore-wrapper.py` to direct Cargo dep on `cbscore`                                         | 4       | Done   |

**Total estimate:** ~27–33 commits across 7 phases.

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
  (`Config.paths.versions` + `--versions-dir`). Originally drafted to interleave
  into M1; that interleave slipped (Phase 6 landed without it, Phase 7
  followed). seq-004 landed on top of the M2 release as a backwards-compatible
  additive change — existing operator configs keep working unchanged via the
  `<git-root>/_versions` fallback. Plan:
  [`004-20260513T0900-configurable-version-descriptor-location.md`](004-20260513T0900-configurable-version-descriptor-location.md)
  (3 commits; `Status: Done`).
- **seq-005** — optional positional VERSION on `cbsbuild versions create`
  (UUIDv7 default when omitted; supplied VERSION gets Python-shape regex
  validation via the new `validate_version`). Landed post-M2 as a
  backwards-compatible additive change on top of the 1.0.0 baseline. Plan:
  [`005-20260521T1300-optional-version-on-versions-create.md`](005-20260521T1300-optional-version-on-versions-create.md)
  (3 commits; `Status: Done`).

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
