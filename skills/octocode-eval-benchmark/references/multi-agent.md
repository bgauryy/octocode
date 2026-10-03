# Multi-agent workflow evaluation
Load when the subject is a multi-stage or multi-agent workflow. Do not add agents or a global barrier only to fit the benchmark.

- Keep the production communication topology within a trial; block evaluator feedback and cross-trial leakage (`references/clean-lab.md`).
- Workers return result, status, and evidence. Partial results stay partial.
- Measure the user-visible outcome at the graph boundary. Per-node outcomes, stage latency, and routing or merge errors locate a failure. Total cost, time, collisions, and access violations are guardrails. More nodes or finished workers is not improvement.
- Compare against a simpler baseline.
- Map real data dependencies; a consumer waits only for outputs it needs.
- When the outcome drops, replay relevant nodes with frozen inputs. Attribute a defect only when local evidence reproduces it; separate bad upstream data, a failed node, and a faulty merge.
- Add verifiers or stages only for a measured failure.

| Failure mode | Sensor or fix |
|---|---|
| Race on shared state | Before fan-out, answer: where each agent works, who owns the merge, what happens on disagreement. No answer: fix isolation first; prompting cannot. |
| Goodhart | Pair each primary KPI with a guardrail the agent cannot tune. Primary up, guardrail down: stop and reframe. |
| Missing anchors | Executed tests with exit codes, deterministic verifiers, frozen criteria the subject cannot change. |
| Other | Opaque state, no checkpoint or resume, unbounded permissions, missing human gates. Add a suite case on first trace appearance. |
