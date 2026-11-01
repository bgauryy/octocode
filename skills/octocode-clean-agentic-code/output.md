# Output

<!-- report-contract {"statuses":["Done","Partial"],"required":["Removed","Deferred","Checks"],"anyOf":[]} -->

## Response

BLUF: outcome first, then removed or consolidated items as claim–evidence (proof of redundancy, behavior preserved), checks run, uncertain deferrals. Markdown, task content only (no probe output or process metadata); the source diff is the implementation.

## Saved result

Approved edits stay in place. Report: `<workspace>/.octocode/octocode-clean-agentic-code/<YYYY-MM-DD>-<slug>.md`; `<workspace>` = repository root, else current directory, never inside an existing `.octocode/`. A user- or parent-named path wins; harness or subagent scratch folders do not (link from scratch notes).

## Presentation choices

Removed or consolidated items → proof → preserved behavior → deferred candidates and checks.

| Format | Use when / why |
|---|---|
| Short evidence table / `diff` fence | Tie each removal to proof and its behavior check; show focused changes. |
| `flowchart` | Explain ownership or dependency changes when the refactor is hard to follow. |
| `chart` bar | Compare measured size or runtime before/after; fewer lines alone do not prove better quality. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

Save on a table, diagram, over ~40 lines or five findings, or user request; else answer briefly in chat. Save every result when user or project instructions say so.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Done`, `Partial`.

Then the `#` title and a lead paragraph stating the answer. Always include `## Removed`, `## Deferred`, and `## Checks` (write `None.` under an empty one), with these tables under the first two (Removed rows are claim–evidence; Deferred rows are unresolved hypothesis tests, each with the check that would settle it):

| Path | Change | Proof of redundancy | Behavior check |
|---|---|---|---|

| Item | Why deferred | What would settle it |
|---|---|---|

Report-only defects (error masking, stubs, test or grader gaming) go under `## Deferred` as one SBAR line each below the table.

End `## Checks` with the change command, such as `git diff -- <paths>`.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
