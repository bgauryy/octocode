# Output

<!-- report-contract {"statuses":["Complete","Partial","Blocked"],"required":["Answer","Pages covered","Blocked or unread","Session"],"anyOf":[]} -->

Lead with the answer and confidence (verified: span read; partial; inferred; never a percentage), each claim beside its source URL and saved artifact path, then coverage and blocked or unread scope. Separate source content from interpretation; keep source text, HTML, JSON and captures in [session formats](references/corpus.md).

## Saved result

Sessions use `<workspace>/.octocode/octocode-scraping/tmp/<sessionId>/`; `--out` stays under `<workspace>/.octocode/`. Reports use `<workspace>/.octocode/octocode-scraping/<YYYY-MM-DD>-<slug>.md`. Workspace is the repository root, else cwd, never an existing `.octocode/`. A user-named report path wins; subagent scratch does not. Use a [coverage table or extracted JSON/CSV](references/report-output.md#choose-the-format) when useful; attribute images and link full captures.

## Saved report

Save when requested or when extracted records and coverage need a reusable report; otherwise answer in chat. Link fetched pages and `reports/summary.md`; never paste them.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Complete`, `Partial`, `Blocked`.

Then the `#` title and a BLUF lead paragraph (answer first). Add `session: <workspace-relative session folder>` to the header. Always include `## Answer`, `## Pages covered`, `## Blocked or unread`, and `## Session` (the folder and its `AGENT_INDEX.json`).

Fill `## Pages covered` with the output of `node <skill-folder>/scripts/corpus-inspect.mjs --session-dir <d> --format coverage-md`: a count line and a `| URL | Status | Session file | Notes |` table with one row per `sources.jsonl` row plus each URL the latest fetch refused or skipped for robots, URLs and notes redacted. Paste it unchanged; rows whose Status ends in `failed`, or is `refused` or `robots skipped`, also go under `## Blocked or unread`. Exit 1 means the session has no source rows.

`## Blocked or unread` is the stop report: a `Partial` result hands over unfinished work as a Handoff packet; a `Blocked` one asks for someone else's action as an SBAR ([recovery](references/recovery.md#on-a-stop)). The header `next` is that report's next action.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
