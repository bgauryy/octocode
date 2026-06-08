# Headroom Benchmark

Four-way comparison of AI-assisted code research agents, evaluated on **token efficiency, research depth, and answer quality**. This is the only benchmark in this suite that measures **actual LLM token counts** (not just tool I/O character proxies).

---

## What Is Being Compared

| Agent | How it works | Local? | Token tracking |
|---|---|---|---|
| `octocode` | Structured GitHub API + local filesystem + LSP via Octocode MCP tools | ✅ | chars/4 proxy |
| `octocode+headroom` | Same Octocode tools, routed through headroom proxy — tool outputs compressed before LLM sees them | ✅ | **actual LLM tokens** via `headroom_stats` |
| `rtk` | Shell-level CLI tools through RTK output filter — rg, ls, cat, gh compressed at stdout | ✅ | chars/4 proxy |
| `gh` | GitHub CLI (`gh`) — raw API calls, no compression or structured tooling | ❌ | chars/4 proxy |

The key question: **does headroom's context compression maintain research quality while reducing real LLM token cost?** And how does compressed-octocode compare to RTK (which already compresses at the shell layer) and raw gh?

---

## Why This Benchmark Exposes Real Tradeoffs

The existing `github/` and `rtk/` benchmarks measure tool I/O characters as a token proxy. This undercounts real LLM consumption by 3–10× because it ignores accumulated conversation history, system prompt, tool schemas, and multi-turn context.

Headroom proxy intercepts every LLM API call and reports `tokens_in` / `tokens_out` per turn — the only way to measure full context cost without instrumenting the model provider directly.

Key tradeoffs exposed:

| Dimension | Questions that test it |
|---|---|
| **Compression quality preservation** — does headroom's SmartCrusher strip important code search context? | Q1, Q3, Q11, Q19 |
| **Multi-turn context accumulation** — how does per-turn token cost grow across a question session? | All questions (tracked via turns × tokens_per_turn) |
| **Large output compression** — does headroom save tokens on big file reads / PR diffs? | Q5, Q7, Q8, Q13, Q15 |
| **LSP semantic navigation** — compression impact on structured symbol resolution output | Q17, Q18, Q20 |
| **Depth vs efficiency tradeoff** — does compressing outputs force more follow-up tool calls (more turns)? | All questions (turns_per_point metric) |

---

## Per-Question Capability Matrix

Not all agents can answer all questions. This matrix shows capability scope before a run so agents know which questions they can compete on.

| Q | Category | octocode | octocode+headroom | rtk | gh | What the gap tests |
|---|---|---|---|---|---|---|
| Q1 | SEARCH | ✅ | ✅ | ⚠️ result cap | ✅ | Result completeness |
| Q2 | SEARCH | ✅ bulk 3-in-1 | ✅ | ⚠️ 3 calls | ✅ 3 calls | Bulk query efficiency |
| Q3 | SEARCH | ✅ textMatch | ✅ | ⚠️ limited context | ✅ | Match context quality |
| Q4 | SEARCH | ✅ AND-intersect | ✅ | ❌ OR-union | ❌ OR-union | AND-intersection |
| Q5 | CONTENT | ✅ matchString | ✅ | ⚠️ 2000-char cap | ✅ blob | Targeted read vs full blob |
| Q6 | STRUCTURE | ✅ | ✅ | ✅ rtk ls | ✅ | — |
| Q7 | STRUCTURE | ✅ | ✅ | ✅ rtk ls | ✅ | — |
| Q8 | PR | ✅ withCommits | ✅ | ⚠️ limited body | ✅ | PR body + file list |
| Q9 | PR | ✅ inline comments | ✅ | ❌ no inline | ❌ no inline | Inline review thread access |
| Q10 | PACKAGE | ✅ packageSearch | ✅ | ❌ out of scope | ❌ out of scope | Registry lookup (octocode-exclusive) |
| Q11 | LOCAL | ✅ localSearchCode | ✅ | ✅ rtk rg | ❌ no local | Local capability gap |
| Q12 | LOCAL | ✅ localSearchCode | ✅ | ⚠️ comment strip risk | ❌ no local | Comment text in search |
| Q13 | LOCAL | ✅ localFindFiles | ✅ | ⚠️ no size metadata | ❌ no local | File metadata |
| Q14 | LOCAL | ✅ localViewStructure | ✅ | ✅ rtk ls | ❌ no local | — |
| Q15 | LOCAL | ✅ localGetFileContent | ✅ | ⚠️ comment strip | ❌ no local | Comment preservation |
| Q16 | LOCAL | ✅ localGetFileContent | ✅ | ⚠️ comment strip | ❌ no local | Comment preservation |
| Q17 | LSP | ✅ lspGotoDefinition | ✅ | ⚠️ grep-only | ❌ no local/LSP | Symbol definition precision |
| Q18 | LSP | ✅ lspGotoDefinition | ✅ | ⚠️ grep-only | ❌ no local/LSP | Type definition precision |
| Q19 | LOCAL | ✅ localSearchCode | ✅ | ✅ rtk rg | ❌ no local | Cross-package exhaustiveness |
| Q20 | LSP | ✅ lspCallHierarchy | ✅ | ❌ no call hierarchy | ❌ no local/LSP | Call hierarchy (octocode-exclusive) |

**Score any ❌ question as Q=0, D=0 with note "capability gap — out of scope" (not penalized in the tradeoff tally).**

The benchmark answers three overlapping research questions:
1. **Compression benefit**: does `octocode+headroom` maintain quality vs `octocode` while spending fewer real LLM tokens?
2. **Tool capability**: how does octocode's structured API compare to rtk's shell compression and gh's raw CLI?
3. **Full-stack coverage**: which agent handles the complete spectrum (GitHub API + local + LSP + package registry)?

---

## Target Repository

All questions are about **`vercel/next.js`** — a large TypeScript monorepo with rich GitHub history, over-size-limit files, dense PR activity, and deep LSP symbol graphs.

Question set: [`benchmark/questions/nextjs.md`](../questions/nextjs.md) — 20 questions:
- **Q1–Q10**: external (GitHub API tools)
- **Q11–Q20**: local (requires clone + local tools + LSP)

Clone for local researchers:
```bash
git clone --depth 1 https://github.com/vercel/next.js /tmp/nextjs-bench
```

---

## Agents

### `octocode`
Uses Octocode MCP tools directly. Baseline: no compression, full structured output.

Metering: `scripts/octo-meas.sh` (same as `github/` benchmark).

### `octocode+headroom`
Runs Octocode MCP tools but routes all LLM calls through a local headroom proxy. Headroom compresses MCP tool outputs before they reach the LLM context window.

Setup:
```bash
pip install "headroom-ai[all,mcp]"
headroom proxy --port 8787 --log-tokens &   # start headroom proxy
export ANTHROPIC_BASE_URL=http://localhost:8787   # route LLM calls through proxy
```

Metering: `scripts/headroom-meas.sh` wraps Octocode tool calls the same as `octo-meas.sh`, but also captures headroom performance stats after each question to record actual LLM tokens.

After each question, query headroom stats and reset the counter:
```bash
headroom perf --json > /tmp/headroom-q<n>-stats.json
headroom perf --reset
```

The stats file records: `tokens_input`, `tokens_output`, `compression_ratio`, `turns` (LLM API call count), `savings_percent`.

> Note: `headroom perf` is the headroom CLI command for session performance stats. The `headroom_stats` MCP tool provides the same data inside an MCP client session. Use whichever is available in your integration; both output the same JSON schema.

### `rtk`
Runs shell commands through RTK's output filter. Same as `rtk/` benchmark — see [`../rtk/README.md`](../rtk/README.md) for setup.

Metering: `scripts/rtk-meas.sh`.

### `gh`
Uses GitHub CLI directly. Same as `github/` benchmark — see [`../github/README.md`](../github/README.md) for setup.

Metering: `scripts/gh-meas.sh`.

---

## Metrics

### Per-question data (`q<n>.json`)

Standard fields (all agents):
```json
{
  "calls":         <tool invocations>,
  "in_chars":      <query payload chars>,
  "out_chars":     <tool output chars>,
  "total_chars":   <in + out>,
  "approx_tokens": <total_chars / 4>,
  "tool_elapsed_ms": <tool wall time>,
  "q_elapsed_ms":  <full Q wall clock>,
  "reasoning_ms":  <q_elapsed - tool_elapsed>
}
```

Additional fields for `octocode+headroom` agent only:
```json
{
  "lm_tokens_in":       <actual LLM input tokens from headroom_stats>,
  "lm_tokens_out":      <actual LLM output tokens from headroom_stats>,
  "lm_turns":           <LLM API call count from headroom_stats>,
  "compression_ratio":  <headroom compression ratio 0.0–1.0>,
  "savings_percent":    <headroom reported savings %>
}
```

### Scoring (judge-assigned)

```
Q = quality_score    (0–3)   factual accuracy
D = depth_score      (0–3)   research thoroughness + citations
T = calls                    tool invocations (= turns proxy for non-headroom agents)
  = lm_turns                 actual LLM turns (headroom agent only)

effective_chars = in_chars + out_chars   (+ amortized MCP init for octocode agents)
effective_tokens = lm_tokens_in + lm_tokens_out   (headroom agent only; others use effective_chars / 4)

research_score  = Q × D                                 (0–9)
tradeoff_score  = research_score / (effective_chars / 1000)
turns_per_point = T / max(Q, 0.5)
```

Winner axis: `tradeoff_score`. Secondary: `turns_per_point`. Wall-clock time is context-only.

For the headroom agent, `summary.md` reports BOTH `tradeoff_score` (chars-based, comparable to other agents) and `tradeoff_score_tokens` (actual LLM token-based, headroom only).

---

## Output Layout

```
benchmark/headroom/output/
├── octocode/
│   ├── log.jsonl
│   ├── q1.md  q1.json  ...
│   ├── output.md
│   └── summary.json
├── octocode-headroom/
│   ├── log.jsonl
│   ├── q1.md  q1.json          ← includes lm_tokens_in/out, lm_turns, compression_ratio
│   ├── headroom-stats.jsonl    ← raw headroom_stats output per question
│   ├── output.md
│   └── summary.json
├── rtk/
│   └── ...
├── gh/
│   └── ...
└── summary.md                  ← judge output (4-way comparison)
```

---

## Scripts

| Script | Who uses it | Purpose |
|---|---|---|
| `scripts/init-run.sh <agent>` | operator | Creates `output/<agent>/`, exports `$SESSION`, `$RUN`, `$LOG`. Agent = `octocode` \| `octocode-headroom` \| `rtk` \| `gh` |
| `scripts/set-q.sh <n>` | researcher | Sets current question sentinel, starts Q wall-clock |
| `scripts/octo-meas.sh <tool> '<queries-json>'` | octocode researcher | Wraps `octocode tools`, logs char I/O |
| `scripts/headroom-meas.sh <tool> '<queries-json>'` | octocode+headroom researcher | Wraps `octocode tools` via headroom proxy, logs char I/O + headroom token stats |
| `scripts/rtk-meas.sh <rtk args>` | rtk researcher | Wraps `rtk`, logs argv/stdout I/O |
| `scripts/gh-meas.sh <gh args>` | gh researcher | Wraps `gh`, logs argv/stdout I/O |
| `scripts/record.sh <n> <model> /tmp/answer.md` | researcher | Writes `q<n>.md` + `q<n>.json` |
| `scripts/finalize.mjs <run-dir>` | researcher | Writes `output.md` + `summary.json` |
| `scripts/aggregate.mjs` | internal | Sums log rows for one Q |

---

## Dependencies

| Agent | Dependencies |
|---|---|
| `octocode` | `octocode-cli` installed (`npm install -g octocode-cli`) |
| `octocode+headroom` | `octocode-cli` + `headroom-ai[all]` + headroom proxy running on port 8787 |
| `rtk` | `rtk` ≥ 0.28 installed, repo cloned at `/tmp/nextjs-bench` |
| `gh` | `gh` CLI installed and authenticated |

All agents need `node` for metering scripts.

---

## If You Are an Agent: Choose Your Role First

| Role | What you do | Output dir |
|---|---|---|
| `researcher: octocode` | Answer all 20 questions using metered `octo-meas.sh` calls | `benchmark/headroom/output/octocode/` |
| `researcher: octocode+headroom` | Answer all 20 questions using metered `headroom-meas.sh` calls via headroom proxy | `benchmark/headroom/output/octocode-headroom/` |
| `researcher: rtk` | Answer all 20 questions using metered `rtk-meas.sh` calls | `benchmark/headroom/output/rtk/` |
| `researcher: gh` | Answer all 20 questions using metered `gh-meas.sh` calls | `benchmark/headroom/output/gh/` |
| `judge` | Score all 4 completed runs and write `summary.md` | `benchmark/headroom/output/summary.md` |

---

## Researcher Instructions (all roles)

### Setup

```bash
# Clone target repo (rtk and local-question agents need this)
git clone --depth 1 https://github.com/vercel/next.js /tmp/nextjs-bench

# For octocode+headroom only: start headroom proxy
headroom proxy --port 8787 --log-tokens &
export ANTHROPIC_BASE_URL=http://localhost:8787

# Init the run
rm -rf benchmark/headroom/output/<agent>
source benchmark/headroom/scripts/init-run.sh <agent>
```

### Per-question loop

```bash
bash benchmark/headroom/scripts/set-q.sh <n>
# research with metered wrapper
# write answer to /tmp/answer.md
bash benchmark/headroom/scripts/record.sh <n> "<model-id>" /tmp/answer.md
```

For `octocode+headroom`, after research and before `record.sh`:
```bash
headroom perf --json > /tmp/headroom-q<n>-stats.json
headroom perf --reset
```

### Finalize

```bash
node benchmark/headroom/scripts/finalize.mjs benchmark/headroom/output/<agent>
```

---

## Judge Instructions

Use the unified judge at [`benchmark/judge/prompt.md`](../judge/prompt.md).

Fill in the placeholder values:

```
AGENTS:     octocode, octocode-headroom, rtk, gh
QUESTIONS:  benchmark/questions/nextjs.md
OUTPUT:     benchmark/headroom/output/summary.md
```

The judge:
1. Reads all 4 completed run dirs
2. Independently fact-checks each answer against `vercel/next.js` source/PRs
3. Assigns `quality_score (0–3)` and `depth_score (0–3)` per question per agent
4. Computes `research_score`, `tradeoff_score`, `turns_per_point` per Q
5. Writes the 4-way comparison summary

For the `octocode-headroom` agent, the judge also reports:
- `lm_tokens_in` / `lm_tokens_out` (actual LLM cost from headroom stats)
- `compression_ratio` per question
- `tradeoff_score_tokens` (quality×depth / (lm_tokens_total / 1000))

---

## Publication-Quality Run Standard

A run is publication-ready when it includes:

- Raw run dirs for all 4 agents with `log.jsonl`, every `q<n>.md`, `q<n>.json`, `output.md`, `summary.json`
- For `octocode-headroom`: `headroom-stats.jsonl` with per-question token data
- A judge summary with depth score evidence for every score below 3
- Model IDs, tool versions, headroom version, `vercel/next.js` commit SHA, benchmark commit SHA
- At least three same-agent runs for variance reporting

---

## Links

- Questions: [`benchmark/questions/nextjs.md`](../questions/nextjs.md)
- Unified judge: [`benchmark/judge/prompt.md`](../judge/prompt.md)
- Headroom: [https://github.com/chopratejas/headroom](https://github.com/chopratejas/headroom)
- RTK (ships inside headroom): [https://github.com/rtk-ai/rtk](https://github.com/rtk-ai/rtk)
- Related benchmarks: [`benchmark/github/`](../github/README.md) · [`benchmark/rtk/`](../rtk/README.md)
