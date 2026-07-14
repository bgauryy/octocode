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
Track `claim | evidence | confidence | artifact/section | next proof`; only confirmed/likely claims may support a recommendation, uncertain claims become open questions.
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
- Tabular content renders as a real markdown table, not prose.
- Every citation states why it matters; every artifact stays dense — no filler, no duplicate phrasing, no data loss.
- Close or explicitly defer every open question before Ready for Review.
- Reject a brainstorming handoff marked Prototype First, Narrow, Park, or not ready.

## Reassess existing RFCs (audit)
Run this whenever asked to review, rate, clean up, or revisit `.octocode/rfc/`, and before any delete/archive/keep call on an existing RFC — not only when writing a new one.
1. Read every file in the RFC's folder (or the single `RFC.md`), not just the header.
2. Re-derive scope from the RFC text, then grep/read the live packages it claims to touch — never trust prior checkboxes as proof.
3. Classify: Implemented, Partially implemented (name exactly what's open), Not implemented, or Superseded/Obsolete — and flag any RFC that contradicts a more recently accepted one (e.g. two RFCs both claiming schema/API ownership).
4. Write or refresh the RFC's `## Audit Reasoning` block (template: `references/rfc-audit.md`) in place, dated, with file:line evidence for both what shipped and what's missing.
5. Recommend one of: **Delete/archive** (implemented-and-stale, or superseded with no unique open item), **Fix-and-keep** (partially done; document is otherwise still the right owner), or **Keep-as-TODO** (untouched, still wanted).
Do not silently delete — surface the recommendation and act only on explicit approval.
6. If an RFC is deleted or archived, check other kept RFCs for dependency notes pointing at it and correct or re-point them in the same pass.

## Validate and deliver
Run deterministic checks that can fail, including `scripts/eval-rfc.mjs --case <id>` when applicable.
Confirm goals/non-goals, evidence, fair alternatives, pre-mortem, blast radius, dependency order, V&V, rollback, KPI guardrails, and complete traceability.
Deliver: `Status`, `Decision`, `Why`, `Alternatives`, `Risk`, `Success signal`, `Next step`; then ask before saving.
Save approved full sets under `.octocode/rfc/{name}/`; otherwise keep the result in chat.
