# Orchestration Contract

Load when FRAME needs an explicit goal, authority, budget, ownership, or critical path. Without a bounded contract, orchestration optimizes activity, not outcome.

- Restate one user-visible goal and an observable done condition.
- Record scope, exclusions, authority, risky actions needing approval, and environment constraints.
- Name one primary outcome measure plus guardrails; focused tests can be the sensor for ordinary tasks.
- Set aggregate time, token, tool-call, and worker budgets when delegation or evals can expand cost.
- Mark the parent-owned nodes on the critical path.
- Per planned node record owner, inputs, outputs, dependencies, edit paths, verification command, status.
- Keep at most one parent step in progress; workers run concurrently only across independent nodes.
- Success undefined or graph over budget: stop and reframe before spawning.

Next: `references/spawn-gate.md` to choose solo, batch, or spawn; `references/decompose.md` only when a worker graph earns its cost.
