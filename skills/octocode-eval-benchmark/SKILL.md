---
name: octocode-eval-benchmark
description: "Evaluate whether code, prompts, models, workflows or skills improve measured outcomes. Design baseline comparisons, held-out cases, activation tests and calibrated graders; distinguish editorial review from behavioral evidence."
---

# Octocode eval benchmark

| Claim | Check |
|---|---|
| Editorial or structural | Review intent, links, examples and checks. Report behavior as unmeasured. |
| Deterministic regression | Reproduce the failure, pass the same check after the fix and verify affected contracts. |
| Comparative gain or reliability | Use a representative comparison below. Scale sample and budget to consequence and uncertainty. |
| The host activates the right skill | Follow the [skill activation procedure](references/skill-activation.md). Record which skill the target host loads first, including none. A description-only check measures routing. |

## Run a comparison

1. Freeze the [KPI contract](references/design.md#kpi-contract) and [splits](references/design.md#splits-and-exposure) before comparing. [Public benchmarks](references/design.md#public-benchmarks) orient. Decide on representative private tasks. Version harness, suite or grader changes and reassess both arms. Only a grader-only fix may reuse sealed outputs with all required evidence.
2. Isolate solvers from evaluation material using the [harness and access rules](references/harness.md). Give both arms their deployment instructions. Answer-specific coaching leaks the answer.
3. Prefer deterministic outcome checks. [Test graders and calibrate judges](references/grading.md) before freezing them. Account for failures, Unknowns, errors and retries in the denominator and report their cost.
4. Iterate on development tasks using the [Keep/Discard loop](references/loop.md#keepdiscard-loop). Both labels are provisional. Select on validation before opening the sealed test. Confirm the selected candidate once, with both arms under identical conditions.
5. Assign the [final verdict](references/design.md#verdict) of `Accept`, `Revert`, `Inconclusive` or `Invalid`. Stop after a second Inconclusive or Invalid final run on fresh or unexposed tasks. Report the limit and the next measurement needed.

## Related skills

Use `octocode-research` and the [MCP/CLI guide](references/octocode-usage.md) for repository evidence, `octocode-agentic-prompts` for instruction failures, `octocode-skills` for skill structure or descriptions and [workflow composition](../octocode-workflows/references/composition.md) for a coordinated evaluation.

## Output

Follow [output.md](output.md) for responses and run folders. Save every measured run, including smoke tests. Use [render-report.mjs](scripts/render-report.mjs) to render or list reports.
