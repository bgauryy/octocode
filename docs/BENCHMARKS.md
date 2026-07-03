# Octocode Benchmark Strategy

> How to measure Octocode against other coding agents and LLMs on public benchmarks.

## How Octocode Competes

Octocode is a **tool provider (MCP server / CLI)** — not a standalone agent.
The comparison is always: **LLM + Octocode tools** vs. **LLM + Claude Code tools** vs. **LLM + Cursor tools**.

The benchmark question: *which tool provider helps an LLM solve coding tasks better, cheaper, and faster?*

Octocode's differentiated tools:
- `localSearchCode` — ripgrep + AST structural search
- `lspGetSemantics` — LSP symbols + find-references
- `localGetFileContent` — file reader with context-window minification
- `ghSearchCode` / `ghGetFileContent` — GitHub code search + file fetch
- `ghHistoryResearch` — commit + PR history research
- `localViewStructure` / `localFindFiles` — repo navigation
- `npmSearch` — package registry search
- `localBinaryInspect` — archive/binary inspection

---

## Benchmark Landscape

### Tier 1 — Leaderboard Standards (Agent-grade)

| Benchmark | Tasks | What it tests | Current SOTA | Leaderboard |
|---|---|---|---|---|
| **SWE-bench Verified** | 500 | Fix real GitHub issues; patch must pass existing test suite | Claude Fable 5 @ 95% | [swebench.com](https://www.swebench.com) |
| **SWE-bench Pro** | 731 public | Multi-file, multi-repo, long-horizon; contamination-resistant | Auggie (Claude Opus 4.5) @ 51.8% | [labs.scale.com](https://labs.scale.com/leaderboard/swe_bench_pro_public) |
| **LiveCodeBench** | 1000+ (v6) | Competitive programming (LeetCode/AtCoder/Codeforces), continuously refreshed | Claude Fable 5 @ 89.78% | [livebench.ai](https://livebench.ai) |

> **SWE-bench Verified is saturating** — Claude Fable 5 hits 95%, forcing the community toward SWE-bench Pro as the new difficulty frontier.

---

### Tier 2 — Composite / Multi-Dimension (Agentic)

| Index / Benchmark | Components | Operator |
|---|---|---|
| **Artificial Analysis Coding Agent Index** | DeepSWE (113 tasks) + Terminal-Bench v2 (84 tasks) + SWE-Atlas-QnA (124 tasks) | [artificialanalysis.ai](https://artificialanalysis.ai/agents/coding-agents) |
| **ProjDevBench** | Architecture design → functional correctness → iterative refinement (OJ + LLM judge) | arxiv (2025) |

Artificial Analysis tracks **cost per task**, **tokens per task**, **cache hit rate**, and **execution time** alongside accuracy — across harnesses (Claude Code vs. Cursor vs. OpenCode). Best single dashboard for apples-to-apples comparison.

---

### Tier 3 — Component Benchmarks (Model-grade)

| Benchmark | What it tests | Note |
|---|---|---|
| **HumanEval / EvalPlus** | Python function completion from docstrings | Saturated — top models hit 87%+. Dead for agent evals. |
| **MBPP** | ~500 crowd-sourced Python problems | Same saturation as HumanEval |
| **LiveCodeBench** *(also Tier 1)* | Continuously new problems — avoids contamination | Best contamination-resistant code gen baseline |

---

### Tier 4 — Specialized / Emerging

| Benchmark | Focus | Tasks |
|---|---|---|
| **SWE-Atlas-QnA** | Repository understanding + deep technical Q&A, rubric-scored | 124 |
| **Terminal-Bench v2** | Multi-step agentic shell/CLI workflows | 84 |
| **DeepSWE** | SE implementation tasks, anti-contamination design | 113 |
| **WebArena / WorkArena** | Web-agent tasks — relevant when coding agent uses browser | 810+ |

---

## Octocode-Specific Benchmark Mapping

### 🥇 Priority 1 — SWE-Atlas-QnA (Octocode's Home Turf)

**What it tests:** 124 deep technical Q&A tasks. Agent is given a real repo in Docker and must answer questions like *"how does module X handle edge case Y"*. Graded by expert-written rubrics. Pure **code comprehension + retrieval** — no code writing.

**Why this is Octocode's strongest benchmark:** No code generation required — just finding the right answer in a codebase. This directly exercises `localSearchCode` + `lspGetSemantics` + `ghHistoryResearch`. Octocode's AST search and LSP pipeline are built exactly for this.

**Key comparison:** Octocode-MCP vs. Claude Code's native bash tools — same LLM (e.g. Claude Opus 4.7), different tool providers — on the same 124 tasks.

**How to run:**
- Submit to Scale AI: [labs.scale.com/leaderboard/sweatlas-qna](https://labs.scale.com/leaderboard/sweatlas-qna)
- Tasks provide a Docker container with the target repo pre-loaded
- Agent must answer via natural language; graded against rubric items (0.0–1.0 per task)
- Primary metric: **Task Resolve Rate** (% of tasks scoring 1.0)

---

### 🥇 Priority 2 — SWE-bench Verified (Industry Standard)

**What it tests:** 500 human-verified GitHub issues across Python repos. Agent reads the repo, understands the bug, writes a patch — evaluated by running the repo's own test suite in Docker.

**Why it fits Octocode:** The full agentic workflow maps directly to Octocode's tool suite:

```
Issue description → ghSearchCode / localSearchCode → find relevant files
                 → lspGetSemantics → understand symbols + call graph
                 → localGetFileContent → read files (minified = fewer tokens)
                 → ghHistoryResearch → understand prior fixes / context
                 → [LLM writes patch] → test suite validates
```

**How to run:**
```bash
# Install SWE-bench harness
pip install swebench

# Run evaluation against your agent's predictions
python -m swebench.harness.run_evaluation \
  --dataset_name princeton-nlp/SWE-bench_Verified \
  --predictions_path ./octocode-agent-predictions.jsonl \
  --run_id octocode-vs-cursor-opus47

# Submit to leaderboard
# https://www.swebench.com — verified submissions go public
```

**Predictions format** (`predictions.jsonl`):
```jsonl
{"instance_id": "django__django-11099", "model_patch": "diff --git ...", "model_name_or_path": "octocode+claude-opus-4.7"}
```

**Scaffold options:**
- [SWE-agent](https://github.com/SWE-agent/SWE-agent) — most popular open-source harness, YAML tool bundles
- [mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) — lightweight, used by Artificial Analysis
- Custom: wire Octocode MCP server into any agent loop

**Compare against:** Claude Code, Cursor, OpenCode — same 500 tasks, same LLM, different tool scaffolds.

---

### 🥈 Priority 3 — SWE-bench Pro (Hardest Realistic Bar)

**What it tests:** 731 public tasks requiring multi-file edits across large repos. Performance drops from 70%+ on Verified to 23–51% on Pro — resists saturation and benchmark gaming.

**Why Octocode has an edge here:** Multi-file, cross-module issues reward structured code search over raw `grep`. Octocode's AST metavar patterns and LSP find-references are specifically better than shell-based competitors for navigating complex dependency graphs.

**Key finding:** Same model (Claude Opus 4.5), different harness = 49.75% (Claude Code) vs. 51.80% (Auggie). **The scaffold matters as much as the model.** This is exactly the gap Octocode should target.

**How to run:**
- Submit to Scale AI: [labs.scale.com/leaderboard/swe_bench_pro_public](https://labs.scale.com/leaderboard/swe_bench_pro_public)
- Private subset available for verified researchers — harder than public

---

### 🥈 Priority 4 — DeepSWE (Anti-Contamination Signal)

**What it tests:** 113 SE implementation tasks, designed by Datacurve specifically to avoid SWE-bench training contamination. Tasks are newer GitHub issues unlikely to appear in any model's training data.

**Why it matters:** SWE-bench Verified scores are partially inflated by training-set leakage. DeepSWE gives a cleaner signal.

**How to run:** Part of the Artificial Analysis Coding Agent Index. Contact Artificial Analysis or Datacurve to submit.

---

### 🥉 Priority 5 — LiveCodeBench (Baseline)

**What it tests:** Competitive programming on problems continuously pulled from LeetCode, AtCoder, and Codeforces. No repo navigation — pure code generation.

**Relevance to Octocode:** Low fit — Octocode is a retrieval tool, not a code generator. Useful only as a **baseline**: does adding Octocode tools hurt pure generation performance?

**How to run:** Fully open source — run locally.
```bash
git clone https://github.com/LiveCodeBench/LiveCodeBench
cd LiveCodeBench
pip install -e .
python -m lcb_runner.runner.main --model claude-opus-4-7 --scenario codegeneration
```

---

## What Octocode Already Benchmarks

| Dimension | Existing Script | Maps To |
|---|---|---|
| Tool reliability (all 13 tools) | `benchmark/octocode/run-live-smoke.mjs` | Artificial Analysis tool coverage |
| Code navigation (text/AST/symbols) | `benchmark/repo/run.mjs` — React, Tokio, Spring, Next.js, Chromium | SWE-Atlas-QnA upstream capability |
| Language/format support | `benchmark/check-matrix.mjs` — 151 extensions | Artificial Analysis format matrix |
| Minification / token efficiency | `benchmark/minify/check-minify.mjs` | Cost-per-task metric on AA index |
| AST search quality | `benchmark/ast/compare-ast-grep-cli.mjs` | SWE-bench code navigation sub-task |
| LSP quality | `benchmark/lsp/check-lsp.mjs` | SWE-Atlas-QnA symbol resolution |

---

## What's Missing (Build Targets)

| Gap | What to Build | Priority |
|---|---|---|
| End-to-end SE task solve rate | SWE-bench harness: spawn Octocode MCP server, run agent loop, output `predictions.jsonl` | **P0** |
| Per-task token comparison | Instrument MCP calls to count tokens; compare vs. Claude Code on same tasks | **P0** |
| SWE-Atlas-QnA runner | Docker task runner that feeds repo+question to Octocode+LLM, captures answer | **P1** |
| Leaderboard submission pipeline | CI job: run benchmark → format output → submit to swebench.com / Scale AI | **P1** |
| Harness comparison (same LLM) | Run Claude Opus 4.7 through: (a) Octocode-MCP, (b) bare bash, (c) Claude Code harness | **P1** |

---

## Key Findings from Research

1. **Same model, different harness = very different scores.** On SWE-bench Pro, Auggie vs. Claude Code vs. Cursor all used Claude Opus 4.5 but ranged 49.75% → 51.80%. The scaffold is the variable Octocode controls.

2. **SWE-bench Verified is saturating.** Claude Fable 5 hits 95%. The meaningful bar is SWE-bench Pro (51% ceiling) and SWE-Atlas-QnA (rubric-scored, partial credit).

3. **HumanEval is dead for agent evals.** Tests single-function completion in isolation, not multi-file agentic work.

4. **Cost/token efficiency is a tracked dimension.** Artificial Analysis plots performance vs. cost per task. Octocode's minification is a direct differentiator — measure it.

5. **Contamination is real.** LiveCodeBench's rotating pool and DeepSWE's fresh tasks are the gold standard for clean signal. SWE-bench Verified scores from frontier models should be read skeptically.

6. **SWE-Atlas-QnA is the most direct fit.** It measures code comprehension + retrieval — exactly what Octocode's tools are optimized for. This should be the first benchmark to run.

---

## Priority Execution Order

```
1. SWE-Atlas-QnA      ← best fit; pure retrieval; Octocode's direct advantage
2. SWE-bench Verified ← industry standard; gets Octocode on the public leaderboard  
3. SWE-bench Pro      ← hardest signal; multi-file tasks reward AST/LSP over bash grep
4. DeepSWE            ← anti-contamination; part of Artificial Analysis index
5. LiveCodeBench      ← baseline only; least differentiated for Octocode
```

**Fastest path to a comparison:** Run SWE-Atlas-QnA with `LLM + Octocode-MCP` vs. `LLM + bash tools` on the same 124 tasks. Directly measures what Octocode's search/LSP pipeline is worth vs. raw shell access — with the same model, same tasks, different tool provider.

---

## Resources

| Resource | URL |
|---|---|
| SWE-bench leaderboard | https://www.swebench.com |
| SWE-bench evaluation guide | https://www.swebench.com/SWE-bench/guides/evaluation |
| SWE-bench Pro leaderboard | https://labs.scale.com/leaderboard/swe_bench_pro_public |
| SWE-Atlas-QnA leaderboard | https://labs.scale.com/leaderboard/sweatlas-qna |
| Artificial Analysis Coding Agent Index | https://artificialanalysis.ai/agents/coding-agents |
| LiveCodeBench (open source) | https://github.com/LiveCodeBench/LiveCodeBench |
| SWE-agent harness | https://github.com/SWE-agent/SWE-agent |
| mini-swe-agent harness | https://github.com/SWE-agent/mini-swe-agent |
| Auggie vs. Cursor on SWE-bench Pro | https://www.augmentcode.com/blog/auggie-tops-swe-bench-pro |
| JetBrains TeamCity + SWE-bench guide | https://blog.jetbrains.com/teamcity/2025/09/testing-ai-coding-agents-with-teamcity-and-swe-bench |
| Existing Octocode benchmarks | `packages/octocode-benchmark/` |
| MCP vs CLI benchmark (ScaleKit) | https://github.com/scalekit-inc/mcp-vs-cli-benchmark |
| AXI design principles | https://axi.md |
| gh CLI issues for agents | https://github.com/cli/cli/issues/12522 |

---

## gh CLI vs GitHub MCP vs Octocode

> The industry has been debating MCP vs CLI. Octocode is neither — it's a third category: an agent-native code intelligence layer.

### The Three Contenders

| | `gh` CLI | GitHub MCP Server | Octocode MCP |
|---|---|---|---|
| **What it is** | GitHub's official CLI — thin REST/GraphQL wrapper | GitHub's official MCP server | Purpose-built agent code intelligence layer |
| **Code search** | Text keyword only (`gh search code`) | Text keyword only | Text + AST structural + LSP + OQL |
| **Local repo search** | ❌ No | ❌ No | ✅ Yes (ripgrep + AST + LSP) |
| **AST search** | ❌ No | ❌ No | ✅ Yes (metavar patterns, 151 formats) |
| **LSP semantics** | ❌ No | ❌ No | ✅ Yes (find-references, symbols) |
| **Output minification** | ❌ No — raw JSON dump | ❌ No — raw JSON dump | ✅ Yes — 28–58% token cut by default |
| **Caching** | ❌ No | ❌ No | ✅ Yes — per-session, deduplicates repeat calls |
| **Secret redaction** | ❌ No | ❌ No | ✅ Yes — built-in across all output |
| **npm package search** | ❌ No | ❌ No | ✅ Yes (`npmSearch`) |
| **Binary/archive inspection** | ❌ No | ❌ No | ✅ Yes (`localBinaryInspect`) |
| **Code search rate limit** | 10 req/min (shared API) | 10 req/min (shared API) | 10 req/min + session cache reduces calls |
| **Token cost per operation** | ~200 tokens (raw command) | ~44K tokens (full schema upfront) | Optimized output — schema amortized, content minified |
| **Primary failure mode** | Text-only, misses structural patterns | Schema bloat kills context window | Rate limit on heavy GitHub search sessions |

---

### What the Benchmarks Say (External Evidence)

**Source: Kun Chen, March 2026** — benchmarked GitHub MCP vs `gh` CLI vs Tool Search vs Code Mode on Claude Code across 5 real GitHub tasks:

| Condition | Avg tokens/task | Success rate | Avg latency |
|---|---|---|---|
| `gh` CLI (raw) | ~200 per cmd | High | Fast |
| GitHub MCP (raw) | ~44K upfront schema + task tokens | Moderate | Moderate |
| GitHub MCP + Tool Search | 16K start → converges to 43K+ | Lower on complex tasks | Moderate |
| GitHub MCP + Code Mode | Lowest of MCP variants | 5/5 on complex, 0/5 on edge cases | Slowest (43s avg) |

Key findings:
- **GitHub MCP is 2–3× more expensive than `gh` CLI** in token cost — pure schema overhead.
- **`gh` CLI wins on cost and latency** but has zero code intelligence — it's `grep` over GitHub text search.
- **Code Mode (batching MCP calls in scripts)** is cheapest MCP variant but 2× CLI cost and slowest (43s avg).
- **Neither CLI nor MCP solves the real problem**: output is not designed for agents — raw JSON dumps with 10+ fields per item.

**Source: ScaleKit benchmark** — gh CLI vs GitHub MCP on 5 GitHub tasks with Claude Sonnet 4:
- CLI was **10–32× cheaper** in tokens, more reliable in task success.
- MCP failure mode: schema loading consumes context before any task work begins.

**The community conclusion (Y Combinator / Garry Tan):** *"MCP sucks honestly. It eats too much context window… I vibe coded a CLI wrapper in 30 minutes and it worked 100× better."*

---

### Where Octocode Is Differentiated

The MCP vs CLI debate frames the wrong question. Both share the same fundamental flaw: **output not designed for agent consumption**. Octocode attacks this directly.

#### 1. Output Minification (measured, from `benchmark/minify/summary.json`)

Octocode's native minification across 46 measured file formats:

| Mode | Avg content cut | Notes |
|---|---|---|
| `standard` (content view) | **27.7%** | Default; removes noise, preserves structure |
| `minify` (apply mode) | **31.8%** | Stronger cut, still semantically correct |
| `symbols` (skeleton mode) | **58%** | Function signatures only — max context savings |

Per-language examples (measured):

| Language | Standard cut | Symbols cut |
|---|---|---|
| TypeScript | 59.6% | 69.2% |
| Java | 64.8% | 87.3% |
| Rust | 62.2% | 66.1% |
| Go | 34.1% | 86.8% |
| Python | 21.3% | 53.3% |
| Ruby | 64.2% | 81.5% |
| PHP | 41.4% | 87.1% |
| Dart | 85.5% | 98.8% |
| Scala | 80.7% | 94.1% |
| Swift | 65.5% | 81.3% |

**At symbols mode, a 500-line TypeScript file returns ~155 lines** (function signatures + exports only). This directly solves the MCP token problem without sacrificing navigability.

#### 2. AST Structural Search (no equivalent in gh CLI or GitHub MCP)

```
gh search code "useState"                  # text match — finds comments, strings, variable names
octocode search --pattern "useState($$$)"  # AST match — finds only call expressions
```

AST search eliminates false positives that text search returns. On a task like *"find all useState calls with a callback pattern"*, gh CLI returns every occurrence of the string; Octocode returns only structurally correct AST matches.

#### 3. LSP Find-References (unique to Octocode)

```
# gh CLI: impossible — no concept of LSP
# GitHub MCP: impossible — no LSP
octocode search src/auth/token.ts --op findReferences  # exact symbol usage across repo
```

For SWE-bench style tasks (fix a bug in a function), knowing every call site is critical. Grep-based search misses dynamic dispatch and re-exports.

#### 4. Rate Limit Handling

All three tools hit the **same GitHub Code Search rate limit: 10 requests/minute**.

Octocode mitigates this with:
- **Per-session result cache** — identical queries in a session are served from cache, not re-fetched
- **OQL query planning** — batches related searches into fewer API calls
- **Local-first routing** — `localSearchCode` for cloned repos bypasses GitHub API entirely

gh CLI and GitHub MCP have no equivalent mitigation.

#### 5. The AXI Principle (what Octocode implements natively)

The benchmark winner in Kun Chen's study was **AXI** — a set of 10 agent-ergonomic design principles that achieved 100% success at lowest cost/latency. Octocode implements these natively:

| AXI Principle | Octocode Implementation |
|---|---|
| Token-efficient output | Minification (28–58% cuts), symbols mode |
| Minimal default schemas | Compact JSON output by default (`--compact`) |
| Content truncation with size hints | `Large file (N lines) — use startLine/endLine` hints |
| Content first | Results include code snippets, not just file paths |
| Contextual disclosure | `bestContinuation` field in search results |
| Pre-computed fields | `evidence`, `anchors`, pagination metadata |
| Definitive empty states | Explicit `"No results found"` with query suggestions |
| Graceful error handling | Structured errors with recovery hints |
| Output discipline | `stdout` data, `stderr` debug; clean exit codes |
| Consistent help | `--scheme` flag outputs tool schema on demand |

---

### How to Benchmark This Directly

The cleanest experiment: same LLM, same task set, three tool providers.

```bash
# Task: answer 10 SWE-Atlas-QnA style codebase questions about a known repo
# Measure: tokens used, turns taken, correct answers

# Condition A — gh CLI only
CONDITION=gh-cli ./run-benchmark.sh

# Condition B — GitHub MCP server
CONDITION=github-mcp ./run-benchmark.sh

# Condition C — Octocode MCP
CONDITION=octocode-mcp ./run-benchmark.sh
```

Metrics to capture per condition:
- **Total tokens** (input + output) per task
- **Turns to answer** (fewer = better)
- **Answer accuracy** (rubric-graded or pass/fail)
- **Rate limit hits** (403 responses)
- **Latency** (wall time per task)

This maps directly to the ScaleKit and Kun Chen benchmark methodology — runnable with their open-source harnesses.

**Harnesses:**
- [scalekit-inc/mcp-vs-cli-benchmark](https://github.com/scalekit-inc/mcp-vs-cli-benchmark) — drop-in: set `BENCHMARK_REPO` and add Octocode as a third condition
- Custom: `packages/octocode-benchmark/benchmark/octocode/run-live-smoke.mjs` already instruments token counts from CLI output

---

### Summary Verdict

| Dimension | gh CLI | GitHub MCP | Octocode |
|---|---|---|---|
| Token cost | ✅ Lowest (raw) | ❌ 2–3× higher | ✅ Competitive (minified output) |
| Code intelligence | ❌ Text only | ❌ Text only | ✅ AST + LSP + OQL |
| Local repo search | ❌ No | ❌ No | ✅ Yes |
| Rate limit resilience | ❌ None | ❌ None | ✅ Caching + local routing |
| Security (secret redaction) | ❌ No | ❌ No | ✅ Built-in |
| Agent-ergonomic output | ❌ Raw JSON dump | ❌ Schema-heavy | ✅ Minified, structured, hinted |
| Best for | Quick one-off GitHub ops | Multi-user auth/governance | Agent coding tasks at scale |
