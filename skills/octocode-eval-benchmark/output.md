# Output

<!-- report-contract {"statuses":["Keep","Discard","Accept","Revert","Inconclusive","Invalid"],"required":["Question","Conditions","Results","Guardrails","Limits","Decision"],"anyOf":[]} -->

Task content only: no probe output, traces, receipts, run IDs, or process metadata in report prose (run-folder paths and receipt files in the run folder are evidence, linked not pasted). Lead with the verdict; visualize metric comparisons and retain exact values in tables. Machine results stay JSON or JSONL.

## Response

BLUF: result and confidence, then question, checks, limits, next check; a comparison adds baseline and candidate conditions (scope, budget), metrics, guardrails, uncertainty, coverage, failures, cost.

## Saved result

Save and link run evidence at `<workspace>/.octocode/octocode-eval-benchmark/<YYYY-MM-DD>-<slug>/`, never inside the installed skill. `<workspace>`: repository root, else current directory, never inside an existing `.octocode/`. A user-named path wins; a scratch folder (`.octocode/tmp/`) does not: save here, link from scratch notes.

## Presentation choices

Question and conditions → results → guardrails and limits → decision. Keep baseline and candidate comparable.

| Format | Use when / why |
|---|---|
| `chart` bar or line | Compare measured outcomes / ordered observations; label units, sample size and uncertainty nearby. |
| Metric tables | Expose exact values, failures and split coverage; distinguish development from held-out results. |
| Linked JSON/JSONL or CSV | Preserve reusable raw results without duplicating them in the report. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

Save every measured run, even a smoke test, with `README.md` as entry point. Editorial and regression results follow Response; save them under the general rule (table, diagram, about 40 lines, or on request), writing `None.` under sections that do not apply.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Keep`, `Discard`, `Accept`, `Revert`, `Inconclusive`, `Invalid`.

Then the `#` title and a lead paragraph stating the answer. Always include `## Question`, `## Conditions`, `## Results`, `## Guardrails`, `## Limits`, and `## Decision`, with a metric table per split under Results. `Keep` and `Discard` mark development evidence. Link JSON or JSONL results in `dev/` and `held-out/`; don't paste them.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
