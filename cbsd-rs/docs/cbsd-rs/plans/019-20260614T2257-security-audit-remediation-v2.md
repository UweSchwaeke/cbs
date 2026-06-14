# Security Audit Remediation — Plan Addendum v2 (Commit 21 scope)

| Field  | Value                                                              |
| ------ | ------------------------------------------------------------------ |
| Type   | Plan addendum (v2)                                                 |
| Seq    | 019                                                                |
| Date   | 2026-06-14                                                         |
| Amends | `019-20260516T1033-security-audit-remediation.md` (Commit 21 only) |
| Design | `019-20260614T2257-security-audit-remediation-v2.md` (D13 scope)   |
| Status | Accepted                                                           |

> This is an **addendum**, not a replacement. It rescopes Commit 21 and does not
> reproduce the plan. Commits 1–20 and all other plan content stand.

## Commit 21 — rescoped to option A (server + protocol only)

Per design addendum v2, the single-connection `cbsd-worker` cannot receive a
migration-supersede revoke (it is sent on the old, already-dropped socket), so
the worker-side D13 work is dropped. Commit 21 now delivers only the reachable,
best-effort part:

**Subject:** `cbsd-rs: keep dispatched ...` → unchanged intent, narrower body —
the server sends a best-effort `BuildRevoke { MigrationSupersede }` on a
superseded connection before removing its sender; the `BuildRevoke.reason`
protocol field is added (and used to label `Admin` / `UnauthorizedAction`
revokes).

**Implemented:**

- `cbsd-proto`: `BuildRevokeReason` enum + `BuildRevoke.reason: Option<…>`
  (`#[serde(default, skip_serializing_if = "Option::is_none")]`); repair of all
  `BuildRevoke` construct/destructure sites; 4 serde-compatibility tests.
- `cbsd-server`: `send_migration_supersede_revokes` helper + its wiring in the
  same-worker migration path (before old-sender removal); 2 tests.
- `cbsd-worker`: `BuildRevoke` destructure ignores `reason` (no behaviour
  change).

**Dropped** (see design v2 for the rationale): `migration_plausible()`,
`last_authenticated_connect_at`, `tokio::time::Instant` clock injection,
migration-scoped drain-then-revoke, and the 7 same-worker migration / timing
boundary tests that depended on them.

## Sizing exception withdrawn

The original plan's §"Open questions" accepted Commit 21 as a **~700 LOC**
single-commit exception ("splitting creates non-compiling intermediates"). With
the worker side dropped, Commit 21 is **~250 LOC** — comfortably within the
400–800 budget. **The size exception no longer applies** and is withdrawn; it
remains a single, coherent commit on its own merits (a wire-field addition plus
its sole server producer, repaired atomically).

## Unchanged

Commit 18's `cbsd-proto` SI-18 regression test remains the protective gate for
the `BuildRevoke.reason` addition (no `deny_unknown_fields`).
