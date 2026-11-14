---
name: octocode-rfc-generator
description: "Use when writing, revising or reviewing an RFC, design proposal, migration plan or implementation plan with decisions, alternatives, validation and rollout. Use octocode-documentation for settled explanations."
---

# Octocode RFC Generator

Pin the requested decision; when prior RFCs may own it, find them with `--manifest` ([commands](references/report-output.md#render-and-retrieve)). Review-only returns findings against current code. A settled decision with a plan-only request needs one plan. Otherwise keep the decision and implementation plan in one RFC; extra deliverables require a request. Writing an RFC grants no implementation authority.

## Work loop

1. Frame the problem, users, current behavior, goals and constraints. Verify deciding claims in original sources. Compare viable alternatives, including the status quo when useful.
2. Record the decision, evidence, consequences and what would reverse it. Try the strongest counterargument; keep unsupported likelihoods unknown.
3. Order implementation by dependency. Name compatibility, acceptance, rollout and rollback for each material change. For risky plans, work backward from failure: what escapes containment, affects downstream consumers or cannot roll back? Prefer a reversible trial when it resolves uncertainty.
4. Check real outcomes independently of success metrics. Resolve decision blockers with evidence; otherwise deliver provisional direction, the gaps and the next deciding checks. Execution details may be deferred only with an owner and a trigger that cannot reverse the decision.

## Conditional detail

| Need | Read or run |
|---|---|
| Evidence gathering, auditing an existing RFC or unresolved disagreement | [Workflow](references/workflow.md); use optional `clasify` review only when source review and direct checks cannot settle a consequential disagreement |
| Completeness, blocker closure or a useful diagram | [Checklist](references/checklist.md) |
| Octocode invocation or recovery | [MCP/CLI guide](references/octocode-usage.md) |
| Required sections, saving or rendering | [Output](output.md); [render-report.mjs](scripts/render-report.mjs) for accepted HTML or catalog discovery |

Keep `status: Proposed` until the owner accepts. A ready recommendation has no unresolved decision blocker; report remaining risks, confidence and unchecked scope. Save every authored RFC using [delivery](output.md#delivery) and lead with the proposal and effect. Render HTML when requested or accepted.

Neighbors: `octocode-research` verifies current behavior; `octocode-engineer` evaluates design trade-offs; `octocode-brainstorming` expands options; `octocode-documentation` explains settled decisions; `octocode-agentic` designs measurements; `octocode-workflows` designs workflows; `octocode-orchestrator` coordinates live agents.
