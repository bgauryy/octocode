---
name: octocode-rfc-generator
description: "Load this skill first when asked to write, revise or review an RFC, design proposal, migration or implementation plan. Trace decisions and alternatives to tasks, checks and rollout."
---

# Octocode RFC Generator

Identify the decision, its owner and the requested deliverable. For review, compare the existing RFC with current code and return findings. For a settled decision, write the requested implementation plan. For an open decision, keep the proposal and plan in one RFC. Read the [artifact structure](output.md) before drafting. Implementation requires its own authorization.

## Work loop

1. Frame the problem, users, current behavior, goals, constraints and prerequisites. Trace the entities and current flow from source; verify deciding claims in original sources and keep their links for Sources. Put terms a newcomer could misread in the glossary under their code names.
2. Decide: compare viable alternatives, the status quo included when useful; for each real choice record evidence, reasoning, rejected options and what would reverse it. Try the strongest counterargument; keep unsupported likelihoods unknown.
3. Plan features (who observes which change), the proposed flow beside the current one, and tasks (what to make, where, after what, done when), thin end-to-end slice first. Name compatibility, acceptance, rollout and rollback for each material change. For risky plans, work backward from failure: what escapes containment, affects downstream consumers or cannot roll back? Prefer a reversible trial when it resolves uncertainty.
4. Check real outcomes independently of success metrics, and that features, tasks, checks and sources trace: run `node <skill-folder>/scripts/rfc-trace.mjs <RFC.md>` and read it with the [trace rules](references/checklist.md#trace). Resolve decision blockers with evidence; otherwise deliver provisional direction, the gaps and the next deciding checks. Defer an execution detail only with an owner and a trigger that cannot reverse the decision.

## Conditional detail

| Need | Read or run |
|---|---|
| Planning features and tasks, tracing them, closing blockers | [Checklist](references/checklist.md) |
| A flow, diagram, graph or chart | [Diagrams](references/diagrams.md) |
| Evidence gathering, auditing an existing RFC, unresolved disagreement | [Workflow](references/workflow.md); optional `clasify` review only when source review and direct checks cannot settle a consequential disagreement |
| Locate an existing RFC or render accepted HTML | Use the [report commands](references/report-output.md#render-and-retrieve) and [renderer](scripts/render-report.mjs). |

Save an authored RFC using the [saved report rules](output.md#saved-report). Keep `status: Proposed` until the owner accepts. Identify any `Blocking:` questions, who can settle them and the next check. Return the decision, expected effect, risks and document path.
