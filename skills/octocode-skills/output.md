# Output

<!-- report-contract {"statuses":["Ready","Needs work"],"required":["Unchecked"],"anyOf":[["Findings","Changes"]]} -->

## Response

Lead with **Ready** or **Needs work** for the checked scope. Return findings or changes, evidence, checks run and unchecked areas. Separate structural facts, editorial judgment and observed activation; rate only against stated criteria.

- Review: each material finding's location, trigger, impact, smallest repair and deciding check, most consequential first; name missing checks. For collections, assess each skill in a row.
- Edits: changed skills/resources and validation. For shortening, report discovery, entry and selected-branch size separately with units; do not infer runtime gains.
- Discovery: source/revision, skill path/name, fit, host requirements, license and unchecked risks. No installation unless authorized.
- Install/sync: destination plan, applied results, conflicts, recovery and discovery verification. File existence is not activation.

Checker JSON ([fields](references/run-checks.md#3-run-structural-checks)) and sync JSON (a plan or applied results) are not quality scores. Use prose for simple results and tables for exact comparisons. Load [format guidance](references/report-output.md#choose-the-format) only for a richer report; render HTML when requested or accepted.

## Saved report

Keep authored skills in their canonical folders. Save a separate review when requested or useful for a durable handoff, under `<workspace>/.octocode/octocode-skills/<YYYY-MM-DD>-<slug>.md` unless another destination is named.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Ready`, `Needs work`.
State the answer under the title. Include `## Findings` or `## Changes`, plus `## Unchecked`. Findings record skill, location, evidence, impact, repair and check; changes record each skill's actual edits and validation. Label finding kind (`lint`, `editorial`, `activation`) and severity with its justification.

For requested HTML, use [render commands](references/report-output.md#render-and-retrieve) and the bundled [renderer](scripts/render-report.mjs) (`--help` for arguments). Preserve Markdown/data sources; distinguish generated from visually verified output. Use `--manifest` for prior-report discovery.
