# Benchmark design

This package measures **repository research and code understanding**, always as **Octocode
vs one baseline CLI** on the same questions. It does not measure patching or test execution.

> **Canonical run-flow lives here.** README, `INSTRUCTIONS.md`, and the skill point here —
> edit the flow **once, in this file**.

## Anchor arm + pairwise matchups

Octocode is the **anchor**; each baseline CLI is a **separate matchup** that pits Octocode
against exactly that one baseline. There is no single three-way run — a campaign runs one
matchup, and the rollup presents the matchups side by side.

| Matchup dir | Anchor (arm) | Baseline (arm) | Baseline surface | Wrappers |
|---|---|---|---|---|
| `octocode-vs-gh` | Octocode | plain gh | `gh <args>` (read-only) | `bin/octoc` · `bin/ghm` |
| `octocode-vs-gh-rtk` | Octocode | gh + RTK | `rtk gh <args>` (read-only) | `bin/octoc` · `bin/rtkm` |
| `octocode-vs-gh-headroom` | Octocode | gh + Headroom | `./bin/ghc <gh args>` (compressed) | `bin/octoc` · `bin/ghc` |

Within a matchup, each question is answered by **two isolated runner agents** (anchor +
baseline) — same question and frozen refs, only the CLI differs. A separate **blind judge**
grades the two answers as **X / Y** (order randomized per question, tools hidden) and must
**reason its way to a verdict** — establish ground truth, then explain per answer *why* it
earns each score — before ranking. A **decisive or contested** result is confirmed by an
order-swap and/or a second independent judge; an unresolvable correctness disagreement is
marked **unresolved** and excluded. Run **≥3 passes** for stability. An **orchestrator**
aggregates the verdicts + instrumented logs into the matchup report; the rollup shows every
matchup together (so you see Octocode / RTK / Headroom correctness), but every blind grade
and character ratio is **pairwise**.

**Strict phase order — do not overlap.** For a question, Phase 1 (both runners answer) must
finish before Phase 2 (the judge grades X/Y) begins. A judge that starts before both answer
sections exist, or a runner that peeks at a verdict, invalidates the question.

## Orchestrated flow (4 phases)

Phase 0 preflight → Phase 1 answer (2 isolated runners per question/pass) → Phase 2 blind
judge (X/Y, reasons then scores) → Phase 3 validate + aggregate. Concrete recipe:
[`run-with-agents.md`](run-with-agents.md).

**Isolation is the whole point:** a fresh agent per (question, arm, pass), one judge per
question, no shared transcripts, no answer key. The judge builds ground truth itself.

## The fairness rule (non-negotiable)

Give **every** arm its **leanest legitimate path**: targeted region reads, snippet-bearing
searches, minimal `--json` fields, raw file media. **Never pull a whole git tree or a whole
large file when a targeted read or search answers the question** — on any arm. Handicapping
an arm with a bloated fetch distorts the char comparison and invalidates the run. `gh` has
no server-side region read, so a whole-file `raw` fetch is its *legitimate* cost when a
region is genuinely needed — that is fair; a recursive `git/trees` dump when three file
reads suffice is not.

## What is measured

- **Correctness** (0–10), **research depth** (1–5), **workflow** (1–5) — from the blind judge.
- **Context characters delivered** = model-in (tool output pulled into context) **+**
  model-out (commands/args written + final answer). Both directions; Unicode code points;
  from the instrumented log, never self-reported. Deterministic, tokenizer-independent —
  not tokens, latency, or cost.

**Correctness first.** Fewer characters break an essentially-equal-correctness tie but
never rescue a wrong answer.

## Aggregation

Per question, **paired**; headline the **geometric mean of per-question ratios** (+ median +
leaner win-rate), never a pooled sum; correctness near ceiling → paired win/tie/loss;
repeat ≥3 passes. Method + worked example: [`aggregation-and-stats.md`](aggregation-and-stats.md).

## Results

Rollup: [`results/SUMMARY.md`](../../../results/SUMMARY.md). It uses the latest complete campaign and
never pools invalid/incompatible runs into one synthetic total. Current honest reading
(fair leanest-path run): correctness is a near-ceiling tie; Octocode is **~2× leaner than
raw `gh`/rtk** and **~1.4× vs a lossless compressor (Headroom)** overall, with the advantage
concentrated on large-file / commit-scoped / multi-hop reads and roughly even on small
structured lookups. **No "several× better than everything" claim** — earlier inflated
figures came from baseline runners violating the fairness rule above.

Treat this public question suite as comparative orientation, not a shipping gate.
