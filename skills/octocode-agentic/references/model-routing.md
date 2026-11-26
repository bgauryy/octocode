Load when routing work across models or deciding when to escalate to a stronger one.

# Model routing

## Multi-model patterns

| Pattern | Architecture | Use when |
|---|---|---|
| Fast → strong | Small model for easy tasks, strong model for hard cases | The workload mixes easy and hard requests ([escalation policy](#escalation-policy)) |
| Planner → executor | Reasoning model plans; execution model runs bounded steps | A complex workflow splits into a plan and bounded steps; check the plan first ([break a task down](reasoning.md#break-a-task-down)) |
| Retriever → synthesizer | Light agent gathers context; strong model reasons | Research or code analysis where gathering context does not need the strong model |
| Generator → verifier | One produces; another checks against evidence | A wrong output has high consequences |
| Router → specialists | Router delegates to domain models | The workload spans different domains |
| Independent cross-check | Models analyze the same evidence separately | An error must be caught and each model gets the evidence, not the other's draft ([verify independently](grounding.md#reduce-unsupported-claims)) |
| Local → cloud | Local model for limited tasks, remote when needed | Privacy or cost limits what may go to the remote model |

## Escalation policy

| After the fast model classifies, routes or extracts | Next action |
|---|---|
| Its output passes the task's check | Return the verified result. |
| Evidence is missing | Retrieve it first ([evidence loop](evidence-loop.md#evidence-driven-loop)); a stronger model cannot supply it. |
| Evidence is present, but the reasoning is too complex for the fast model, the output fails the check, the model abstains, or an error would be irreversible | Escalate to a stronger model and rerun the same check. |
| A risky assertion still needs proof | Run the test or read the source; no extra model call. |
