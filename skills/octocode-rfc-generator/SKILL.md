---
name: octocode-rfc-generator
description: "Use when consequential architecture, migration, public-contract, or multi-phase changes need a reviewed decision. Not for open-ended ideation or trivial edits."
---

# Octocode RFC Generator

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Produce evidence-backed decisions and plans that builders and reviewers can execute.
Flow: `UNDERSTAND → RESEARCH → PREREQUISITES → CLOSE DECISION BLOCKERS → DECIDE/CONFIRM → DEFINE ACCEPTANCE → PLAN → VALIDATE → DELIVER`.
For an existing RFC, use the reassessment route in `references/workflow.md` and audit against live code rather than prior checkboxes.

RFCs: `<output>/rfc/`; scratch: `<output>/tmp/octocode-rfc-generator/`. Chat-only proposals stay in chat; approved source edits keep their named paths.

## Lobby rules
- Skip RFC mode for trivial edits. Ask one focused question only when uncertainty changes shape, owner, scope, or decision criteria.
- Compare the viable alternatives and include the status quo when it is a real option; skip ceremonial options that cannot satisfy the decision.
- Recommendations require verifiable facts; cite exact anchors and commands/checks that ran.
- `RFC.md` owns goals, scope, and a consequential decision; standalone `PLAN.md` owns its plan context. Linked artifacts reference that primary document instead of restating it.
- Close decision-blocking questions with evidence before comparing options or recommending. Mark other uncertainty with confidence, impact, owner, and either a proof trigger or a deferral trigger.
- Define acceptance before implementation steps. Order steps by dependency, not estimates, and link every step to acceptance and verification.
- Reassessing `.octocode/rfc/` requires fresh reads of live code and a dated audit result. An audit block is the only append-only exception to an accepted RFC's freeze: write it only with source-edit authority; otherwise return it in chat.
- Never assert RFC status from memory or from another RFC's claims.
- Stop when the work is a trivial edit; a brainstorming handoff is not RFC-ready; uncertainty changes artifact shape, owner, scope, or tradeoff priority; another research pass is unlikely to close a blocker; independent decisions need separate RFCs; or a save, edit, delete, or archive awaits approval.

## Artifact route
For a consequential decision, start with `RFC.md`; an RFC-linked execution plan is `IMPLEMENTATION.md`. For a settled decision that only needs execution planning, use standalone `PLAN.md` and do not invent alternatives. Add `PREREQUISITES.md`, `KPI.md`, or `RESOURCES.md` only when readiness, measurement, or source volume needs its own lifecycle. After save approval, place the set under `<output>/rfc/{name}/`.

## Smart routes — load only what the current step needs
- To understand the ask and select a mode before drafting, load `references/workflow.md` — gates, claim ledger, artifact set, traceability, validation, and delivery order.
- When researching evidence, use `octocode-research`; then load `references/research-playbook.md` to keep claims auditable.
- When existing code has readiness work, load `references/rfc-prerequisites.md` before planning — expose baselines, blockers, owners, and setup.
- After decision blockers close, when ready to decide between options or confirm the settled decision, load `references/rfc-template.md` — structure alternatives, goals/non-goals, reversibility, and pre-mortem.
- Before planning, if acceptance or KPI targets need their own lifecycle, load `references/rfc-kpi.md` — connect requirements, plan steps, metrics, decision rules, and verification.
- When building `PLAN.md` or `IMPLEMENTATION.md`, load `references/rfc-implementation.md` — define inline acceptance when `KPI.md` is absent, close execution questions, order dependencies, and define rollout/rollback.
- When preserving sources, load `references/rfc-resources.md` — record provenance without moving decisive citations out of the RFC.
- When reassessing, rating, or cleaning up an existing RFC, load `references/rfc-audit.md` — produce a dated audit result with live-code evidence before any keep/fix/delete recommendation.
- When improving this skill, use `octocode-eval-benchmark`.

## Related routes and verification
- Use `octocode-brainstorming` before RFC when worth-building is unresolved and `octocode-eval-benchmark` for KPI rigor. To close factual questions, `octocode-research` owns the MCP/CLI workflow and live tool/grammar discovery.
- Use `octocode-skills` when changing this skill folder.
- Before delivery, run `node scripts/validate-rfc.mjs <file-or-folder>` for saved artifacts; use `--self-test` when changing the validator. For chat-only output, apply the same section, dependency, and traceability checks manually. Report the real result.
