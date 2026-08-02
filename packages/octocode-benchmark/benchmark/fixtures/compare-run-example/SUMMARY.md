# Benchmark Fixture Summary

## Goal
Validate the benchmark output contract without requiring network, model calls, or private run artifacts.

## KPI
Fixture includes a minimal `kpi.json` conforming to `benchmark/schemas/kpi.schema.json`.

## Loop level
Harness-contract smoke fixture.

## Checks run
Schema and link validation only. This is not a scored product benchmark.

## Verdict
PASS as a fixture when `yarn workspace @octocodeai/octocode-benchmark test` succeeds.
