---
name: octocode-benchmark
description: "Use when planning, running, grading, or reporting a by-hand Octocode CLI comparison benchmark from markdown questions — isolated runners, a blind grader, results measured in characters."
---

# Octocode benchmark

Plain-markdown, run-by-hand CLI comparison: for each question, two isolated runners answer (baseline CLI vs `npx octocode tools …`) and a blind grader scores both. No harness, no JSON. Design and rationale live once in `../../BENCHMARK.md`.

```
question              →  runner A + runner B (isolated)  →  blind grader  →  write-up
(github-questions/ or                                                        results/<matchup>-<HHMMSS>-<YYYY-MM-DD>.md
 a matchup's questions/)                                                     (per-question table + summary, in characters)
```

## Operate it correctly (skip one and the run is worthless)

- **Isolate the three roles.** Spawn each runner and the grader as a *separate* agent/context. Never let one context play two roles, read another's transcript, or see any answer key — that contaminates the result.
- **Same packet, one variable.** Both arms get the same question, budget, and frozen refs; only the CLI differs. Freeze every mutable ref (branch / PR state / SHA + UTC) *before* answering and put it in the answer.
- **Blind the grader.** Give it the two answers as X/Y with tool names hidden; it establishes ground truth by its own research; un-blind only when you tabulate.
- **Measure characters of raw CLI output, never tokens.** For an instrumented arm (`rtk`/`headroom`) read chars from the log, never a runner's self-report — models miscount their own context.
- **Decide correctness-first.** Leaner (fewer chars at equal correctness) breaks ties; a confidently-wrong answer cannot win. One pass is a snapshot — repeat for a stable claim.

## Run it

1. **Preflight** — confirm both CLIs are installed and authenticated, and pin their versions, per the matchup `README.md`.
2. **Per question** — Runner A and Runner B each answer per `../../RUNNER.md`; the Grader scores blind per `../../JUDGING.md`.
3. **Score** each answer (correctness, depth, workflow, chars) per `../../SCORING.md`.
4. **Write up** to `results/<matchup>-<HHMMSS>-<YYYY-MM-DD>.md` per `../../REPORT_TEMPLATE.md`.

Full step list: `../../INSTRUCTIONS.md`.

## Where things are

- `compare/<matchup>/README.md` — the two arms + how to run each. Authoring/reviewing that README → load `references/matchup-readme.md`.
- Questions — title + `id` + `## Question` only. GitHub matchups share `compare/github-questions/`; a corpus-local matchup keeps its own `compare/<matchup>/questions/`.
- `results/` — finished write-ups.

## Add a question

Copy an existing `Q<n>.md`, bump the number, edit title / `id` / `## Question` — nothing else, no scripts. GitHub → `compare/github-questions/`; corpus-local → that matchup's `questions/`. Add its row to that set's `README.md`.
