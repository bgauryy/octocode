# rtk-gh Benchmarks — Toolchain Comparisons for Research Agents

Reproducible methodology for comparing agent toolchains ("arms") on identical
code-research questions. Independent LLM solver agents run per arm; every
research command is logged (tokens/bytes/time/exit); answers are judged on
TWO independent axes — **correctness** (vs. pre-verified ground truth) and
**depth of quality** (anchor precision, reasoning depth, calibration).

## Benchmark index

| Benchmark | Arms | Scope | Definition | Results |
|---|---|---|---|---|
| `rtk-gh-vs-octocode-flows` | `rtk`+`gh` vs octocode CLI | 100% remote GitHub research, 10 flow categories | [`rtk-gh-vs-octocode-flows/`](./rtk-gh-vs-octocode-flows/) | [`RESULTS.md`](./rtk-gh-vs-octocode-flows/RESULTS.md) |

Single active benchmark — earlier variants were retired; their lessons are folded into the rules below.

## Methodology

1. **Questions** (`questions.md`) — objective answers (file:line, sha, PR#, value). Mix flow types. Exploratory/open-ended questions allowed but must be marked and scored on a looser rubric — never averaged into a strict count silently.
2. **Ground truth** (`ground-truth.json`) — verify every answer FIRST, with a method outside BOTH arms (hard rule — see file's `verificationCaveat` for why this matters). A third independent method (`WebFetch` on `raw.githubusercontent.com`, GitHub's web UI, or a manual clone) must confirm any fact that materially drives scoring; log it under `verification.independentChecks`. Cross-arm convergence in the run is corroborating, not a substitute. Solvers never read this file.
3. **Arms** — each is a strict whitelist (plus `sh -c '… | head/tail/wc'` trimming). Nothing else, including the host agent's own Read/Grep.
4. **Solvers** — N agents per arm (default 3, to average LLM variance), same `prompt-template.md`, same step budget (default 8/question; raise for questions with 3+ sub-parts or multiple files).
5. **Question freeze (hard rule)** — `questions.md`/`ground-truth.json` freeze the moment the first solver is dispatched. Never append mid-run; extending the set always means a new run.
6. **Logging** — every command via `run-step.mjs <agentOutDir> <stepId> -- <cmd…>`, appending `{id, cmd, exit, ms, bytes, tokens, tokenizer}` to `commands.ndjson`. Tokens are the primary cost metric (real BPE via `gpt-tokenizer` if installed, else `token-estimate.mjs` heuristic — do not mix methods within a run).
7. **Run layout** — `../../recipes/agent-benchmark-runbook.md`: `output/<benchmark>-<TS>/` with `manifest.json`, `agents/<arm>-<n>/{answers.md,commands.ndjson,raw/}`, then judge outputs (`scores.json`, `quality.json`, `results.md`, `summary.json`, `reflection.md`, `ratings.json`).
8. **Judging** — an independent judge subagent (not the orchestrator, not a solver) scores:
   - **Correctness** vs. `ground-truth.json` → `scores.json` (0/0.5/1 per sub-question).
   - **Depth of quality**, independent of correctness → `quality.json` (1-5): anchor precision, reasoning that connects anchors, calibration honesty. Correct-but-thin and imperfect-but-deep are different scores — never conflate.
   - Then `node aggregate.mjs <runDir>` → `results.md`.
   - **Judge discipline** (hardened from real catches — see `rtk-gh-vs-octocode-flows/README.md`'s "Why these flow categories"): read the actual chosen answer, never trust keyword-match alone; independently re-derive any low-confidence/disputed cell; prefer arm-blinded scoring when practical; a single pass is a lower bound — for high-stakes calls, run a second independent judge and treat disagreement as a signal, not noise.
8b. **Automated pre-check** (run before `quality.json`) — `node check-answer-depth.mjs <runDir> <groundTruthPath>` flags (1) coverage gaps (a ground-truth `keyTerm` never mentioned — `criticalKeyTerm` for a loud flag) and (2) calibration mismatches (`Confidence: high` + hedge language). Writes `depth-check.json`. Heuristic pre-check, not a score — any `COVERAGE_CRITICAL` must be explicitly resolved before publishing.
9. **Integrity audit (required)** — `node check-run-integrity.mjs <runDir>`: ndjson parses, every step has raw evidence with reconciling bytes, answers cover all questions + end with Totals, flags truncation markers and missing/mixed token methods. No report without `INTEGRITY OK`.

## Fairness rules

- **Capability parity (hard rule)** — every question must be solvable by each arm's own whitelist; never borrow a tool for one side. Show capability gaps in a separate section, never the scored set.
- Same questions, same run window, same auth parity for both arms.
- Learning cost counts as logged steps.
- Never report a single agent as "the" arm result — report per-arm mean/min/max over ≥3 agents, per-agent rows visible.
- Confident-wrong scores 0; partial anchors max 0.5.

## Presenting results

`results.md`, in order: (1) headline table — mean correctness/N, quality/5, steps, tokens+KB, wall-clock, bold the winner per column; (2) per-question matrices for both correctness and quality; (3) cost-per-correct-answer table; (4) qualitative findings/failure modes with quotes; (5) caveats (nondeterminism, tokenizer method, what's not measured). Never silently average away within-arm disagreement — show and discuss split votes.

## Adding a new benchmark

Copy `rtk-gh-vs-octocode-flows/` as a template: write questions, verify ground truth outside both arms, define arm whitelists, fill `prompt-template.md`, freeze before dispatch, ≥3 solvers/arm, judge both axes, aggregate.
