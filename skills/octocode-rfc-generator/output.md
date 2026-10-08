# Output

Deliver one Markdown RFC, defaulting to `RFC.md`. Keep the decision and execution plan together. Never add probe output or generated process metadata to the deliverable.

## Structure

Start with a title and a short proposal. Use the sections that help a reviewer decide; combine or omit sections as appropriate. Headings are adaptable, and no frontmatter or metadata table is needed.

| Section | Useful content |
|---|---|
| Summary | Proposed decision, practical effect, and why it is worth making. Say plainly if the choice remains provisional. |
| Problem and context | Current behavior, affected users, concrete use cases, deciding evidence, and cost of doing nothing. |
| Goals and scope | Checkable outcomes, constraints, and meaningful exclusions. |
| Proposed design | Intended behavior with examples; affected boundaries, interfaces, data flow, failure handling, compatibility, and security where relevant. Use a diagram when it clarifies the design. |
| Alternatives and rationale | Viable choices, their costs and benefits, why the proposed choice fits, and evidence that could change it. |
| Risks and mitigations | Material drawbacks, failure conditions, and how they will be prevented or detected. |
| Implementation and rollout | Prerequisites, dependency-ordered steps, affected components, migration, rollout signals, and rollback actions. Keep all steps in this file. |
| Validation and success | Observable acceptance criteria, relevant checks, measured baselines or targets when needed, and regression or rollback thresholds. Separate measurements from estimates. |
| Open questions | Unresolved decisions and execution details, their impact, and what would settle them. Include an accountable owner when action requires one. |
| References | Sources supporting the decision. Prefer citations beside claims; add a short reference list only when useful. |

A step should explain what changes, what it depends on, and how its outcome is checked. Use concise prose, a checklist, or a table according to complexity. Implementation facts belong here; research call logs and diagnostic transcripts do not.

## Language

- Use clear technical English unless the user requests another language. Write for a capable reader who does not know this system yet.
- Prefer concrete verbs, short paragraphs, consistent terms, and defined acronyms. Preserve exact code and API identifiers.
- Describe current behavior as observed fact and proposed behavior as a proposal. Explain why each significant choice helps the stated goals.
- State obligations precisely. Use “must” for a real requirement and “should” for a recommendation; formal uppercase requirement keywords need an actual protocol convention.
- Support performance, reliability, and compatibility claims with evidence. Replace vague promises with observable outcomes and name material uncertainty.
- Remove repetition, sales language, work narration, author/date/status tables, generated timestamps, worker or tool inventories, scores about the writing, and probe or review receipts. Express necessary decision state naturally in the summary.

## Delivery

Save at the requested location or follow the repository's RFC naming convention. Return a link with a brief statement of the decision and any unresolved blocker. A request for review alone returns substantive findings without creating extra files.

Keep related material inside the RFC using sections and links; do not create companion `PLAN.md`, `KPI.md`, `PREREQUISITES.md`, `IMPLEMENTATION.md`, or `RESOURCES.md` files. Refer to existing external evidence without copying it.

Only on an explicit export request, use `node scripts/render-rfc.mjs <RFC.md> --no-open` for an HTML view of the same document; `--out <file.html>` chooses its destination. Markdown remains the authored source.
