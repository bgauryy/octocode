---
name: octocode-benchmark
description: "Use when planning, running, grading, or reporting the by-hand Octocode research benchmark — pairwise matchups (Octocode anchor vs one baseline: gh+RTK, gh+Headroom, or plain gh) over markdown questions, with a fresh isolated runner agent per (question, arm, pass), one blind judge per question grading two answers X/Y in randomized order, and an orchestrator that summarizes accuracy/quality/workflow/characters. Results measured in total characters through the model (model-in delivered + model-out commands/args + final answer)."
---

# Octocode benchmark

Plain-markdown, run-by-hand CLI research comparison. Octocode is the **anchor**; each baseline
is a **separate pairwise matchup** (`octocode` vs `rtk` | `headroom` | `gh`). Per question,
per pass: two isolated runners answer, one blind judge grades them **X / Y** (randomized per
question). Run ≥3 passes; the rollup shows every matchup together. No harness, no JSON.

## Flow (4 phases)

0. **Preflight** — verify + pin every arm; a failure invalidates the run.
1. **Answer** — 2 isolated runners (anchor + baseline) per question/pass, leanest-legal path, each appends a `## Q<n>` section to `answers/<arm>-p<pass>.md`.
2. **Judge** — after both sections exist, one blind judge reasons to a verdict, then scores.
3. **Summarize** — validate logs, aggregate paired stats, update the rollup.

## Hard gates (skip one and the run is worthless)

- **Isolation** — a fresh agent per (question, arm, pass) + a separate judge; no shared transcript, no answer key. Batch questions within an arm, never mix arms.
- **Fairness** — leanest-legal path on every arm; no whole-tree/whole-file dump where a targeted read/search answers (inflates chars, invalidates the ratio).
- **Blind + reasoned** — grade X/Y in randomized order; the judge reasons before scoring, correctness-first; a confidently-wrong answer never wins.
- **Measured, not self-reported** — `total_chars = model-in + model-out` from the instrumented log.
- **Honest stats** — geometric-mean char ratio (never a pooled sum) + bootstrap CI; ≥3 passes; the public set is orientation, not a shipping gate.

## Routes — load only what the step needs

| When | Load |
|---|---|
| understand the design | `references/BENCHMARK.md` |
| run a matchup | `references/INSTRUCTIONS.md` then `references/run-with-agents.md` |
| brief a runner | `references/RUNNER.md` + `references/RUNNER_TOOL_CONTEXT.md` |
| judge a question | `references/JUDGING.md` + `references/example-verdict.md` |
| score + aggregate | `references/SCORING.md` then `references/aggregation-and-stats.md` |
| write the report | `references/REPORT_TEMPLATE.md` |
| author a matchup README | `references/matchup-readme.md` |

## Scripts + done gate

`scripts/check-prereqs.sh` gates Phase 0; `scripts/measure.sh` is the fallback char wrapper.
Run `bin/validate_campaign.py` before reporting; recompute every figure from the logs.

## Stop when

The matchup's questions are answered, judged, and aggregated across ≥3 passes with CIs — or a
preflight/fairness violation blocks the run; fix before continuing.

## Add a question

Copy an existing `Q<n>.md`, bump the number, edit title / `id` / `## Question` only. GitHub →
`compare/github-questions/`; corpus-local → that matchup's `questions/`; add its README row.
