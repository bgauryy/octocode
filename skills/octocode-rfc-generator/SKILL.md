---
name: octocode-rfc-generator
description: "Use when the user needs an RFC, design doc, architecture proposal, migration plan, or research-backed decision before coding. Triggers: cross-package changes, risky refactors, option comparisons, blast-radius mapping, or improving an existing RFC/plan."
---

# Octocode RFC Generator

For a change that needs **thinking before coding** — or to **improve an existing RFC/plan** (close gaps, research open questions, add verification). Output is evidence-backed and actionable, not a brainstorm.

```text
UNDERSTAND → RESEARCH (octocode-research) → PREREQUISITES? → COMPARE OPTIONS → WRITE RFC → CLOSE OPEN QUESTIONS → DERIVE KPIs → VALIDATE → DELIVER
```

For trivial one-file edits with no design choice, skip RFC mode and route to `octocode-research` Change mode.
Read `references/octocode.md` before evidence gathering; it delegates Octocode research rules to `octocode-research`.
Avoid one-shot RFCs: if the problem, output mode, decision flow, or tradeoff priority is unclear, ask one focused question before drafting.

## Output: a folder, not a file

When a save is approved, write a folder `<workspace>/.octocode/rfc/{name}/` containing the RFC document set; existing-code RFCs add `PREREQUISITES.md` before `IMPLEMENTATION.md` (fall back to global `~/.octocode/rfc/{name}/` only when the workspace has no `.octocode/` or is unwritable — see `<doc_placement>`):

| File | Role · reader · lifecycle |
|---|---|
| `RFC.md` | **Decide** — reviewer-facing. **Frozen at decision. Single source of truth (SSOT) for goals/scope/decision.** |
| `PREREQUISITES.md` | **Ready** — existing-code RFCs only. **Written before the plan.** Preconditions, baseline evidence, blockers, owners, and setup needed before implementation. |
| `IMPLEMENTATION.md` | **Build** — implementer-facing. **Live.** Every RFC open question is closed here with `octocode-research` evidence. |
| `KPI.md` | **Verify** — outlives the ship date. Acceptance criteria + measurable success signals + how to check the RFC *and* its implementation post-ship. |

**Small-feature mode:** for a small, reversible, single-package change, produce **only `RFC.md`** (plan + acceptance criteria inline) and say so. The full folder is the default for anything irreversible, cross-package, or public-contract/data/security impact.

## Reference Map

- `references/workflow.md` — read first for mode selection, gates, claim ledger, output structure, SSOT rule, traceability, validation, and delivery.
- `references/research-playbook.md` — when gathering RFC evidence by delegating Octocode-backed research to `octocode-research`.
- `references/rfc-template.md` — when producing `RFC.md` (Summary → Prior Art, Goals/Non-Goals, reversibility tag, pre-mortem).
- `references/rfc-prerequisites.md` — when an RFC changes existing code and must produce `PREREQUISITES.md` before the implementation plan.
- `references/rfc-implementation.md` — when producing `IMPLEMENTATION.md` (open questions closed via research, dependency-ordered steps, V&V test plan, rollout/rollback).
- `references/rfc-kpi.md` — when producing `KPI.md` (user stories, Gherkin acceptance, success metrics, decision rule, traceability matrix).

## Non-negotiables

- Do not guess facts that tools can verify; cite local claims with `file:line` and external claims with a GitHub path/line or PR/commit link. Use `octocode-research` for Octocode-backed claims.
- Ask before deciding the flow when uncertainty changes the RFC shape, owner, scope, or decision criteria.
- Always compare at least two alternatives — including do-nothing — unless the user explicitly asks for a single implementation plan.
- **SSOT rule:** `RFC.md` owns goals, scope, and the decision. `IMPLEMENTATION.md` and `KPI.md` **reference RFC section anchors — never restate them**, so the files cannot drift apart.
- **Close open questions in `IMPLEMENTATION.md`:** every `Unresolved Question` in `RFC.md` must be resolved with an `octocode-research` citation, or explicitly deferred with a reason. No recommendation may rest on an `uncertain` claim.
- **Bind the files with a traceability matrix** in `KPI.md`: `RFC requirement → user story → acceptance criteria → verification method → post-ship status`.
- Order implementation steps by dependency, not preference; avoid time estimates. Generate the document set in **one pass from one claim ledger**, then self-check with `scripts/eval-rfc.mjs --case <id>` before delivery.
