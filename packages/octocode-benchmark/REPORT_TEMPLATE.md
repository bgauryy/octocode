# Write-up template

Save finished runs in `results/<comparison-name>-<HHMMSS>-<YYYY-MM-DD>.md` — the comparison name, then the run's start time `HHMMSS` (24-hour, so two runs on the same day never collide), then the date. Example: `results/octocode-vs-gh-rtk-021054-2026-08-05.md`. Lead with the outcome. Measure in **characters** (raw CLI output pulled into context) — never tokens.

Record the question-set version/hash, question count, all tool versions, and the
commit/hash of the fixed `RUNNER_TOOL_CONTEXT.md` supplied to each runner.

## Per-question table

One row per question:

| Question | Correctness A | Correctness B | Depth A | Depth B | Workflow A | Workflow B | Chars A | Chars B | Leaner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|

## Summary of all

Aggregate **per question, paired** (see `skills/octocode-benchmark/references/aggregation-and-stats.md`).
Headline characters with the geometric-mean ratio, not the pooled sum.

| Metric | A (baseline) | B (Octocode) |
|---|---:|---:|
| Correctness — paired win/tie/loss (+ sign-test p) | | |
| Correctness (mean, secondary — note ceiling) | | |
| Research depth (mean) | | |
| Workflow (mean) | | |
| **Char ratio A/B — geometric mean** (headline) | | |
| Char ratio A/B — median (min…max) | | |
| Questions leaner (of N) + sign-test p | | |
| Chars pooled sum — *outlier-sensitive* | | |
|   top-question share of heavier arm; leave-one-out ratio | | |

## Per question (detail)

For each: the two answers (short), each answer's research steps, and the per-question numbers (correctness, depth, workflow, chars in/out). Note any fairness caveat — e.g. a policy limit that constrained one arm, or a full-file pull where a search snippet would answer.

## Bottom line

Whether the two arms were equally correct, and if so who was leaner (in characters) and where.
