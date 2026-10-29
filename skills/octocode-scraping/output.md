# Output

<!-- report-contract {"statuses":["Complete","Partial","Blocked"],"required":["Answer","Pages covered","Blocked or unread","Session"],"anyOf":[]} -->

Deliver task content only; no probe output or process metadata. Answers and citations in Markdown; source text, HTML, JSON, and captures stay in session formats ([corpus](references/corpus.md)).

## Response

BLUF: answer and confidence first, then evidence (Claim–evidence: each claim beside its source URL and saved artifact path), pages covered, and blocked or unread portions.

## Saved result

Session pages: `<workspace>/.octocode/octocode-scraping/tmp/<sessionId>/` (`--out` must stay under `<workspace>/.octocode/`). Report: `<workspace>/.octocode/octocode-scraping/<YYYY-MM-DD>-<slug>.md`. `<workspace>` is the project root (repository root, else current directory), never inside an existing `.octocode/`. A user-named path wins; a harness or subagent scratch folder such as `.octocode/tmp/` does not: save here and link from scratch notes.

## Presentation choices

Answer → covered pages → blocked or unread portions → source artifacts. Keep source content distinct from interpretation.

| Format | Use when / why |
|---|---|
| Coverage table / source links | Make retrieved, blocked and unread evidence explicit. |
| Markdown images | Show a relevant captured figure or UI state with attribution and capture context. |
| `flowchart` / linked JSON or CSV | Explain a meaningful crawl relationship / provide reusable extracted records; avoid decorative page-count charts. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

Save when the result has a table, a diagram, over ~40 lines or five findings, or the user or project instructions (`AGENTS.md`) ask; keep a short answer in chat. Link fetched pages and `reports/summary.md`; never paste them.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Complete`, `Partial`, `Blocked`.

Then the `#` title and a BLUF lead paragraph (answer first). Add `session: <workspace-relative session folder>` to the header. Always include `## Answer`, `## Pages covered`, `## Blocked or unread`, and `## Session` (the folder and its `AGENT_INDEX.json`):

| URL | Status | Session file | Notes |
|---|---|---|---|

`## Blocked or unread` is the stop report: a `Partial` result hands over unfinished work as a Handoff packet; a `Blocked` one asks for someone else's action as an SBAR ([recovery](references/recovery.md#on-a-stop)). The header `next` is that report's next action.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
