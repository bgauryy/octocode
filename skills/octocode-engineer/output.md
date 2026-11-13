# Output

<!-- report-contract {"statuses":["Proposed","Accepted","Needs work"],"required":["Findings","Decision","Checks"],"anyOf":[]} -->

Lead with the decision or finding, then the evidence, material trade-offs and remaining uncertainty. Answer in chat unless a saved result serves the task.

## Design answer

Recommend the simplest option that fits the workload. Explain the deciding trade-off, strongest alternative and what would change the recommendation. Include relevant failure modes and the test or measurement that can settle the choice.

## Review findings

Order findings by impact. Give the location, trigger, consequence and proposed repair with its evidence. Separate confirmed defects from unresolved candidates; identify missing checks rather than assigning decorative scores.

## Decision record

For consequential work, preserve the problem, constraints, decision, rationale, alternatives, accepted risks and acceptance check. Include migration and rollback when they affect the choice. A small change needs only its effect and verification. Record coverage gaps that limit the conclusion; do not build a second workflow ledger.

## Presentation choices

Use a comparison table or boundary diagram when it clarifies the decision. Mark uncertain relationships. The [format guide](references/report-output.md#choose-the-format) supplies optional syntax.

## Saved report

Save when requested, required by project instructions, or a durable decision record serves the task. Keep approved edits in source. The default report path is `<workspace>/.octocode/octocode-engineer/<YYYY-MM-DD>-<slug>.md`; use the repository root, else the current directory, without nesting `.octocode/`. A user- or parent-named destination wins.

For the report viewer, use the [shared header](references/report-output.md#report-header), then a title and `## Findings`, `## Decision`, `## Checks`. Use prose or tables to fit the content; an empty section can say `None.`. These fields belong to the viewer format, not ordinary answers. Accepted `status` values: `Proposed`, `Accepted`, `Needs work`.

For accepted HTML, follow [rendering and retrieval](references/report-output.md#render-and-retrieve); [presentation](references/presentation.md#html-delivery) covers delivery.
