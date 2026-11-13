# Output

<!-- report-contract {"statuses":["Pass","Fail","Partial"],"required":["Finding","Steps","Gaps"],"anyOf":[]} -->

Lead with the supported finding, source URL or capture path, observed post-action state and gaps. Keep screenshots beside the claim, with alt text and viewport context. Link full JSON, HAR and traces. Choose a [format](references/report-output.md#choose-the-format) only when it clarifies the evidence.

## Saved result

Captures keep their returned formats under `<workspace>/.octocode/octocode-chrome-devtools/tmp/`; durable `session` captures use `<workspace>/.octocode/chromedevtools/<id>/`. Reports use `<workspace>/.octocode/octocode-chrome-devtools/<YYYY-MM-DD>-<slug>.md`. Workspace is the repository root, else cwd, never an existing `.octocode/`. A user-named destination wins; subagent scratch does not.

## Saved report

Save when requested or when captures and steps need a reusable report; otherwise answer in chat.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Pass`, `Fail`, `Partial`.

After the `#` title and lead, include `## Finding`, `## Steps` and `## Gaps` (`None.` when none). Steps is one table:

| # | Action | Observed state | Capture | Result |
|---|---|---|---|---|

Result: `Pass`, `Fail`, or `Partial`. Capture files (screenshots, HAR, traces): add `captures: <workspace-relative capture folder>` to the header and a `## Captures` section linking each; inline-only captures get `inline only` in the Capture cell.

For unfinished work, set `next` and retain state, evidence, attempts, blockers, constraints and required target/frame/session IDs. When someone else's action is needed, lead with situation, background, assessment and recommendation (SBAR).

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
