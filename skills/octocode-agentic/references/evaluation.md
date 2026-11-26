Load first for any measurement claim: match the claim to its check and follow the comparison steps to the detail pages.

# Evaluation

## Match the claim to the check

| Claim | Check |
|---|---|
| Editorial or structural | Review intent, links, examples and checks. Report behavior as unmeasured. |
| Deterministic regression | Reproduce the failure, pass the same check after the fix and verify affected contracts. |
| Comparative gain or reliability | Use a representative comparison below. Scale sample and budget to consequence and uncertainty, and fix them in the [KPI contract](eval-kpi.md#kpi-contract) before the baseline. |
| The host activates the right skill | Follow the [skill activation procedure](skill-activation.md). Record which skill the target host loads first, including none. A description-only check measures routing. |

## Metrics

Optimize verified outcomes per cost, latency and resources; track correctness, safety and critical regressions separately so savings cannot hide failures. Choose metrics that can change the decision: verified success, false completion, unsupported claims, retrieval coverage, unnecessary calls, tokens/cost per successful task, latency tails, cache reuse, continuation fidelity or retry/delegation overhead. Activity and shorter output are not success.

For repeated-trial success metrics (`pass@1`, `pass@k`, `pass^k`), use [grading metrics](eval-grading.md#metrics).

## Run a comparison

1. Freeze the [KPI contract](eval-kpi.md#kpi-contract) and [splits](eval-cases.md#splits-and-exposure) before comparing. [Public benchmarks](eval-cases.md#public-benchmarks) orient. Decide on representative private tasks. Version harness, suite or grader changes and reassess both arms ([grader-only fix](eval-diagnose.md#diagnose-failures)).
2. Isolate solvers from evaluation material using the [access rules](eval-isolation.md#roles-and-access). Give both arms their deployment instructions. Answer-specific coaching leaks the answer.
3. Prefer deterministic outcome checks. [Test graders and calibrate judges](eval-grading.md) before freezing them, and count failures as [comparable measurements](eval-kpi.md#comparable-measurements) describes.
4. Iterate on development tasks using the [Keep/Discard loop](eval-loop.md#keepdiscard-loop). Both labels are provisional. Select on validation before opening the sealed test.
5. Assign the [final verdict](eval-kpi.md#verdict) of `Accept`, `Revert`, `Inconclusive` or `Invalid`. Stop under the declared budget and stopping rule; report uncertainty and the next measurement needed.

## Baseline and observability

Record verified success, false completions, calls, tokens, cost and latency per baseline task. Use existing traces to locate a failure or support a claimed measurement; add only missing evidence ([reason-aware invocation](tool-descriptions.md#reason-aware-invocation)).
