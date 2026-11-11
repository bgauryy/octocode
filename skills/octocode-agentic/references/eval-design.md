Load when planning a comparison: KPI, cases, splits, measurement, or final verdict.

## KPI contract

- One user-visible goal, one primary metric, plus guardrails; leading measures only diagnose.
- Record each applicable field before the baseline; name consequential omissions; never invent an unmeasured baseline or target.
- Deterministic optimizations (exact byte counts) need only a short record.

| Field | Record before baseline |
|---|---|
| Goal / subject | User-visible goal; mutable surface; baseline and candidate versions |
| Primary | Metric, direction, runnable sensor, aggregation, effect threshold |
| Guardrails | Correctness/safety floors, cost/latency bounds, no-regression slices |
| Data / access | Split IDs, provenance, unit, exposure owner; [roles](eval-harness.md#roles-and-access), resets, checks |
| Harness hashes | Cases, references, graders, schemas, prompts, runtime, tools |
| Judge | [Calibration](eval-grading.md#calibrate-a-judge), tolerances, abstention, adjudication |
| Budget | Tasks, repeats, attempts, repairs, candidate limit, time/token/cost ceiling |
| Comparison | Pairing, order, uncertainty method, exclusions, retries, missing data, stop rule |
| Decision | Keep rule and verdict rule |

## Cases from failures

1. Collect representative traces (production, dogfood, or synthetic starter); a domain expert notes each one's first clear failure. Fix a task set from real workloads, including hard and adversarial cases, before changing anything.
2. Cluster into a failure taxonomy with frequencies until new traces stop changing it; inspect rare high-impact failures separately.
3. One grader or case family per top failure mode, tagged `failureSignature: mechanism:stale-cache|cause:missing-invalidation`; rank by frequency × impact; rare critical failures become guardrails.

- Keep representative samples apart from enriched stress tests.
- Cascading failures: fix or tag the first upstream break. Product or model shift: redo the analysis.

## Public benchmarks

- Reported public gain: audit transcripts, else weak evidence. Check it measures your skill.
- Famous: assume contamination (training, prompts, RAG). Near ceiling: harder tasks or a new suite. pass@100 is 0%: suspect task or grader.
- SWE-style coding: passing tests are not merge-ready; a saturated board is never sole proof.
- Contaminated, saturated, or gamed: retire to regression smoke; build a fresh private suite.

## Splits and exposure

- **Development**: inspect failures, tune within budget. **Validation**: select candidates under a fixed budget.
- **Sealed test**: confirm the selected candidate once under the preregistered plan; no tuning. **Regression**: known successes that must stay correct; not generalization evidence.
- Split before making variants, by source problem, repository, customer, template, or time; near-duplicates and paraphrases share a split.
- Synthetic cases supplement real ones after solvability and reference checks; template variants are not independent.
- Record split version and every final-test access; repeated pass/fail summaries leak too.
- Final-test feedback guided a change: retire those items to development or regression; get fresh confirmation tasks. Private is not unexposed.

## Comparable measurements

- Freeze model and tool versions, context, permissions, concurrency, retries, warm-up; randomize or interleave arm order; record seeds.
- Report paired deltas, per-slice regressions, numerators, denominators, intervals: `41/50 (82%, 95% CI 69–90%)`. Binomial for independent binary trials; else paired cluster bootstrap over tasks or families, repeats inside their cluster. No interval makes a tiny or unrepresentative sample generalize.
- Never stop on a peeked win: fixed horizon or justified sequential or multiple-comparison procedure; log every candidate.
- Count solver failures, timeouts, infrastructure and judge errors, Unknowns, missing artifacts separately, with coverage; rerun affected pairs. Missing data never raises the pass rate.

## Verdict

| Verdict | When | Then |
|---|---|---|
| Accept | valid, comparable sealed evidence meets the effect and uncertainty rule and every critical guardrail | — |
| Revert | a valid comparison fails the rule or breaches a guardrail | — |
| Inconclusive | uncertainty or coverage cannot decide; proves nothing | collect more only under the plan, or start a new experiment |
| Invalid | leakage, changed harness, or environment mismatch; proves nothing | repair and rebaseline |
