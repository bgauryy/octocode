# Write-up template

Save finished campaigns in `results/<name>-<HHMMSS>-<YYYY-MM-DD>.md` — name, run start
`HHMMSS` (24-hour), then date (e.g. `results/campaign-134700-2026-08-06.md`). Lead with the
outcome. Measure in **total characters** = model-in (tool output pulled into context) **+**
model-out (commands/args + final answer), from the instrumented log — never tokens, never
self-reported.

Record: question-set version/hash, question count, **all three tool versions** (Octocode,
`gh`+`rtk`, Headroom + model), and the commit/hash of the fixed `RUNNER_TOOL_CONTEXT.md`.

## Per-question table (one matchup = Octocode vs its baseline)

One row per question (write one such table per matchup):

| Q | Corr O/B | Depth O/B | Workflow O/B | Chars O / B | Winner | Note |
|---|---|---|---|---|---|---|

O = Octocode (anchor), B = this matchup's baseline (gh+RTK, gh+Headroom, or plain gh).
`Chars` = model-in + model-out = total_chars; ratios below use total_chars.

## Summary (paired, per question)

Aggregate per question (see `skills/octocode-benchmark/references/aggregation-and-stats.md`).
Two pairings — **Octocode vs RTK** and **Octocode vs Headroom**:

| Metric | vs RTK | vs Headroom |
|---|---:|---:|
| Correctness — paired win/tie/loss (+ sign-test p) | | |
| Research depth (mean, note ceiling) | | |
| Workflow (mean) | | |
| **Char ratio — geometric mean** (headline) | | |
| Char ratio — median (min…max) | | |
| Questions Octocode leaner (of N) + sign-test p | | |
| Chars pooled sum — *outlier-sensitive* (+ top-Q share, leave-one-out) | | |

## Best-on (orchestrator roll-up)

State plainly who is best on **accuracy**, **quality/depth**, **workflow**, and
**characters** — overall and where each leads. Name the questions that drive any large
ratio, and disclose any fairness caveat (a non-lean path on any arm).

## Per question (detail)

For each: the three short answers, each answer's research steps, and its numbers
(correctness, depth, workflow, chars) **with the judge's one-line reasoning per score**.
Note fairness caveats explicitly, and flag any question confirmed by order-swap/second judge.
List **unresolved** questions (judges disagreed on correctness) separately — they are
excluded from the tally, and the count of exclusions is stated.

## Bottom line

Whether the arms were equally correct, and — at parity — who was leaner and **where**
(which question shapes). Avoid a single headline multiplier that the per-question data does
not support.
