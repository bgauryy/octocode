# Octocode Benchmark

Head-to-head evaluation of octocode against baseline research toolchains —
same model, same bank questions, same budget; the only variable is the tool.

**Status: v2, NOT YET SCORED.** Ledgers: [`results/`](results/). v1 suites,
their runs, and their evidence were removed from the tree on 2026-08-03; v1
numbers must not be cited as current.

## Metric

[**VRPT**](SCORING.md) = `harmonic_mean(Correctness, Precision, Recall) / 10` per 100k tokens —
three judge scores (1–10 each): did it answer correctly, avoid false outputs, and find
everything? Per-question **median** is primary; `VR ≥ 0.6` floor; tool-property
KPIs (p90 tail tokens, consistency, raw emission, signal ratio) reported
alongside. Judged blind per [JUDGING.md](JUDGING.md).

**Verdict gates (hard):** VRPT's denominator is provider **runner tokens** — a
WIN requires `tokenSource=runner` (estimated byte proxies are reported as
`VRPT-est`, DRAFT only), `nEligible ≥ 12`, `k ≥ 3`, and non-overlapping 95% CIs
on the deciding metric. See [SCORING.md](SCORING.md).

## Suites

| Suite | Arms (A vs B) | Bank | Status |
|---|---|---|---|
| [octocode-vs-gh](compare/octocode-vs-gh/) | `gh` CLI vs Octocode MCP (remote GitHub) | [github/research-v2](questions/github/research-v2/) · Q1–Q14 | [NOT YET SCORED](results/octocode-vs-gh.md) |
| [octocode-vs-gh-rtk](compare/octocode-vs-gh-rtk/) | `rtk`+`gh` vs Octocode MCP (remote GitHub) | [github/research-v2](questions/github/research-v2/) · Q1–Q14 | [NOT YET SCORED](results/octocode-vs-gh-rtk.md) |
| [octocode-vs-ast-grep](compare/octocode-vs-ast-grep/) | `ast-grep` vs Octocode CLI (local) | [local-code/ast-grep-react-v2](questions/local-code/ast-grep-react-v2/) · Q1–Q10 | [NOT YET SCORED](results/octocode-vs-ast-grep.md) |

Questions and judge-only oracles live once per bank under
[`questions/`](questions/); each compare folder holds only the run contract.
Every suite runs three arms — no-tools control (contamination), baseline,
treatment.

## Run a benchmark

1. Follow [`INSTRUCTIONS.md`](INSTRUCTIONS.md) — freeze the bank, run
   the control, isolate solver trials, measure, judge blind, report.
2. Write run artifacts to gitignored `output/<run-name>/` per
   [`REPORT_TEMPLATE.md`](REPORT_TEMPLATE.md); machine rollup per
   [`schemas/kpi.schema.json`](schemas/kpi.schema.json)
   (exemplar: [`fixtures/compare-run-example/`](fixtures/compare-run-example/)).
3. Refresh the suite's tracked ledger at `results/<suite>.md`, latest run
   first, append-only.

## Docs

| Doc | Owns |
|---|---|
| [README.md](README.md) | methodology: arms, method, metrics, validity gates |
| [INSTRUCTIONS.md](INSTRUCTIONS.md) | run steps: freeze → solve → measure → judge → report |
| [JUDGING.md](JUDGING.md) | blind judge protocol (2 stages, anchor verification) |
| [SCORING.md](SCORING.md) | VRPT formula, aggregation rules, tool-property KPIs |
| [REPORT_TEMPLATE.md](REPORT_TEMPLATE.md) | report shape per run |
