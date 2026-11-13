Load when choosing proof, metrics, cases, KPI, splits, comparison conditions or a final verdict for an agent change.

# Evaluation

## Match the claim to the check

| Claim | Check |
|---|---|
| Editorial or structural | Review intent, links, examples and checks. Report behavior as unmeasured. |
| Deterministic regression | Reproduce the failure, pass the same check after the fix and verify affected contracts. |
| Comparative gain or reliability | Use a representative comparison below. Scale sample and budget to consequence and uncertainty. |
| The host activates the right skill | Follow the [skill activation procedure](skill-activation.md). Record which skill the target host loads first, including none. A description-only check measures routing. |

## Metrics

Optimize verified outcomes per cost, latency and resources; track correctness, safety and critical regressions separately so savings cannot hide failures. Choose metrics that can change the decision: verified success, false completion, unsupported claims, retrieval coverage, unnecessary calls, tokens/cost per successful task, latency tails, cache reuse, continuation fidelity or retry/delegation overhead. Activity and shorter output are not success.

For repeated-trial success metrics (`pass@1`, `pass@k`, `pass^k`), use [grading metrics](eval-grading.md#metrics).

## Run a comparison

1. Freeze the [KPI contract](#kpi-contract) and [splits](#splits-and-exposure) before comparing. [Public benchmarks](#public-benchmarks) orient. Decide on representative private tasks. Version harness, suite or grader changes and reassess both arms. Only a grader-only fix may reuse sealed outputs with all required evidence.
2. Isolate solvers from evaluation material using the [harness and access rules](eval-harness.md). Give both arms their deployment instructions. Answer-specific coaching leaks the answer.
3. Prefer deterministic outcome checks. [Test graders and calibrate judges](eval-grading.md) before freezing them. Account for failures, Unknowns, errors and retries in the denominator and report their cost.
4. Iterate on development tasks using the [Keep/Discard loop](eval-loop.md#keepdiscard-loop). Both labels are provisional. Select on validation before opening the sealed test. Confirm the selected candidate once, with both arms under identical conditions.
5. Assign the [final verdict](#verdict) of `Accept`, `Revert`, `Inconclusive` or `Invalid`. Stop under the declared budget and stopping rule; report uncertainty and the next measurement needed.

## Baseline and observability

Record verified success, false completions, calls, tokens, cost and latency per baseline task. Use [cases from failures](#cases-from-failures), the [Keep/Discard loop](eval-loop.md#keepdiscard-loop) and [comparable measurements](#comparable-measurements) for task selection, one-factor changes, paired comparisons and regressions.

Use existing traces to locate a failure or support a claimed measurement; add only missing evidence ([reason-aware invocation](tools-and-mcp.md#reason-aware-invocation)). An LLM judge can grade, but cannot replace compilation, tests, runtime evidence or authoritative data.

Retain the artifacts supporting measured claims; link existing runner output instead of creating duplicate [run records](../output.md#saved-result).

## KPI contract

- Define the user-visible success measure and independent guardrails; leading measures only diagnose.
- Record each applicable field before the baseline; name consequential omissions; never invent an unmeasured baseline or target.
- Deterministic optimizations (exact byte counts) need only a short record.

| Field | Record before baseline |
|---|---|
| Goal / subject | User-visible goal; mutable surface; baseline and candidate versions |
| Primary | Metric, direction, runnable sensor, aggregation, effect threshold |
| Guardrails | Correctness/safety floors, cost/latency bounds, no-regression slices |
| Data / access | Split IDs, provenance, unit, exposure owner; [roles](eval-harness.md#roles-and-access), resets, checks |
| Comparable setup | Cases, grader, subject and runtime settings that could change the result; reuse runner records |
| Judge | [Calibration](eval-grading.md#calibrate-a-judge), tolerances, abstention, adjudication |
| Budget | Tasks, repeats, attempts, repairs, candidate limit, time/token/cost ceiling |
| Comparison | Pairing, order, uncertainty method, exclusions, retries, missing data, stop rule |
| Decision | Keep rule and verdict rule |

## Cases from failures

1. Collect representative traces (production, dogfood, or synthetic starter); a domain expert notes each one's first clear failure. Fix a task set from real workloads, including hard and adversarial cases, before changing anything.
2. Cluster into a failure taxonomy with frequencies until new traces stop changing it; inspect rare high-impact failures separately.
3. One grader or case family per top failure mode, grouped by the observed mechanism; prioritize frequency and impact while keeping rare critical failures as guardrails.

- Keep representative samples apart from enriched stress tests.
- Cascading failures: fix or tag the first upstream break. Product or model shift: redo the analysis.

## Public benchmarks

- Reported public gain: audit transcripts, else weak evidence. Check it measures your skill.
- Check exposure through training, prompts or retrieval. Near-ceiling or uniformly failing tasks need solvability and grader checks before interpretation.
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
- Report paired deltas, per-slice regressions, numerators, denominators, uncertainty intervals. Binomial for independent binary trials; else paired cluster bootstrap over tasks or families, repeats inside their cluster. No interval makes a tiny or unrepresentative sample generalize.
- Never stop on a peeked win: fixed horizon or justified sequential or multiple-comparison procedure; log every candidate.
- Count solver failures, timeouts, infrastructure and judge errors, Unknowns, missing artifacts separately, with coverage; rerun affected pairs. Missing data never raises the pass rate.

## Verdict

| Verdict | When | Then |
|---|---|---|
| Accept | valid, comparable sealed evidence meets the effect and uncertainty rule and every critical guardrail | — |
| Revert | a valid comparison fails the rule or breaches a guardrail | — |
| Inconclusive | uncertainty or coverage cannot decide; proves nothing | collect more only under the plan, or start a new experiment |
| Invalid | leakage, changed harness, or environment mismatch; proves nothing | repair and rebaseline |
