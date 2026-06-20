# Plan 007: Periodic Short IDs

**Design document:** `docs/cbc/design/007-20260619T2132-periodic-short-ids.md`

## Progress

| #   | Commit                                           | ~LOC | Status |
| --- | ------------------------------------------------ | ---- | ------ |
| 1   | `cbc: minimum-viable short ids in periodic list` | ~70  | TODO   |
| 2   | `cbc: accept periodic task id prefixes`          | ~110 | TODO   |

## Commit boundaries

Two commits, split by capability (not by layer):

1. **Display** — `list` shows resolvable, minimum-viable short IDs. Pure,
   unit-tested helpers; fixes the duplicate-short-ID latent bug. Delivers a
   visible improvement on its own and compiles/tests independently.
2. **Resolution** — every ID-taking subcommand accepts a prefix. Depends on
   nothing from commit 1 (the helpers differ), but lands second because the
   display change is what makes the prefixes worth copying.

Both are well under the 400-LOC floor, but each is a coherent, independently
revertable, independently testable change with no dead code — splitting matches
the two distinct capabilities and keeps blast radius minimal. They are not
merged because neither needs the other to compile or be useful.

All changes are confined to one file: `cbsd-rs/cbc/src/periodic.rs`. No server,
DB, sqlx, or `Cargo.toml` change.

---

## Commit 1: `cbc: minimum-viable short ids in periodic list`

Replace the fixed `&task.id[..8]` truncation in `cmd_list` with a
collision-aware, uniform component width computed over the fetched list.

### Files

```
cbsd-rs/cbc/src/periodic.rs
```

### Content

Two pure helpers:

```rust
/// Byte-slice the first `n` dash-separated UUID groups (n >= 1).
/// n == 1 -> first 8 hex chars; fewer than n groups -> the whole id.
fn truncate_components(id: &str, n: usize) -> &str {
    match id.match_indices('-').nth(n - 1) {
        Some((idx, _)) => &id[..idx],
        None => id,
    }
}

/// Fewest leading UUID groups (1..=5) that make every id distinct.
fn min_unique_components(ids: &[&str]) -> usize {
    for n in 1..=5 {
        let mut seen = std::collections::HashSet::new();
        if ids.iter().all(|id| seen.insert(truncate_components(id, n))) {
            return n;
        }
    }
    5
}
```

`cmd_list` computes `n` once over all IDs, renders each row with
`truncate_components(&task.id, n)`, and derives the ID column width from the
displayed length — applying that width to **both** the header and the data rows
(replacing the hard-coded `{:<10}`) so the table stays aligned when `n > 1`.

### Tests

New `#[cfg(test)] mod tests` (the file currently has none):

- `min_unique_components`: distinct first groups → 1; a forced first-group
  collision → 2; single ID → 1; empty slice handled.
- `truncate_components`: `n == 1` yields 8 chars; `n` larger than the group
  count yields the whole ID. (Callers only ever pass `n >= 1`, which
  `min_unique_components` guarantees.)

### Verification

```bash
cargo fmt --all
cargo clippy --workspace
cargo check --workspace
cargo test -p cbc
```

---

## Commit 2: `cbc: accept periodic task id prefixes`

Add `resolve_periodic_id` and call it from every ID-taking subcommand.

### Files

```
cbsd-rs/cbc/src/periodic.rs
```

### Content

`resolve_periodic_id(client, prefix)` mirrors `resolve_worker_id` in
`worker.rs`: fetch `GET /api/periodic`; on a 403 (no `periodic:view`) fall back
to returning `prefix` verbatim (treated as a full UUID); otherwise filter by
`id.starts_with(prefix)` and return the full UUID for exactly one match,
erroring on zero or multiple. The 403 fallback is **required**, not optional:
`update`, `delete`, `enable`, and `disable` gate on `periodic:manage:*` (not
`periodic:view`), so a manage-only caller must keep its existing ability to act
by full UUID even though it cannot list. See design 007 ("403 fallback").

All decision logic — the 403 fallback **and** the zero/one/many matching — is
factored into a pure helper that takes the fetch _result_,
`resolve_from_fetch(Result<Vec<PeriodicListItem>, Error>, prefix) -> Result<String, Error>`,
so every branch (including the 403-fallback branch that prevents the
manage-without-view regression) is unit-testable from constructed values with no
HTTP mock. `resolve_periodic_id` is then a thin async wrapper:
`resolve_from_fetch(client.get("periodic").await, prefix)`.

Wire into each handler — resolve once at the top, then use the resolved full
UUID at **every** site that currently reads `args.id`:

- `cmd_get` — the `periodic/{id}` GET (periodic.rs:398).
- `cmd_update` — the existing-task fetch (periodic.rs:586), the `PUT`
  (periodic.rs:721), and the success message (periodic.rs:726). Resolve
  **after** the existing `has_field` guard (periodic.rs:533), so a no-field
  update still fails fast without a wasted list fetch.
- `cmd_delete` — the `DELETE` (periodic.rs:747) and the success message
  (periodic.rs:748, which currently prints the prefix); echo the resolved full
  UUID before the destructive call.
- `cmd_enable` — the enable `PUT` (periodic.rs:765) and message
  (periodic.rs:767).
- `cmd_disable` — the disable `PUT` (periodic.rs:784) and message
  (periodic.rs:786).

Resolution runs after `cmd_delete`'s `--yes-i-really-mean-it` gate, so it does
not change the confirmation flow.

### Tests

- Unit-test the pure `resolve_from_fetch` helper across **all** branches from
  constructed values (no HTTP mock — `Error::Api` is an in-crate variant with
  public bare fields, so
  `Err(Error::Api { status: 403, message: String::new() })` is constructible
  directly):
  - matching: `Ok(vec![...])` with no match → error; one → that task; multiple →
    ambiguity error; a full-UUID prefix → that single task.
  - 403 fallback: `Err(Error::Api { status: 403, .. })` → input returned
    verbatim. This is the branch that prevents the manage-without-view
    regression; it is pure logic, not untestable I/O.
  - other error: `Err(Error::Api { status: 500, .. })` (or `Error::Other`) →
    propagated unchanged.
  - empty prefix: matches all → resolves with a single-task list, ambiguous with
    multiple, "no task" with an empty list.
- Manual: `get` / `enable` / `disable` / `update` / `delete` by short prefix
  against the dev stack; a bogus prefix errors with the no-match message; and a
  `periodic:manage`-only token still acts by full UUID while `cbc periodic list`
  is forbidden (exercises the 403 fallback end-to-end).

### Verification

```bash
cargo fmt --all
cargo clippy --workspace
cargo check --workspace
cargo test -p cbc
```

---

## Pre-commit checks (both commits)

Per `cbsd-rs/CLAUDE.md`, in order, before staging each commit:

```bash
cargo fmt --all
cargo clippy --workspace
cargo check --workspace
```

GitNexus: run `detect_changes({scope: "compare", base_ref: "main"})` before each
commit to confirm only `periodic.rs` symbols and the `run` dispatch flow are
affected. No `.sqlx/` regeneration (no query change).

## Commit conventions

- Component prefix `cbc:` (the file lives under `cbsd-rs/cbc/`).
- DCO sign-off (`-s`) on every commit; never GPG-sign autonomously.
- Exactly one `Co-authored-by` trailer for the active Claude instance.
- Separate `git add` and `git commit`; discrete commands only.
