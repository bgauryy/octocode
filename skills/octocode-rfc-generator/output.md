# Output

<!-- report-contract {"statuses":["Proposed","Accepted","Implemented","Rejected","Superseded"],"required":["Summary","Features","Proposed design","Tasks and rollout","Validation and success","Open questions","Sources"],"anyOf":[]} -->

## Structure

Required, under these names: `## Summary`, `## Features`, `## Proposed design`, `## Tasks and rollout`, `## Validation and success`, `## Open questions`, `## Sources`. Add the others only when they carry content, keeping the table's names and order. IDs cross-reference sections (`P` prerequisite, `F` feature, `D` decision, `T` task, `V` check, `Q` question).

| Section | Holds |
|---|---|
| Summary | Decision, effect, why worth it, one line per feature; flags a provisional choice |
| Glossary | Term · meaning here · where it lives (identifier or path). Only terms a newcomer could misread; entities go in their own table |
| Problem and context | Current behavior, affected users, deciding evidence, cost of inaction |
| Goals and scope | Checkable outcomes, constraints, exclusions |
| Prerequisites | `P` · what must hold before the first task (access, tools, data, an owner decision, other work) · check that shows it holds · owner |
| Entities and flows | Entity · what it is · lives in (path, store, schema) · relations and states · `new`, `changed`, `removed` or `read`; then the current and proposed flow of each behavior that moves ([diagrams](references/diagrams.md)) |
| Features | `F` · consumer (user, agent, caller, operator) · before → after, observable · example call or input and its new result · accepted by (`V`). A proposal of options adds priority (Must, Should, Could, Won't) with evidence of need. A refactor's feature is what callers observe, "nothing changes" included |
| Proposed design | Behavior per feature with examples; boundaries, interfaces, failures, compatibility, security, observability; agent-facing text changed (instruction sections, role files, tool descriptions, promptGuidelines), with its owning file and the test that pins it. UI prototypes are labelled mockups |
| Decisions and reasoning | When a choice had viable alternatives, one block each: **D1: choice.** Evidence · Reasoning (why the evidence selects it) · Rejected (option and the evidence against it) · Reverses if |
| Risks and mitigations | Drawbacks, failure conditions, prevention or detection |
| Tasks and rollout | `T` · what to make (module, function, field, doc, test, config, script) · where (exact paths) · serves (`F`, `P`) · depends on (`T`, `P`) · done when (a check someone else can run), in dependency order; the [task graph](references/diagrams.md#choose-by-the-readers-question) when dependencies branch; migration, rollout signals, rollback. A revision of accepted work adds a State column: `todo`, `doing`, `blocked` with its `Q`, `done` with its evidence |
| Validation and success | `V` as Given–When–Then (given <state>, when <action>, then <observable result>), naming the `F` it accepts; baselines, targets, rollback thresholds; measurements apart from estimates |
| Open questions | Each starts `Blocking:` (an objection and its consequence) or `Non-blocking:` (a concern): what is unresolved, impact, what settles it, owner; `None.` when nothing is open |
| Sources | Every cited source as a link: code `path:line` at a commit, issues and PRs, prior RFCs, docs and specs, measurements with their command |

## Language

- Clear technical English (unless asked otherwise) for a capable newcomer to this system: concrete verbs, short paragraphs, exact identifiers, one name per thing: terms, entities and flows keep their code names everywhere.
- Facts with evidence, assumptions marked, unknowns named (what nobody checked); the proposal stays apart from current behavior.
- Cite beside the claim as a link and list the source in Sources; an uncited deciding claim is an assumption. The [trace](references/trace.md) lists cited links missing from Sources and local links that do not resolve.
- "must" = requirement, "should" = recommendation; uppercase only under a real protocol convention. Observable outcomes, not promises.
- Each paragraph carries a decision, evidence or a consequence; check drafts against [writing tells](references/writing-tells.md).
- Cut repetition, sales language, narration, author/date/status tables, timestamps (the header `status` and the dated folder are the only date and status fields), tool inventories, scores, logs, transcripts, receipts.

## Saved report

Save at `<workspace>/.octocode/octocode-rfc-generator/<YYYY-MM-DD>-<slug>/RFC.md`; `<workspace>` is the project root (repository root, else current directory), never inside an existing `.octocode/`. A user-named path wins (including the repository's RFC folder); a scratch folder like `.octocode/tmp/` does not. Scratch notes and long requests go to `<workspace>/.octocode/tmp/octocode-rfc-generator/`. One file: no companion `PLAN.md`, `KPI.md`, `PREREQUISITES.md`, `IMPLEMENTATION.md` or `RESOURCES.md`.

The file starts with the [shared header](references/report-output.md#report-header), then the `#` title, a lead paragraph stating the answer, and the sections. Before replying, run the [trace](references/trace.md) and resolve or report each finding. Accepted `status` values: `Proposed`, `Accepted`, `Implemented`, `Rejected`, `Superseded`.
`Implemented` means built and verified, with `next` naming any eval still owed; a `Superseded` lead names the replacement.

Reply BLUF: decision and confidence, expected effect, risks, any unresolved blocker, the `RFC.md` path, the HTML link if rendered. Review-only request: findings, no files. Then follow the [HTML offer](references/presentation.md#html-delivery); once HTML is accepted, rerender after each revision with the [report commands](references/report-output.md#render-and-retrieve), which also run `--manifest` discovery.
