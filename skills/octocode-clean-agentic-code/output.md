# Output

<!-- report-contract {"statuses":["Done","Partial"],"required":["Removed","Deferred","Checks"],"anyOf":[]} -->

Lead with the outcome. Connect each removal or consolidation to its redundancy proof and behavior check. Explain deferred candidates and unrun checks; the source diff carries the implementation.

## Saved result

Save when requested, required by project instructions, or a durable removal record serves the task; otherwise answer in chat. Keep approved edits in source. The default report path is `<workspace>/.octocode/octocode-clean-agentic-code/<YYYY-MM-DD>-<slug>.md`; use the repository root, else the current directory, without nesting `.octocode/`. A user- or parent-named destination wins.

## Saved report

For the report viewer, use the [shared header](references/report-output.md#report-header), then a title and `## Removed`, `## Deferred`, `## Checks`. Choose concise prose, a table or a focused diff; empty sections can say `None.`. These fields belong to the viewer format, not ordinary answers. Accepted `status` values: `Done`, `Partial`.

Put report-only defects under `Deferred`, explaining the masked failure, impact and recommended repair. Distinguish them from candidates whose redundancy remains unresolved.

Use the [format guide](references/report-output.md#choose-the-format) only when presentation needs it. For accepted HTML, follow [rendering and retrieval](references/report-output.md#render-and-retrieve) and [delivery](references/presentation.md#html-delivery).
