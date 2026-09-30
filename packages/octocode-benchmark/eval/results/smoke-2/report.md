# Run smoke-2

## Tokens with and without Octocode

#### All 2 questions (2 runs per side)

| Metric | With Octocode | Without (rg + gh) | Ratio (with ÷ without) |
|---|---:|---:|---:|
| Correct answers | 1 / 2 | 2 / 2 | 0.50× |
| Partial / wrong | 1 / 0 | 0 / 0 |  |
| Mean score (0–3) | 2.50 | 3.00 |  |
| **Total tokens per run** (mean) | **151.7k** | **45.6k** | **3.32×** |
| – input, fresh | 7 | 13 | 0.54× |
| – input, cache write | 8.1k | 6.9k | 1.17× |
| – input, cache read | 140.9k | 36.4k | 3.87× |
| – output | 2.6k | 2.3k | 1.16× |
| Fixed overhead per run (est.) | 142.4k | 30.2k | 4.71× |
| Research tokens per run (est.) | 9.3k | 15.4k | 0.60× |
| Total tokens, all runs | 303.3k | 91.3k | 3.32× |
| Total tokens per pass (min–max) | 303.3k–303.3k | 91.3k–91.3k |  |
| Cost per run, USD (mean) | $0.087 | $0.058 | 1.51× |
| Tool calls per run (mean) | 3.0 | 5.5 | 0.55× |
| Wall time per run, s (mean) | 21 | 24 | 0.85× |

#### GitHub questions (1 questions, 1 runs per side)

| Metric | With Octocode | Without (rg + gh) | Ratio (with ÷ without) |
|---|---:|---:|---:|
| Correct answers | 1 / 1 | 1 / 1 | 1.00× |
| Partial / wrong | 0 / 0 | 0 / 0 |  |
| Mean score (0–3) | 3.00 | 3.00 |  |
| **Total tokens per run** (mean) | **130.8k** | **47.6k** | **2.75×** |
| – input, fresh | 6 | 14 | 0.43× |
| – input, cache write | 8.2k | 5.8k | 1.43× |
| – input, cache read | 120.1k | 39.7k | 3.02× |
| – output | 2.5k | 2.1k | 1.23× |
| Fixed overhead per run (est.) | 122.0k | 32.5k | 3.76× |
| Research tokens per run (est.) | 8.9k | 15.1k | 0.59× |
| Total tokens, all runs | 130.8k | 47.6k | 2.75× |
| Total tokens per pass (min–max) | 130.8k–130.8k | 47.6k–47.6k |  |
| Cost per run, USD (mean) | $0.082 | $0.052 | 1.59× |
| Tool calls per run (mean) | 3.0 | 6.0 | 0.50× |
| Wall time per run, s (mean) | 22 | 25 | 0.89× |

#### Local questions (1 questions, 1 runs per side)

| Metric | With Octocode | Without (rg + gh) | Ratio (with ÷ without) |
|---|---:|---:|---:|
| Correct answers | 0 / 1 | 1 / 1 | 0.00× |
| Partial / wrong | 1 / 0 | 0 / 0 |  |
| Mean score (0–3) | 2.00 | 3.00 |  |
| **Total tokens per run** (mean) | **172.5k** | **43.7k** | **3.95×** |
| – input, fresh | 8 | 12 | 0.67× |
| – input, cache write | 8.1k | 8.1k | 0.99× |
| – input, cache read | 161.7k | 33.1k | 4.89× |
| – output | 2.7k | 2.5k | 1.10× |
| Fixed overhead per run (est.) | 162.7k | 28.0k | 5.81× |
| Research tokens per run (est.) | 9.8k | 15.7k | 0.62× |
| Total tokens, all runs | 172.5k | 43.7k | 3.95× |
| Total tokens per pass (min–max) | 172.5k–172.5k | 43.7k–43.7k |  |
| Cost per run, USD (mean) | $0.092 | $0.064 | 1.43× |
| Tool calls per run (mean) | 3.0 | 5.0 | 0.60× |
| Wall time per run, s (mean) | 19 | 24 | 0.80× |

## Fixed overhead

The first model request of every run carries a fixed prefix: Claude Code's system prompt, the tool definitions and, for Octocode, the MCP server instructions, followed by the question. Every later request in the run re-reads that prefix, almost always from the prompt cache. The table measures the prefix from each run's own first request, so it reflects exactly what the solver saw.

| | With Octocode | Without (rg + gh) | Difference |
|---|---:|---:|---:|
| Input of the first request (median over runs) | 40.7k | 4.7k | 36.0k |
| Model requests per run (mean) | 3.5 | 6.5 | |
| Fixed overhead per run (first-request input × requests) | 142.4k | 30.2k | 4.71× |
| Research tokens per run (total − overhead) | 9.3k | 15.4k | 0.60× |
| Share of total tokens that is fixed overhead | 94% | 66% | |

Research tokens are everything above the fixed prefix: tool results, and the model's own earlier turns re-read on each request. The prefix includes the question itself (a few hundred tokens, the same in both arms). Cached prefix tokens are billed at a fraction of the fresh-input price, so the overhead weighs less in cost than in token counts.

## Per-question tokens

| Question | Type | With Octocode tokens | Without (rg + gh) tokens | Ratio | With Octocode correct | Without (rg + gh) correct |
|---|---|---:|---:|---:|:-:|:-:|
| [G09](../packages/octocode-benchmark/eval/QUESTIONS.md#g09) | github · multi-hop | 130.8k | 47.6k | 2.75× | ✓ | ✓ |
| [L03](../packages/octocode-benchmark/eval/QUESTIONS.md#l03) | local · trace | 172.5k | 43.7k | 3.95× | ½ | ✓ |

Tokens are the mean total per run over 1 passes. Marks show each pass: ✓ correct, ½ partial, ✗ wrong, ? unresolved. Ratio below 1 means Octocode used fewer tokens.

| Token ratio across 2 questions | Value |
|---|---:|
| Median of per-question ratios | 3.35× |
| Mean of per-question ratios | 3.35× |
| Geometric mean of per-question ratios | 3.30× |
| Ratio of summed tokens | 3.32× |
| Questions where Octocode used fewer tokens | 0 of 2 |
| Median / mean of per-question research-token ratios | 0.60× / 0.60× |

## Correctness

| Metric | With Octocode | Without (rg + gh) |
|---|---:|---:|
| Correct / partial / wrong | 1 / 1 / 0 | 2 / 0 / 0 |
| Correct rate | 50% | 100% |
| Mean score (0–3) | 2.50 | 3.00 |
| Correct answers per pass | 1 | 2 |
| Mean score per pass | 2.50 | 3.00 |
| Pass-to-pass score spread (max − min) | 0.00 | 0.00 |
| Unresolved or unjudged (excluded) | 0 | 0 |
| Turns per run (mean) | 4.0 | 6.5 |
| Tool errors, all runs | 1 | 6 |
| Denied tool calls, all runs | 0 | 5 |
| Run status | ok 2 | ok 2 |

By question type:

| Type | Questions | With Octocode score | Without (rg + gh) score | With Octocode correct | Without (rg + gh) correct | Token ratio |
|---|---:|---:|---:|---:|---:|---:|
| multi-hop | 1 | 3.00 | 3.00 | 1/1 | 1/1 | 2.75× |
| trace | 1 | 2.00 | 3.00 | 0/1 | 1/1 | 3.95× |

Per question (mean over passes):

| Question | With Octocode score | Without (rg + gh) score | With Octocode calls | Without (rg + gh) calls | With Octocode time (s) | Without (rg + gh) time (s) | With Octocode cost | Without (rg + gh) cost |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| G09 | 3.00 | 3.00 | 3.0 | 6.0 | 22 | 25 | $0.082 | $0.052 |
| L03 | 2.00 | 3.00 | 3.0 | 5.0 | 19 | 24 | $0.092 | $0.064 |

## Judge agreement

- Pairs judged: 2 (4 judge calls, 0 unparseable after one retry).
- Both orders gave the same verdict for both answers: 2 of 2 (100%). Same scores: 2 (100%). Same preferred answer: 1 (50%).
- Tie-break calls: 0. Verdicts still unresolved after the tie-break (excluded from correctness totals): 0.
- Preferred answer across all judge calls: With Octocode 1, Without (rg + gh) 2, tie 1.
- Judge cost: $0.53.

## Findings

- Score: across 2 questions, the mean per-question score difference (with minus without Octocode) is -0.50 on the 0–3 scale.
  - Octocode scored at least 0.5 higher on: none.
  - rg + gh scored at least 0.5 higher on: L03.
  - Within 0.5 of each other: G09.
- Tokens: the median per-question ratio is 3.35×; Octocode used fewer tokens on 0 of 2 questions.

**clasify usage.** The Octocode agent had clasify available but did not call it in any of the 2 runs.
