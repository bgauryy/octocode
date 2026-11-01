# Output

<!-- report-contract {"statuses":["Proposed","Accepted","Implemented","Rejected","Superseded"],"required":["Summary","Proposed design","Validation and success","Open questions"],"anyOf":[]} -->

## Structure

Always include `## Summary`, `## Proposed design`, `## Validation and success`, and `## Open questions` (each question starts `Blocking:`, an objection stating its consequence, or `Non-blocking:`, a concern; `None.` when nothing is open) under exactly those names. Other sections are optional: combine or omit them, keeping the table's names.

| Section | Content |
|---|---|
| Summary | Decision, effect, why worth it; flag a provisional choice |
| Problem and context | Current behavior, affected users, deciding evidence, cost of inaction |
| Goals and scope | Checkable outcomes, constraints, exclusions |
| Proposed design | Behavior with examples; boundaries, interfaces, data flow, failures, compatibility, security; agent-facing text changed (instruction sections, role files, tool descriptions and promptGuidelines), with the file that owns each and the test that pins it |
| Alternatives and rationale | Viable options, costs, benefits, why this fits, what could change it |
| Risks and mitigations | Drawbacks, failure conditions, prevention or detection |
| Implementation and rollout | Prerequisites, dependency-ordered steps, migration, rollout signals, rollback |
| Validation and success | Acceptance checks as Given–When–Then (given <state>, when <action>, then <observable result>); baselines or targets, rollback thresholds; measurements apart from estimates |
| Open questions | What is unresolved, impact, what settles it, owner when needed |
| References | Citations beside claims; a short list only when useful |


## Language

- Clear technical English (unless asked otherwise) for a capable newcomer to this system.
- Concrete verbs, short paragraphs, consistent terms, defined acronyms, exact identifiers.
- Facts, assumptions, unknowns: current behavior with its evidence as facts, assumptions marked, unknowns named (what nobody checked); the proposal stays apart; never present a guess as a fact.
- "must" = requirement, "should" = recommendation; uppercase only under a real protocol convention.
- Observable outcomes, not promises.
- Cut repetition, sales language, narration, author/date/status tables, timestamps, tool inventories, scores, logs, transcripts, receipts. Header `status` owns decision state.

## Delivery

Save at `<workspace>/.octocode/octocode-rfc-generator/<YYYY-MM-DD>-<slug>/RFC.md`; `<workspace>` is the project root (repository root, else current directory), never inside an existing `.octocode/`. A user-named path wins (including the repository's RFC folder); a scratch folder like `.octocode/tmp/` does not. Reply BLUF: decision and confidence first, then any unresolved blocker, the `RFC.md` path, and the HTML link if requested. Review-only request: findings, no files.

No companion `PLAN.md`, `KPI.md`, `PREREQUISITES.md`, `IMPLEMENTATION.md`, or `RESOURCES.md`.

## Presentation choices

Summary → proposed behavior → alternatives and rollout → validation → open questions. Keep current facts distinct from proposed behavior.

| Format | Use when / why |
|---|---|
| `flowchart` / `sequenceDiagram` / `stateDiagram-v2` | Explain proposed data flow / interactions / lifecycle, including failure paths. |
| Alternatives table / `gantt` | Compare tradeoffs / show a schedule only when dates and dependencies are justified. |
| Markdown images / linked prototype | Show proposed UI with a clear mockup label / let readers inspect a separate interactive prototype. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

Save every RFC; when HTML was requested, rerender its folder after each revision.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Proposed`, `Accepted`, `Implemented`, `Rejected`, `Superseded`.

`Implemented` means built and verified; `next` names any eval still owed. For `Superseded`, name the replacement in the lead.

Then the `#` title, a lead paragraph stating the answer, and the Structure sections.

Scratch notes and long requests: `<workspace>/.octocode/octocode-rfc-generator/tmp/`.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
