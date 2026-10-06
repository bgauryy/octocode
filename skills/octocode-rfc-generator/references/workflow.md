# RFC / plan workflow

Load before drafting, improving, or auditing any RFC or plan. Why: this page chooses the mode, the artifact set, and the delivery values before a draft exists.

## Select mode
| Mode | Primary artifact |
|---|---|
| Decision | One `RFC.md`. Put the plan in that file after Unresolved Questions. |
| Decision already settled | One `PLAN.md`. Do not invent alternatives. |
| Audit | The same `RFC.md`. Append the audit block. Do not open a second document. |

Route a trivial one-file edit to `octocode-research`. A migration, a public contract, and an architecture change use RFC mode. Put current and target state in Motivation and Current State and, when the design changes, Reference-Level Explanation.

## One ledger, dependency gates
Track `claim | evidence | confidence | artifact/section | next proof`. Only confirmed or likely claims support a recommendation; uncertain claims become open questions. Do not keep a second ledger.
Scaffold early if useful, but complete each dependency before its consumer:
1. Readiness stays in Motivation and Current State. Open `PREREQUISITES.md` only when that evidence has its own lifecycle.
2. `RFC.md` or standalone `PLAN.md`: goals and scope before a final recommendation.
3. Put the acceptance contract in the plan, after goals and before steps. Open `KPI.md` only when measurement has its own lifecycle.
4. Steps inside `RFC.md`, or inside `PLAN.md` when there is no RFC. Move them to `IMPLEMENTATION.md` only when the build leaves the RFC.
5. Cite sources in the sections that use them. Open `RESOURCES.md` only when the inventory has its own lifecycle.

## Gates
- A brainstorming handoff marked Prototype First, Narrow, or Park is not RFC-ready.
- Research current state before recommending, and state why each citation matters.
- Public API, data, security, or compatibility changes require rollout, a rollback trigger, and an owner.
- Render tabular content as a markdown table. Keep artifacts dense: no filler, no duplicate phrasing, no data loss.

## Audit an existing RFC
1. Run when asked to review, rate, clean up, or revisit `.octocode/rfc/`, and before any delete, archive, or keep call. Read every file in the RFC folder, not only the header.
2. Re-derive scope from the RFC text. Use `octocode-research` to inspect the live packages it claims to touch.
3. Classify: Implemented, Partially implemented (name what is open), Not implemented, or Superseded/Obsolete. Flag an RFC that contradicts a more accepted one, for example two RFCs that claim the same schema ownership.
4. Write a dated block with this shape under the `RFC.md` header fields. Never rewrite the accepted decision.
   `## Audit Reasoning — kept/updated ({date})` with bullets **Status** (verified in code and tests), **Why kept** (open wanted gap, dependency, or live use; if none, recommend deletion), **Evidence** (`file:line`, symbol, or command for presence and absence), **Remaining work** (unclosed items, or "entire RFC").
5. Recommend **Delete/archive** (implemented and stale, or superseded with no unique open item), **Fix-and-keep** (refresh open items with `references/rfc-implementation.md`), or **Keep-as-TODO** (untouched, still wanted).
6. Do not delete silently. Act only within deletion or archive authority already given; ask only if it is missing. After a delete or archive, re-point dependency notes in the kept RFCs in the same pass.

## Validate and deliver
Run the validators named in `SKILL.md` § Scripts. Both modes reject missing sections, forward step references, missing acceptance links, and phases that consume unavailable outputs. Unresolved decision blockers fail readiness. In `--draft` mode open blockers are valid; also inspect substantive comparisons for a hidden winner.

Deliver one document in template order. Keep every heading you include. In chat, shorten a section only by keeping its heading and the deciding facts. A file that has left the RFC uses the same headings. Set the template header fields to the values for the mode:

| Mode | Header values |
|---|---|
| Blocked Draft | `Status: Draft`, `Recommendation: none`, `Comparison outcome: unresolved`, `Decision blockers: open`. Each open blocker has an owner, an evidence gap, and a next check. |
| Ready RFC | `Recommendation: final`, `Comparison outcome: final`, `Decision blockers: none` or `resolved`. |
| Plan | Title `# Plan:` or `# Implementation:`. Use the plan headings. |

Next: evidence → `references/research-playbook.md`; decision → `references/rfc-template.md`; readiness → `references/rfc-prerequisites.md`; acceptance → `references/rfc-kpi.md`; build → `references/rfc-implementation.md`.
