---
name: octocode-workflows
description: "Load this skill first when designing, running or resuming a multi-stage process. Choose a workflow, map dependencies and handoffs, and verify checkpoints, retries or rollback."
metadata:
  excluded-hosts: "octocode-code"
---

# Octocode Workflows

State the outcome and find its owner in the [workflow catalog](references/catalog.md). If the domain skill can complete the request directly, use it. For selection only, return the chosen path and why it fits. For execution or resume, reuse the existing [coverage ledger](references/verification.md#keep-one-compact-coverage-ledger).

Use `octocode-orchestrator` when ongoing agent assignments, integration or replanning are needed. Keep the workflow’s acceptance criteria and ledger through the handoff.

1. Establish current state, constraints and the observable acceptance check.
2. Identify participants, including tools, services, stores and artifacts. Give each shared state one writer and name the readers that depend on it.
3. Order dependencies and specify each handoff’s inputs, outputs, owner and recovery. Parallelize separable authorized work and serialize conflicting writes. State whether an independent check was available.
4. Before execution, use [composition](references/composition.md) for assignments. On interruption, use [verification and recovery](references/verification.md) to check effects before replay. For a consequential path, test a failed assumption and its downstream effects. Use [mental models](references/mental-models.md) when selecting that check.

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
