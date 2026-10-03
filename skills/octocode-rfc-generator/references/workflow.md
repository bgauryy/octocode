# RFC / plan workflow

Load before drafting, improving, or auditing any RFC or plan. Choose the smallest artifact set and keep one decision and claim spine.

## Select mode
| Mode | Primary artifact and content |
|---|---|
| RFC / design / architecture | `RFC.md`: decision, alternatives, rationale, risks, implementation, KPIs |
| Plan (decision settled) | Standalone `PLAN.md`, or `IMPLEMENTATION.md` when linked to an RFC. Define acceptance inline or in `KPI.md` before steps. No invented alternatives. |
| Decision | Options matrix, recommendation, adoption, rollback |
| Migration | Current and target state, compatibility, phases, rollout, rollback |
| Validate / improve | Upgrade the existing artifact in place. Keep prior decisions and reasoning. |

Route trivial one-file edits to `octocode-research` Change mode. Add companions only when their content needs separate ownership or lifecycle.

## One ledger, dependency gates
Track `claim | evidence | confidence | artifact/section | next proof`. Only confirmed or likely claims support a recommendation; uncertain claims become open questions. Extend this ledger with question IDs, counterevidence, owner, and closure status from `references/rfc-completeness.md`. Do not keep a second ledger.
Scaffold early if useful, but complete each dependency before its consumer:
1. `PREREQUISITES.md` when needed: current-state evidence, baselines, blockers, owners. Close blockers before the decision or Step 1.
2. `RFC.md` decision or `PLAN.md` context: goals, scope, constraints, settled direction.
3. `KPI.md` when separate: acceptance, metrics, guardrails, rollback threshold, decision rule. Complete it before implementation steps. Without `KPI.md`, put a compact acceptance contract before the steps in `PLAN.md` or `IMPLEMENTATION.md`.
4. `IMPLEMENTATION.md`: execution questions, dependency-ordered build, verification, rollout, rollback. Standalone mode keeps these in `PLAN.md`.
5. `RESOURCES.md`: source inventory. It never replaces inline decision evidence.

## Gates
- Ask when flow uncertainty changes artifact shape, owner, scope, or tradeoff priority. Split independent decisions. Reject a brainstorming handoff marked Prototype First, Narrow, Park, or not ready.
- Research current state before recommending. Keep exact citations, and state why each one matters.
- Public API, data, security, or compatibility changes require rollout, a rollback trigger, and an owner.
- Render tabular content as a markdown table. Keep artifacts dense: no filler, no duplicate phrasing, no data loss. Add diagrams per `references/rfc-diagrams.md`.
- Execution questions may be resolved or explicitly deferred with impact, owner, and trigger before Ready for Review. Decision blockers must close with evidence.

## Audit an existing RFC
1. Run when asked to review, rate, clean up, or revisit `.octocode/rfc/`, and before any delete, archive, or keep call. Read every file in the RFC folder, not only the header.
2. Re-derive scope from the RFC text. Use `octocode-research` to inspect the live packages it claims to touch. Prior checkboxes are not proof.
3. Classify: Implemented, Partially implemented (name what is open), Not implemented, or Superseded/Obsolete. Flag an RFC that contradicts a more accepted one, for example two RFCs that claim the same schema ownership.
4. Write a dated block with this shape. With source-edit authority, put it under the `RFC.md` header fields; otherwise return it in chat. Never rewrite the accepted decision.
   `## Audit Reasoning — kept/updated ({date})` with bullets **Status** (verified in code and tests), **Why kept** (open wanted gap, dependency, or live use; if none, recommend deletion), **Evidence** (`file:line`, symbol, or command for presence and absence), **Remaining work** (unclosed items, or "entire RFC").
5. Recommend **Delete/archive** (implemented and stale, or superseded with no unique open item), **Fix-and-keep** (refresh open items with `references/rfc-implementation.md`), or **Keep-as-TODO** (untouched, still wanted).
6. Do not delete silently. Act only within deletion or archive authority already given; ask only if it is missing. After a delete or archive, re-point dependency notes in the kept RFCs in the same pass.

## Validate and deliver
Run the validators named in `SKILL.md` § Scripts. Both modes reject missing sections, forward step references, missing acceptance links, and phases that consume unavailable outputs. Unresolved decision blockers fail readiness. In `--draft` mode open blockers are valid; also inspect substantive comparisons for a hidden winner.
| Mode | Deliver |
|---|---|
| Decision | `Status`, `Decision`, `Why`, `Alternatives`, `Risk`, `Success signal`, `Next step` |
| Blocked Draft | as Decision, with `Provisional alternatives` instead of `Decision`, plus blockers and deciding checks |
| Plan | `Status`, `Context`, `Risks`, `Success signal`, `Dependency-ordered steps`, `Next step` |

Use `<output>/rfc/{name}/` for new authorized artifact sets. Saving never implies acceptance or implementation authority. After a saved set validates, open it with `scripts/render-rfc.mjs` (see `SKILL.md` § Scripts).
Next: evidence → `references/research-playbook.md`; decision → `references/rfc-template.md`; readiness → `references/rfc-prerequisites.md`; acceptance → `references/rfc-kpi.md`; build → `references/rfc-implementation.md`.
