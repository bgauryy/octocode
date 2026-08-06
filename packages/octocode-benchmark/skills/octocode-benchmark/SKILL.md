---
name: octocode-benchmark
description: "Use when planning, running, grading, or reporting a by-hand Octocode CLI comparison benchmark from markdown questions — a fresh isolated agent per question and per tool arm, one blind judge agent per question, results measured in total characters through the model (model-in delivered into context + model-out commands/args + final answer)."
---

# Octocode benchmark

Plain-markdown, run-by-hand CLI comparison. For each question: two isolated runner
agents answer (baseline CLI vs `npx octocode tools …`) and one blind judge agent scores
both. No harness, no JSON. Design and rationale live once in `../../BENCHMARK.md`; the
concrete agent-orchestration recipe (preflight, wrapper, spawn packets, outputs) lives in
`references/run-with-agents.md`.

## Workflow

```
 PREFLIGHT (once per matchup)
   verify + pin each arm: gh · rtk · headroom bin/ghc · npx octocode
   read compare/<matchup>/README.md  → the allowed read-only surface
                       │
                       ▼
 FOR EACH question Qn  ───────────── fan out, fully isolated ──────────────┐
 (github-questions/ or matchup questions/)                                 │
                       │                                                   │
     ┌──── agent: RUNNER A ────┐      ┌──── agent: RUNNER B ────┐   arms    │
     │ RUNNER.md + RTK/headroom│      │ RUNNER.md + Octocode    │   never   │
     │ primer; freeze refs;    │      │ primer; npx octocode    │   share a │
     │ research A's surface;   │      │ tools … only;           │   context │
     │ every cmd → measure.sh A│      │ every cmd → measure.sh B│   or see  │
     │ → answer + calls.jsonl  │      │ → answer + calls.jsonl  │   the judge│
     └───────────┬─────────────┘      └────────────┬────────────┘          │
                 └──── answers relabelled X / Y (tool names hidden) ────────┘
                       │
                       ▼
     ┌──── agent: JUDGE — one per question, blind ────┐
     │ own research → ground truth; score X then Y:   │
     │ correctness · depth · workflow · chars         │
     │ (JUDGING.md + SCORING.md; correctness-first)   │
     └───────────────────┬────────────────────────────┘
                         ▼   (repeat for every question)
 WRITE-UP  results/<matchup>-<HHMMSS>-<YYYY-MM-DD>.md   (per-question table +
                       │                                summary; REPORT_TEMPLATE.md)
                       ▼
 ROLL UP   update the matchup's headline row in results/SUMMARY.md + results/README.md
```

## Operate it correctly (skip one and the run is worthless)

- **One agent per role per question.** Spawn a *separate* agent/context for runner A,
  runner B, and the judge. Never let one context play two roles, read another's
  transcript, or see any answer key. (You MAY batch questions within one arm's agent to
  save processes — but never mix arms, and never let the judge share a transcript with a runner.)
- **Same packet, one variable.** Both arms get the same question, budget, and frozen refs;
  only the CLI differs. Freeze every mutable ref (branch / PR state / SHA + UTC) *before*
  answering and put it in the answer.
- **Blind the judge.** Give it the two answers as X/Y with tool names hidden; it
  establishes ground truth by its own research; un-blind only when you tabulate.
- **Measure total characters through the model in both directions, never tokens.** The
  scored figure is `total_chars` = model-in (CLI/tool output pulled into context) **+**
  model-out (the commands/args the model wrote + its final answer). Both directions spend
  the context budget. Read chars from the wrapper/instrument log (`bin/sumlog.py`), never a
  runner's self-report — models miscount their own context.
- **Aggregate per question, paired; never let one question decide.** The unit is the
  question and the arms are matched pairs. Headline characters as the **geometric mean of
  per-question A/B ratios** + median + leaner win-rate (sign test) — NOT a pooled sum of
  raw chars (a single heavy question dominates a sum). If you report a pooled sum, disclose
  the top question's share and a leave-one-out. Correctness clusters near ceiling: use
  paired win/tie/loss, not mean gaps. Full method + worked example:
  `references/aggregation-and-stats.md`.
- **Decide correctness-first.** At statistically indistinguishable correctness, fewer
  characters breaks the tie by geometric-mean ratio (not sum); a confidently-wrong answer
  cannot win. One pass is a snapshot — repeat (≥3 passes) for a stable, testable claim.

## Run it (full recipe: `references/run-with-agents.md`)

1. **Preflight** — confirm each arm's CLI/script is installed, authed, and working, and
   pin versions: `gh --version` + `gh auth status`; `rtk --version`; headroom
   `./compare/octocode-vs-gh-headroom/bin/ghc api rate_limit` (0% ratio = compression off,
   invalid); `npx octocode --version` + one live `npx octocode tools …` probe. Read the
   matchup `compare/<matchup>/README.md` for the allowed surface.
2. **Set up measurement** — create the run dir + transparent wrapper (`references/run-with-agents.md` §1);
   every research command runs as `.octocode/tmp/measure.sh <A|B> Q<n> <label> -- <cmd>`.
3. **Spawn runners** — one agent per (question, arm), each given only `../../RUNNER.md`,
   its arm section from `../../RUNNER_TOOL_CONTEXT.md`, and the question. Arm B uses
   `npx octocode tools <tool> --queries '<json>'` only.
4. **Spawn the judge** — one per question, given the two answers as X/Y (tools hidden) and
   its own research tools; it scores per `../../JUDGING.md` (correctness, depth, workflow,
   chars) and `../../SCORING.md`, then compares.
5. **Write up + roll up** — recompute per-question paired stats from `<RUNDIR>/*/calls.jsonl`
   per `references/aggregation-and-stats.md` (geometric-mean + median char ratio, leaner
   win-rate + sign test, plus top-contributor share and leave-one-out for any pooled sum), write
   `results/<matchup>-<HHMMSS>-<YYYY-MM-DD>.md` per `../../REPORT_TEMPLATE.md`, and update
   the matchup's headline row in `results/SUMMARY.md` and `results/README.md`.

Full step list for the whole campaign: `../../INSTRUCTIONS.md`.

## Where things are

- `../../BENCHMARK.md` — design + run-flow (single source of truth).
- `../../RUNNER.md` · `../../RUNNER_TOOL_CONTEXT.md` — runner instructions + per-arm primers.
- `../../JUDGING.md` · `../../SCORING.md` — grading + scoring rules.
- `../../INSTRUCTIONS.md` · `../../REPORT_TEMPLATE.md` — campaign steps + report shape.
- `references/run-with-agents.md` — preflight, wrapper script, spawn packets, outputs.
- `references/aggregation-and-stats.md` — how to aggregate/test results (paired per-question,
  geometric-mean char ratio, sign test, outlier disclosure) so one question can't set the verdict.
- `compare/<matchup>/README.md` — the two arms + how to run each. Authoring/reviewing that
  README → load `references/matchup-readme.md`.
- Questions — title + `id` + `## Question` only. GitHub matchups share
  `compare/github-questions/`; a corpus-local matchup keeps its own `compare/<matchup>/questions/`.
- `results/` — finished write-ups + rollup.

## Add a question

Copy an existing `Q<n>.md`, bump the number, edit title / `id` / `## Question` — nothing
else, no scripts. GitHub → `compare/github-questions/`; corpus-local → that matchup's
`questions/`. Add its row to that set's `README.md`.
