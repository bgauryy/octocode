# Run full-2026-09-30

## Tokens with and without Octocode

#### All 20 questions (60 runs per side)

| Metric | With Octocode | Without (rg + gh) | Ratio (with ÷ without) |
|---|---:|---:|---:|
| Correct answers | 48 / 60 | 56 / 60 | 0.86× |
| Partial / wrong | 12 / 0 | 4 / 0 |  |
| Mean score (0–3) | 2.80 | 2.93 |  |
| **Total tokens per run** (mean) | **209.8k** | **60.7k** | **3.46×** |
| – input, fresh | 9 | 12 | 0.74× |
| – input, cache write | 12.0k | 10.5k | 1.14× |
| – input, cache read | 195.0k | 47.5k | 4.10× |
| – output | 2.8k | 2.6k | 1.08× |
| Fixed overhead per run (est.) | 183.3k | 28.5k | 6.43× |
| Research tokens per run (est.) | 26.5k | 32.2k | 0.82× |
| Total tokens, all runs | 12.59M | 3.64M | 3.46× |
| Total tokens per pass (min–max) | 4.07M–4.32M | 997.4k–1.39M |  |
| Cost per run, USD (mean) | $0.115 | $0.078 | 1.48× |
| Tool calls per run (mean) | 4.2 | 6.5 | 0.64× |
| Wall time per run, s (mean) | 26 | 25 | 1.05× |

#### GitHub questions (10 questions, 30 runs per side)

| Metric | With Octocode | Without (rg + gh) | Ratio (with ÷ without) |
|---|---:|---:|---:|
| Correct answers | 28 / 30 | 30 / 30 | 0.93× |
| Partial / wrong | 2 / 0 | 0 / 0 |  |
| Mean score (0–3) | 2.93 | 3.00 |  |
| **Total tokens per run** (mean) | **152.8k** | **37.8k** | **4.04×** |
| – input, fresh | 7 | 10 | 0.68× |
| – input, cache write | 9.4k | 8.0k | 1.18× |
| – input, cache read | 141.4k | 27.9k | 5.08× |
| – output | 2.0k | 2.0k | 0.99× |
| Fixed overhead per run (est.) | 139.5k | 23.2k | 6.00× |
| Research tokens per run (est.) | 13.2k | 14.6k | 0.91× |
| Total tokens, all runs | 4.58M | 1.13M | 4.04× |
| Total tokens per pass (min–max) | 1.46M–1.62M | 320.1k–417.2k |  |
| Cost per run, USD (mean) | $0.085 | $0.057 | 1.49× |
| Tool calls per run (mean) | 2.7 | 4.9 | 0.55× |
| Wall time per run, s (mean) | 20 | 21 | 0.96× |

#### Local questions (10 questions, 30 runs per side)

| Metric | With Octocode | Without (rg + gh) | Ratio (with ÷ without) |
|---|---:|---:|---:|
| Correct answers | 20 / 30 | 26 / 30 | 0.77× |
| Partial / wrong | 10 / 0 | 4 / 0 |  |
| Mean score (0–3) | 2.67 | 2.87 |  |
| **Total tokens per run** (mean) | **266.8k** | **83.5k** | **3.19×** |
| – input, fresh | 11 | 14 | 0.78× |
| – input, cache write | 14.5k | 13.1k | 1.11× |
| – input, cache read | 248.5k | 67.1k | 3.70× |
| – output | 3.7k | 3.3k | 1.13× |
| Fixed overhead per run (est.) | 227.0k | 33.8k | 6.72× |
| Research tokens per run (est.) | 39.8k | 49.7k | 0.80× |
| Total tokens, all runs | 8.00M | 2.51M | 3.19× |
| Total tokens per pass (min–max) | 2.57M–2.73M | 677.3k–974.9k |  |
| Cost per run, USD (mean) | $0.145 | $0.099 | 1.47× |
| Tool calls per run (mean) | 5.7 | 8.1 | 0.70× |
| Wall time per run, s (mean) | 32 | 28 | 1.12× |

## Fixed overhead

The first model request of every run carries a fixed prefix: Claude Code's system prompt, the tool definitions and, for Octocode, the MCP server instructions, followed by the question. Every later request in the run re-reads that prefix, almost always from the prompt cache. The table measures the prefix from each run's own first request, so it reflects exactly what the solver saw.

| | With Octocode | Without (rg + gh) | Difference |
|---|---:|---:|---:|
| Input of the first request (median over runs) | 40.7k | 4.7k | 36.0k |
| Model requests per run (mean) | 4.5 | 6.1 | |
| Fixed overhead per run (first-request input × requests) | 183.3k | 28.5k | 6.43× |
| Research tokens per run (total − overhead) | 26.5k | 32.2k | 0.82× |
| Share of total tokens that is fixed overhead | 87% | 47% | |

Research tokens are everything above the fixed prefix: tool results, and the model's own earlier turns re-read on each request. The prefix includes the question itself (a few hundred tokens, the same in both arms). Cached prefix tokens are billed at a fraction of the fresh-input price, so the overhead weighs less in cost than in token counts.

## Per-question tokens

| Question | Type | With Octocode tokens | Without (rg + gh) tokens | Ratio | With Octocode correct | Without (rg + gh) correct |
|---|---|---:|---:|---:|:-:|:-:|
| [G01](../packages/octocode-benchmark/eval/QUESTIONS.md#g01) | github · locate | 125.2k | 23.3k | 5.37× | ✓✓✓ | ✓✓✓ |
| [G02](../packages/octocode-benchmark/eval/QUESTIONS.md#g02) | github · history | 129.5k | 21.3k | 6.08× | ✓✓✓ | ✓✓✓ |
| [G03](../packages/octocode-benchmark/eval/QUESTIONS.md#g03) | github · history | 149.0k | 63.5k | 2.35× | ✓✓✓ | ✓✓✓ |
| [G04](../packages/octocode-benchmark/eval/QUESTIONS.md#g04) | github · trace | 169.9k | 19.7k | 8.62× | ✓✓✓ | ✓✓✓ |
| [G05](../packages/octocode-benchmark/eval/QUESTIONS.md#g05) | github · history | 134.9k | 38.4k | 3.51× | ✓✓✓ | ✓✓✓ |
| [G06](../packages/octocode-benchmark/eval/QUESTIONS.md#g06) | github · semantic | 131.9k | 35.5k | 3.72× | ✓✓✓ | ✓✓✓ |
| [G07](../packages/octocode-benchmark/eval/QUESTIONS.md#g07) | github · trace | 245.3k | 46.1k | 5.33× | ✓✓✓ | ✓✓✓ |
| [G08](../packages/octocode-benchmark/eval/QUESTIONS.md#g08) | github · history | 141.8k | 28.5k | 4.98× | ✓✓✓ | ✓✓✓ |
| [G09](../packages/octocode-benchmark/eval/QUESTIONS.md#g09) | github · multi-hop | 129.3k | 43.9k | 2.94× | ✓✓½ | ✓✓✓ |
| [G10](../packages/octocode-benchmark/eval/QUESTIONS.md#g10) | github · semantic | 170.7k | 58.1k | 2.94× | ✓✓½ | ✓✓✓ |
| [L01](../packages/octocode-benchmark/eval/QUESTIONS.md#l01) | local · locate | 86.4k | 16.3k | 5.28× | ✓✓✓ | ✓✓✓ |
| [L02](../packages/octocode-benchmark/eval/QUESTIONS.md#l02) | local · locate | 125.8k | 26.7k | 4.71× | ✓✓✓ | ✓✓✓ |
| [L03](../packages/octocode-benchmark/eval/QUESTIONS.md#l03) | local · trace | 202.4k | 48.9k | 4.14× | ✓½½ | ½✓✓ |
| [L04](../packages/octocode-benchmark/eval/QUESTIONS.md#l04) | local · trace | 201.7k | 31.4k | 6.42× | ½½✓ | ✓½✓ |
| [L05](../packages/octocode-benchmark/eval/QUESTIONS.md#l05) | local · multi-hop | 191.6k | 38.4k | 5.00× | ✓✓½ | ✓✓✓ |
| [L06](../packages/octocode-benchmark/eval/QUESTIONS.md#l06) | local · multi-hop | 287.0k | 87.9k | 3.26× | ✓✓✓ | ✓✓✓ |
| [L07](../packages/octocode-benchmark/eval/QUESTIONS.md#l07) | local · multi-hop | 545.2k | 296.6k | 1.84× | ½✓½ | ✓✓✓ |
| [L08](../packages/octocode-benchmark/eval/QUESTIONS.md#l08) | local · semantic | 318.9k | 95.4k | 3.34× | ✓✓½ | ✓✓✓ |
| [L09](../packages/octocode-benchmark/eval/QUESTIONS.md#l09) | local · semantic | 337.2k | 78.4k | 4.30× | ✓✓✓ | ✓✓½ |
| [L10](../packages/octocode-benchmark/eval/QUESTIONS.md#l10) | local · semantic | 371.5k | 115.2k | 3.23× | ½✓½ | ✓½✓ |

Tokens are the mean total per run over 3 passes. Marks show each pass: ✓ correct, ½ partial, ✗ wrong, ? unresolved. Ratio below 1 means Octocode used fewer tokens.

| Token ratio across 20 questions | Value |
|---|---:|
| Median of per-question ratios | 4.22× |
| Mean of per-question ratios | 4.37× |
| Geometric mean of per-question ratios | 4.10× |
| Ratio of summed tokens | 3.46× |
| Questions where Octocode used fewer tokens | 0 of 20 |
| Median / mean of per-question research-token ratios | 0.92× / 1.08× |

## Correctness

| Metric | With Octocode | Without (rg + gh) |
|---|---:|---:|
| Correct / partial / wrong | 48 / 12 / 0 | 56 / 4 / 0 |
| Correct rate | 80% | 93% |
| Mean score (0–3) | 2.80 | 2.93 |
| Correct answers per pass | 17, 18, 13 | 19, 18, 19 |
| Mean score per pass | 2.85, 2.90, 2.65 | 2.95, 2.90, 2.95 |
| Pass-to-pass score spread (max − min) | 0.25 | 0.05 |
| Unresolved or unjudged (excluded) | 0 | 0 |
| Turns per run (mean) | 5.2 | 7.5 |
| Tool errors, all runs | 36 | 124 |
| Denied tool calls, all runs | 0 | 123 |
| Run status | ok 60 | ok 60 |

By question type:

| Type | Questions | With Octocode score | Without (rg + gh) score | With Octocode correct | Without (rg + gh) correct | Token ratio |
|---|---:|---:|---:|---:|---:|---:|
| locate | 3 | 3.00 | 3.00 | 9/9 | 9/9 | 5.08× |
| history | 4 | 3.00 | 3.00 | 12/12 | 12/12 | 3.66× |
| trace | 4 | 2.67 | 2.83 | 8/12 | 10/12 | 5.61× |
| semantic | 5 | 2.73 | 2.87 | 11/15 | 13/15 | 3.48× |
| multi-hop | 4 | 2.67 | 3.00 | 8/12 | 12/12 | 2.47× |

Per question (mean over passes):

| Question | With Octocode score | Without (rg + gh) score | With Octocode calls | Without (rg + gh) calls | With Octocode time (s) | Without (rg + gh) time (s) | With Octocode cost | Without (rg + gh) cost |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| G01 | 3.00 | 3.00 | 2.0 | 3.0 | 12 | 12 | $0.054 | $0.032 |
| G02 | 3.00 | 3.00 | 3.3 | 3.0 | 15 | 15 | $0.065 | $0.034 |
| G03 | 3.00 | 3.00 | 2.3 | 6.0 | 25 | 28 | $0.096 | $0.103 |
| G04 | 3.00 | 3.00 | 4.3 | 2.7 | 17 | 14 | $0.070 | $0.028 |
| G05 | 3.00 | 3.00 | 2.0 | 4.7 | 20 | 22 | $0.086 | $0.057 |
| G06 | 3.00 | 3.00 | 2.0 | 5.3 | 27 | 21 | $0.087 | $0.058 |
| G07 | 3.00 | 3.00 | 4.3 | 9.0 | 28 | 32 | $0.124 | $0.076 |
| G08 | 3.00 | 3.00 | 2.0 | 3.0 | 18 | 18 | $0.093 | $0.051 |
| G09 | 2.67 | 3.00 | 2.0 | 5.7 | 18 | 25 | $0.072 | $0.054 |
| G10 | 2.67 | 3.00 | 2.7 | 7.0 | 23 | 28 | $0.109 | $0.081 |
| L01 | 3.00 | 3.00 | 1.0 | 2.0 | 10 | 10 | $0.055 | $0.027 |
| L02 | 3.00 | 3.00 | 3.0 | 4.0 | 12 | 14 | $0.057 | $0.034 |
| L03 | 2.33 | 2.67 | 5.0 | 7.0 | 21 | 23 | $0.101 | $0.070 |
| L04 | 2.33 | 2.67 | 3.7 | 5.3 | 21 | 21 | $0.097 | $0.052 |
| L05 | 2.67 | 3.00 | 4.0 | 6.3 | 26 | 19 | $0.100 | $0.061 |
| L06 | 3.00 | 3.00 | 5.3 | 10.7 | 39 | 36 | $0.149 | $0.112 |
| L07 | 2.33 | 3.00 | 13.0 | 21.0 | 58 | 57 | $0.314 | $0.270 |
| L08 | 2.67 | 3.00 | 7.0 | 10.0 | 47 | 37 | $0.179 | $0.112 |
| L09 | 3.00 | 2.67 | 7.0 | 5.3 | 43 | 32 | $0.198 | $0.115 |
| L10 | 2.33 | 2.67 | 8.0 | 9.7 | 40 | 36 | $0.200 | $0.132 |

## Judge agreement

- Pairs judged: 60 (129 judge calls, 0 unparseable after one retry).
- Both orders gave the same verdict for both answers: 51 of 60 (85%). Same scores: 51 (85%). Same preferred answer: 35 (58%).
- Tie-break calls: 9. Verdicts still unresolved after the tie-break (excluded from correctness totals): 0.
- Preferred answer across all judge calls: With Octocode 35, Without (rg + gh) 60, tie 34.
- Judge cost: $21.33.

## Findings

- Score: across 20 questions, the mean per-question score difference (with minus without Octocode) is -0.13 on the 0–3 scale.
  - Octocode scored at least 0.5 higher on: none.
  - rg + gh scored at least 0.5 higher on: L07.
  - Within 0.5 of each other: G01, G02, G03, G04, G05, G06, G07, G08, G09, G10, L01, L02, L03, L04, L05, L06, L08, L09, L10.
- Tokens: the median per-question ratio is 4.22×; Octocode used fewer tokens on 0 of 20 questions.

**clasify usage.** The Octocode agent had clasify available but did not call it in any of the 60 runs.
