# Agent-Arms Benchmarks — Toolchain Comparisons for Research Agents

Reproducible methodology for comparing **agent toolchains** ("arms") on identical
code-research questions. An *arm* is a fixed set of allowed CLI tools; several
independent LLM solver agents run per arm; every research command is logged with
byte/time/exit metrics; answers are judged against pre-verified ground truth.

## Benchmark index

| Benchmark | Arms | Scope | Definition |
|---|---|---|---|
| `rtk-gh-vs-octocode` | `rtk`+`gh` vs octocode CLI | Mixed: local root-cause + remote research | [`rtk-gh-vs-octocode/`](./rtk-gh-vs-octocode/) |

(A pure-GitHub `gh-vs-octocode` benchmark ran once on 2026-07-10, drove the
releases-surface/`--quiet`/did-you-mean fixes, and was then retired by request;
its methodology lives on in this README.)

## Methodology (replicate this)

1. **Questions** (`questions.md`) — 10 concrete research/root-cause questions with
   objective answers (file:line, sha, PR number, value). Mix flow types: search,
   exact read, structure browse, history/PR archaeology, cross-repo, discovery.
2. **Ground truth** (`ground-truth.json`) — the benchmark author verifies every
   answer FIRST, with tools outside both arms if possible, and writes a
   0 / 0.5 / 1 scoring rubric per question. Solvers must never read this file.
3. **Arms** — each arm is a whitelist. Solvers may run ONLY whitelisted commands
   (plus `sh -c '… | head/tail/wc'` trimming). Everything else — including the
   host agent's own Read/Grep tools on research targets — is forbidden.
4. **Solvers** — N independent LLM agents per arm (default 3, to average over
   LLM variance), same prompt template (`prompt-template.md`), same step budget
   (default: max 8 logged steps per question).
5. **Logging** — every research command goes through `run-step.mjs`:
   `node run-step.mjs <agentOutDir> <qN-sM> -- <cmd…>`. It appends
   `{id, cmd, exit, ms, bytes}` to `commands.ndjson` and stores raw output under
   `raw/`. `bytes` (stdout+stderr actually shown to the agent) is the
   token-cost proxy; trimmed pipelines count post-trim bytes — i.e. what really
   entered the agent's context.
6. **Run layout** — follows `../../recipes/agent-benchmark-runbook.md`:
   `output/<benchmark>-<YYYYMMDDTHHMMSSZ>/` with `manifest.json`, `schemes/`,
   `agents/<arm>-<n>/{answers.md, commands.ndjson, raw/}`, then judge outputs
   (`scores.json`, `results.md`, `summary.json`, `reflection.md`, `ratings.json`).
7. **Judging** — the author (not a solver) scores each `answers.md` against the
   rubric, writes `scores.json`, then runs
   `node aggregate.mjs <runDir>` to produce the per-arm tables for `results.md`.
8. **Integrity audit (required before publishing)** — run
   `node check-run-integrity.mjs <runDir>`: verifies every ndjson line parses,
   every step has its raw evidence file with reconciling byte counts, answer
   sheets cover all questions and end with Totals, and flags provider
   truncation markers (`matchTruncated`/`incomplete_results`) plus empty
   successful outputs so scored answers can be audited for truncated evidence.
   A run may not be reported without `INTEGRITY OK`.

## Fairness rules

- **Capability parity (hard rule).** Every question must be solvable with each
  arm's own whitelist — never "borrow" a tool for one arm (e.g. an npm-metadata
  question when one arm has no package-registry surface). If you want to show a
  capability GAP, do it in a separate coverage section, never in the scored set.
  (Learned the hard way: run `rtk-gh-vs-octocode-20260710T182339Z` Q10 was
  npm-based and had to be retired from headline scoring.)
- Both arms answer the SAME questions in the same run window (provider drift).
- Auth parity: both arms use the same GitHub token chain; record auth state.
- Learning cost is real cost: `--help`/scheme reading counts as logged steps.
- LLM solvers vary; never report a single-agent result as an arm result — report
  per-arm mean/min/max over ≥3 agents and keep per-agent rows visible.
- A wrong answer with confident tone scores 0; partial anchors score 0.5 max.

## Presenting results (best practice)

`results.md` must contain, in order:
1. **Headline table** — one row per arm: mean correctness /10, mean steps, mean
   KB consumed, mean wall-clock; bold the winner per column.
2. **Per-question matrix** — questions × agents, scores colored/marked, so
   readers see WHERE each arm wins (flow-type patterns), not just totals.
3. **Efficiency scatter data** — per-agent (correctness vs bytes) table; calling
   out cost-per-correct-answer (KB / correct).
4. **Qualitative findings** — friction notes from solver `answers.md` Totals
   sections, with agent quotes; failure modes per arm.
5. **Caveats** — nondeterminism (provider drift, LLM variance, cache state),
   what the benchmark does NOT measure.
Never average away disagreement silently: if agents within an arm split on a
question, show it in the matrix and discuss it.

## Adding a new benchmark

Copy `rtk-gh-vs-octocode/` as a template: write questions, verify ground truth
yourself, define arm whitelists in the README, fill `prompt-template.md`
placeholders, spawn ≥3 solvers per arm, judge, aggregate.
