Load when checking a saved report before delivery, rendering it to HTML, or finding saved reports.

<!-- Generated copies: edit skills/common/references/report-render.md, then run skills/common/build.mjs. -->
# Check, render and retrieve a report

## Check each save

After every save of a report, before delivering it, run `node <skill-folder>/scripts/render-report.mjs <file.md> --check` (`--json`: one JSON line). It writes no HTML. It checks the file against its owner's `report-contract`: the skill whose `.octocode/<skill>/` folder holds it, else the skill whose script runs. `--help` lists the rules, the same ones `--strict` applies.

| Exit | Output | Do |
|---|---|---|
| 0 | `passed: N reports checked against <skill>` (`"validation":"passed"`) | Deliver |
| 2 | One `violation: <file>: <problem>` line each (`warnings[]`) | Fix each, save, rerun until exit 0 |
| 1 | stderr names a missing file, a bad option or a malformed contract | Fix the command; never call the report checked |

## Render and retrieve

Destination, header and format are in [report output](report-output.md). Apply [HTML delivery](presentation.md#html-delivery) after completing the task. Saving Markdown does not request HTML. The commands below apply when HTML was requested or the user accepted the offer.

1. Run `node <skill-folder>/scripts/render-report.mjs report.md --out report.html --json --strict`. Exit 2 means fix reported validation issues and rerun; no new HTML was written, so do not deliver an older file as the result.
2. Success reports `render: unverified`. Open the page and run **Render check** until Ready; it checks diagrams, widgets and image loading. Inspect alt text, readable sizing and nearby source links; check other relevant links too. Correct failures or disclose unverified delivery. Browser unavailable: say rendering was not verified; generation alone is not visual verification.
3. Rerender after edits; deliver the HTML link and useful source/data links. `--open` is opt-in. A folder renders a navigable library; `.octocode/` Markdown defaults to sibling HTML, other destinations come from the command result.
4. Discover with `node <skill-folder>/scripts/render-report.mjs <workspace>/.octocode/<skill-name> --manifest --query "topic"`; follow returned `next` arguments for remaining records. Use `--all` only for file processing, not model context.
5. Read relevant Markdown only; never load generated HTML just to review content. Use compact `--json` results. Do not equate artifact bytes with token savings. Viewer changes also require narrow-screen, light/dark, keyboard, long-table and print checks; disclose missing checks.
