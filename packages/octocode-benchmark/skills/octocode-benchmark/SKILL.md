---
name: octocode-benchmark
description: "Use when running the Octocode package research benchmark to compare CLI builds or Octocode with gh, RTK or Headroom. Blind-grade correctness and measure characters from instrumented logs; use octocode-agentic for general experiment design."
compatibility: "Requires the Octocode benchmark package checkout and its compare/bin tooling."
---

# Octocode benchmark

Run from `packages/octocode-benchmark/` and compare Octocode with one baseline per matchup. Read [preflight and instrumentation](references/run-preflight.md), then [runner, judge and aggregation phases](references/run-phases.md). Use [quickstart](references/quickstart.md) for command examples, [design](references/BENCHMARK.md) for methodology and the [fairness checklist](references/INSTRUCTIONS.md#rules-that-keep-it-fair) before accepting a result. General experiment design and skill activation belong to `octocode-agentic`.

## Run a matchup

1. **Preflight.** Pin and verify each arm with [check-prereqs.sh](scripts/check-prereqs.sh). A failed preflight invalidates the comparison. Diagnose Headroom with its [check](scripts/_hr_check.py) and [import probe](scripts/_hr_import.py). The phase references above own setup and execution.
2. **Answer.** For every question, arm and pass, give a fresh runner the same question, revision and budget, plus only the [runner brief](references/RUNNER.md) and its [arm primer](references/RUNNER_TOOL_CONTEXT.md). Keep transcripts separate and the answer key inaccessible. Use the leanest legal read that answers the question. Each runner appends `## Q<n>` to `answers/<arm>-p<pass>.md` and records the answer with `compare/bin/record_answer.py`.
3. **Judge.** When both answers exist, use `compare/bin/build_blind_packet.py` to randomize X/Y. A separate [blind judge](references/JUDGING.md) establishes ground truth before scoring correctness. Calibrate with the [example verdict](references/example-verdict.md). A smaller answer cannot rescue incorrect research.
4. **Summarize.** Validate the explicit campaign with `compare/bin/validate_campaign.py` and recompute Unicode character totals with `compare/bin/sumlog.py --strict`. Inspect each `FAIRNESS:` flag for an avoidable dump. Resolve a violation before counting the pairing. Report after at least three passes using [scoring](references/SCORING.md) and [paired statistics](references/aggregation-and-stats.md). Give the geometric mean of per-question ratios and a bootstrap interval. Count unresolved cases. The public question set supports orientation.

For wrappers and exact input/output accounting, read [measurement](references/measurement.md). Use [measure.sh](scripts/measure.sh) for an arm without a dedicated wrapper. Instrumented JSONL owns measured characters. Historical rollup scripts have fixed campaign names.

## Add a question

Copy a `Q<n>.md` and change its number, title, `id` and `## Question`. GitHub questions belong in `compare/github-questions/`, corpus-local questions in the matchup’s `questions/`. Add its [matchup README row](references/matchup-readme.md).

## Output

Write the [report](references/REPORT_TEMPLATE.md) and preserve the complete campaign under the [output contract](output.md). Retain published `results/` unless a change was requested.
