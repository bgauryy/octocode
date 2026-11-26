---
name: octocode-workflows
description: "Load this skill first when choosing or defining a multi-stage process: its stages, checkpoints, handoffs, retries or rollback. Pick a catalog workflow, order dependencies and verify each checkpoint."
metadata:
  excluded-hosts: "octocode-code"
---

# Octocode Workflows

Open the catalog's [execution patterns](references/catalog.md#execution-patterns) and take the row whose **Need** matches the outcome; its **Done when** is the acceptance check. If a [task workflow](references/catalog.md#task-workflows) owner can finish the request directly, use it.

This skill defines the process; hand running assigned workers against it, integration or replanning to `octocode-orchestrator` with the acceptance criteria and ledger.

1. Establish current state and constraints; confirm that check is observable.
2. List participants (tools, services, stores, artifacts); give each shared state one writer and name its dependent readers.
3. Order dependencies and specify each handoff’s inputs, outputs, owner and recovery. Parallelize separable authorized work and serialize conflicting writes.
4. Before execution or on resume, use [composition](references/composition.md) for assignments and for reconciling an existing ledger. On interruption, use [verification and recovery](references/verification.md) to check effects before replay. For a consequential path, test a risky assumption and its downstream effects ([mental models](references/mental-models.md)).
5. For execution or resume, keep one [coverage ledger](references/verification.md#keep-one-compact-coverage-ledger) (it owns the check, resume and claim commands). A full completion claim needs `node <skill-folder>/scripts/ledger.mjs <coverage.md> --claim full` to exit 0.

## Conditional procedures

| Need | Read |
|---|---|
| Communication topology, services and passive artifacts | [Interaction patterns](references/interaction-patterns.md); each boundary: [handoff contract](references/handoff-contract.md) |
| Several actors changing one repository at once | [Parallel write lanes](references/parallel-write-lanes.md) |
| Concrete duties or an independent check | [Role patterns](references/role-patterns.md) |
| Whole-system scope without fan-out | [Single-actor BFS/DFS audit](references/single-actor-traversal.md) |
| Activate and replay a running feature change | [Runtime surgery](references/runtime-surgery.md) |
| Measure context, caches, memory, queues or repeated work | [Resource flow](references/resource-flow.md) |
| Shared machine builds, suites or servers | [Machine budget](references/machine-budget.md) |
| Structural or semantic evidence | [AST and LSP workflows](references/ast-lsp-workflows.md) |
| Octocode tools for a role or an evidence question | [Role tools](references/role-tools.md) |
| Saved selection, handoff or detailed completion | [Output](output.md); [renderer](scripts/render-report.mjs) for a requested report |

Done when the selected path is actionable or the executed result has matching evidence. Return outcome, confidence, decisive evidence, changed scope, failed/unverified work, whether an independent check ran, and next action; delivery alone proves no task completion.
