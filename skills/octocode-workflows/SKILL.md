---
name: octocode-workflows
description: "Choose, compose, coordinate, resume or verify an agentic workflow, including whole-system audits, role-based investigations, runtime surgery and independent acceptance. Ordinary domain tasks use their owner skill directly."
---

# Octocode Workflows

Use the domain skill directly for ordinary tasks. For a workflow selection, choose one primary workflow from the [catalog](references/catalog.md) by the outcome or evidence needed. A selection alone needs no ledger.

## Execute the selected workflow

1. Follow the user's actor assignments and sequence. Before assigning roles, use [composition](references/composition.md#orchestrator-and-actor-contract). One accountable orchestrator owns scope, the sole ledger, verification and the final result. At least one responsible actor executes a bounded Task contract and returns raw observations keyed to its row.
2. Add a [role pattern](references/role-patterns.md) or evidence technique only for a distinct check or required capability within the user's authority. Keep requested work sequential. Only the orchestrator reassigns roles within that authority. Actors never spawn actors. Report a missing required capability and the affected check.
3. The orchestrator rechecks deciding anchors, merges observations and accepts or reassigns the row. Follow [verification](references/verification.md) for the five ledger states, invalidating dependent evidence and reconciling uncertain mutations before retry. Runtime claims require proof of the loaded artifact's identity. Apply the domain's stricter browser or sealed-evaluation safeguards. On [resume](references/composition.md#compose-and-resume), reconcile the existing ledger and assign the next outstanding row.

| Selected work | Procedure |
|---|---|
| Bounded execution or whole-system traversal | Assign one actor. The [BFS/DFS audit](references/single-actor-traversal.md) reconciles the inventory and traces each path without fan-out. |
| Change a running feature | [Runtime surgery](references/runtime-surgery.md) separates the code/architecture actor from the runtime actor that activates and replays the change. |
| Context growth, runtime resources, memory or caches | [Resource flow](references/resource-flow.md) maps the boundary, measures it and replays the correctness check. |
| Structural or semantic evidence in any workflow | [AST and LSP workflows](references/ast-lsp-workflows.md) distinguish syntax candidates from resolved identities and coverage gaps. |

## Use Octocode across workflows

The orchestrator and every actor may use Octocode directly, within their authority, in any workflow or role. Prefer it for supported repository, source, history and package evidence. Choose the capability from the [tool-use map](references/composition.md#octocode-tools-for-every-role) by the question. Use `octocode-research` for live schemas, exact reads, AST/LSP semantics and continuations. A dedicated research actor is optional.

## Related skills

Domain skills own methods, permissions and output. Load `octocode-architect` for code boundaries and change discipline, `octocode-agents-communication` for cross-session identity, leases or delivery, and `octocode-eval-benchmark` to measure a workflow or role split.

## Output

Follow the domain owner's format where applicable. Otherwise use [output.md](output.md) for a selection, completion or saved handoff and [render-report.mjs](scripts/render-report.mjs) for reports.
