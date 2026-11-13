# Output

<!-- report-contract {"statuses":["APPROVE","COMMENT","REQUEST_CHANGES"],"required":["Answer","Scope","Findings","Evidence","Gaps","Checks"],"anyOf":[]} -->

Cite full blob URLs remotely and `file:line` locally. When findings need cross-references, use labels such as `SEC-1`; bare `#N` auto-links GitHub issues.

## Response

Lead with the verdict, the main risk and what was verified. Then give the most consequential findings, each with location, evidence, impact and fix, followed by material gaps. When findings exceed the summary, keep every supported finding in the saved report and link it.

## Findings

Order findings by severity; a table helps compare several:

| ID | Location | Finding and evidence | Impact | Severity | Confidence | Fix |
|---|---|---|---|---|---|---|
| `SEC-1` | `src/auth.ts:42` | `issue (blocking):` the mechanism, with its exact source anchor | Who or what breaks, and when | `High` | `high` | The smallest repair |

- **Finding:** state the defect and whether it blocks. Use Conventional Comments labels when the review destination expects them.
- **Severity** measures impact: `Critical`, `High`, `Medium`, `Low`.
- **Confidence** measures proof: `high` confirmed, `medium` likely, `low` uncertain.
- **No findings:** say so under `## Findings` and name the scope that makes that claim hold.

## Saved report

Save when the review has several findings, a diagram or evidence comparison helps, or the user or project instructions ask for a record. A short review can stay in chat.

Path: `<workspace>/.octocode/octocode-pr-review/<YYYY-MM-DD>-pr-<number>.md`, or `<YYYY-MM-DD>-review-<branch>.md` without a PR; intermediate files go in its `tmp/`. `<workspace>` is the repository root, else the current directory, never a folder inside `.octocode/`. A user-named path wins.

Use the [shared header](references/report-output.md#report-header). `status` is the verdict. Accepted `status` values: `APPROVE`, `COMMENT`, `REQUEST_CHANGES`.

Title the report with the reviewed target and verdict; lead with the recommendation, main risk and what was verified.

Include, in this order:

- `## Answer`: verdict, confidence and why.
- `## Scope`: repository, reviewed revision (head or merge SHA, or working tree), and files read.
- `## Findings`: the table above.
- `## Evidence`: anchors behind the findings, and checked non-findings with what cleared them.
- `## Gaps`: unread files, unproven claims and any assumption marked as one.
- `## Checks`: each check run or skipped, with its result.
- `## Next`: only when something is pending.

Use a [`flowchart` or `sequenceDiagram`](references/report-output.md#choose-the-format) when a verified path explains a finding, with source anchors nearby.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
