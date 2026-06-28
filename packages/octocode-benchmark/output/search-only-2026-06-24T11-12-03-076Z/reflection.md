# Reflection

## What Worked
- 20 row(s) passed with raw command artifacts preserved.
- Existing command ledger and raw outputs were sufficient to reconstruct schema-valid summaries.

## What Did Not Work
- 3 row(s) were partial/warn and 0 row(s) failed.

## Missing
- Legacy manifest did not capture the exact git commit at original run time; migrated summary records the current checkout and keeps gitDirty true when applicable.

## Possible Improvements
- Use benchmark/octocode/run-live-smoke.mjs for future live runs so summaries are schema-valid at creation time.

## Praises
- The original run preserved raw stdout/stderr and commands.ndjson, making repair auditable.

## Ratings

See ratings.json and summary.json.

## Next Fix

- Keep search-only-2026-06-24T11-12-03-076Z under validate-output-runs.mjs so summary drift fails fast.
