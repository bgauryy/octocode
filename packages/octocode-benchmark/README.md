# @octocodeai/octocode-benchmark

Plain-markdown, orchestrated CLI **research** benchmark. Each question is answered by **three
isolated agents** — Octocode, `gh`+RTK, `gh`+Headroom — and scored by a fourth, blind judge.
An orchestrator drives all four phases and summarizes the campaign.

**No harness, no JSON schemas.** Everything is markdown you can read and edit.

```
compare/
  github-questions/         the 30 shared GitHub questions — ONE canonical copy
  octocode-vs-gh/           README.md            (arm: plain gh)
  octocode-vs-gh-rtk/       README.md            (arm: gh + RTK)
  octocode-vs-gh-headroom/  README.md + bin/     (arm: gh + Headroom)
results/                    finished campaign write-ups + SUMMARY
tmp/                        run scratch — logs, corpora (gitignored)
```

- A **question** = one `Q<n>.md` with exactly a title, an `id`, and the `## Question` — no scope, hints, claims, or answer.
- The three **arms**: A = `npx octocode tools …`, B = `rtk gh …`, C = `./bin/ghc …` (no MCP, no monorepo entrypoint for arm A).

## The flow (orchestrated, 4 phases)

1. **Phase 0 — preflight.** Orchestrator verifies all three tools are installed/authed/working, pins versions, and reads the questions + [JUDGING](skills/octocode-benchmark/references/JUDGING.md) + [SCORING](skills/octocode-benchmark/references/SCORING.md).
2. **Phase 1 — answer.** Per question, three fresh isolated agents (A/B/C) each research on their assigned CLI's **leanest legal path** and emit an output file (answer + chars-in/out stats).
3. **Phase 2 — judge.** Per question, one blind judge gets the three answers as X/Y/Z (tools hidden), researches ground truth, and scores each on correctness · depth · workflow · chars + checks usage/flow.
4. **Phase 3 — summarize.** Orchestrator aggregates all verdicts into one campaign report: who's best on accuracy, quality, workflow, and characters — per question (paired) and overall.

Isolation makes the numbers trustworthy — one agent per role per question, no shared
transcripts, no answer key. Full design: **[BENCHMARK.md](skills/octocode-benchmark/references/BENCHMARK.md)**.

## Fairness rule

Every arm gets its **leanest legitimate path** (targeted reads, snippets, minimal fields).
**No whole-tree or whole-file dumps where a targeted read or search answers** — on any arm.
Violating this inflates that arm's characters and invalidates the comparison.

## Run one

See [INSTRUCTIONS.md](skills/octocode-benchmark/references/INSTRUCTIONS.md) for the phase checklist, [RUNNER.md](skills/octocode-benchmark/references/RUNNER.md) +
[RUNNER_TOOL_CONTEXT.md](skills/octocode-benchmark/references/RUNNER_TOOL_CONTEXT.md) for runner packets, [JUDGING.md](skills/octocode-benchmark/references/JUDGING.md)
for grading, [SCORING.md](skills/octocode-benchmark/references/SCORING.md) for measurement/aggregation, and
[REPORT_TEMPLATE.md](skills/octocode-benchmark/references/REPORT_TEMPLATE.md) for the write-up. Agent recipe:
[`skills/octocode-benchmark/references/run-with-agents.md`](skills/octocode-benchmark/references/run-with-agents.md).

## Add a question

GitHub question → shared [`compare/github-questions/`](compare/github-questions/) (applies
to all arms); corpus-local → that matchup's own `questions/`. Create `Q<n>.md` with exactly:

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

One self-contained, objectively-checkable prompt. Name the repo(s)/ref(s) or `$CORPUS`
path, and say exactly what to report. No hints, no approach, no answer.
```

Then add its row to that set's `README.md`. No scripts, no JSON.
