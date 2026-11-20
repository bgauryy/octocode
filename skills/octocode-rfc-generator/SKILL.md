---
name: octocode-rfc-generator
description: "Load this skill first when asked to write, revise or review an RFC, design proposal, migration plan or implementation plan: it frames decisions with evidence, alternatives and tasks traced to checks and rollout."
---

# Octocode RFC Generator

Pin the requested decision; when prior RFCs may own it, find them with `--manifest` ([commands](references/report-output.md#render-and-retrieve)). Review-only returns findings against current code. When the decision is already made and the user asks only for the plan, write one plan. Otherwise keep the decision and implementation plan in one RFC; extra deliverables require a request. Writing an RFC grants no implementation authority. Read [Output](output.md) before drafting.

## Work loop

1. Frame the problem, users, current behavior, goals, constraints and prerequisites. Trace the entities and current flow from source; verify deciding claims in original sources and keep their links for Sources. Put terms a newcomer could misread in the glossary under their code names.
2. Decide: compare viable alternatives, the status quo included when useful; for each real choice record evidence, reasoning, rejected options and what would reverse it. Try the strongest counterargument; keep unsupported likelihoods unknown.
3. Plan features (who observes which change), the proposed flow beside the current one, and tasks (what to make, where, after what, done when), thin end-to-end slice first. Name compatibility, acceptance, rollout and rollback for each material change. For risky plans, work backward from failure: what escapes containment, affects downstream consumers or cannot roll back? Prefer a reversible trial when it resolves uncertainty.
4. Check real outcomes independently of success metrics, and that features, tasks, checks and sources trace. Resolve decision blockers with evidence; otherwise deliver provisional direction, the gaps and the next deciding checks. Defer an execution detail only with an owner and a trigger that cannot reverse the decision.

## Conditional detail

| Need | Read or run |
|---|---|
| Planning features and tasks, tracing them, closing blockers | [Checklist](references/checklist.md) |
| A flow, diagram, graph or chart | [Diagrams](references/diagrams.md) |
| Evidence gathering, auditing an existing RFC, unresolved disagreement | [Workflow](references/workflow.md); optional `clasify` review only when source review and direct checks cannot settle a consequential disagreement |
| Octocode invocation or recovery | [MCP/CLI guide](references/octocode-usage.md) |
| Accepted HTML or catalog discovery | [render-report.mjs](scripts/render-report.mjs) |

Keep `status: Proposed` until the owner accepts; a Proposed RFC may list `Blocking:` questions and is ready for acceptance only when none remain. Save every authored RFC per the [saved report](output.md#saved-report) rules, lead with the proposal and effect, and report remaining risks, confidence and unchecked scope.

Neighbors: `octocode-research` verifies current behavior; `octocode-engineer` evaluates design trade-offs; `octocode-brainstorming` expands options; `octocode-documentation` explains settled decisions; `octocode-agentic` designs measurements; `octocode-workflows` designs workflows; `octocode-orchestrator` coordinates live agents.
