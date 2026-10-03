# Held-out and guards
Load before you select or accept a candidate. Adaptive reuse, near-duplicates, and noise can look like improvement.

| Set | Permitted use |
|---|---|
| Development | Inspect failures and tune freely within budget |
| Validation | Select among candidates under a fixed selection budget; feedback creates selection bias |
| Sealed test | Confirm the selected candidate once under a preregistered plan; no tuning from results |
| Regression | Known successes that must stay correct; not evidence of generalization |

## Splits
- Split by source problem, repository, customer, template, or time before you create variants. Keep near-duplicates and paraphrases in one split.
- Synthetic cases supplement real tasks and need a solvability and reference check. More variants of one template are not more independent tasks.
- Record split version, provenance, exposures, candidate count, and final-test accesses. A repeated pass/fail summary also leaks.
- Once final-test feedback guides a change, retire those items to development or regression and get fresh confirmation tasks.
- Never call public fixtures or embedded grader samples held-out. Private does not mean unexposed.

## Comparable measurements
- Pair arms on the same task snapshots and budgets. Freeze model and tool versions, context, permissions, concurrency, retries, and warm-up. Randomize or interleave arm order; record seeds when supported, without assuming reproducibility.
- Predeclare task count, repeats, effect size, uncertainty method, and stop rule. Report paired deltas and per-slice regressions.
- Resample independent tasks or families (for example, paired cluster bootstrap), keeping repeats inside each cluster. Repeats on one task do not add independent tasks.
- Report numerators, denominators, and intervals (binomial for independent binary trials). Tiny or unrepresentative samples never establish generalization, whatever the interval.
- Peeking at intervals and stopping on a win is invalid. Use a fixed horizon or a justified sequential or multiple-comparison procedure; log every candidate.
- Track solver failures, timeouts, infrastructure errors, judge errors, Unknowns, and missing artifacts separately. Predeclare exclusions and retries; show totals and coverage; rerun affected pairs. Missing data never improves the pass rate.

## Verdict
- **ACCEPT**: valid comparable sealed evidence meets the effect and uncertainty rule and every critical guardrail.
- **REVERT**: a valid comparison fails the rule or breaches a guardrail.
- **INCONCLUSIVE**: uncertainty or coverage cannot decide; collect more only under the plan, or start a new experiment.
- **INVALID**: leakage, a changed harness, or environment mismatch; repair and rebaseline.
- If final tests expose a grader defect, version the fix, keep old results, and rerun both arms without cherry-picking.

Next: isolation → `references/clean-lab.md`; report → `references/improve-loop.md`.
