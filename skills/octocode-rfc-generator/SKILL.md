---
name: octocode-rfc-generator
description: "Use when consequential architecture, migration, public-contract, or multi-phase changes need a reviewed decision. Not for open-ended ideation or trivial edits."
---

# Octocode RFC Generator

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies

Produce evidence-backed decisions and plans that builders and reviewers can execute.
When the task authorizes saving, RFC sets go to `<output>/rfc/{name}/`; scratch goes to `<output>/tmp/octocode-rfc-generator/`. Chat-only proposals stay in chat. Approved source edits keep their named paths.

```mermaid
flowchart LR
    U[Understand] --> R[Research + provisional comparison]
    R --> P[Prerequisites]
    P --> B{Decision blockers closed?}
    B -- no --> R
    B -- yes --> D[Decide / confirm]
    D --> A[Define acceptance]
    A --> PL[Plan]
    PL --> V[Validate]
    V --> DL[Deliver]
    DL --> VW[View in browser]
    U -. "before drafting: select mode, artifact set, ledger, gates, audit, delivery order" .-> W1["references/workflow.md"]
    R -. "for evidence planning after octocode-research; record RESOURCES.md" .-> W2["references/research-playbook.md"]
    P -. "when existing code has readiness work" .-> W3["references/rfc-prerequisites.md"]
    B -. "for blocker questions, track closure" .-> W4["references/rfc-completeness.md"]
    D -. "when drafting provisional alternatives or confirm a decision" .-> W5["references/rfc-template.md"]
    A -. "when acceptance or KPI targets need their own lifecycle" .-> W6["references/rfc-kpi.md"]
    PL -. "when building PLAN.md or IMPLEMENTATION.md" .-> W7["references/rfc-implementation.md"]
    PL -. "when a section explains a flow, structure, comparison, proportion, lifecycle, or schedule" .-> W8["references/rfc-diagrams.md"]
    V -. "when classification is admitted: two-worker review" .-> W9["references/jev-review.md"]
    V -. "before a provider-judged debate: dispatch and receipts" .-> W10["references/jev-debate.md"]
    V -. "for clasify setup, request shape, results" .-> W11["references/jev-api.md"]
    V -. "after multi-agent review, or comparing Jev-backed review" .-> W12["references/review-cost.md"]
```

Read it as: the `no` branch returns to research.
To reassess an existing RFC, use the audit route in `references/workflow.md`.

## Lobby rules
- Compare the viable alternatives. Include the status quo when it is a real option. Skip options that cannot satisfy the decision.
- Base recommendations on verifiable facts; `octocode-research` owns citation and evidence rules.
- `RFC.md` owns goals, scope, and a consequential decision. Standalone `PLAN.md` owns its plan context. Linked artifacts reference that primary document and do not restate it.
- During research, compare options provisionally to expose assumptions, counterevidence, and deciding checks. Keep reversal conditions beside each comparison. Eliminate an option only with evidence that it violates a fixed constraint.
- Do not select an overall winner or make a final recommendation until decision-blocking questions close with evidence. Mark other uncertainty with confidence, impact, owner, and a proof trigger or a deferral trigger.
- Show structure as mermaid diagrams, not paragraphs: decisions, flows, comparisons, proportions, lifecycles, and step dependencies. Give each diagram one caption line. Required facts also stay in text or tables.
- Define acceptance before implementation steps. Order steps by dependency, not estimates. Link every step to acceptance and verification.
- Reassessing `.octocode/rfc/` requires fresh reads of live code and a dated audit result. The audit block is the only append-only exception to an accepted RFC's freeze. Write it only with source-edit authority; otherwise return it in chat.
- Never assert RFC status from memory or from another RFC's claims.
- Stop (and ask one focused question when an answer unblocks the work) when: the work is a trivial edit; a brainstorming handoff is not RFC-ready; uncertainty changes artifact shape, owner, scope, or tradeoff priority; another research pass is unlikely to close a blocker; independent decisions need separate RFCs; or a requested action lacks authority. Continue independent authorized work while a specific question stays open.

## Artifacts
- Consequential decision: start with `RFC.md`. Its execution plan is `IMPLEMENTATION.md`.
- Settled decision that needs only execution planning: use standalone `PLAN.md`. Do not invent alternatives.
- Add `PREREQUISITES.md`, `KPI.md`, or `RESOURCES.md` only when readiness, measurement, or source volume needs its own lifecycle.

## Classification
Use ordinary evidence review by default. The `octocode-research` clasify gate owns classification admission. No RFC debate recipe overrides its benefit gate. A judgment never closes a blocker by itself.

## Related skills and authority
- Use `octocode-brainstorming` before RFC when worth-building is unresolved. Use `octocode-eval-benchmark` for KPI rigor. `octocode-research` owns the MCP/CLI workflow for factual questions.
- Use `octocode-skills` when changing this skill folder. Collection-wide review supports structural hygiene claims only. Claim behavioral superiority only against another workflow on the same frozen cases, budget, and independent grader.
- Reuse existing session authority for scoped saves and edits; do not ask again. Ask only for missing information or authority for a new effect.
- Saving a Draft, accepting its decision, and authorizing implementation are distinct. A review-only or chat-only request does not authorize source edits.

## Scripts (run from this skill folder; report the real result)
- Before delivery, run `node scripts/validate-rfc.mjs <file-or-folder>` for readiness checks.
- For an investigative Draft, run `node scripts/validate-rfc.mjs --draft <file-or-folder>`. It requires `Status: Draft`, `Recommendation: none`, `Comparison outcome: unresolved`, complete blocker fields, and no common covert commitment language. It reports `reviewReady:false`.
- `validate-rfc.mjs` also rejects unknown mermaid types and warns when `RFC.md` has no diagram. Its bounded lint cannot prove prose truth, evidence, or authority; inspect those separately. For chat-only output, apply the same checks manually.
- Before submitting a two-agent assessment packet, run `node scripts/validate-debate.mjs request.json worker-packet.json`. It requires a source-free SemanticQuery, a frozen admission gate, and distinct result-dependent actions. A failed preflight stops submission. A pass does not prove evidence truth or worker coverage.
- After a multi-agent or provider review, run `node scripts/validate-review-cost.mjs receipt.json`. Do not present provider-only usage as total cost.
- After a saved RFC set passes validation, run `node scripts/render-rfc.mjs <rfc-folder>`. It builds one HTML page from `assets/rfc-viewer.html` and `assets/rfc-viewer.js` and opens it in the browser. Each file is a page, with search, section TOC, and rendered mermaid. Report the printed path. Use `--no-open` in headless runs; re-run after edits.
- When changing a validator, the viewer, or the debate protocol, run its `--self-test`.
