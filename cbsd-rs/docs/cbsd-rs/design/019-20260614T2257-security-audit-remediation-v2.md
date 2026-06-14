# Security Audit Remediation — Design Addendum v2 (D13 scope)

| Field  | Value                                                                        |
| ------ | ---------------------------------------------------------------------------- |
| Type   | Design addendum (v2)                                                         |
| Seq    | 019                                                                          |
| Date   | 2026-06-14                                                                   |
| Amends | `019-20260514T1040-security-audit-remediation.md` (D13 only)                 |
| Status | Accepted                                                                     |
| Scope  | Narrows audit-rem **D13** to a server-side + protocol-only change (option A) |

> This is an **addendum**, not a replacement. It documents one scoped change to
> the v8 audit-rem design and does not reproduce that document. Everything in
> the original design stands except the D13 worker-side decisions noted below.

## What changed

D13 ("superseded live same-worker connection receives a stop-work command") is
reduced from a server **and** worker change to a **server-side + protocol-only,
best-effort** change. The worker-side migration handling described in the
original design (the `accepted` / `started` / `revoking` /
`terminal-pending-report` migration behaviours, `migration_plausible()`,
`last_authenticated_connect_at`, the `tokio::time::Instant` clock injection, and
the migration-scoped drain-then-revoke) is **dropped**.

## Why (the premise that did not hold)

The original D13 worker-side handling assumes "the two websocket connections
briefly coexist on the worker side" so the worker can read the
`MigrationSupersede` revoke on the old connection. That premise is false for the
current worker:

- `cbsd-worker`'s `reconnect_loop` runs **exactly one connection at a time**:
  `run_connection` owns the split websocket stream and returns (dropping it)
  **before** the loop opens the next connection.
- A same-worker migration fires on the **server** precisely because the server
  sees two connections (stale-old + new) for one worker id — an asymmetry the
  single-connection worker cannot mirror.
- The server sends `BuildRevoke { reason: MigrationSupersede }` on the **old**
  sender. By then the worker has already abandoned and dropped that socket, so
  it **never reads the revoke**.

Consequently the worker-side machinery would execute only under a
duplicate-process misconfiguration (two worker processes sharing one identity),
never under the same-process reconnect it was designed for. It is also largely
redundant: per the commit-19 finding the supervisor **preserves the subprocess
across reconnects** (the build reconnects as `Building`, not run-to-completion
under a terminal row), and D12's dead-worker liveness path is the safety net.

## D13 as implemented (option A)

- **Protocol (`cbsd-proto`).** `ServerMessage::BuildRevoke` gains
  `reason: Option<BuildRevokeReason>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`, and a new
  `BuildRevokeReason { Admin, MigrationSupersede, UnauthorizedAction }` enum.
  The field is forward/backward wire-compatible (absent → `None`; SI-18 forbids
  `deny_unknown_fields`, so an older peer ignores it). This is retained from the
  original design and used to label revokes (`Admin` for admin/drain paths,
  `UnauthorizedAction` for reporter-directed stray revokes).
- **Server (`cbsd-server`).** During same-worker migration, before removing the
  old connection's sender, the server sends
  `BuildRevoke { reason: MigrationSupersede }` on that old sender for each
  migrated build. This is **best-effort**: it lands only if the old socket is
  still writable, and is a no-op in the common reconnect case. It is
  reporter-directed cleanup only (no DB/queue/timer/log-watcher mutation); the
  new connection is already the authoritative owner and the WCP idle/reconnect
  rules decide the final transition.
- **Worker (`cbsd-worker`).** Ignores `BuildRevoke.reason`; existing revoke
  semantics are unchanged.

## Forward note

If the worker ever becomes multi-connection (so it can read a revoke on a
superseded-but-live socket), revisit the dropped worker-side handling (the
original design's "option B"). Until then it would be unreachable code.
