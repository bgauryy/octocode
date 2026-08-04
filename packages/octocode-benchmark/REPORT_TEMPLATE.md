# Write-up template

Save finished runs in `results/<comparison-name>-<HHMMSS>-<YYYY-MM-DD>.md` — the comparison name, then the run's start time `HHMMSS` (24-hour, so two runs on the same day never collide), then the date. Example: `results/octocode-vs-gh-rtk-021054-2026-08-05.md`. Lead with the outcome. Measure in **characters** (raw CLI output pulled into context) — never tokens.

## Per-question table

One row per question:

| Question | Correctness A | Correctness B | Depth A | Depth B | Workflow A | Workflow B | Chars A | Chars B | Leaner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|

## Summary of all

Totals / means across every question:

| Metric | A (baseline) | B (Octocode) |
|---|---:|---:|
| Correctness (mean) | | |
| Research depth (mean) | | |
| Workflow (mean) | | |
| **Chars (total)** | | |
| Questions leaner | | |

## Per question (detail)

For each: the two answers (short), each answer's research steps, and the per-question numbers (correctness, depth, workflow, chars in/out). Note any fairness caveat — e.g. a policy limit that constrained one arm, or a full-file pull where a search snippet would answer.

## Bottom line

Whether the two arms were equally correct, and if so who was leaner (in characters) and where.
