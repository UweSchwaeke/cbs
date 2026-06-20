# 007 — Periodic Short IDs

## Overview

`cbc periodic list` prints each task's UUID truncated to its first 8 hex
characters, but every ID-taking subcommand (`get`, `update`, `delete`, `enable`,
`disable`) requires the full 36-character UUID. A user cannot copy a short ID
from `list` and use it anywhere else.

This design makes the short IDs shown by `list` _minimum-viable unique_ and
makes all ID-taking subcommands accept any unambiguous ID prefix, resolving it
client-side to the full UUID.

It supersedes the ID handling described in design `04 — Periodic Builds`; design
04 remains the record of the original behavior (designs are snapshots in time).

## Problem

Task IDs are server-generated `uuid::Uuid::new_v4()` values (cbsd-server
`routes/periodic.rs`) — standard `8-4-4-4-12` UUIDv4 strings. `cmd_list`
(`cbc/src/periodic.rs`) renders the ID as a fixed 8-char truncation:

```rust
let id_short = if task.id.len() > 8 { &task.id[..8] } else { &task.id };
```

Two consequences:

1. **The short ID is not resolvable.** `cmd_get` passes its argument straight to
   `GET /api/periodic/{id}`, which matches `WHERE id = ?` on the full UUID. Any
   short ID returns 404.
2. **Latent collision bug.** The truncation performs no uniqueness check. If two
   tasks ever shared their first 8 hex characters, `list` would print two
   identical short IDs and neither could be resolved.

## Goals

- `cbc periodic list` displays the _fewest_ leading UUID components that make
  every listed ID unique (the "minimum-viable" short ID).
- `get`, `update`, `delete`, `enable`, and `disable` accept any unambiguous ID
  prefix, resolved client-side to the full UUID.
- No server, database, or wire-format change.

## Non-goals

- No change to the REST API: `GET /api/periodic/{id}` keeps requiring a full
  UUID. Prefix resolution is a client-side convenience only.
- No new runtime dependency (pure string operations; the `uuid` crate is not
  needed).

## Design

### Resolution invariant

Client-side resolution fetches the task list and matches the user's prefix
against it. This is correct only if the list is a superset of what `get` can
return. It is:

- `db::periodic::list_tasks` is
  `SELECT ... FROM periodic_tasks ORDER BY created_at` — no owner filter; it
  returns every task.
- `db::periodic::get_task` is `SELECT ... WHERE id = ?` — no owner filter.
- Both routes gate on the same `periodic:view` capability.

So any task `get` could return is present in `list`. Display and resolution can
therefore use _independent_ granularity and stay consistent by construction:
`list` need only show an unambiguous prefix, and `get` accepts _any_ unambiguous
prefix.

### Minimum-viable short-ID display

`list` computes a single uniform component width across the fetched IDs:

- A UUID is five `-`-separated components (`8-4-4-4-12`).
- Find the fewest leading components (`1..=5`) at which all IDs are distinct.
  One component (8 hex chars) is the floor; the full UUID (five components) is
  the cap and is always unique.
- Render every row truncated to that many components, sizing the ID column to
  the displayed width.

In practice this is always one component (8 hex / 32 bits); the escalation path
only triggers on a first-component collision, which is astronomically unlikely
for the handful of periodic tasks that exist. It exists to make the displayed ID
always resolvable, closing the latent bug.

These display helpers are net-new: `worker list` identifies workers by name, not
by a truncated ID, so there is no existing display logic to reuse — only the
_resolution_ half (below) mirrors `resolve_worker_id`. The width computation
always yields a component count in `1..=5`, so the truncation helper is never
invoked with fewer than one component (its documented `n >= 1` contract).

### Prefix resolution

A helper mirrors the established `resolve_worker_id` (`cbc/src/worker.rs`),
which already solves the identical problem for `cbc worker`. The decision logic
— including the 403 fallback — is factored into a pure function that takes the
fetch _result_, so every branch (fallback, zero/one/many matches) is
unit-testable without an HTTP mock; the async wrapper only performs the fetch:

```rust
/// Resolve a prefix over a list-fetch result. Pure: no I/O.
/// 403 -> treat the input as a full UUID (caller lacks `periodic:view`);
/// otherwise match the prefix against the listed ids.
fn resolve_from_fetch(
    fetched: Result<Vec<PeriodicListItem>, Error>,
    prefix: &str,
) -> Result<String, Error> {
    let tasks = match fetched {
        Ok(list) => list,
        Err(Error::Api { status: 403, .. }) => return Ok(prefix.to_string()),
        Err(e) => return Err(e),
    };
    let matches: Vec<&PeriodicListItem> =
        tasks.iter().filter(|t| t.id.starts_with(prefix)).collect();
    match matches.len() {
        0 => Err(/* no task matching '<prefix>' */),
        1 => Ok(matches[0].id.clone()),
        _ => Err(/* ambiguous '<prefix>' matches: <candidates> */),
    }
}

/// Async wrapper: fetch the list, then resolve.
async fn resolve_periodic_id(
    client: &CbcClient,
    prefix: &str,
) -> Result<String, Error> {
    resolve_from_fetch(client.get("periodic").await, prefix)
}
```

**403 fallback (required).** Like `resolve_worker_id`, resolution falls back to
treating the input as a full UUID when the list fetch returns 403. The
capability set is flat with no implication between caps: `list`/`get` gate on
`periodic:view`, but `update`/`delete`/`enable`/`disable` gate on
`periodic:manage:own`/`:any` (via `can_manage_task`) and never check
`periodic:view`. A `periodic:manage`-only user can therefore mutate tasks by
full UUID today; without the fallback, `resolve_periodic_id` would 403 on the
list and lose mutation by _any_ id — a regression. The fallback is uniform
across all five commands: for `get` it is a harmless no-op (the subsequent
`GET /api/periodic/{id}` would itself 403), and for the four manage commands it
preserves the existing full-UUID path. Short-ID convenience simply requires
`periodic:view` — you cannot resolve a prefix you cannot list.

**Empty prefix.** `"".starts_with(_)` is true for every id, so an empty argument
matches all tasks: it resolves to the sole task when exactly one exists and is
otherwise reported as ambiguous (or "no task" when the list is empty). This
mirrors `resolve_worker_id` and needs no special-casing; clap already requires
the positional argument to be present, so the empty string only arises if passed
explicitly (`cbc periodic get ""`).

**Cost and consistency.** Each id-taking command now performs one extra
`GET /api/periodic` to resolve. For the handful of periodic tasks that exist
this is negligible. There is a benign time-of-check/time-of-use window — a task
could be created or deleted between the resolve fetch and the actual request —
but the resolved value is a full UUID handed straight to the server, which
performs its own authoritative existence and permission checks; a stale
resolution simply surfaces as the server's 404/403, never as acting on the wrong
task.

### Scope

All five ID-taking subcommands resolve first, then use the resolved full UUID
for every request, fetch, and message:

| Command   | Resolve before                                 |
| --------- | ---------------------------------------------- |
| `get`     | `GET /api/periodic/{id}`                       |
| `update`  | existing-task fetch + `PUT /api/periodic/{id}` |
| `enable`  | `PUT /api/periodic/{id}/enable`                |
| `disable` | `PUT /api/periodic/{id}/disable`               |
| `delete`  | `DELETE /api/periodic/{id}`                    |

`delete` additionally echoes the resolved full UUID before the destructive call
(mirroring `worker deregister` surfacing its resolved target), so a prefix that
uniquely matches the _wrong_ task is visible rather than silently deleted.

## Error handling

- No match → `error: no periodic task matching '<prefix>'`.
- Ambiguous prefix → `error: ambiguous task id '<prefix>' matches:` followed by
  a short list of candidate IDs and their cron expressions.
- A full UUID that does not exist resolves to the no-match error instead of a
  server 404, which is clearer.
- A `periodic:manage`-only caller (no `periodic:view`) cannot list, so the input
  is passed through verbatim as a full UUID (the 403 fallback). Short prefixes
  are unavailable to them; a non-matching full UUID surfaces the server's
  404/error.

## Testing

- Unit tests (pure functions, no I/O) for the min-unique computation and
  component truncation: all-distinct first groups, a forced first-group
  collision, a single ID, and boundary `n` values.
- `resolve_from_fetch` is pure, so all of its branches are unit-tested directly
  from constructed values — no HTTP mock needed:
  - matching: zero matches → error; one → that task; many → ambiguity error; a
    full-UUID prefix → that single task.
  - 403 fallback: `Err(Error::Api { status: 403, .. })` → the input returned
    verbatim (this is the branch that prevents the manage-without-view
    regression).
  - other errors: `Err(_)` propagated unchanged.
  - empty prefix: matches all → resolves when exactly one task exists, ambiguous
    otherwise.
- Manual end-to-end against the dev stack: create ≥2 tasks; confirm `list` short
  IDs; `get` / `enable` / `disable` / `update` / `delete` by prefix; a bogus
  prefix errors; and a `periodic:manage`-only token can still act by full UUID
  while `list` is forbidden.

## Alternatives considered

- **Server-side prefix resolution.** Rejected: it would change the REST contract
  and affect the web UI and any other client for a convenience that belongs in
  the CLI. The validated superset invariant makes client-side resolution correct
  without touching the server.
- **Character-granularity display** (git-style minimum prefix length).
  Equivalent in practice — both show 8 chars unless a first-component collision
  occurs. Component granularity matches how the IDs are spoken about and
  self-floors at a whole 8-char group; `get` already accepts any character
  prefix, so the two remain consistent.
