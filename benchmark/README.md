# Benchmark Suite

Benchmarks for AI-assisted code research. The goal is to measure which tool helps an agent produce the best verified answer per measured context budget.

Raw command speed is context only. Winners are decided by answer quality, research depth, and measured character/token cost.

---

## Directory Structure

```text
benchmark/
├── README.md                  # Map, runbook, and artifact contract
├── COMPARISON.md              # Scoring methodology and comparison rules
├── OCTOCODE_RESEARCHER.md     # Paste-ready prompt for Octocode researcher runs
├── judge/
│   └── prompt.md              # Unified judge prompt
├── scripts/
│   └── score-comparison.mjs   # Generic N-agent score combiner
├── github/                    # Runnable suite: octocode vs gh
│   ├── QUESTIONS.md
│   ├── README.md
│   ├── RUN_MANIFEST.template.md
│   ├── prompts/
│   └── scripts/
├── rtk/                       # Runnable suite: octocode vs rtk
│   ├── QUESTIONS.md
│   ├── README.md
│   ├── RUN_MANIFEST.template.md
│   ├── prompts/
│   └── scripts/
└── questions/                 # Reusable question banks, not runnable suites
    ├── README.md
    └── nextjs.md
```

Runnable suites have their own `QUESTIONS.md`, `scripts/`, prompts, manifest template, and `output/` directory. Files in `questions/` are shared question banks; use them by wiring them into a runnable suite or a future benchmark harness.

---

## Runnable Suites

| Suite | Agents | What It Tests |
|---|---|---|
| [`github/`](./github/README.md) | `octocode` vs `gh` | GitHub API breadth: code search, file content, repo structure, PR intelligence, repository search, package registry |
| [`rtk/`](./rtk/README.md) | `octocode` vs `rtk` | Local + GitHub research: result completeness, comment preservation, file metadata, PR body/label coverage |

Shared question banks:

| File | Target | Intended Use |
|---|---|---|
| [`questions/nextjs.md`](./questions/nextjs.md) | `vercel/next.js` | Full code-research comparison across GitHub, local clone, package, and LSP-style tasks |

---

## Core Measurement

The benchmark uses a deterministic character ruler:

```text
total_chars_to_answer = sum(in_chars + out_chars) across every metered call for the question
```

Approximate tokens are display-only:

```text
approx_tokens = ceil(total_chars_to_answer / 4)
```

Actual LLM token counters may be reported when present, but they do not replace the character ruler unless every compared agent has equivalent token accounting.

For the full scoring model, clean-win rules, drift handling, and token/character policy, see [`COMPARISON.md`](./COMPARISON.md).

---

## Scoring Summary

The judge assigns:

| Axis | Range | Meaning |
|---|---:|---|
| `Q` quality | 0-3 | Factual correctness of the final answer |
| `D` depth | 0-3 | Evidence quality, citations, exact lines, quotes, and completeness |
| `T` turns | raw count | Metered tool calls or model turns |

Computed scores:

```text
research_score  = Q * D
tradeoff_score  = research_score / max(total_chars_to_answer / 1000, 0.01)
turns_per_point = T / max(Q, 0.5)
```

Highest `research_score` means best answer quality. Highest `tradeoff_score` means best quality-adjusted cost. If the tradeoff winner has materially lower `Q` or `D`, report it as an efficiency win rather than a clean research win.

---

## Running A Suite

Read the suite README first:

- [`github/README.md`](./github/README.md)
- [`rtk/README.md`](./rtk/README.md)

Quick flow from the repository root:

```bash
# Fresh run for one agent
rm -rf benchmark/<suite>/output/<agent>
source benchmark/<suite>/scripts/init-run.sh <agent>

# For each question
bash benchmark/<suite>/scripts/set-q.sh <n>
# ... metered research through the suite wrapper ...
bash benchmark/<suite>/scripts/record.sh <n> "<model-id>" /tmp/answer.md

# Finalize
node benchmark/<suite>/scripts/finalize.mjs benchmark/<suite>/output/<agent>
```

Every research command must go through the suite's metering wrapper:

| Agent | Wrapper |
|---|---|
| `octocode` | `benchmark/<suite>/scripts/octo-meas.sh` |
| `gh` | `benchmark/github/scripts/gh-meas.sh` |
| `rtk` | `benchmark/rtk/scripts/rtk-meas.sh` |

Bare tool calls are unmetered and make the run invalid.

---

## Agent Prompts

| Role | Prompt |
|---|---|
| `researcher: octocode` | [`OCTOCODE_RESEARCHER.md`](./OCTOCODE_RESEARCHER.md), with `<BENCHMARK>` set to `github` or `rtk` |
| `researcher: gh` | [`github/prompts/researcher.md`](./github/prompts/researcher.md) |
| `researcher: rtk` | [`rtk/prompts/researcher.md`](./rtk/prompts/researcher.md) |
| `judge` | [`judge/prompt.md`](./judge/prompt.md) |

Keep researcher runs blind: do not read another agent's output or `output/summary.md` before finalizing the current run.

---

## Output Contract

Each completed agent run directory must contain:

```text
benchmark/<suite>/output/<agent>/
├── log.jsonl
├── q1.md
├── q1.json
├── ...
├── output.md
└── summary.json
```

Per-question metric shape:

```json
{
  "q": 1,
  "calls": 4,
  "in_chars": 678,
  "out_chars": 9111,
  "total_chars": 9789,
  "approx_tokens": 2448,
  "tool_elapsed_ms": 977,
  "q_elapsed_ms": 45703,
  "reasoning_ms": 44726
}
```

Publication-quality runs also need:

- completed `RUN_MANIFEST.template.md` copy
- exact model IDs, tool versions, auth source, benchmark commit SHA, and retrieval dates
- judge notes for every `Q < 3` or `D < 3`
- drift questions reported separately from main totals
- at least three repeated runs when stochastic agent behavior is being compared

---

## Combining Scores

After the judge writes a quality/depth JSON file, use the generic scorer:

```bash
node benchmark/scripts/score-comparison.mjs \
  --questions benchmark/questions/nextjs.md \
  --scores benchmark/output/quality-depth.json \
  --markdown \
  octocode=benchmark/output/octocode \
  gh=benchmark/output/gh \
  rtk=benchmark/output/rtk
```

The scorer only combines judge-assigned `Q`/`D` scores with measured run metrics. It does not fact-check answers.
