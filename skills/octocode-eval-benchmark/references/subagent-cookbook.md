# Evaluating subagents
Load when the subject is a multi-agent workflow. Why: preserve real collaboration while isolating trials and measuring the final outcome.

This skill owns evaluation; `octocode-subagent` owns authorized spawn mechanics and topology choices. Do not add agents merely to satisfy an evaluation pattern.

## Roles and communication
- A developer may study development failures and propose a candidate. A scored solver gets the production task and permitted raw inputs. A judge gets the sealed artifact/evidence and rubric. Keep these conversations and storage permissions separate (`clean-lab.md`).
- Preserve legitimate production messages within a trial; block evaluator feedback and cross-trial leakage. Recheck consequential claims against evidence before using them downstream.
- Identify real data dependencies and write ownership. Run independent work concurrently only when resources and permissions allow it. Consumers wait for the outputs they actually need; partial results remain partial. Streaming workflows need not impose a global barrier.
- Return the result, status and enough evidence to verify it. Do not impose arbitrary anchor counts or a fixed packet format unless the consumer needs one. Host approval/auth requirements still apply.

## Measurement
Measure user-visible quality at the graph boundary. Use per-node outcomes, stage latency and routing/merge errors to locate a failure; track total cost, time, collisions and access violations as relevant guardrails. Finishing more workers is not an improvement metric.

Compare the same task snapshots and budgets. Establish baseline evidence before changing the subject. Reproduce a suspected node failure with frozen inputs before attributing the overall result to it. Apply the final acceptance rule in `held-out-and-guards.md`; development KEEP remains provisional.

Use outcome checks where possible and calibrated judges where needed. A fresh verifier is less exposed to the executor's framing but is not automatically correct. Judge a trajectory as evidence only when process constraints matter.

Next: dependency and attribution detail → `graph-of-loops.md`; structural risks → `graph-failure-modes.md`; run records → `benchmarks/README.md`.
