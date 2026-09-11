# RFC / plan workflow

Load before drafting or improving any RFC/plan. Why: choose the smallest artifact set and preserve one decision/claim spine.
Advance from framing and research through blocker closure, decision or confirmation, measurable acceptance, dependency-ordered planning, validation, and delivery.

## Select mode and shape
- RFC/design/architecture: decision, alternatives, rationale, risks, implementation, KPIs.
- Plan: use standalone `PLAN.md` when the decision is settled; define acceptance inline or in `KPI.md` before steps. Use `IMPLEMENTATION.md` when linked to an RFC; do not invent alternatives for plan-only work.
- Decision: options matrix, recommendation, adoption, rollback.
- Migration: current/target state, compatibility, phases, rollout, rollback.
- Validation/improve: upgrade the existing artifact in place; preserve prior decisions and reasoning.
Trivial one-file edits route to `octocode-research` Change mode.
Start with the primary artifact for the selected mode: `RFC.md` for a decision or `PLAN.md` for execution of a settled decision. Add companions only when their content needs separate ownership or lifecycle.

## One ledger, dependency gates
Track `claim | evidence | confidence | artifact/section | next proof`; only confirmed/likely claims can support a recommendation, uncertain claims become open questions.
An RFC may be scaffolded early, but complete each dependency before its consumer:
1. `PREREQUISITES.md` when needed — current-state evidence, baselines, blockers, owners; blockers close before decision or Step 1.
2. `RFC.md` decision or `PLAN.md` context — the primary goals/scope/constraints and settled direction.
3. `KPI.md` when separate — acceptance, metrics, guardrails, rollback threshold, and decision rule; complete before implementation steps.
4. `IMPLEMENTATION.md` — RFC-linked execution questions, dependency-ordered build, verification, rollout, and rollback procedure; standalone mode keeps these steps in `PLAN.md`.
5. `RESOURCES.md` — source inventory; never a substitute for inline decision evidence.
If no separate `KPI.md` is warranted, put a compact acceptance contract before the steps in `PLAN.md` or `IMPLEMENTATION.md`. Linked artifacts reference primary anchors and never restate their goals or scope.

## Gates
- Ask when flow uncertainty changes artifact shape, owner, scope, or tradeoff priority.
- Split independent decisions. Compare viable options and include do-nothing when it can satisfy the decision.
- Research current state before recommending; preserve exact citations.
- Public API/data/security/compatibility changes require rollout, rollback trigger, and owner.
- Tabular content renders as a real markdown table, not prose.
- Every citation states why it matters; every artifact stays dense — no filler, no duplicate phrasing, no data loss.
- Close every decision blocker before comparing options or recommending. Execution questions may be resolved or explicitly deferred with impact, owner, and trigger before Ready for Review.
- Reject a brainstorming handoff marked Prototype First, Narrow, Park, or not ready.

## Reassess existing RFCs (audit)
Run this whenever asked to review, rate, clean up, or revisit `.octocode/rfc/`, and before any delete/archive/keep call on an existing RFC — not only when writing a new one.
1. Read every file in the RFC's folder (or the single `RFC.md`), not the header.
2. Re-derive scope from the RFC text, then use `octocode-research` to inspect the live packages it claims to touch. Prior checkboxes are not proof.
3. Classify: Implemented, Partially implemented (name exactly what's open), Not implemented, or Superseded/Obsolete — and flag any RFC that contradicts a more accepted one (for example two RFCs both claiming schema/API ownership).
4. Produce a dated `## Audit Reasoning` block (template: `references/rfc-audit.md`) with evidence for what shipped and what is missing. This append-only block is the sole exception to an accepted RFC's freeze; write it in place only with source-edit authority, otherwise return it in chat.
5. Recommend one of: **Delete/archive** (implemented-and-stale, or superseded with no unique open item), **Fix-and-keep** (partially done; document is otherwise still the right owner), or **Keep-as-TODO** (untouched, still wanted).
Do not silently delete — surface the recommendation and act only on explicit approval.
6. If an RFC is deleted or archived, check other kept RFCs for dependency notes pointing at it and correct or re-point them in the same pass. <!-- style-lint: ignore-line passive-voice -->

## Validate and deliver
For saved artifacts, run `node scripts/validate-rfc.mjs <file-or-folder>`; for chat-only output, apply its section, dependency, and traceability checks manually. Reject forward step references, missing acceptance links, unresolved decision blockers, and phases that consume outputs not yet produced.
Deliver decision modes as `Status`, `Decision`, `Why`, `Alternatives`, `Risk`, `Success signal`, `Next step`. Deliver plan mode as `Status`, `Context`, `Risks`, `Success signal`, `Dependency-ordered steps`, `Next step`. Then ask before saving; save approved sets under `.octocode/rfc/{name}/`, otherwise keep the result in chat.

Next: for evidence load `references/research-playbook.md`; to write the decision `references/rfc-template.md`; for readiness `references/rfc-prerequisites.md`; for build `references/rfc-implementation.md`; for acceptance `references/rfc-kpi.md`; for provenance `references/rfc-resources.md`; when auditing an existing RFC `references/rfc-audit.md`.
