# RFC / Plan Workflow

Load before drafting or improving any RFC/plan. Why: choose the smallest artifact set and preserve one decision/claim spine.
Advance from framing and research through option choice, artifact writing, question closure, measurable acceptance, validation, and delivery.

## Select mode and shape
- RFC/design/architecture: decision, alternatives, rationale, risks, implementation, KPIs.
- Plan: evidence-backed implementation route; include decision/KPI detail only when useful.
- Decision: options matrix, recommendation, adoption, rollback.
- Migration: current/target state, compatibility, phases, rollout, rollback.
- Validation/improve: upgrade the existing artifact in place; preserve prior decisions and reasoning.
Trivial one-file edits route to `octocode-research` Change mode.
Small and reversible uses one `RFC.md`; irreversible, cross-package, public-contract, data, security, or prior-art-heavy work uses the full folder.

## One ledger, ordered artifacts
Track `claim | evidence | confidence | artifact/section | next proof`.
Only confirmed/likely claims may support a recommendation; uncertain claims become open questions.
Write in dependency order:
1. `RFC.md` — goals, scope, decision, alternatives, risks; frozen when accepted.
2. `PREREQUISITES.md` — existing-code readiness, baselines, blockers, owners.
3. `IMPLEMENTATION.md` — closes every open question, then dependency-ordered build/rollout.
4. `KPI.md` — acceptance, metrics, guardrails, decision rule, traceability.
5. `RESOURCES.md` — source inventory; never a substitute for inline decision evidence.
Later artifacts reference RFC anchors and never restate goals/scope.

## Gates
- Ask when flow uncertainty changes artifact shape, owner, scope, or tradeoff priority.
- Split independent decisions; include at least two options, including do-nothing.
- Research current state before recommending; preserve exact citations.
- Public API/data/security/compatibility changes require rollout, rollback trigger, and owner.
- Close or explicitly defer every open question before Ready for Review.
- Reject a brainstorming handoff marked Prototype First, Narrow, Park, or not ready.

## Validate and deliver
Run deterministic checks that can fail, including `scripts/eval-rfc.mjs --case <id>` when applicable.
Confirm goals/non-goals, evidence, fair alternatives, pre-mortem, blast radius, dependency order, V&V, rollback, KPI guardrails, and complete traceability.
Deliver: `Status`, `Decision`, `Why`, `Alternatives`, `Risk`, `Success signal`, `Next step`; then ask before saving.
Save approved full sets under `.octocode/rfc/{name}/`; otherwise keep the result in chat.
