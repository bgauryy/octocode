# Multi-agent workflow evaluation
Load when the subject is a multi-stage or multi-agent workflow. An overall score can hide where a failure starts; topology alone does not buy correctness.

This skill owns evaluation; `octocode-subagent` owns spawn mechanics and topology. Do not add agents or a global barrier only to fit the benchmark.

## Roles
- Developer, scored solver, and judge keep separate conversations and storage (`references/clean-lab.md`).
- Keep legitimate production messages within a trial. Block evaluator feedback and cross-trial leakage.
- Workers return result, status, and evidence to verify it. Partial results stay partial.

## Measure and attribute
- Measure the outcome a product user sees at the graph boundary. Per-node outcomes, stage latency, and routing or merge errors locate a failure. Total cost, time, collisions, and access violations are guardrails. More nodes or finished workers is not improvement.
- Compare against a simpler baseline on the same task snapshots and budgets. Final acceptance uses `references/held-out-and-guards.md`.
- Map real data dependencies; a consumer waits only for outputs it needs.
- When the outcome drops, replay relevant nodes with frozen inputs. Attribute a defect only when local evidence reproduces it; separate bad upstream data, a failed node, and a faulty merge.
- Add verifiers or stages only when a measured failure justifies them.

## Failure modes
| Mode | Sensor or fix |
|---|---|
| Shared context | The judge starts fresh with task, rubric, sealed evidence. Inherited executor instructions are not judge authority; a fresh judge is not automatically right (`references/llm-judge.md`). |
| Race on shared state | Before fan-out, answer: where each agent works, who owns the merge, what happens on disagreement. Missing answer: fix isolation first; prompting cannot. |
| Goodhart | Pair each primary KPI with a guardrail the agent cannot tune. Primary up, guardrail down: stop and reframe. |
| Missing anchors | Executed tests with exit codes, deterministic verifiers, frozen criteria the subject cannot change. |
| Other | Opaque state, no checkpoint or resume, unbounded permissions, missing human gates. Add a suite case on first trace appearance. |

Next: run records → `benchmarks/README.md`; subject improvement → `references/agent-loop.md`.
