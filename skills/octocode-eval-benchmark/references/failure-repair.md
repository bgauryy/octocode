# Failure diagnosis and repair
Load when benchmark results fail, fluctuate, or improve suspiciously. Why: changing the solver before checking the measurement can optimize the wrong defect.

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
1. Turn the development failure into a mechanism-level hypothesis: trigger, failure, expected correction, and counterexample. Keep case-specific answers and IDs out of instructions.
2. Add or identify a failing development check before the patch. If the suite changes, start a new experiment and measure the unchanged subject on that version first.
3. Make one coherent change. Evaluate positive cases, negative cases, valid alternatives, neighboring edge cases, and old successes. Family variants diagnose a mechanism; they do not become independent held-out evidence.
4. Keep/discard from the development rule, considering quality and cost. Log losing candidates and selective retries. Choose the final candidate before sealed testing; apply `held-out-and-guards.md` for acceptance.

If inspecting a final-test failure informs the repair, treat that task/family as exposed and obtain fresh confirmation data. A “fix” that changes the expected answer, grader tolerance, resource budget, and solver together cannot attribute improvement to the solver.

## Validate the repair at the right layer
For a grader-only correction, regrade the same sealed outputs if they contain all required evidence and the task is unchanged. For a changed question, fixture, worker context, or execution environment, rerun both arms; post-hoc rescoring cannot repair a different task. Preserve a versioned before/after record of changed labels and why.

Resource guarantees and hard limits both belong in the environment record. Increasing headroom can remove accidental crashes or enable stronger search; measure which happened rather than automatically classifying all timeouts/OOMs as external noise.

Next: case taxonomy → `error-analysis.md`; subject loop → `agent-loop.md`; judge checks → `llm-judge.md`; provenance → `references.md`.
