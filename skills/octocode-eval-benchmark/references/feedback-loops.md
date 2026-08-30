# Feedback Loops
Load before starting any improvement loop. Why: a loop without a trustworthy sensor optimizes noise.

## Prerequisite gate
No workable feedback loop → build one first (`eval-harness.md`), then loop.
A loop is workable only when all four hold:

| Requirement | Test |
|---|---|
| **Sensor** | One command prints the metric (profiler, benchmark, test suite, eval score) |
| **Deterministic enough** | Same subject → same reading, or known variance with N-trial averaging |
| **Fast & cheap** | Sensor cost per iteration ≪ mutation cost; fits the declared budget |
| **Target** | Numeric threshold or delta declared before iterating (`kpi-contract.md`) |

## Sensor quality ladder
1. Deterministic command exit/score (test pass, benchmark ms, bytes, ERROR count) — best
2. Averaged noisy measure (p95 latency, memory RSS) — fix trial count and environment first
3. Calibrated LLM judge — advisory, or paired with a deterministic floor
4. Human vibes — never a loop sensor; use only to calibrate graders

## Slow or noisy sensor tactics
- Derive a **leading** proxy with faster feedback; keep the lagging primary for final VERIFY.
- Pin the environment (versions, warm-up, isolation) before trusting deltas.
- If sensor noise exceeds the expected effect size → STOP; fix the sensor, not the subject.

## "Don't stop till done" requests
Translate the ask into the contract — sensor command + numeric target + budget — then run
`agent-loop.md` without pausing between iterations. Missing any of the three → build or ask
before the first mutation.

Next: contract fields → `kpi-contract.md`; run the loop → `agent-loop.md`; build the sensor → `eval-harness.md`.
