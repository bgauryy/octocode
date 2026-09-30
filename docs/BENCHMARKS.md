# Benchmarks

The [unified agent benchmark](../packages/octocode-benchmark/compare/unified/README.md) gives two Sonnet workers the same 30 pinned code-research questions (10 GitHub, 20 local). The `octocode` worker has the Octocode MCP tools and no shell. The `rg-gh` worker has a shell with `rg` and `gh`. A blinded Opus judge grades each answer from 0 to 10 in both answer orders.

## Run `full-1` (2026-09-30, pre-slimming build)

Full tables: [results/full-1/REPORT.md](../packages/octocode-benchmark/compare/unified/results/full-1/REPORT.md).

> **Build caveat.** `full-1` ran on the MCP build from the morning of 2026-09-30, before the published tool schemas were slimmed. Its first request carried 40.8k tokens of fixed context. The current build's overhead probe measures about 17.2k. The token and cost figures below therefore overstate what the current build spends, and no run has measured the current build yet.

| Worker | Mean quality | Cost | Weighted tokens | Raw total tokens | Research tokens | Tool calls |
|---|--:|--:|--:|--:|--:|--:|
| `octocode` | 8.42 | $2.96 | 1,024.9k | 5,697.5k | 424.6k | 117 |
| `rg-gh` | 9.12 | $1.86 | 567.8k | 1,049.6k | 457.7k | 92 |

- **Quality.** Octocode lost: it won 3 questions, tied 11 and lost 16, with a mean delta of −0.70. A two-sided sign test on the 19 non-ties gives p ≈ 0.004.
- **Cost.** Octocode cost 1.67× as much per question (mean; median 1.64×). The raw-token ratio of 6.08× overstates the gap. It counts cache reads at full weight, and the tool definitions are cached on every request. Weighted tokens price each token kind by its Claude price multiple (cache write 1.25×, cache read 0.1×, output 5×). They put the mean per-question ratio at 1.89×.
- **Research tokens** (total minus the per-request fixed context) were about even: 1.09× mean, 0.97× median.
- **Where Octocode lost.** Local questions averaged 8.43 vs 9.32, and every local category trailed. The biggest gaps were L04 (6.5 vs 9.0), L09 (7.5 vs 10.0) and L13 (6.0 vs 8.0).
- **Where it held.** On GitHub PR review both workers averaged 8.30. On bug root-cause analysis Octocode scored 9.25 vs 8.75, but that is only 2 questions.
- **Input errors.** 34 of Octocode's 117 tool calls (29%) failed, and 33 of those were input-validation rejections, such as a missing `goal`/`reasoning` or `"10"` sent for an integer. The MCP adapter now coerces lossless numeric and boolean strings, and the published schemas describe the required brief.
- **clasify** was called 0 times, so this run says nothing about clasify.

## What the data does not support

- **Efficiency claims.** None hold for the current build until it is re-run. The report's `efficiency` column divides by raw total tokens, and `weighted efficiency` is the cost-aligned figure.
- **Generalizing from `full-1`.** It is a single pass (n = 1 per question) with no held-out question set.
- **The retired tool-level figures.** These include "clasify 10/10 vs 9/10 for `rg`, 43% fewer files opened". They came from scripted tool calls, not agents, and their records remain only in git history. Treat them as historical, not current.

## Next run

Re-run on the current build before making any further token or quality claim:

```bash
cd packages/octocode-benchmark/compare/unified
node run.mjs --run-id <id> --probes && node judge.mjs --run-id <id> && node report.mjs --run-id <id>
```
