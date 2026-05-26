# Review: impl security-audit-remediation — Phase 2 v2

- **Design:** `019-20260512T2339-impl-cbsd-rs-security-audit.md`
- **Plan:** `019-20260524T0805-impl-security-audit-remediation-phase-2.md`
- **Scope:** commits `9b79e1b7`–`1f335e0b` on `wip/cbsd-rs-security-review`
- **Reviewer:** independent adversarial re-review (v1 finding disposition
  verification)
- **Date:** 2026-05-28

---

## Purpose

This review verifies the disposition of every finding from the v1 mid-phase-2
review (`019-20260524T1031-impl-security-audit-remediation-phase-2-v1.md`). The
v1 verdict was **block-merge / score 35/100**, with four blockers (B1–B4) and
three minor findings (M1–M3).

Five commits are in scope:

| SHA        | Subject                                                 |
| ---------- | ------------------------------------------------------- |
| `9b79e1b7` | tarball containment: Directory arm phase-2 check + test |
| `1666b9f2` | auth: OAuth `email_verified` check + 403 → 401          |
| `92d768a8` | rbac: `periodic:manage` split + stale-doc sweep         |
| `97a06c05` | docs/ROADMAP: defer B1/B2/B3 and M1                     |
| `1f335e0b` | fixup!: rename v1 review file references in ROADMAP     |

---

## 1. Summary Assessment

All seven v1 findings have been given an explicit disposition: B4 and M2/M3 are
implemented; B1/B2/B3 and M1 are durably deferred to the ROADMAP. The
implementations are clean and technically correct. One new minor issue surfaces
— the v1 review file (`019-…-phase-2-v1.md`) is untracked in git, making both
ROADMAP origin links dangling in a fresh clone — and should be resolved before
the branch is merged. All other new observations are below the bar of a finding.

**Verdict: accept-with-fixup.** The v1 block-merge condition is fully resolved.
One required action remains before merge: commit the v1 review file (or remove
the file-specific ROADMAP links).

---

## 2. v1 Finding Dispositions

### B1 — Trigger SI-15 integration tests (deferred)

ROADMAP entry "Testing hardening — Phase 2 audit-remediation deferrals" names
three concrete test categories: SI-15 trigger integration tests, WebSocket
over-cap protocol test, and `tracing-test` log-capture assertions. Each is
described with sufficient technical detail (which production function to
exercise, what scenario to model, what regression it guards) to be a standalone
work item. The trigger condition is explicit: "before the next implementation
phase opens new ground that touches the same production code without
strengthening the test guard, or sooner."

**Disposition: deferral accepted.** Technical detail is adequate; trigger
condition is actionable.

### B2 — WebSocket over-cap protocol-level test (deferred)

Covered by the same ROADMAP entry as B1. See B1 disposition.

**Disposition: deferral accepted.**

### B3 — `tracing-test` log-capture assertions for D9 (deferred)

Covered by the same ROADMAP entry as B1. See B1 disposition.

**Disposition: deferral accepted.**

### B4 — OAuth 403 → 401 (implemented)

`cbsd-server/src/routes/auth.rs` — the `validate_user_info` rejection site
returns `StatusCode::UNAUTHORIZED`. Verified:

- Exactly one `StatusCode::FORBIDDEN` remains in `oauth_callback`: it guards the
  robot-name-prefix check (`cbsk_` / `cbrk_` prefix on email field), which is a
  different semantic from authentication failure. That guard is correctly left
  as FORBIDDEN.
- The callback handler carries no `#[utoipa::path]` annotation, so there is no
  OpenAPI spec to drift.
- The error body is unchanged
  (`"authentication failed; contact your administrator"`), avoiding any
  client-visible contract break.
- Dev-mode path explicitly synthesises `email_verified: true` before calling
  `validate_user_info`, preventing the check from rejecting synthetic logins.
- 7 unit tests in `cbsd-server/src/auth/oauth.rs` pass, including
  `validate_rejects_unverified_email_before_domain_check`.

**Disposition: implemented correctly.**

### M1 — builder role `periodic:manage:own` not seeded (deferred)

ROADMAP entry "Review periodic task capability semantics" documents the
dead-cap-in-isolation problem, the surprising `:view` independence, the OQ1
trigger-time re-validation sub-question, and a concrete scope for resolution.
Priority is M with a trigger condition tied to production deployment of
non-admin periodic caps.

**Disposition: deferral accepted per explicit direction.**

### M2 — docs/rbac.md stale cap names (implemented)

`git show 92d768a8` confirms the sweep covered 8 files. The route table in
`rbac.md` was also corrected: `/trigger` POST and `/retry` POST were replaced
with `/enable` PUT and `/disable` PUT, which match the actual routes in
`cbsd-server/src/routes/periodic.rs`. The `audit_identity_lint` module-level
comment was updated from `periodic:manage` to `periodic:manage:{own,any}`.

A cross-check: `KNOWN_CAPS` in `cbsd-server/src/routes/permissions.rs` contains
`"periodic:manage:own"` and `"periodic:manage:any"`; legacy `"periodic:manage"`
is absent.

**Disposition: implemented and expanded correctly.**

### M3 — Directory arm skips phase-2 realpath check (implemented)

`cbsd-worker/src/build/component.rs` — the Directory arm in `unpack_one` now
calls `verify_parent_realpath_under` BEFORE `create_dir_all(&dest)`. The
ordering is:

1. `create_dir_all(parent)` — creates ancestor chain
2. `verify_parent_realpath_under(unpack_root_real, &dest)` — confirms the
   on-disk parent resolves inside the unpack root
3. `create_dir_all(&dest)` — creates the target directory

This is the correct order: the realpath check happens before the
potentially-exploitable directory creation, not after.

**Test coverage:** `phase2_rejects_directory_creation_through_symlink_swap`
calls `verify_parent_realpath_under` directly with a crafted filesystem
scenario. It does not exercise the wiring through `validate_and_unpack` with a
malformed tarball. The Directory arm's call to `verify_parent_realpath_under` is
verified by code reading only; there is no end-to-end integration test that
triggers the containment error through the full unpack path.

This gap is narrower than the pre-fix state (where the check was absent
entirely). The direct-helper test proves the helper rejects the attack scenario;
the wiring is trivially inspectable in a 4-line arm. Not a blocker.

**Disposition: implemented correctly. Minor test-wiring gap noted (see Section
4).**

---

## 3. New Findings

### N1 — v1 review file is untracked in git (minor)

Both ROADMAP entries cite:

```
Origin: implementation review at
  ./cbsd-rs/reviews/019-20260524T1031-impl-security-audit-remediation-phase-2-v1.md
```

`git status` shows this file as `??` (untracked). A fresh clone will have both
ROADMAP origin pointers pointing to a non-existent file. The file was renamed
outside of git (from the `-mid-v1` suffix form), so no rename record exists in
the history, and the old path is also gone.

**Required action before merge:** commit
`cbsd-rs/docs/cbsd-rs/reviews/019-20260524T1031-impl-security-audit-remediation-phase-2-v1.md`
into git. If committing the v1 review file is not desired, both ROADMAP
`Origin:` lines should be updated to refer only to the design/plan documents
(which are tracked).

---

## 4. Minor Observations

- **M3 test name matches the symlink-swap pattern but tests the helper, not the
  arm.** `phase2_rejects_directory_creation_through_symlink_swap` is
  structurally equivalent to `phase2_rejects_post_unpack_symlink_swap` with a
  different `entry_dest` argument. Neither test exercises `validate_and_unpack`.
  This is acceptable for this iteration given the triviality of the wiring, but
  a tarball-level integration test would close the gap completely and is worth
  adding when the deferred WS over-cap test (B2) is implemented (similar
  "protocol-level, not unit-level" work).

- **`audit_identity_lint.rs` `HUMAN_ONLY_ROUTES` claim for `periodic.rs` is
  pre-existing and incorrect.** The module comment says robots cannot hold
  `periodic:manage:{own,any}` caps, but `ROBOT_FORBIDDEN_CAPS` in
  `extractors.rs` does not include any periodic cap. This means a robot token
  carrying `periodic:manage:own` would pass the `load_authed_user` cap strip and
  reach a periodic handler, yet `periodic.rs` is in the human-only allowlist.
  This was wrong before this iteration (the old `periodic:manage` was also
  absent from `ROBOT_FORBIDDEN_CAPS`). The commit updates the comment text but
  does not change the underlying mismatch; it is not a regression of this
  iteration. The ROADMAP M1 entry should cover this as part of the periodic cap
  semantics review.

- **Route-name corrections bundled into the cap-rename commit.** The `/trigger`
  → `/enable` and `/retry` → `/disable` corrections in `rbac.md` are a separate
  concern from the `periodic:manage` cap rename. Bundling them is harmless and
  arguably correct (both were stale in the same doc), but it mixes a
  spec-accuracy fix with a capability-model change in one commit. Not a problem
  in practice.

---

## 5. Open Questions

**OQ1 (carried from v1) — Trigger-time re-validation scope.** If a task owner
holds `periodic:manage:own` but loses the `periodic:create` or `builds:create`
cap, should the scheduler still fire the task? Today the answer is yes
(trigger-time re-validation only checks `periodic:create` and `builds:create`,
not `:manage:own`). This is documented in the ROADMAP entry but not resolved.
The answer depends on the intended deployment semantics and should be part of
the cap semantics review before periodic caps are granted to non-admin users.

---

## 6. Confidence Score

| Item                                            | Points | Description                                                                                                         |
| ----------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------- |
| Starting score                                  | 100    |                                                                                                                     |
| D11: Untracked v1 review file                   | -5     | ROADMAP links point to a file not in git; dangling in fresh clone                                                   |
| D5: M3 Directory arm wiring untested end-to-end | -5     | Test exercises helper directly; `validate_and_unpack` code path through Directory arm has no tarball-level coverage |
| **Total**                                       | **90** |                                                                                                                     |

**Interpretation: 90 — Ready to merge with one required action (N1).**

---

## 7. Required Actions Before Merge

1. **Commit the v1 review file** — add
   `cbsd-rs/docs/cbsd-rs/reviews/019-20260524T1031-impl-security-audit-remediation-phase-2-v1.md`
   to git, or remove the file-specific `Origin:` links from the ROADMAP entries.
   The ROADMAP as committed has two dangling links.

---

## 8. Summary

The v1 block-merge verdict is resolved. B4 (OAuth 401), M2 (doc sweep), and M3
(Directory arm fix) are implemented correctly. B1/B2/B3 and M1 are durably
deferred with adequate technical detail and actionable trigger conditions in the
ROADMAP.

One required action remains: the v1 review file that both ROADMAP entries cite
must be committed, or those links must be removed. Absent that fix, the ROADMAP
is structurally broken in any fresh clone.

All 256 workspace tests pass (`SQLX_OFFLINE=true cargo test --workspace`).
