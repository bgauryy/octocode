---
name: octocode-rfc-generator
description: "Use when consequential architecture, migration, public-contract, or multi-phase changes need a reviewed decision. Not for open-ended ideation or trivial edits."
---

# Octocode RFC Generator

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Produce evidence-backed decisions that an implementer and reviewer can execute.
Flow: `UNDERSTAND → RESEARCH → PREREQUISITES → COMPARE → WRITE → CLOSE QUESTIONS → KPI → VALIDATE → DELIVER`.
For an existing RFC, use the reassessment route in `references/workflow.md` and audit against live code rather than prior checkboxes.

RFCs: `<output>/rfc/`; scratch: `<output>/tmp/octocode-rfc-generator/`. Chat-only proposals stay in chat; approved source edits keep their named paths.

## Lobby rules
- Skip RFC mode for trivial edits. Ask one focused question when uncertainty changes shape, owner, scope, or decision criteria.
- Compare the viable alternatives and include the status quo when it is a real option; skip ceremonial options that cannot satisfy the decision.
- Recommendations require verifiable facts; cite exact anchors and commands/checks that ran.
- `RFC.md` owns goals, scope, and decision. Other files link to its anchors rather than restating them.
- Resolve decision-blocking questions with evidence. Mark other uncertainty with confidence, impact, owner, and either a proof trigger or a deferral trigger.
- Order implementation by dependency, not estimates; bind requirements to acceptance and verification.
- Reassessing `.octocode/rfc/` requires fresh reads of live code and a dated audit result. Write the `## Audit Reasoning` block into the RFC only when source edits are authorized; otherwise return it in chat. <!-- style-lint: ignore-line passive-voice -->
- Never assert RFC status from memory or from another RFC's claims.
- Stop when the work is a trivial edit; a brainstorming handoff is not RFC-ready; uncertainty changes artifact shape, owner, scope, or tradeoff priority; another research pass is unlikely to close a blocker; independent decisions need separate RFCs; or a save, edit, delete, or archive awaits approval.

## Artifact route
Start with `RFC.md`. Add `PREREQUISITES.md`, `IMPLEMENTATION.md`, `KPI.md`, or `RESOURCES.md` only when readiness, execution, measurement, or source volume needs its own lifecycle. After save approval, place the set under `<output>/rfc/{name}/`.

## Smart routes — load only what the current step needs
- To understand the ask and select a mode before drafting, load `references/workflow.md` — gates, claim ledger, artifact set, traceability, validation, and delivery order.
- When researching evidence, use `octocode-research`; if unavailable, use `octocode-mcp` or `npx octocode`, inspect live context/schema once, and follow executable continuations. Then load `references/research-playbook.md` to keep claims auditable.
- When existing code has readiness work, load `references/rfc-prerequisites.md` before planning — expose baselines, blockers, owners, and setup.
- When comparing options and writing the decision, load `references/rfc-template.md` — structure alternatives, goals/non-goals, reversibility, and pre-mortem.
- When building the execution plan, load `references/rfc-implementation.md` — close open questions, order dependencies, and define rollout/rollback.
- When defining acceptance and KPI targets, load `references/rfc-kpi.md` — connect user stories, metrics, decision rules, and verification in a traceability matrix.
- When preserving sources, load `references/rfc-resources.md` — record provenance without moving decisive citations out of the RFC.
- When you reassess, rate, or clean up an existing RFC, load `references/rfc-audit.md` — produce a dated audit result with live-code evidence before any keep/fix/delete recommendation.
- When improving this skill, use `octocode-eval-benchmark`; if unavailable, record goal, KPI, baseline, smallest change, comparable result, and accept/revert.

## Related routes and verification
- Use `octocode-brainstorming` before RFC when worth-building is unresolved and `octocode-eval-benchmark` for KPI rigor. To close factual questions, `octocode-research` owns the MCP/CLI workflow and live tool/grammar discovery. <!-- style-lint: ignore-line passive-voice -->
- Use `octocode-skills` when changing this skill folder.
- Before delivery validate the document contract section by section and report the real result.
