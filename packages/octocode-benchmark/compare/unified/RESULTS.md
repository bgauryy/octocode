# Benchmarks

The [unified agent benchmark](README.md) compares Octocode MCP with a shell using `rg` and REST-only `gh`. Both workers answer the same pinned questions using concrete Sonnet 5.5. Opus 5.5 grades every pair in both answer orders. References stay outside the solver filesystem boundary.

## Repaired candidate — 2026-10-01

[Full report](results/production-review-fixed-v3-20261001/REPORT.md) · [each question and worker](results/production-review-fixed-v3-20261001/PER_QUESTION.csv).

Run `production-review-fixed-v3-20261001` completes 60 valid workers and incremental reflections, four readiness/isolation probes, and 60 primary judging orders. Independent raw-stream reconciliation finds no metric/cost discrepancy; all 71 frozen source, executable, configuration, build, dependency and corpus checks match. The candidate uses Node 24.15.0, Claude Code 2.1.286 and macOS arm64 debug interfaces.

| Metric | Octocode MCP | rg/gh CLI |
|---|---:|---:|
| Mean quality, 0–10 | 8.3833 | 8.975 |
| Raw research-session tokens | 2,361,178 | 940,528 |
| Reported research-session cost | $2.2462194 | $1.7094944 |
| Tool invocations | 125 MCP | 83 Bash |
| Initial context probe | approximately 16.4k | approximately 4.7k |

Octocode trails by 0.5917 quality points, uses 2.51× aggregate raw tokens and 1.31× reported research cost. Initial context includes each profile's instructions and tool exposure; the whole difference cannot be attributed to schemas alone. Raw tokens count cache reads at full weight. All cache writes have verified one-hour TTL; weighted tokens are unknown without a frozen tariff. Context/research decomposition is estimated, not per-request verified accounting.

Five distinct native calls fail and recover: four structured row errors plus one top-level-only error, overlapping three explicit SDK error results. No transport errors occur. MCP and Bash invocations are different units; upstream HTTP counts were not persisted. Only six of thirteen offered MCP tools are selected, with no AST, LSP, clasify or artifact calls. Dedicated tool acceptance covers those families separately; this trial does not measure their agent-selected quality.

The generated report uses a ±0.5-point tie band: three Octocode wins, fourteen ties and thirteen losses. Exact numeric comparison gives six wins, six equal scores and eighteen losses. One paired trial and an uncalibrated judge do not establish production thresholds. Both orders agree on preferred answers in 24/30 cases. Reference issues and wrong claims remain in the full report.

Reported trial Claude cost totals $14.6727408: research $3.9557138, incremental reflections $3.202548, probes $0.104092, and judge $7.410387. These exclude earlier failed runs, smokes, tool validation and review-agent usage; they are not independently verified invoices. Classification telemetry is absent in question sessions, rather than a fabricated numeric zero. Provider pricing/failed-attempt billing and total system cost remain unknown.

The published-core release gate separately fails because the expected authored core 19.1.6 is absent from npm. Local contract equality does not satisfy publication order. Cross-platform release and independently calibrated quality acceptance remain pending.

## Historical evidence

[Run full-1](results/full-1/REPORT.md) used an earlier build and harness. Its provisional stream accounting, assumed cache weights and weaker isolation do not establish current costs, efficiency or production quality. Preserve its receipts as historical diagnostics; do not treat the new trial as a controlled causal improvement over it. Retired scripted tool scores likewise do not establish agent-selected quality.

## Next evaluation

Measure smaller context profiles and deciding-source/citation verification on held-out questions, with predeclared quality gates and judge calibration. Explicitly test AST/LSP/semantic routing where those tools should change the next action. Preserve complete accounting, strict isolation and original failed receipts.

Use concrete model names and the canonical prerequisites in the [harness guide](README.md):

```bash
cd packages/octocode-benchmark/compare/unified
node run.mjs --run-id <new-id> --model claude-sonnet-5-5 --probes
node judge.mjs --run-id <new-id> --model claude-opus-5-5
node report.mjs --run-id <new-id>
```
