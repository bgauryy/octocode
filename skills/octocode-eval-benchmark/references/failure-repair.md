# Failure diagnosis and repair
Load when benchmark results fail, fluctuate, or improve suspiciously. Check the measurement before you change the solver.

## Reproduce and locate the first divergence
Preserve the task, permitted inputs, actual worker prompt, environment, answer, trace, grader output and versions. Reproduce under the same budget; record every retry. Locate the first violated task contract, verify its evidence, and test an alternate explanation. Multiple categories may apply; do not force a single label.

| Finding | Discriminating check | Repair owner and action |
|---|---|---|
| Missing requirement or misleading question | Can an independent person solve from only worker-visible inputs? Do hidden tests require an unstated path, API or exact phrasing? | Task owner clarifies genuine requirements; new dataset version, rerun both arms. Never add the solution as a hint. |
| Wrong or narrow reference/test | Test known valid alternatives and deliberately wrong/incomplete outputs; inspect actual final state | Grader owner fixes false rejects/accepts and validates against independent labels; preserve old grades and rescore sealed artifacts where sufficient. |
| Model judge bias or error | Blinded order swaps, domain labels, evidence checks, Unknown/error rate | Evaluator recalibrates/version-pins the judge; adjudicates disputes. More agreeing judges alone is not a fix. |
| Infrastructure or fixture fault | Setup/control task, clean replay, resource/timeout logs, dependency availability | Harness owner repairs reproducibility; new environment receipt and paired rerun. A score change after raising budgets is not a solver gain. |
| Leakage or grader exploitation | Inspect actual exports, tool reads, history, shared state and suspicious answer retrieval | Controller invalidates affected evidence, closes access paths, rotates exposed final tasks; rerun in a verified clean lab. |
| Genuine solver defect | Task and grader are sound; trace shows supported requirement violated | Developer patches the smallest general mechanism under the unchanged development harness. |
| Sampling noise / inadequate coverage | Paired task-level uncertainty, per-slice results, infrastructure covariance | Follow the frozen sampling plan or report INCONCLUSIVE; do not repeat until a favorable run appears. |

## Improve the subject without teaching the test
Run `references/agent-loop.md` (one change, keep/discard, log losers, select before sealed testing). Add these repair rules:
- Write a mechanism-level hypothesis: trigger, failure, expected correction, counterexample. Keep case-specific answers and IDs out of instructions.
- If the suite changes, measure the unchanged subject on the new version first.
- Evaluate positive and negative cases, valid alternatives, neighboring edge cases, and old successes. Family variants diagnose a mechanism; they are not independent held-out evidence.
- If a final-test failure informs the repair, treat that task or family as exposed and get fresh confirmation data.
- A fix that changes the expected answer, grader tolerance, resource budget, and solver together cannot attribute a gain to the solver.

## Validate the repair at the right layer
For a grader-only correction, regrade the same sealed outputs if they contain all required evidence and the task is unchanged. For a changed question, fixture, worker context, or execution environment, rerun both arms; post-hoc rescoring cannot repair a different task. Preserve a versioned before/after record of changed labels and why.

Record resource guarantees and hard limits in the environment record. More headroom can remove accidental crashes or enable stronger search; measure which happened. Do not classify every timeout or OOM as external noise.

Next: case taxonomy → `references/error-analysis.md`; subject loop → `references/agent-loop.md`; judge checks → `references/llm-judge.md`; sources → `references/references.md`.
