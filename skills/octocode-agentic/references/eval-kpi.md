Load when writing the KPI contract of an agent change, making two measurements comparable, or giving the final verdict.

# KPI contract and verdict

## KPI contract

- Define the user-visible success measure and independent guardrails; leading measures only diagnose.
- Record each applicable field before the baseline; name consequential omissions; never invent an unmeasured baseline or target.
- Deterministic optimizations (exact byte counts) need only a short record.

| Field | Record before baseline |
|---|---|
| Goal / subject | User-visible goal; mutable surface; baseline and candidate versions |
| Primary | Metric, direction, runnable sensor, aggregation, effect threshold |
| Guardrails | Correctness/safety floors, cost/latency bounds, no-regression slices |
| Data / access | Split IDs, provenance, unit, exposure owner; [roles](eval-isolation.md#roles-and-access), resets, checks |
| Comparable setup | Cases, grader, subject and runtime settings that could change the result |
| Judge | [Calibration](eval-grading.md#calibrate-a-judge), tolerances, abstention, adjudication |
| Budget | Tasks, repeats, attempts, repairs, candidate limit, time/token/cost ceiling |
| Comparison | Pairing, order, uncertainty method, exclusions, retries, missing data, stop rule |
| Decision | Keep rule and verdict rule |

## Comparable measurements

- Freeze model and tool versions, prompt wording, context, permissions, concurrency, retries, warm-up (rewording a prompt can move results more than rerunning it); randomize or interleave arm order; record seeds.
- Report paired deltas, per-slice regressions, numerators, denominators and uncertainty intervals: `node <skill-folder>/scripts/run-stats.mjs <runs.jsonl> --baseline <arm> --candidate <arm>` ([run-stats.mjs](../scripts/run-stats.mjs); `--help` gives the line format) prints them from a paired cluster bootstrap with repeats kept inside their task. Use the family id as `task` when tasks cluster by family; run it once per slice. Read `unpaired` and `undefinedResamples` before the interval. No interval makes a tiny or unrepresentative sample generalize.
- Never stop on a peeked win: fixed horizon or justified sequential or multiple-comparison procedure; log every candidate.
- Count solver failures, timeouts, infrastructure and judge errors, Unknowns, missing artifacts separately, with coverage; keep them and retries in the denominator and report their cost; rerun affected pairs. Missing data never raises the pass rate.

## Verdict

| Verdict | When | Then |
|---|---|---|
| Accept | valid, comparable sealed evidence meets the effect and uncertainty rule and every critical guardrail | — |
| Revert | a valid comparison fails the rule or breaches a guardrail | — |
| Inconclusive | uncertainty or coverage cannot decide; proves nothing | collect more only under the plan, or start a new experiment |
| Invalid | leakage, changed harness, or environment mismatch; proves nothing | repair and rebaseline |
