# cbsd-rs Roadmap

Forward-looking items deferred from current implementation work. Each entry
records the motivation, the affected component, and the trigger that decides
when the item moves from roadmap to a design/plan document.

This file is component-organized; sequence numbering and design/plan authoring
conventions still follow the seq-docs convention when individual items are
picked up for implementation.

## cbsd-rs (server + worker)

### Native TLS termination in `cbsd-server`

- Motivation: `cbsd-server` currently has no native TLS support and relies on an
  upstream TLS-terminating reverse proxy in every deployment. This is acceptable
  for current production topology but couples the security posture to operator
  discipline.
- Origin: security audit finding F6 (review
  `019-20260512T2339-impl-cbsd-rs-security-audit-v1.md`, reclassified as
  informational in the v1.1 follow-up).
- Scope: optional `axum-server` rustls integration with HSTS, an explicit
  configuration toggle, and clear documentation of when each mode is
  appropriate.
- Trigger: when operators want to deploy `cbsd-server` without an external
  reverse proxy, or when a deployment context requires TLS termination inside
  the trust boundary of the server process.

### Migrate `cbscore` from Python to Rust (`cbscore-rs` crate)

- Motivation: the worker currently invokes `cbscore` through a Python subprocess
  (`scripts/cbscore-wrapper.py`). This adds a `python3` dependency on the worker
  host, a `$PATH` resolution surface, and fork-time overhead. A native Rust
  crate consumed by `cbsd-worker` (and potentially other consumers) removes
  those concerns and tightens the type contract between the worker and the build
  engine.
- Origin: cross-cutting; subsumes security audit findings F9 (PATH resolution of
  `python3`) and F12 (dev OAuth bypass acceptable today because `cbscore`
  enforces actual upstream access to S3/Harbor/etc.).
- Scope: new `cbscore-rs` crate in the workspace, consumed in-process by
  `cbsd-worker`; the existing Python `cbscore` and the wrapper script are
  deprecated and eventually removed.
- Trigger: when the Python `cbscore` reaches feature stability for the current
  build pipeline and the team has bandwidth to port the build engine.

### Pre-commit / commit-hook tooling comparison

- Motivation: design 019 (security audit remediation) introduces a
  CI/commit-time grep gate (D10) to keep token material out of `tracing::`
  arguments. The project currently uses Lefthook for some checks. Before
  committing to a single tool for the secret-redaction gate (and any future
  policy checks of similar shape), we want a written comparison of Lefthook vs.
  `pre-commit` (the Python tool) and other alternatives, including evaluation
  criteria such as ergonomics, language ecosystem, runtime dependencies, sharing
  of hook config across contributors, CI parity, and per-file scoping.
- Origin: design 019 D10 follow-up; deferred per maintainer decision until after
  design 019 is implemented.
- Scope: research-only deliverable (short comparison document under
  `cbsd-rs/docs/`), then a follow-up decision design when the comparison is
  reviewed.
- Trigger: after design 019 implementation completes, before introducing the
  next class of commit-time policy gate.

## cbc (CLI client)

(No roadmap items yet.)
