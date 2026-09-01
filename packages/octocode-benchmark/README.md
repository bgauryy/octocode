# @octocodeai/octocode-benchmark

Plain-markdown, orchestrated CLI **research** benchmark. Each question is answered by **three
isolated agents** — Octocode, `gh`+RTK, `gh`+Headroom — and scored by a fourth, blind judge.
An orchestrator drives all four phases and summarizes the campaign.

The orchestrated research campaign itself has **no harness and no JSON
schemas**. Its questions and grading flow remain markdown you can read and
edit. A separate deterministic regression gate measures the unified Octocode
tool-routing contract: [Unified routing held-out eval](docs/UNIFIED_ROUTING_EVAL.md).

## Characters delivered per tool (≈ tokens)

Characters pushed through the model to answer the **same 30 research questions** (model-in +
model-out; Unicode code points ≈ tokens). Correctness is a near-ceiling tie across tools, so
this is the difference that matters — **fewer characters = leaner context**.

| Tool | Mean chars / question | Total over 30 Q | vs Octocode |
|---|---:|---:|---:|
| **Octocode** | **22,111** | **663,319** | — (anchor) |
| gh | 114,849 | 3,445,482 | **2.01×** more |
| gh + RTK | 368,586 | 11,057,569 | **3.22×** more |
| gh + Headroom | 382,598 | 11,477,951 | **2.73×** more |

*“vs Octocode” = per-question geometric-mean ratio (outlier-resistant, the fair headline).
Numbers recomputed from per-call logs by [`compare/bin/per_question_summary.py`](compare/bin/per_question_summary.py);
see [`results/SUMMARY.md`](results/SUMMARY.md) for CIs and [`results/index.html`](results/index.html) for the visual report. Local build v18.1.1, 3 passes, blind gpt-5.5 judge.*

```
compare/
  bin/                      shared wrappers + scripts (octoc, ghm, rtkm, ghc, …)
  github-questions/         the 30 shared GitHub questions — ONE canonical copy
  octocode-vs-gh/           README.md            (arm: plain gh)
  octocode-vs-gh-rtk/       README.md            (arm: gh + RTK)
  octocode-vs-gh-headroom/  README.md            (arm: gh + Headroom)
results/                    finished campaign write-ups + SUMMARY
tmp/                        run scratch — logs, corpora (gitignored)
```

- A **question** = one `Q<n>.md` with exactly a title, an `id`, and the `## Question` — no scope, hints, claims, or answer.
- The **arms** (named, not lettered): `octocode` = `npx octocode tools …`, `rtk` = `rtk gh …`, `headroom` = `compare/bin/ghc …` (the `octocode` arm uses no MCP and no monorepo entrypoint).

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
