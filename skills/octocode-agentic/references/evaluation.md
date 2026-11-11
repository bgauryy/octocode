Load when choosing agent metrics, matching a claim to its check, trading efficiency against quality, or proving that a change helped.

# Evaluation

## Match the claim to the check

| Claim | Check |
|---|---|
| Editorial or structural | Review intent, links, examples and checks. Report behavior as unmeasured. |
| Deterministic regression | Reproduce the failure, pass the same check after the fix and verify affected contracts. |
| Comparative gain or reliability | Use a representative comparison below. Scale sample and budget to consequence and uncertainty. |
| The host activates the right skill | Follow the [skill activation procedure](skill-activation.md). Record which skill the target host loads first, including none. A description-only check measures routing. |

## Objective

```text
agent efficiency = verified successful outcomes / (cost + time + resource usage)
```

A design objective, not one literal number. Track correctness and safety separately so cost savings cannot hide quality regressions. Measure verified completion, not activity.

## Dimensions

Efficiency, verbosity and hallucination are separate dimensions; improving one can hurt another.

| Dimension | Goal | Anti-pattern |
|---|---|---|
| Token efficiency | Minimal necessary context and output | Repeated full files |
| Reasoning efficiency | Enough analysis for correctness | Unnecessary deliberation |
| Tool efficiency | No unnecessary calls | Repeated broad searches |
| Latency efficiency | Short critical path | Serial independent tasks |
| Cost efficiency | Model capability matched to the task | Strongest model for trivial work |
| Evidence quality | Claims backed by reliable sources | Unverified conclusions |
| Answer conciseness | What the consumer needs | Long process narration |
| Correctness | Actual requirements met | Plausible but superficial output |
| Recovery efficiency | Resume without repeating work | No checkpoints |
| Operational safety | Limited unintended effects | Unbounded autonomous actions |

## Metrics

| Metric | Measures |
|---|---|
| Task success rate | Completed tasks meeting criteria |
| Verified success rate | Success backed by external checks |
| False completion rate | Success claims without satisfaction |
| Hallucination rate | Unsupported or false claims |
| Tool selection accuracy | Correct capability choice |
| Unnecessary tool-call rate | Avoidable tool activity |
| Retrieval precision / recall | Evidence quality and coverage |
| Context utilization | Relevant evidence per context cost |
| Cache hit rate | Reuse of eligible cached context |
| Tokens per successful task | Efficiency adjusted for quality |
| Cost per verified task | Economic efficiency |
| p50 / p95 / p99 latency | Typical and tail response time |
| Compaction fidelity | Essential state preserved |
| Continuation success | Correctness after resuming |
| Delegation overhead | Cost and latency of subagents |
| Retry amplification | Repeated execution caused by failures |
| Security violation rate | Unauthorized or unsafe actions |
| Regression rate | Previously passing tasks that now fail |

For repeated-trial success metrics (`pass@1`, `pass@k`, `pass^k`), use [grading metrics](eval-grading.md#metrics).

## Run a comparison

1. Freeze the [KPI contract](eval-design.md#kpi-contract) and [splits](eval-design.md#splits-and-exposure) before comparing. [Public benchmarks](eval-design.md#public-benchmarks) orient. Decide on representative private tasks. Version harness, suite or grader changes and reassess both arms. Only a grader-only fix may reuse sealed outputs with all required evidence.
2. Isolate solvers from evaluation material using the [harness and access rules](eval-harness.md). Give both arms their deployment instructions. Answer-specific coaching leaks the answer.
3. Prefer deterministic outcome checks. [Test graders and calibrate judges](eval-grading.md) before freezing them. Account for failures, Unknowns, errors and retries in the denominator and report their cost.
4. Iterate on development tasks using the [Keep/Discard loop](eval-loop.md#keepdiscard-loop). Both labels are provisional. Select on validation before opening the sealed test. Confirm the selected candidate once, with both arms under identical conditions.
5. Assign the [final verdict](eval-design.md#verdict) of `Accept`, `Revert`, `Inconclusive` or `Invalid`. Stop after a second Inconclusive or Invalid final run on fresh or unexposed tasks. Report the limit and the next measurement needed.

## Baseline and observability

Record verified success, false completions, calls, tokens, cost and latency per baseline task. Use [cases from failures](eval-design.md#cases-from-failures), the [Keep/Discard loop](eval-loop.md#keepdiscard-loop) and [comparable measurements](eval-design.md#comparable-measurements) for task selection, one-factor changes, paired comparisons and regressions.

Trace observability per call: task and step IDs, tool, reason, duration, tokens, result status and next decision ([reason-aware invocation](tools-and-mcp.md#reason-aware-invocation)). An LLM judge can grade, but cannot replace compilation, tests, runtime evidence or authoritative data.

## Evidence and delivery

Use `octocode-research` and the [MCP/CLI guide](octocode-usage.md) for repository evidence, [prompt writing](prompt-writing.md) for instruction failures, `octocode-skills` for skill structure or descriptions and `octocode-workflows` for a coordinated evaluation.

Follow [output.md](../output.md) for responses and run folders. Save every measured run, including smoke tests. Use [render-report.mjs](../scripts/render-report.mjs) to render or list reports.
