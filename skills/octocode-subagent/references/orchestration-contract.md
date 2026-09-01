# Orchestration Contract

Load when FRAME needs an explicit goal, authority, budget, ownership, or critical path. Why: orchestration without a bounded contract optimizes activity instead of the user's outcome.

## Frame

- Restate one user-visible goal and observable done condition.
- Record scope, exclusions, authority, risky actions requiring approval, and environment constraints.
- Name one primary outcome measure plus guardrails; ordinary tasks may use focused tests as the sensor.
- Set aggregate time, token, tool-call, and worker budgets when delegation or evals can materially expand cost.
- Identify the parent-owned critical path: user decisions, integration, irreversible actions, and final evidence.

## Working contract

For each planned node record owner, inputs, outputs, dependencies, edit paths, verification command, and status. Keep at most one parent step in progress; workers may run concurrently only across independent nodes.

Never delegate approval, permission escalation, user intent, or the final verdict. If success is undefined, authority is missing, or the graph cannot fit its budget, stop and reframe before spawning.

Next: load `references/spawn-gate.md` to choose solo/batch/spawn, then `references/decompose.md` only when a worker graph earns its cost.
