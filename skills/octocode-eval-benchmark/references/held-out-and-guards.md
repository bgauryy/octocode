# Held-out and guards
Load before selecting or accepting a candidate. Why: adaptive reuse, near-duplicates, and noise can masquerade as improvement.

## Data roles
| Set | Permitted use |
|---|---|
| Development | Inspect failures and tune the subject freely within budget |
| Validation | Select among candidates under a fixed selection budget; feedback creates selection bias |
| Sealed test | Confirm the selected candidate once under a preregistered trial plan; no tuning from results |
| Regression | Known successes that must remain correct; not evidence of unseen generalization |

Split by source problem, repository, customer, template, or time as appropriate before producing variants. Keep near-duplicates and paraphrases in the same split. Synthetic cases supplement representative real tasks and require a solvability/reference check; more variants of one template are not more independent tasks.

Record split version, provenance, exposures, number of candidates tried, and final-test accesses. A pass/fail summary also leaks information when repeatedly used to choose edits. Once final-test feedback guides a change, retire those items to development/regression and obtain fresh confirmation tasks. Never call public fixtures or embedded grader samples held-out. Private alone does not mean unexposed.

## Comparable measurements
- Pair baseline and candidate on the same task snapshots and budgets. Freeze model/tool versions, available context, permissions, concurrency, retries, and warm-up policy. Randomize/interleave arm order to limit service or time drift; record seeds when supported without assuming reproducibility.
- Predeclare task count, repeats, meaningful effect size, uncertainty method, and stop rule. Report paired deltas and per-slice regressions. For sampled tasks, resample independent tasks/families (e.g. paired cluster bootstrap), retaining repeats within each cluster; repeated calls on one task do not increase independent task count.
- Report success numerators/denominators and intervals; use an appropriate binomial interval for independent binary trials. Tiny or unrepresentative samples do not establish generalization, regardless of interval width.
- Repeatedly checking ordinary confidence intervals and stopping on a win is not valid sequential inference. Use a fixed horizon or a justified sequential/multiple-comparison procedure; log all attempted candidates, not only the winner.
- Track solver failures, timeouts, infrastructure errors, judge errors, Unknowns, and missing artifacts separately. Predeclare exclusions/retries, show totals and coverage, and rerun affected pairs where appropriate. Missing data must never improve the pass rate.

## Verdict
- **ACCEPT:** valid comparable sealed evidence meets the declared effect/uncertainty rule and every critical guardrail.
- **REVERT:** a valid comparison fails the decision rule or breaches a guardrail.
- **INCONCLUSIVE:** uncertainty or coverage cannot decide; collect more only under the declared plan, or start a new experiment.
- **INVALID:** leakage, changed harness, or environment mismatch compromises the comparison; repair and rebaseline.

Inner-loop KEEP selects a candidate on development evidence; it is not release acceptance. If final tests expose a grader defect, version the correction, preserve old results, and rerun both arms without cherry-picking cases.

Next: isolation → `clean-lab.md`; report → `output.md`.
