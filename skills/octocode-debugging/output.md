# Communicating a runtime finding

Give the answer and the observation that supports it. Include the scope, units or uncertainty that affect how the user should interpret it. For a wrong-value diagnosis, the observed transition and writer’s frame can be enough. For a crash dump, distinguish the faulting frame from any inference about an earlier cause.

Choose the presentation for the question. A time series can show when memory grew. A trace can show which dependency held up a request. A stack profile can show which callers consumed CPU. For Node metrics, the bundled [timeline viewer](scripts/metrics-report.mjs) plots a capture and exposes its interval values.

## Saved report

<!-- report-contract {"statuses":["Measured","Verified fix","Unresolved"],"required":[],"anyOf":[]} -->

When a file serves the task, use the user's format and destination. Markdown, HTML, Word and PDF can all carry the same finding. Link the selected evidence or time range and check that the delivered artifact opens with its visuals and links intact.

For the Octocode report viewer, the default Markdown location is `<workspace>/.octocode/octocode-debugging/<YYYY-MM-DD>-<run>.md`. Its [report header and rendering instructions](references/report-output.md) apply to that format. Accepted `status` values: `Measured`, `Verified fix`, `Unresolved`.
