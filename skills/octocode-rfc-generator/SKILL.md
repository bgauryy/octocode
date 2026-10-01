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
Flow: `UNDERSTAND → RESEARCH / PROVISIONAL COMPARISON → PREREQUISITES → CLOSE DECISION BLOCKERS → DECIDE/CONFIRM → DEFINE ACCEPTANCE → PLAN → VALIDATE → DELIVER`.
For an existing RFC, use the reassessment route in `references/workflow.md` and audit against live code rather than prior checkboxes.

RFCs: `<output>/rfc/`; scratch: `<output>/tmp/octocode-rfc-generator/`. Chat-only proposals stay in chat; approved source edits keep their named paths.

## Lobby rules
- Skip RFC mode for trivial edits. Ask one focused question only when uncertainty changes shape, owner, scope, or decision criteria.
- Compare the viable alternatives and include the status quo when it is a real option; skip ceremonial options that cannot satisfy the decision.
- Recommendations require verifiable facts; cite exact anchors and commands/checks that ran.
- `RFC.md` owns goals, scope, and a consequential decision; standalone `PLAN.md` owns its plan context. Linked artifacts reference that primary document instead of restating it.
- Compare options provisionally during research to expose assumptions, counterevidence and deciding checks. Keep reversal conditions beside each affected comparison; eliminate an option only with evidence that it violates a fixed constraint. Do not select an overall winner or make a final recommendation until decision-blocking questions close with evidence. Mark other uncertainty with confidence, impact, owner, and either a proof trigger or a deferral trigger.
- Define acceptance before implementation steps. Order steps by dependency, not estimates, and link every step to acceptance and verification.
- Reassessing `.octocode/rfc/` requires fresh reads of live code and a dated audit result. An audit block is the only append-only exception to an accepted RFC's freeze: write it only with source-edit authority; otherwise return it in chat.
- Never assert RFC status from memory or from another RFC's claims.
- Stop when the work is a trivial edit; a brainstorming handoff is not RFC-ready; uncertainty changes artifact shape, owner, scope, or tradeoff priority; another research pass is unlikely to close a blocker; independent decisions need separate RFCs; or a requested action lacks authority. Continue independent authorized work while a specific question remains open.

## Artifact route
For a consequential decision, start with `RFC.md`; an RFC-linked execution plan is `IMPLEMENTATION.md`. For a settled decision that only needs execution planning, use standalone `PLAN.md` and do not invent alternatives. Add `PREREQUISITES.md`, `KPI.md`, or `RESOURCES.md` only when readiness, measurement, or source volume needs its own lifecycle. When the task authorizes saving, place the set under `<output>/rfc/{name}/`.

## Completeness and optional semantic assessment
At blocker closure and validation, self-ask what must be true, what could refute the proposed answer, and which missing answer could reverse the decision. Use `references/rfc-completeness.md` to discover questions and track their closure in the existing ledger.

Use ordinary evidence review by default. The `octocode-research` clasify gate owns classification admission; no RFC debate recipe overrides its benefit gate. Only for an admitted classification request or experiment, load `references/jev-review.md` for the optional frozen two-worker protocol. A judgment never closes a blocker by itself.

## Smart routes — load only what the current step needs
- To understand the ask and select a mode before drafting, load `references/workflow.md` — gates, claim ledger, artifact set, traceability, validation, and delivery order.
- When researching evidence, use `octocode-research`; then load `references/research-playbook.md` to keep claims auditable.
- When existing code has readiness work, load `references/rfc-prerequisites.md` before planning — expose baselines, blockers, owners, and setup.
- To draft provisional alternatives during research or confirm a settled decision after blockers close, load `references/rfc-template.md` — structure alternatives, goals/non-goals, reversibility, and pre-mortem.
- Before planning, if acceptance or KPI targets need their own lifecycle, load `references/rfc-kpi.md` — connect requirements, plan steps, metrics, decision rules, and verification.
- When building `PLAN.md` or `IMPLEMENTATION.md`, load `references/rfc-implementation.md` — define inline acceptance when `KPI.md` is absent, close execution questions, order dependencies, and define rollout/rollback.
- When preserving sources, load `references/rfc-resources.md` — record provenance without moving decisive citations out of the RFC.
- When reassessing, rating, or cleaning up an existing RFC, load `references/rfc-audit.md` — produce a dated audit result with live-code evidence before any keep/fix/delete recommendation.
- Before a provider-judged debate, load `references/jev-debate.md` for worker dispatch and receipt integrity; for `clasify` setup, route selection and results, load `references/jev-api.md`.
- After multi-agent review, load `references/review-cost.md` to capture whole-workflow observable cost and explicit unknowns. When improving or comparing Jev-backed review, load `references/jev-evaluation.md` and use `octocode-eval-benchmark`; when tracing the protocol's origin or API ownership, load `references/jev-sources.md`.

## Related routes and verification
- Use `octocode-brainstorming` before RFC when worth-building is unresolved and `octocode-eval-benchmark` for KPI rigor. To close factual questions, `octocode-research` owns the MCP/CLI workflow and live tool/grammar discovery.
- Use `octocode-skills` when changing this skill folder.
- When comparing this skill with peers, collection-wide `octocode-skills` review supports structural hygiene claims only. Claim behavioral superiority only against another workflow solving the same frozen cases with the same budget and independent grader.
- Reuse existing session authority for scoped saves and edits; do not ask again. Ask only for missing information or authority for a new effect. Saving a Draft, accepting its decision, and authorizing implementation are distinct. A review-only or chat-only request does not authorize source edits; accepted decision bodies keep the audit rules above.
- Before submitting a two-agent assessment packet, run `node scripts/validate-debate.mjs request.json worker-packet.json` from this skill folder. It requires a source-free SemanticQuery, frozen unresolved-disagreement admission gate, and distinct result-dependent actions. Use `--self-test` when changing the protocol. Failed preflight stops submission; passing it does not prove evidence truth or actual worker coverage.
- After a multi-agent/provider review, run `node scripts/validate-review-cost.mjs receipt.json`; record observable wall time, calls, reads, retries, failures, requested model, provider-resolved model(s), usage, and explicit unknowns instead of presenting provider-only usage as total cost.
- Before delivery, run `node scripts/validate-rfc.mjs <file-or-folder>` for readiness checks. For an investigative RFC Draft, use `node scripts/validate-rfc.mjs --draft <file-or-folder>`: requires `Status: Draft`, `Recommendation: none`, `Comparison outcome: unresolved`, complete blocker fields, and no common covert commitment language; it reports `reviewReady:false`. Its bounded lint cannot prove prose truth, evidence, or authority, so inspect those separately. Use `--self-test` when changing the validator. For chat-only output, apply the same checks manually. Report the real result.
