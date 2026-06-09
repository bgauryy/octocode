# Benchmark Suite

Comparative benchmarks for AI-assisted code research. Each benchmark measures research agents on the same question set and reports **answer quality, research depth, and token efficiency**.

---

## Benchmarks

| Benchmark | Agents | What It Tests |
|---|---|---|
| [`github/`](./github/README.md) | octocode · gh CLI | GitHub API breadth — code search, file content, repo structure, PR intelligence, package registry |
| [`rtk/`](./rtk/README.md) | octocode · rtk CLI | Local + GitHub research — comment preservation, result completeness, PR metadata |

Researcher prompts: [`OCTOCODE_RESEARCHER.md`](./OCTOCODE_RESEARCHER.md) (octocode) · each benchmark's `prompts/` for the comparison agent.

---

## Scoring Model

Applied per question by the judge.

### Quality `Q` (0–3) — factual correctness

| Score | Meaning |
|---:|---|
| 3 | All load-bearing facts correct and complete |
| 2 | Mostly correct — one sub-fact missing or inaccurate |
| 1 | Partially correct — unsupported claim or key fact missing |
| 0 | Wrong, empty, or `UNKNOWN` |

### Depth `D` (0–3) — research thoroughness

| Score | Meaning |
|---:|---|
| 3 | All sub-questions answered with file:line citations; cross-references verified at source |
| 2 | Most sub-questions answered; citations present but gaps or surface treatment |
| 1 | Some sub-questions answered; superficial; missing context or citations |
| 0 | No meaningful depth — no citations, surface-level or hallucinated |

### Turns `T` — tool invocations
`calls` from per-Q JSON. Fewer turns at the same quality is more efficient.

### Composite scores

```
research_score        = Q × D
total_chars_to_answer = Σ(in_chars + out_chars) across ALL calls for the question
tradeoff_score        = research_score / (total_chars_to_answer / 1000)
turns_per_point       = T / max(Q, 0.5)
```

`total_chars_to_answer` is the canonical cost proxy — it counts every follow-up call a tool makes to get the answer, not just the first. Targeted retrieval that answers in one call pays less than three follow-up calls for the same result.

Winner axis: **tradeoff_score**. Ties broken by `turns_per_point`.

> Wall-clock time is context-only — it never decides the winner.

---

## Publication-Quality Run Standard

A run is considered publication-ready when it includes:

- Raw agent run directories with `log.jsonl`, every `q<n>.md`, every `q<n>.json`, `output.md`, and `summary.json`.
- A judge summary with evidence notes for every score below 3, plus clear treatment of drift questions.
- A completed `RUN_MANIFEST.template.md` copy with model IDs, tool versions, refs, and retrieval dates.
- Repository refs or retrieval dates for facts that can drift over time.
- At least three same-agent runs when stochastic agent behavior is being compared, with variance reported when repeated runs exist.
- The exact model IDs, tool versions, authentication source, and benchmark commit SHA used for the run.

---

## Agent Prompts

| Role | Prompt file | Usage |
|---|---|---|
| `researcher: octocode` | [`OCTOCODE_RESEARCHER.md`](./OCTOCODE_RESEARCHER.md) | Fill in `<BENCHMARK>` with `github` or `rtk` |
| `judge` | [`judge/prompt.md`](./judge/prompt.md) | Fill in `AGENTS`, `RUNS`, `QUESTIONS`, `OUTPUT` placeholders |

---

## Running a Benchmark

Full instructions in each benchmark's `README.md`. Quick reference:

```bash
# Init
source benchmark/<suite>/scripts/init-run.sh <agent>

# Per question
bash benchmark/<suite>/scripts/set-q.sh <n>
# ... metered research ...
bash benchmark/<suite>/scripts/record.sh <n> "<model-id>" /tmp/answer.md

# Finalize
node benchmark/<suite>/scripts/finalize.mjs benchmark/<suite>/output/<agent>
```

Fresh start:
```bash
rm -rf benchmark/<suite>/output/<agent>
```

---

## Output Layout

```
benchmark/<suite>/output/
├── <agent-a>/
│   ├── log.jsonl
│   ├── q1.md  q1.json  ...  qN.md  qN.json
│   ├── output.md
│   └── summary.json
├── <agent-b>/
│   └── ...
└── summary.md          ← judge output
```

`summary.json` per-question schema:
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

Judge-assigned fields (in `summary.md`): `Q`, `D`, `research_score`, `tradeoff_score`, `turns_per_point`.
