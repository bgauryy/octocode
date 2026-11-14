---
name: octocode-workflows
description: "Use when designing, running or resuming a multi-step process with ordered stages, dependencies, retries, checkpoints or rollback, such as pipelines, migrations, releases or data jobs across tools and services. Load it before scripting the steps. Use octocode-orchestrator to coordinate live agents."
---

# Octocode Workflows

Choose one primary workflow from the [catalog](references/catalog.md) by the needed outcome or proof. Ordinary tasks use their domain skill directly; a workflow selection needs no ledger or extra agent.

For ongoing agent assignments, result integration and replanning, use `octocode-orchestrator` when installed. Keep this workflow's acceptance and existing ledger; the coordinator does not create a second one.

1. State the observable outcome, current state, constraints and acceptance check.
2. Name participants and capabilities: agents, tools, services, processes, stores or artifacts. Give each shared state a writer and every passive object a reader/writer.
3. Order dependencies and define handoff inputs, outputs, ownership and recovery. Parallelize separable authorized work; serialize conflicting writes. One agent may carry several duties; disclose when a check is not independent.
4. Before execution, use [composition](references/composition.md) for assignments and one reusable ledger; [verification](references/verification.md) defines evidence states and recovery. A task contract names scope, authority, inputs, output and completion. Delegation requires a useful capability, evidence lane or independent check; the orchestrator owns integration and the final result.

For consequential paths, test a failed assumption, harmful downstream effect and interruption. Prefer reversible actions; reconcile uncertain effects before replay. On resume, update from current state. Judge the intended outcome and containment separately from scores or worker completion. [Mental models](references/mental-models.md) offer additional deciding checks.

## Conditional procedures

| Need | Read |
|---|---|
| Communication topology, including services and passive artifacts | [Interaction patterns](references/interaction-patterns.md) |
| Concrete duties or an independent check | [Role patterns](references/role-patterns.md) |
| Whole-system scope without fan-out | [Single-actor BFS/DFS audit](references/single-actor-traversal.md) |
| Activate and replay a running feature change | [Runtime surgery](references/runtime-surgery.md) |
| Measure context, caches, memory, queues or repeated work | [Resource flow](references/resource-flow.md) |
| Shared machine builds, suites or servers | [Machine budget](references/machine-budget.md) |
| Structural or semantic evidence | [AST and LSP workflows](references/ast-lsp-workflows.md) |
| Saved selection, handoff or detailed completion | [Output](output.md); run [renderer](scripts/render-report.mjs) only for the requested report |

## Use Octocode across workflows

Every role may use available Octocode tools within its authority; see the [tool-use map](references/composition.md#octocode-tools-for-every-role). `octocode-research` supplies repository methods, `octocode-engineer` code trade-offs and `octocode-agentic` coordination setup or measured role comparisons, when installed. Domain acceptance, permissions and output remain authoritative.

Done when the selected path is actionable or the executed result has matching evidence. Return outcome, confidence, decisive evidence, changed scope, failed/unverified work and next action; delivery alone proves no task completion.
