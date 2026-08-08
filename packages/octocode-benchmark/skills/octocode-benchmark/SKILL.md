---
name: octocode-benchmark
description: "Use when planning, running, grading, or reporting the by-hand Octocode research benchmark — pairwise matchups (Octocode anchor vs one baseline: gh+RTK, gh+Headroom, or plain gh) over markdown questions, with a fresh isolated runner agent per (question, arm, pass), one blind judge per question grading two answers X/Y in randomized order, and an orchestrator that summarizes accuracy/quality/workflow/characters. Results measured in total characters through the model (model-in delivered + model-out commands/args + final answer)."
---

# Octocode benchmark

Plain-markdown, run-by-hand CLI research comparison. Octocode is the **anchor**; each baseline
is a **separate pairwise matchup** (`octocode` vs `rtk` | `headroom` | `gh`). Per question,
per pass: two isolated runners answer, one blind judge grades them **X / Y** (randomized per
question). Run ≥3 passes; the rollup shows every matchup together. No harness, no JSON.

Paths below are relative to the package root `packages/octocode-benchmark/`. Shared tooling
lives in `compare/bin/`; questions in `compare/github-questions/`; reports in `results/`.

## Flow (4 phases)

0. **Preflight** — verify + pin every arm; a failure invalidates the run.
1. **Answer** — 2 isolated runners (anchor + baseline) per question/pass, leanest-legal path, each appends a `## Q<n>` section to `answers/<arm>-p<pass>.md`.
2. **Judge** — after both sections exist, one blind judge reasons to a verdict, then scores.
3. **Summarize** — validate logs, aggregate paired stats, update the rollup.

## How it is MEASURED (characters, never self-reported)

The metric is **`total_chars = model-in + model-out`** in Unicode code points, from an
instrumented log — the *tool transcript only* (excludes system prompt, tool schemas, model
reasoning; the fixed per-arm primer is excluded by rule; any later help/schema/failed call
**is** counted).

- **model-out** = the command string + args the model wrote, plus the final answer.
- **model-in** = the tool output pulled back into context (for Headroom, the *compressed* output).

Mechanism: every research command runs through its arm's thin wrapper, which shells the real
CLI unchanged, prints output verbatim, and appends one JSONL row per call:

| Arm | Wrapper | Runs | Log env |
|---|---|---|---|
| octocode (local build) | `compare/bin/octoc` | `npx octocode tools …` | `OCTO_LOG` |
| octocode (published pin) | `compare/bin/octoc1822` | `npx -y octocode@18.2.2 tools …` | `OCTO_LOG` |
| gh+RTK | `compare/bin/rtkm` | `rtk gh …` | `RTK_LOG` |
| gh+Headroom | `compare/bin/ghc` | `gh …` → Headroom compress | `GHC_LOG` (+ `HR_PY`) |
| plain gh | `compare/bin/ghm` | `gh …` (read-only) | — |

The final answer is logged as pure model-out via `compare/bin/record_answer.py`. Per-question
total = `compare/bin/sumlog.py --strict <log>`; the whole campaign is checked byte-faithfully
by `compare/bin/validate_campaign.py`. **Never trust a hand-counted number** — recompute from
the JSONL. Only `elapsed_ms` (octocode/rtk/gh) is captured for time; it is not a fair latency
metric (npx bootstrap per call, no Headroom timing) — do not headline it.

## How it is SCORED (blind judge, correctness-first)

One blind judge per question grades the two answers as **X / Y** (order randomized per
question, tool identity redacted). It **reasons to ground truth first**, then scores each
answer: **correctness 0–10, research depth 1–5, workflow 1–5** (rubric in `references/JUDGING.md`).

Decision per pairing: if one arm is net strictly more correct (paired sign test) it **wins** —
a confidently-wrong answer never wins on footprint. If correctness is statistically tied,
**characters decide** by the **geometric-mean of per-question ratios** (baseline ÷ octocode) +
median + leaner win-rate + bootstrap CI — never a pooled sum alone. Aggregate paired, per
question, over **≥3 passes**. Method + worked example: `references/aggregation-and-stats.md`.

## Quickstart (copy-paste, one matchup, one pass)

```bash
cd packages/octocode-benchmark
export HR_PY="$HOME/.local/share/uv/tools/headroom-ai/bin/python"   # Headroom arm only

# Phase 0 — preflight (non-zero exit = fix before running)
bash skills/octocode-benchmark/scripts/check-prereqs.sh 18.2.2

# set up a campaign dir
CAMP="campaigns/run-$(date -u +%H%M%S)-$(date -u +%Y-%m-%d)"; mkdir -p "$CAMP/answers" "$CAMP/judge"

# Phase 1 — answer (spawn ONE isolated agent per arm; never mix arms in an agent).
# Each research call sets its per-question log, e.g. octocode Q4:
OCTO_LOG="$CAMP/octocode-p1-Q4.jsonl" ./compare/bin/octoc1822 ghGetFileContent \
  --queries '{"owner":"axios","repo":"axios","path":"lib/adapters/http.js","matchString":"follow-redirects"}'
# baseline (gh+RTK) Q4:
RTK_LOG="$CAMP/rtk-p1-Q4.jsonl" ./compare/bin/rtkm search code --repo axios/axios follow-redirects --limit 20
# log the final answer, then append a "## Q4" section (Answer + Research steps) to answers/<arm>-p1.md
python3 compare/bin/record_answer.py --log "$CAMP/octocode-p1-Q4.jsonl" --question Q4 --file answer.txt

# Phase 2 — judge: build the blind packet, then one reasoning-first verdict per question
python3 compare/bin/build_blind_packet.py --help          # X/Y randomized, tool identity redacted

# Phase 3 — validate + aggregate + report
python3 compare/bin/sumlog.py --strict "$CAMP/octocode-p1-Q4.jsonl"
python3 compare/bin/validate_campaign.py "$CAMP" --question-count 30
python3 compare/bin/per_question_summary.py --out results/PER_QUESTION_SUMMARY.md --json results/per_question_summary.json
```

Runner/judge briefing packets, spawn scaling (batch Q1-15/Q16-30 within one arm), and output
layout: `references/run-with-agents.md` → `run-preflight.md` + `run-phases.md`.

## Hard gates (skip one and the run is worthless)

- **Isolation** — a fresh agent per (question, arm, pass) + a separate judge; no shared transcript, no answer key. Batch questions within an arm, never mix arms.
- **Fairness** — leanest-legal path on every arm; no whole-tree/whole-file dump where a targeted read/search answers (inflates chars, invalidates the ratio). `sumlog.py` emits advisory `FAIRNESS:` lines for `recursive=1` dumps / oversized reads — review them.
- **Blind + reasoned** — grade X/Y in randomized order; the judge reasons before scoring, correctness-first; a confidently-wrong answer never wins.
- **Measured, not self-reported** — `total_chars = model-in + model-out` from the instrumented log; recompute, never hand-count.
- **Honest stats** — geometric-mean char ratio (never a pooled sum) + bootstrap CI; ≥3 passes; the public set is orientation, not a shipping gate.

## Routes — load only what the step needs

| When | Load |
|---|---|
| understand the design | `references/BENCHMARK.md` |
| run a matchup | `references/INSTRUCTIONS.md` then `references/run-with-agents.md` |
| brief a runner | `references/RUNNER.md` + `references/RUNNER_TOOL_CONTEXT.md` (+ the arm's `primer-*.md`) |
| judge a question | `references/JUDGING.md` + `references/example-verdict.md` |
| score + aggregate | `references/SCORING.md` then `references/aggregation-and-stats.md` |
| write the report | `references/REPORT_TEMPLATE.md` |
| author a matchup README | `references/matchup-readme.md` |

## Scripts + tooling

- `scripts/check-prereqs.sh` — Phase 0 gate (all arms + questions + primers).
- `scripts/measure.sh` — fallback char wrapper for an arm without a dedicated `bin/` wrapper.
- `compare/bin/`: `octoc` · `octoc1822` · `rtkm` · `ghc` · `ghm` (arm wrappers) ·
  `instrument_command.py` / `hr_compress.py` (char capture) · `record_answer.py` ·
  `sumlog.py` (per-question total, `--strict`) · `build_blind_packet.py` (X/Y packet) ·
  `validate_campaign.py` (byte-faithful campaign check) · `per_question_summary.py`
  (per-question + overall chars & correctness across all 4 arms) · `test_instrumentation.py`.

## Stop when

The matchup's questions are answered, judged, and aggregated across ≥3 passes with CIs — or a
preflight/fairness violation blocks the run; fix before continuing.

## Add a question

Copy an existing `Q<n>.md`, bump the number, edit title / `id` / `## Question` only. GitHub →
`compare/github-questions/`; corpus-local → that matchup's `questions/`; add its README row.
