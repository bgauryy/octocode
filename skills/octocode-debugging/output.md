# Communicating a runtime finding

Give the answer and the observation that supports it. Include the scope, units or uncertainty that affect how the user should interpret it. For a wrong-value diagnosis, the observed transition and writer’s frame can be enough. For a crash dump, distinguish the faulting frame from any inference about an earlier cause.

Match the presentation to the question: a time series for when memory grew, a trace for which dependency held up a request, a stack profile for which callers consumed CPU. For Node metrics, the bundled [timeline viewer](scripts/metrics-report.mjs) plots a capture and exposes its interval values.

## Saved report

<!-- report-contract {"statuses":["Measured","Verified fix","Unresolved"],"required":[],"anyOf":[]} -->

When a file serves the task, use the user's format (Markdown, HTML, Word or PDF) and destination. Link the selected evidence or time range, then open the delivered artifact and check that its visuals and links work.

For the Octocode report viewer, the default Markdown location is `<workspace>/.octocode/octocode-debugging/<YYYY-MM-DD>-<run>.md`. Its [report header and rendering instructions](references/report-output.md) apply to that format. Accepted `status` values: `Measured`, `Verified fix`, `Unresolved`.
