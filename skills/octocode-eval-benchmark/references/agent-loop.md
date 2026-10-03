# Agent loop and loop levels
Load when you improve a subject with a validated frozen harness, or choose which loop to run.

1. Before you change the subject, record the development baseline, harness hashes, comparable budget, and a failing case or below-target outcome.
2. State the failure mechanism and predicted effect. Make the smallest change that tests it; one coherent factor aids attribution.
3. Compare baseline and candidate under the frozen plan. Keep raw outcomes, cost, error categories, and every hypothesis tried.
4. KEEP a candidate only when the selection rule and guardrails hold; else DISCARD its targeted changes. Never revert unrelated workspace edits.
5. Stop at the declared budget, target, or no-new-hypothesis condition. Check development failures for sensor defects or missing coverage before you escalate a loop level.

- TDD: a failing development check before the fix, the same check after.
- Parallel hypotheses (when authorized) use separate subject copies and trials (`references/clean-lab.md`). More candidates raise selection bias. Never report the luckiest attempt as pass@1.
- First verify that the sensor measures the intended outcome at useful cost; if noise hides the expected effect, improve measurement first. A cheap leading proxy can guide development; final verification measures the user-visible outcome.

## Loop levels
| Loop | Cycle | Owner | KPI | Actuators |
|---|---|---|---|---|
| Experiment (inner) | baseline → mutate → measure → keep or discard | Developer or optimizer; solvers get task inputs only | Primary metric | One file, prompt, or skill paragraph |
| Suite (middle) | error-analyze traces → add or fix tasks → rebalance capability and regression | Human + agent | Coverage of top failure modes; regression stay-green rate | Cases, criteria, failure categories |
| Meta (outer) | improve program, skill, graders, budgets | Authorized maintainer | Fewer repeated failure signatures | Skill lobby and refs, harness |

- Grow the suite only between experiments, from real failures.
- A meta change needs a new baseline and sealed VERIFY. Outer loops inspect development traces only.
- Inner loop flat: missing cases → suite; bad program → meta.
- Bilevel meta: if the inner loop is flat, no new hypothesis category appears, and error analysis finds no new failure mode, read the inner-loop trace, find recurrent search patterns, and generate a new search strategy (code or program change), not only `program.md` wording.
