# Orchestration Contract

Load when FRAME needs an explicit goal, authority, budget, ownership, or critical path. Without a bounded contract, orchestration optimizes activity, not the outcome.

## Frame

- Restate one user-visible goal and an observable done condition.
- Record scope, exclusions, authority, risky actions that need approval, and environment constraints.
- Name one primary outcome measure plus guardrails. Ordinary tasks can use focused tests as the sensor.
- Set aggregate time, token, tool-call, and worker budgets when delegation or evals can expand cost.
- Name the parent-owned critical path: user decisions, integration, irreversible actions, and final evidence.

## Worker contract

- For each planned node, record owner, inputs, outputs, dependencies, edit paths, verification command, and status.
- Keep at most one parent step in progress. Workers run concurrently only across independent nodes.
- Never delegate approval, permission escalation, user intent, or the final verdict.
- If success is undefined, authority is missing, or the graph cannot fit its budget, stop and reframe before spawning.

Next: load `references/spawn-gate.md` to choose solo, batch, or spawn; load `references/decompose.md` only when a worker graph earns its cost.
