# Failure diagnosis and repair

Load when benchmark results fail, fluctuate, or improve suspiciously. Check the measurement before you change the solver.

Preserve the task, permitted inputs, actual worker prompt, environment, answer, trace, grader output, and versions. Reproduce under the same budget; record every retry. Locate the first violated task contract, verify its evidence, and test an alternate explanation. Several categories can apply; do not force one label.

| Finding | Discriminating check | Owner and repair |
|---|---|---|
| Missing requirement or misleading question | Can an independent person solve from worker-visible inputs only? Do hidden tests require an unstated path, API, or phrasing? | Task owner clarifies genuine requirements; new dataset version. |
| Wrong or narrow reference/test | Test valid alternatives and deliberately wrong or incomplete outputs; inspect the actual final state | Grader owner fixes false rejects/accepts and validates against independent labels. |
| Model judge bias or error | Blinded order swaps, domain labels, evidence checks, Unknown/error rate | Evaluator recalibrates or version-pins the judge and adjudicates disputes. |
| Infrastructure or fixture fault | Setup/control task, clean replay, resource/timeout logs, dependency availability | Harness owner repairs reproducibility; new environment receipt; paired rerun. A score change after raising budgets is not a solver gain. |
| Leakage or grader exploitation | Actual exports, tool reads, history, shared state, suspicious answer retrieval | Controller invalidates affected evidence, closes access paths, rotates exposed final tasks; rerun in a verified clean lab. |
| Genuine solver defect | Task and grader are sound; trace shows a supported requirement violated | Developer patches the smallest general mechanism under the unchanged development harness. |
| Sampling noise, thin coverage | Paired task-level uncertainty, per-slice results, infrastructure covariance | Follow the frozen sampling plan or report INCONCLUSIVE. |

## Improve the subject without teaching the test

Run `references/agent-loop.md`, plus these rules:
- Name the trigger and a counterexample in the hypothesis. Keep case-specific answers and IDs out of instructions.
- If the suite changes, measure the unchanged subject on the new version first.
- Also evaluate valid alternatives, neighboring edge cases, and old successes.

## Validate at the right layer

- Grader-only correction: regrade the same sealed outputs if they hold all required evidence and the task is unchanged.
- Changed question, fixture, worker context, or environment: rerun both arms; rescoring cannot repair a different task. Keep a versioned before/after record of changed labels and why.
- Record resource guarantees and hard limits in the environment record. More headroom can remove accidental crashes or enable stronger search; measure which. Not every timeout or OOM is external noise.
