# Output

<!-- report-contract {"statuses":["Ready","Needs work"],"required":["Unchecked"],"anyOf":[["Findings","Changes"]]} -->

Markdown, task content only. `skill-review.mjs --json` returns `errorCount`, `warnCount`, and per-skill `results` (structural findings, not quality scores); `skill-sync.mjs --json` returns a destination plan or applied results.

## Response

- Review: BLUF readiness verdict; findings as review comments (Conventional Comments): label (issue, suggestion, question, nitpick, praise), blocking or not, file anchor, evidence, why (impact, kind), repair and its check; unchecked areas.
- Edits: changed skills, support-file changes, checks run.
- Install: destination, reload step.

## Presentation choices

Readiness verdict → findings or changes → checks and unchecked areas. Separate structural, editorial and activation evidence.

| Format | Use when / why |
|---|---|
| Short comparison table / finding subsections | Compare skills against the same criteria; move long evidence into sections. |
| `flowchart` / `agentflow-beta` | Explain skill activation, resource routes or agent task flow. |
| `chart` bar / linked JSON | Compare measured activation outcomes / preserve checker results; do not turn editorial impressions into measured scores. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

Authored skills stay in their canonical folders; a separate review goes to `<workspace>/.octocode/octocode-skills/<YYYY-MM-DD>-<slug>.md` unless the user names a path. Save a review, or a multi-file change report, when it has a table, a diagram, over ~40 lines or five findings, or the user asks; keep every result when the user or project instructions say so.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Ready`, `Needs work`.

A lead paragraph under the `#` title states the answer.

Put findings under `## Findings` as one table, most severe first, then add `## Unchecked` for areas that were not reviewed:

| Skill | Location | Finding | Kind | Severity | Fix |
|---|---|---|---|---|---|

`Kind`: `lint`, `editorial`, or `activation`; severity: `Critical`, `High`, `Medium`, or `Low`. A change report has `## Changes`, one row per edited file, ending with the `skill-review.mjs` result. Always include `## Unchecked` for remaining verification gaps. Every saved report has either `## Findings` (a review) or `## Changes` (a change report).

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
