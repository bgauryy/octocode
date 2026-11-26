---
name: octocode-rfc-generator
description: "Load this skill first when asked to write, revise or review an RFC, design proposal or migration plan. Weigh alternatives, record each decision and trace it to tasks, checks and rollout."
---

# Octocode RFC Generator

Identify the decision, its owner and the requested deliverable. If the decision or owner is missing, ask; if no answer can come, mark the assumption and add a `Blocking:` question naming who settles it. For review, compare the existing RFC with current code and return findings. For a settled decision, write the requested implementation plan. For an open decision, keep the proposal and plan in one RFC; with no option to propose yet, first use `octocode-brainstorming` (web evidence) or `octocode-engineer` (codebase trade-offs). Read the [artifact structure](output.md) before drafting. Implementation requires its own authorization.

## Work loop

1. Frame the problem, users, current behavior, goals, constraints and prerequisites. Trace the entities and current flow from source; verify deciding claims in original sources and keep their links for Sources.
2. Decide: compare viable alternatives; for each real choice record evidence, reasoning, rejected options and what would reverse it. Try the strongest counterargument; keep unsupported likelihoods unknown.
3. Plan features (who observes which change), the proposed flow beside the current one, and tasks (what to make, where, after what, done when), thin end-to-end slice first. Name compatibility, acceptance, rollout and rollback for each material change. For risky plans, work backward from failure: what escapes containment or affects downstream consumers? Prefer a reversible trial when it resolves uncertainty.
4. Check real outcomes independently of success metrics, and that features, tasks, checks and sources trace: run `node <skill-folder>/scripts/rfc-trace.mjs <RFC.md>` and read it with the [trace rules](references/trace.md). Resolve decision blockers with evidence; otherwise deliver provisional direction, the gaps and the next deciding checks. Defer an execution detail only with an owner and a trigger that cannot reverse the decision.

## Conditional detail

| Need | Read or run |
|---|---|
| Completeness, planning features and tasks; tracing them; closing blockers or declaring ready | [Checklist](references/checklist.md); [trace](references/trace.md); [questions](references/questions.md) |
| A flow, diagram, graph or chart | [Diagrams](references/diagrams.md), then [drawing rules](references/diagram-drawing.md) |
| Evidence gathering, auditing an existing RFC, a disagreement source review cannot settle | [Workflow](references/workflow.md), with its optional `clasify` review |
| Locate an existing RFC or render accepted HTML | [Report commands](references/report-output.md#render-and-retrieve), [renderer](scripts/render-report.mjs) |

Save an authored RFC and reply per the [saved report rules](output.md#saved-report): decision, expected effect, risks and document path. Keep `status: Proposed` until the owner accepts.
