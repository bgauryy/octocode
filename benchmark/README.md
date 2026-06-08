# Benchmark Suite

Comparative benchmarks for AI-assisted code research. Each benchmark measures one or more research agents on the same question set and reports **three axes**: answer quality, research depth, and token efficiency.

---

## Benchmarks

| Benchmark | Agents | Question Set | What It Tests |
|---|---|---|---|
| [`github/`](./github/README.md) | octocode · gh CLI | [`github/QUESTIONS.md`](./github/QUESTIONS.md) | GitHub API breadth — code search, file content, repo structure, PR intelligence, repo search |
| [`rtk/`](./rtk/README.md) | octocode · rtk CLI | [`rtk/QUESTIONS.md`](./rtk/QUESTIONS.md) | Local + GitHub research — comment preservation, result completeness, PR metadata, remote content |
| [`headroom/`](./headroom/README.md) | octocode · octocode+headroom · rtk · gh | [`questions/nextjs.md`](./questions/nextjs.md) | Full-stack research — GitHub API + local clone + LSP, measuring actual LLM token cost and research depth |

---

## Scoring Model

All benchmarks use the **same three-axis scoring model** applied per question by the judge:

### Axis 1 — Answer Quality `Q` (0–3)
Factual correctness verified against source.

| Score | Meaning |
|---:|---|
| 3 | All load-bearing facts correct and complete |
| 2 | Mostly correct — one sub-fact missing or inaccurate |
| 1 | Partially correct — unsupported claim present, or key fact missing |
| 0 | Wrong, empty, or `UNKNOWN` |

### Axis 2 — Research Depth `D` (0–3)
How thoroughly the agent researched the answer.

| Score | Meaning |
|---:|---|
| 3 | All sub-questions answered with file:line citations; cross-references followed and verified at source |
| 2 | Most sub-questions answered; citations present but some gaps or surface-level treatment |
| 1 | Some sub-questions answered; superficial; missing important context or citations |
| 0 | No meaningful depth — no citations, entirely surface-level or hallucinated |

### Axis 3 — Turns `T`
Number of tool invocations (`calls` from per-Q JSON). Fewer turns at the same quality is more efficient.

### Composite Scores

```
research_score   = Q × D                              # 0–9; penalizes shallow-but-correct
tradeoff_score   = research_score / (effective_chars / 1000)   # research value per 1k chars
turns_per_point  = T / max(Q, 0.5)                   # avg calls needed per quality point
```

`effective_chars = in_chars + out_chars (+ amortized MCP init for octocode)` — the canonical tokenizer-independent cost proxy.

Winner axis: **tradeoff_score** (research value per measured character budget). Ties broken by `turns_per_point` (fewer turns at same efficiency = better).

> Wall-clock time is always context-only — it never decides the winner.

---

## Questions

The [`questions/`](./questions/) folder is the shared question bank.

| File | Scope | Questions |
|---|---|---|
| [`questions/nextjs.md`](./questions/nextjs.md) | `vercel/next.js` — external (GitHub API) + local (clone + LSP) | 20 |

Each per-benchmark `QUESTIONS.md` may use a subset or a dedicated set. See [`questions/README.md`](./questions/README.md) for the question format specification and how to add new question sets.

---

## Unified Judge

All benchmarks share a single judge design documented in [`judge/prompt.md`](./judge/prompt.md). Each per-benchmark `prompts/judge.md` is a thin wrapper that fills in the agent names, run paths, and question set path before delegating to the unified scoring logic.

The judge:
1. Independently fact-checks each answer against live sources before scoring
2. Assigns `Q` (quality) and `D` (depth) per question per agent
3. Reads `calls`, `in_chars`, `out_chars` from per-Q JSON artifacts
4. Computes `tradeoff_score` and `turns_per_point` per question
5. Writes `summary.md` in the benchmark output dir

---

## Running a Benchmark

Each benchmark's `README.md` contains full operator instructions. Quick reference:

```bash
# 1. Init the run (creates output/<agent>/, sets $RUN/$LOG)
source benchmark/<suite>/scripts/init-run.sh <agent>

# 2. For each question: set-q, research, record
bash benchmark/<suite>/scripts/set-q.sh <n>
# ... research with metered wrapper ...
bash benchmark/<suite>/scripts/record.sh <n> "<model-id>" /tmp/answer.md

# 3. Finalize
node benchmark/<suite>/scripts/finalize.mjs benchmark/<suite>/output/<agent>

# 4. Judge (paste judge/prompt.md to agent with paths filled in)
```

Fresh start for a run:
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

`summary.json` schema per question:
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

Judge-assigned fields (written into `summary.md`, not `summary.json`):
- `quality_score` (0–3), `depth_score` (0–3), `research_score` (Q×D), `tradeoff_score`, `turns_per_point`
