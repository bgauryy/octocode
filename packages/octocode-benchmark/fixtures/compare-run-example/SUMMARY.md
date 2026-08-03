# Benchmark Fixture Summary

Fixture-only contract data. This is not a real benchmark run and makes no
claim about product quality.

## Goal
Validate that the committed comparison fixture exercises the benchmark output
and loop-report contracts without network access, model calls, or private run
artifacts.

## KPI
- Primary (lagging): fixture contract validity (higher is better);
  baseline=Unavailable, result=fixture files present, target=all required
  fixture checks pass.
- Guardrails: remain explicitly fixture-only; do not interpret the example
  scores or token counts as measured product results.

## Loop level
suite

## Budget / trials
One deterministic fixture; no solver or judge trials.

## Subject changed
Fixture contract files only.

## Harness unchanged? (yes/no)
yes

## Checks run
- `node skills/octocode-graph-eval/scripts/loop-report.mjs --input packages/octocode-benchmark/fixtures/compare-run-example/SUMMARY.md --json` — required loop-report check.
- Benchmark fixture schema and link checks — run by the benchmark test suite;
  no live product benchmark was executed.
- Held-out: Unavailable for a deterministic contract fixture.

## Transcript note
No solver transcript exists; all values are synthetic fixture data.

## Verdict
CONTINUE

## Next
Keep this fixture in the benchmark validation suite. ACCEPT is reserved for a
real measured loop with completed checks.
