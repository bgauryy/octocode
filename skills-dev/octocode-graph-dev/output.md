# Output

Follow the [shared presentation and HTML delivery rules](references/presentation.md); retain this skill's artifact format.

## Response

BLUF: what changed in the engine and whether the gate held. Then claim–evidence: the tests and commands run with their results, and the `bench-compare.mjs` table for any battery run. Name rows not run, tests not run, and every recall drop with its reason. Unfinished work: Handoff packet.

## Saved report

Save when a change ran the battery or settled an open decision; otherwise answer in chat. Use `<workspace>/.octocode/octocode-graph-dev/<YYYY-MM-DD>-<slug>/` with `report.md` beside the complete `before.json` and `after.json` (pass `--out <dir>/before.json` and `--out <dir>/after.json`; the default `bench.json` is for a single run); scratch goes in its `tmp/`.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Ready`, `Needs work`. After the lead, add `## Comparison` (the `bench-compare.mjs` table), `## Explained rows` (each miss, extra and recall drop with its source evidence), and `## Checks` (tests run, versions bumped, doc sections updated).

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.

Protocol definitions: [shared catalog](references/protocol-catalog.md#catalog).
