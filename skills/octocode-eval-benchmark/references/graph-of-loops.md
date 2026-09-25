# Graph evaluation
Load when evaluating a workflow with multiple stages or agents. Why: an overall score can hide where a failure originates.

Measure the user-visible outcome at the graph boundary. Use node outcomes and stage latency to diagnose it; track total cost, latency, access scope and conflicts as relevant guardrails. Do not equate more nodes with better coverage.

Map actual dependencies: a consumer waits when it needs an upstream result. Independent stages may run concurrently when resources allow. A sequential specialist pipeline can still be useful; lack of parallelism alone does not make its graph invalid. Test proposed topology changes against a simpler baseline.

When the overall outcome drops, replay relevant nodes with frozen inputs. Attribute a defect only when local evidence reproduces it; distinguish bad upstream data, a failed node and a faulty merge. Check operational state and executable outcomes where possible, calibrated judgment where necessary.

Choose additional verifiers or specialized stages only when a measured failure justifies them. Preserve production communication and permissions during an evaluation; do not impose a topology or global synchronization barrier simply to fit the benchmark.

Next: worker roles/communication → `subagent-cookbook.md`; shared-state and metric risks → `graph-failure-modes.md`; subject improvement → `agent-loop.md`.
