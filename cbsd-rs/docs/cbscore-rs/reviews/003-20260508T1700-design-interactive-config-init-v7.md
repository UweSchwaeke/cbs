# Design Review v7: Interactive `config init` for `cbsbuild`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/003-20260427T1255-interactive-config-init.md`

**Prior reviews:** `003-20260428T1401-design-interactive-config-init-v1.md`,
`003-20260429T0929-design-interactive-config-init-v2.md`,
`003-20260429T1633-design-interactive-config-init-v3.md`,
`003-20260430T1208-design-interactive-config-init-v4.md`,
`003-20260506T1000-design-interactive-config-init-v5.md`,
`003-20260506T1400-design-interactive-config-init-v6.md`

**Changes since v6:** None. Design 003 is unchanged on disk since the v6 review.

---

## Summary Assessment

**Verdict: approve, no changes needed.**

Design 003 is unchanged since v6. All prior findings remain closed. This pass
probed two angles not emphasised in earlier reviews: the `ScriptedPrompter`
exhaustion contract, and whether the §Final confirmation step 2 / step 4 split
is internally consistent. Both check out. No regression, no new coherence gap.

---

## Fresh Probe A: `ScriptedPrompter` answer exhaustion

The design specifies `ScriptedPrompter { answers: VecDeque<PromptAnswer> }` but
does not define what happens when the deque is empty and a prompt call arrives.
This is intentionally left as an implementation detail — the design focuses on
the _shape_ of the abstraction, not on every panic vs. error path inside a test
helper. At implementation time the implementer will choose either `panic!`
(simplest — a test that runs out of answers has a bug) or an
`Err(PromptError::Exhausted)` return. Either is acceptable; the design does not
need to prescribe this. No issue.

## Fresh Probe B: Final confirmation steps 2 and 4

The §Final confirmation section lists:

- Step 2: overwrite-confirm if the file already exists.
- Step 4: confirm write.

These are two separate `dialoguer::Confirm` prompts when the target file exists.
The design states "Python emits both prompts when the file exists", and
references lines 290-292 for step 2 and line 302 for step 4. The intent is that
step 4 is the load-bearing write gate; step 2 is an early-exit path only. An
operator can decline at step 2 (exit without writing) _or_ pass step 2 and then
decline at step 4 (also exit without writing). This is consistent with Python
parity and is described accurately. No confusion issue.

One observation — not a finding — for the implementer: the exit code on step 2
decline is `ENOTRECOVERABLE` (matching step 4's code per §Final confirmation).
The design is explicit about this. Correct.

---

## Summary of Action Items

None.
