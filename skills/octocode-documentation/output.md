# Output

<!-- report-contract {"statuses":["Needs work","Ready"],"required":["Unverified"],"anyOf":[["Findings","Destination"]]} -->

Return the document in its requested format and repository style, or a Markdown review: verdict, [review comments](references/style-guide.md#review-procedure), claim/link checks and unverified facts.

## Saved result

| Result | Location |
|---|---|
| Shipped document (README, guide, ADR) | The requested path or project docs location |
| Review, audit, or draft with no named destination | `<workspace>/.octocode/octocode-documentation/<YYYY-MM-DD>-<slug>.md` |

A user-named path wins.

Preserve the document's natural structure. Use [tables, diagrams or images](references/report-output.md#choose-the-format) only to clarify comparisons, relationships or UI evidence; keep prerequisites and expected results beside commands.

## Saved report

Save when requested or when the review or draft needs a reusable artifact; otherwise answer in chat.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Needs work`, `Ready`.

Open the `#` title with a lead stating the answer.

| Saved report | After the lead | Then |
|---|---|---|
| Review | `## Findings`: one table, most severe first | `## Unverified`: facts that still need a check |
| Draft | The document | `## Destination` (where it ships), then `## Unverified` |

Every saved report has either `## Findings` (a review) or `## Destination` (a draft). The findings table:

| Location | Issue | Correction | Severity | Verified |
|---|---|---|---|---|

Severity: `Critical`, `High`, `Medium`, `Low`; `Verified`: `Yes` or `No`.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
