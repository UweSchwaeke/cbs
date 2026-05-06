# Review 018-v1 — Plan: `serde_yml` → `serde-saphyr` migration

- **Component:** `cbsd-rs` (`cbsd-server` + `cbsd-worker`)
- **Design:**
  [018 — serde-saphyr migration](../design/018-20260506T1015-serde-saphyr-migration.md)
- **Plan:**
  [018 — serde-saphyr migration plan](../plans/018-20260506T1213-serde-saphyr-migration.md)
- **Reviewer:** Claude Sonnet 4.6
- **Review date:** 2026-05-06
- **Status:** Approved with conditions

## Executive summary

The plan is well-scoped, mechanically sound, and correctly resolves all three
open questions from the design. The audit of authoritative YAML files is
thorough and the rollback story is clean. Two substantive concerns exist: (1) a
formatting inconsistency introduced between the server's `read` panic and
`parse` panic that the plan does not acknowledge, and (2) the rich-error
rendering decision is elevated to in-scope but is only manually verified — there
is no regression test that would catch a future refactor silently flattening the
snippet. Neither is a blocker; both are addressable before or during
implementation without a re-review.

## Verified facts (confirmed by reading source)

Before evaluating the plan, the following were verified from the actual code and
configuration files:

| Claim                                                                                   | Verified |
| --------------------------------------------------------------------------------------- | -------- |
| `serde_yml` appears at exactly 6 locations (2 Cargo.toml, 4 .rs)                        | Yes      |
| No external constructor of `ConfigError::Parse` outside `config.rs:136`                 | Yes      |
| `cbsd-worker/src/main.rs:104` renders `{err}` with `eprintln!` (multi-line safe)        | Yes      |
| All 3 YAML files are single-document, no YAML 1.1 divergence classes                    | Yes      |
| `cbsd-rs/Cargo.toml` workspace deps: `serde`, `serde_json`, `chrono` only               | Yes      |
| `cbsd-server/Cargo.toml` and `cbsd-worker/Cargo.toml` each carry `serde_yml = "0.0.12"` | Yes      |
| No tests exist in `cbsd-worker/src/config.rs` (`#[cfg(test)]` absent)                   | Yes      |

## Resolved decisions — assessment

### Decision 1: Per-crate deps (not workspace-hoisted)

Confirmed correct. `cbsd-rs/Cargo.toml` hoists only `serde`, `serde_json`, and
`chrono` — dependencies shared with `cbsd-proto` and `cbc`. YAML parsing is used
by exactly the two binary crates and is not shared with `cbsd-proto` or `cbc`.
Per-crate placement is internally consistent with the workspace's existing
hoisting policy.

### Decision 2: Pin form `"0.0.26"` (not `=0.0.26`)

Correct. For `0.0.x` versions, Cargo's semver resolution makes `"0.0.26"` and
`"=0.0.26"` equivalent in practice — only `0.0.26` satisfies the caret
constraint in the zerover range. The caret form is easier to read and matches
the existing `serde_yml = "0.0.12"` style.

### Decision 3: Rich snippet errors — in scope

Technically achievable: `serde-saphyr`'s `from_str` uses `Options::default()`
with `with_snippet: true`, so the snippet is embedded in `Display` output
without any additional API call. The plan's mechanism — prefixing with
`'{path}':\n{e}` — correctly places the snippet on its own line.

**Concern:** This decision is verified only by a manual corruption-and- revert
step (plan verification step 4). There is no automated test that asserts the
snippet appears on its own line. If a future refactor drops the `\n` separator,
the rich rendering silently degrades with no CI signal. See Concern 2 below.

## Strengths

1. **Minimal, atomic footprint.** The plan correctly identifies the change as
   irreducible: 2 Cargo.toml edits, 4 source edits, 1 enum variant arity change.
   Nothing can be deferred or split without creating an uncompilable
   intermediate state. The one-commit structure is correct and the commit
   message draft is accurate and well-scoped.

2. **Thorough YAML compatibility audit.** The design, which the plan inherits,
   audits all three authoritative YAML files against every known YAML 1.1 vs 1.2
   divergence class: Norway problem, leading-zero integers, sexagesimals,
   unquoted scalars containing special characters, anchors, multi-document
   streams, and duplicate keys. The audit conclusion is confirmed by reading the
   files directly. `serde-saphyr`'s YAML 1.1-leaning defaults (accepting
   `yes`/`no` as bools) preserve today's behaviour without any field changes.

3. **`#[non_exhaustive]` on `serde_saphyr::Error` correctly dismissed.** The
   plan notes we never `match` on `serde_saphyr::Error` variants — only
   `Display` and `std::error::Error::source` are used. Both are stable
   regardless of future variant additions. The plan names this correctly as a
   non-issue for our usage pattern.

4. **`ConfigError::Parse` arity change is locally contained.** Confirmed by
   exhaustive grep: `ConfigError::Parse` is constructed at exactly one site
   (`config.rs:136`) and matched in exactly two impls (`Display` and `source`)
   both in the same file. The plan's instruction to verify no other constructors
   exist is prudent; the answer (there are none) is confirmed here.

5. **Rollback is unconditionally clean.** No schema changes, no persisted-state
   migrations, no config-file format changes, no API surface changes. Operators
   see no observable difference between `serde_yml` and `serde-saphyr` on these
   inputs. `git revert` is sufficient.

## Concerns

### Concern 1 — Server config formatting inconsistency (minor) 🟡

**Location:** `cbsd-server/src/config.rs:340–344`

After plan §1.3 is applied, `load_config` will contain two adjacent panics with
visually divergent formatting:

```rust
// read panic (unchanged):
.unwrap_or_else(|e| panic!("failed to read config file {}: {e}", path.display()));
// parse panic (updated by plan):
.unwrap_or_else(|e| panic!("failed to parse config file '{}':\n{e}", path.display()));
```

The read panic has no quotes around the path and no newline before the error
body. The parse panic gets both. An operator seeing either message in systemd
journal output will encounter inconsistent formatting from the same function.
The plan does not acknowledge this divergence.

**Recommendation:** Either (a) align the read panic to use the same `'{}':`
style in the same commit, keeping the change minimal, or (b) add a sentence to
the plan explicitly noting the divergence is accepted because `io::Error`
display is already a single short line and doesn't benefit from a newline
separator. Option (a) is the cleaner operator experience and is two characters
of diff.

The same divergence exists between the worker's `ConfigError::Read` and
`ConfigError::Parse` display: `Read` formats as `'{}': {err}` (single line)
while new `Parse` formats as `'{}':\n{err}`. This is more intentional — a saphyr
snippet is multi-line by nature while an `io::Error` is typically a single
sentence — but the plan should state this is deliberate.

### Concern 2 — Rich-error rendering: in-scope but unasserted 🟡

**Location:** Plan §1.4 and verification step 4

Decision 3 elevates rich snippet rendering to in-scope for this commit. The
mechanism relies on the `\n` separator in the format strings at all three sites.
However:

- No existing tests cover `WorkerConfig::load` at all — the worker's `config.rs`
  has no `#[cfg(test)]` block.
- The plan's verification step 4 ("corrupt a config file, confirm the snippet
  appears, revert") is manual and leaves no automated signal.

If a subsequent commit refactors the Display impl or the error-wrapping closure
and drops the `\n`, the snippet silently collapses to a single line with no test
failure.

**Recommendation:** Plan §1.4 already mentions "add a minimal positive- path
unit test." Extend that test to also exercise the negative path: a deliberately
invalid YAML string deserialized with `WorkerConfig` should produce a
`ConfigError::Parse` whose `Display` contains `\n` (confirming the snippet lands
on its own line). This is a 5-line addition to the test and would anchor the
rich-error rendering against regression. Alternatively, if the rich-error
rendering is considered a QoL side effect rather than a spec requirement,
downgrade decision 3 to "delivered but not regression-tested" and remove the
manual step from the verification checklist.

### Concern 3 — `#[non_exhaustive]` invariant not documented 🟢

`serde_saphyr::Error` is `#[non_exhaustive]`. The plan correctly dismisses this
as a non-issue today. However, neither the plan nor the code leaves a comment
capturing the invariant: "we only use Display + source, never match on
variants." Without this, a future author adding error handling may be tempted to
match on `serde_saphyr::Error` variants directly, breaking silently on the next
saphyr minor bump.

**Recommendation:** Add a one-line doc comment to `ConfigError::Parse` in
`cbsd-worker/src/config.rs` noting that `serde_saphyr::Error` is
`#[non_exhaustive]` and only `Display`/`source` are used. This costs one line
and pays forward for the lifetime of this variant.

## Code-level corrections before implementation

### 1. Plan §1.2 diff is incomplete

The diff in plan §1.2 (`cbsd-server/src/components/mod.rs`) shows the error
format string changing from:

```
format!("failed to parse {}: {e}", yaml_path.display())
```

to:

```
format!("failed to parse '{}':\n{e}", yaml_path.display())
```

This is correct. No correction needed here.

### 2. Plan §1.4 `map_err` form is correct

The plan correctly switches from `map_err(ConfigError::Parse)` (which would no
longer compile once `Parse` takes two arguments) to:

```rust
.map_err(|e| ConfigError::Parse(path.to_path_buf(), e))?;
```

The `path` variable is in scope at that call site (`WorkerConfig::load` receives
`path: &std::path::Path`). The `to_path_buf()` allocation is necessary and
correct. No correction needed.

### 3. Minor: `pretty` format in `Display` for `ConfigError::Read`

Not introduced by this plan, but the adjacent `Read` variant currently formats
as:

```rust
write!(f, "failed to read config file '{}': {err}", path.display())
```

That is single-line. Adding alignment here (in the same commit as the `Parse`
reformat) would be the cleanest operator experience. See Concern 1. This is a
suggestion, not a blocker.

## Verification plan assessment

The plan's verification steps are correct and complete:

1. `cargo fmt` / `clippy` / `check` / `test` — standard and appropriate.
2. Worker config load test — plan mentions adding one if absent (none exist; the
   suggestion is correct).
3. Server config load — smoke-load acceptable for a panic-path; `load_config`
   returns the struct directly with no `Result` wrapper, so a non-panic
   execution is sufficient evidence.
4. Component discovery — single `name` field read; no schema drift possible;
   confirmation is correct.
5. Rich-error sanity check — manual only (see Concern 2).

The instruction "verify no other code constructs `ConfigError::Parse` with the
old single-argument signature" is confirmed by this review: there are none. The
implementer can skip that grep.

## Out-of-scope items — accepted

The plan's "Out of scope" section correctly defers:

- `MessageFormatter` / `Localizer` customisation
- Strict YAML 1.2 boolean enforcement
- YAML serialization
- Workspace hoist of `serde-saphyr`

These are legitimate scope-control decisions, not deferred requirements. No D1
deductions apply.

## Confidence score

This score is applied to the plan as the artifact under review
(pre-implementation). Criteria that apply only to implemented code (D2
duplication, D6 dead code, D7 security gap) are not applicable.

| Item                                                            | Points | Description                                                                                                                                                      |
| --------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Starting score                                                  | 100    |                                                                                                                                                                  |
| D5: no automated test for rich-error snippet rendering          | -5     | Decision 3 is in-scope but unasserted; manual-only verification is weak for a formatting invariant that is easy to accidentally drop                             |
| D9: server-side read panic diverges from parse panic formatting | -5     | After §1.3, adjacent panics in `load_config` have inconsistent path quoting and newline style; operator-facing inconsistency with no acknowledgement in the plan |
| **Total**                                                       | **90** |                                                                                                                                                                  |

**Interpretation:** 90/100 — Acceptable; address noted improvements before or
during implementation. The two deductions are both in the same risk band
(formatting quality / observability) and neither blocks compilation,
correctness, or rollback safety.

## Recommendation

**Proceed with changes.** The plan is mechanically correct and operationally
safe. Before committing:

1. (Required) Acknowledge or fix the read-vs-parse panic formatting divergence
   in `cbsd-server/src/config.rs` (Concern 1). One sentence in the plan, or two
   characters of diff in the implementation.

2. (Strongly recommended) Extend the positive-path worker config unit test
   (which the plan already calls for) to include a negative-path assertion:
   invalid YAML → `ConfigError::Parse` → `Display` contains `\n` (Concern 2).

3. (Optional, low effort) Add a one-line doc comment on `ConfigError::Parse`
   noting `serde_saphyr::Error` is `#[non_exhaustive]` and only
   `Display`/`source` are used (Concern 3).

None of these require a re-review.
