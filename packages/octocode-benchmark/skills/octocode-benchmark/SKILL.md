---
name: octocode-benchmark
description: "Use when planning, running, grading, or reporting a by-hand Octocode CLI comparison benchmark from markdown questions."
---

# Octocode benchmark

Plain-markdown, run-by-hand CLI comparison. No harness, no JSON, no schemas — just markdown questions and a markdown write-up.

## The flow

For every question, three separate people/agents work alone: two answer it (one per tool), one grades. Then everything rolls up into one write-up.

```
QUESTION  (compare/<matchup>/questions/Q<n>.md)
        │
  ┌─────┼─────────────────────────────────┐
  │ Runner A  answers with the baseline CLI │
  │ Runner B  answers with npx octocode …   │
  │ Grader    grades both answers blind      │
  └─────┬─────────────────────────────────┘
        ▼
  results/<matchup>-<name>.md   (chars + correctness + quality, per question, then a summary of all)
```

Keep the two runners unaware of each other and of the grader. Same question, same budget, same frozen refs; only the CLI differs. The grader checks each answer on its own evidence before comparing.

## Where things are

- `compare/<matchup>/README.md` — the two arms and their allowed surface.
- `compare/<matchup>/questions/Q<n>.md` — one question each: just a title, an `id`, and the `## Question` (no scope, claims, or answer).
- `results/` — finished write-ups.

## How to run

Follow `../../INSTRUCTIONS.md`. Roles: `../../RUNNER.md` (runner) and `../../JUDGING.md` (grader). Measure per `../../SCORING.md`. Report per `../../REPORT_TEMPLATE.md`.

## Rules

- Octocode arm = `npx octocode tools <tool> …` only (no MCP, no monorepo entrypoint).
- Freeze mutable refs (branches, PR state, SHAs) before answering; put them in the answer.
- Give each arm its leanest legitimate path; record any known handicap as a fairness caveat.
- Questions carry no answer key; the grader establishes ground truth by its own research.
- Grade semantic support, not wording, length, citations, or tool order.
- Correctness first; on a tie, the leaner tool (fewer CLI-output characters at equal correctness) wins the tie-break. Report the per-question table and a summary of all.

## Add a question

Copy an existing `questions/Q<n>.md`, bump the number, edit the title, `id`, and `## Question`. Nothing else. No scripts.
