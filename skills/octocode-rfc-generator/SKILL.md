---
name: octocode-rfc-generator
description: "Use when a consequential change needs a decision before coding: write or improve an RFC, design doc, architecture proposal, migration plan, option comparison, rollout plan, or measurable implementation contract."
---

# Octocode RFC Generator

Produce evidence-backed decisions that an implementer and reviewer can execute. Flow:
`UNDERSTAND → RESEARCH → PREREQUISITES? → COMPARE → WRITE → CLOSE QUESTIONS → KPIs → VALIDATE → DELIVER`.
For existing RFCs, run `REASSESS` (see `references/workflow.md` § Reassess existing RFCs) instead of WRITE — audit against live code, not prior checkboxes.

## Lobby rules
- Skip RFC mode for trivial edits. Ask one focused question when uncertainty changes shape, owner, scope, or decision criteria.
- Compare at least two alternatives, including do-nothing, unless the user explicitly requests one implementation plan.
- Recommendations require verifiable facts; cite exact anchors and commands/checks that actually ran.
- `RFC.md` owns goals, scope, and decision. Other files link to its anchors rather than restating them.
- Resolve every open question with evidence or an explicit deferral; no recommendation may rest on an uncertain claim.
- Order implementation by dependency, not estimates; bind requirements to acceptance and verification.
- Reviewing, rating, or cleaning up `.octocode/rfc/` (delete/implemented/fixed calls) always requires a dated `## Audit Reasoning` block written into the RFC itself, backed by fresh reads of the live code.
- Never assert RFC status from memory or from another RFC's claims.

## Artifact route
- Small, reversible, single-package work: produce only `RFC.md` with plan, acceptance, and inline references.
- Otherwise, after save approval, create `<workspace>/.octocode/rfc/{name}/`: `RFC.md` (decide), `PREREQUISITES.md` (ready, existing code only), `IMPLEMENTATION.md` (build), `KPI.md` (verify), and `RESOURCES.md` (source appendix).

## Smart routes — load only what the current step needs
- Before drafting, load `references/workflow.md` — select mode, gates, claim ledger, artifact set, traceability, validation, and delivery.
- When gathering evidence, load `references/octocode.md` then `references/research-playbook.md` — delegate exact research and keep claims auditable.
- When writing the decision, load `references/rfc-template.md` — structure options, goals/non-goals, reversibility, and pre-mortem.
- When existing code has readiness work, load `references/rfc-prerequisites.md` before planning — expose baselines, blockers, owners, and setup.
- When building the execution plan, load `references/rfc-implementation.md` — close questions, order dependencies, and define rollout/rollback.
- When defining acceptance, load `references/rfc-kpi.md` — connect user stories, metrics, decision rules, and verification in a traceability matrix.
- When preserving sources, load `references/rfc-resources.md` — record provenance without moving decisive citations out of the RFC.
- When improving this skill, prefer `octocode-eval`; otherwise load `references/improve-loop.md` — enforce measurable accept/revert.

## Related routes and verification
- Use `octocode-brainstorming` before RFC when worth-building is unresolved; `octocode-research` to close factual questions; `octocode-eval` for KPI rigor.
- Use `octocode-awareness` while coordinating drafts; `octocode-skills` when changing this skill folder.
- Before delivery run `scripts/eval-rfc.mjs --case <id>` — validate the document contract and report the real result.
