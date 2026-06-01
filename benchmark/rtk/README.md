# RTK vs Octocode Benchmark

This directory benchmarks two approaches to LLM-assisted code research on the same codebase: **rtk CLI filtering** vs **Octocode MCP tools**. The metric is **semantic answer quality per measured character**. Characters are the deterministic token-usage proxy: every metered call records `in_chars + out_chars`. Elapsed time is recorded for context only.

---

## What is being compared

| Dimension | rtk researcher | octocode researcher |
|---|---|---|
| **How it works** | Runs native CLI tools (`rg`, `ls`, `find`, `cat`, `gh`) through rtk's output filter, which compresses text before the LLM sees it | Calls GitHub API and local filesystem directly via structured MCP tools |
| **Code search** | `rtk rg <pattern> <path>` — has configured result limits and long-line compression | `localSearchCode` — full results, explicit pagination, no line truncation |
| **File content** | `rtk read <file>` — language-aware filter **strips comments by default** (`Minimal` level) | `localGetFileContent` — full fidelity, char-offset pagination, `matchString` anchor |
| **Directory listing** | `rtk ls <path>` / `rtk tree <path>` — applies its configured directory filters | `localViewStructure` — full tree, structured metadata, configurable depth |
| **File finding** | `rtk find <path>` — same hidden dirs, no size/mtime metadata | `localFindFiles` — size, mtime, extension filters, structured output |
| **GitHub PR research** | `rtk gh pr view <n>` — drops labels, comments, assignees, file change list | `githubSearchPullRequests` — all metadata, comments option, diff access |
| **GitHub file content** | `rtk gh api repos/.../contents/path` — 2000-char passthrough window | `githubGetFileContent` — full content with char-offset pagination |
| **Package lookup** | Out of scope for rtk's CLI filtering model | `packageSearch` — npm/PyPI registry API: version, downloads, homepage |
| **LSP navigation** | Out of scope for rtk's CLI filtering model | `lspGotoDefinition`, `lspFindReferences`, `lspCallHierarchy` |

---

## Why This Benchmark Exposes Real Tradeoffs

The questions compare how each toolset behaves on four code-research dimensions where output filtering and structured retrieval make different tradeoffs:

| Dimension | Which questions test it |
|---|---|
| **Comment preservation** — source comments often carry architectural decisions, TODOs, safety annotations, and API contracts. | Q3, Q4, Q5, Q19 |
| **Result completeness** — result ceilings and long-line compression can affect exhaustive counts. | Q1, Q2, Q15, Q16 |
| **PR metadata coverage** — labels, comments, assignees, files, and CI details can matter for PR archaeology. | Q10, Q11, Q12, Q17 |
| **Remote content breadth** — remote file and directory retrieval can require pagination or additional calls. | Q13, Q14, Q20 |

Questions Q6–Q9 and Q18 test structural, metadata, and registry capabilities that sit outside rtk's core filtering model.

---

## Target repository

All questions are about **`rtk-ai/rtk`** (the Rust Token Killer itself).

- The **rtk researcher** clones the repo locally and answers local questions using `rtk` CLI commands against the clone. For GitHub operations (PRs, remote file content), use `rtk gh` commands.
- The **octocode researcher** answers entirely via remote Octocode MCP tools — no local clone needed.

Cloning for the rtk researcher:
```bash
git clone https://github.com/rtk-ai/rtk /tmp/rtk-bench
```

---

## If you are an agent: choose your role first

Start by confirming which role you were assigned.

| Assigned role | What you do | Output directory |
|---|---|---|
| `researcher: octocode` | Answer all questions using only metered Octocode MCP calls | `benchmark/rtk/output/octocode/` |
| `researcher: rtk` | Answer all questions using only metered `rtk` CLI commands | `benchmark/rtk/output/rtk/` |
| `judge` | Compare completed runs semantically and by efficiency | `benchmark/rtk/output/summary.md` |

If your assigned role is unclear, ask before starting.

---

## Dependencies

- **rtk researcher**: `rtk` ≥ 0.28 installed, `git`, `node` (for metering script), repo cloned at `/tmp/rtk-bench`
- **octocode researcher**: Octocode MCP server, `node` (for `mcp-meas.mjs`)
- Metering is character-only. Tokenizer libraries are outside this benchmark's ruler.

---

## Publication-Quality Run Standard

A run is considered publication-ready when it includes:

- Raw `octocode` and `rtk` run directories with `log.jsonl`, every `q<n>.md`, every `q<n>.json`, `output.md`, and `summary.json`.
- A judge summary with evidence notes for every score below 3, plus clear treatment of date-sensitive questions.
- A completed `RUN_MANIFEST.template.md` copy with model IDs, tool versions, refs, and retrieval dates.
- Repository refs or retrieval dates for facts that can drift over time.
- At least three same-agent runs when stochastic agent behavior is being compared, with variance reported when repeated runs exist.
- The exact model IDs, tool versions, authentication source, `rtk-ai/rtk` commit SHA or retrieval date, and benchmark commit SHA used for the run.

---

## Output layout

```text
benchmark/rtk/output/
├── octocode/
│   ├── log.jsonl
│   ├── q1.md
│   ├── q1.json
│   ├── ...
│   ├── output.md
│   └── summary.json
├── rtk/
│   ├── log.jsonl
│   ├── q1.md
│   ├── q1.json
│   ├── ...
│   ├── output.md
│   └── summary.json
└── summary.md              # judge output
```

Fresh benchmark:
```bash
rm -rf benchmark/rtk/output/octocode benchmark/rtk/output/rtk
```

---

## How metering works

Every tool call goes through a wrapper that logs:
```json
{"q": 1, "agent": "rtk", "cmd": "rtk rg ...", "in_chars": 42, "out_chars": 1800, "elapsed_ms": 12, "exit": 0}
```

| Agent | Hook | `in_chars` | `out_chars` |
|---|---|---|---|
| `octocode` | `scripts/mcp-meas.mjs` proxies MCP stdio | Unicode codepoints of `JSON.stringify(params.arguments)` for `tools/call` | Unicode codepoints of `result.content[].text` joined in order |
| `rtk` | `scripts/rtk-meas.sh` → `rtk-meas.mjs` | Unicode codepoints of the full rtk command argv (no `rtk ` prefix) | Unicode codepoints of exact stdout + stderr |

### MCP init/context cost

The Octocode run pays a one-time init cost for `initialize` and `tools/list` (server instructions + all tool schemas enter agent context). `mcp-meas.mjs` logs these as `q=0` rows. The judge must include them in total Octocode chars.

The rtk run has no equivalent schema-loading cost.

---

## Script reference

| Script | Who uses it | Purpose |
|---|---|---|
| `scripts/init-run.sh <agent>` | operator | Creates `output/<agent>/`, exports `$SESSION`, `$RUN`, `$LOG` |
| `scripts/set-q.sh <n>` | researcher | Sets current question sentinel, starts Q wall-clock |
| `scripts/mcp-meas.mjs <server-cmd>` | octocode researcher MCP config | Transparent MCP proxy; logs init + every `tools/call` |
| `scripts/rtk-meas.sh <rtk args>` | rtk researcher | Wraps `rtk`; logs argv/stdout/stderr |
| `scripts/rtk-meas.mjs <rtk args>` | rtk researcher via wrapper | Spawns `rtk`; measures char I/O |
| `scripts/record.sh <n> <model> /tmp/answer.md` | researcher | Writes `q<n>.md` + `q<n>.json` |
| `scripts/finalize.mjs <run-dir>` | researcher | Writes `output.md` + `summary.json` |
| `scripts/aggregate.mjs` | internal | Sums log rows for one Q |
| `scripts/chars.mjs` | metering | Counts Unicode codepoints |

---

# Researcher instructions: `octocode`

Use this section only if your assigned role is `researcher: octocode`.

## Validity Requirements

- Read `benchmark/rtk/QUESTIONS.md`.
- Keep the run blind: leave the rtk researcher's output and `summary.md` unread during the run.
- Use **any Octocode MCP tool** when every call goes through `scripts/mcp-meas.mjs`.
- Keep research inside the metered Octocode path; bare Octocode tools, `rtk`, `rg`, `cat`, `find`, `gh`, web search, and local clone files are outside this run.
- Run questions sequentially.

## Setup

```bash
rm -rf benchmark/rtk/output/octocode
source benchmark/rtk/scripts/init-run.sh octocode
```

Configure MCP client:
```text
command: node
args: [benchmark/rtk/scripts/mcp-meas.mjs, <octocode-server-cmd>]
env: { RUN, LOG }
```

Verify init was logged before Q1:
```bash
grep '"cmd":"_initialize"' "$RUN/log.jsonl"
grep '"cmd":"_tools/list"' "$RUN/log.jsonl"
```

## Per-question loop

```bash
bash benchmark/rtk/scripts/set-q.sh <n>
# research with metered MCP tools
# write answer to /tmp/answer.md
bash benchmark/rtk/scripts/record.sh <n> "<model-id>" /tmp/answer.md
```

## Finalize

```bash
node benchmark/rtk/scripts/finalize.mjs benchmark/rtk/output/octocode
```

---

# Researcher instructions: `rtk`

Use this section only if your assigned role is `researcher: rtk`.

## Validity Requirements

- Read `benchmark/rtk/QUESTIONS.md`.
- Keep the run blind: leave the octocode researcher's output and `summary.md` unread during the run.
- Route every `rtk` command through `scripts/rtk-meas.sh`. Bare `rtk` is unmetered.
- Keep research inside the metered `rtk` wrapper; Octocode MCP tools, bare `rg`, bare `cat`, bare `gh`, and web search are outside this run.
- For local operations: use the clone at `/tmp/rtk-bench`.
- For GitHub operations: use `rtk gh` (through the wrapper).
- Run questions sequentially.

## Setup

```bash
git clone https://github.com/rtk-ai/rtk /tmp/rtk-bench
rm -rf benchmark/rtk/output/rtk
source benchmark/rtk/scripts/init-run.sh rtk
```

## How to call rtk

Every rtk call uses the wrapper:
```bash
bash benchmark/rtk/scripts/rtk-meas.sh <rtk-subcommand-and-args>
```

Examples:
```bash
bash benchmark/rtk/scripts/rtk-meas.sh rg 'fn run' /tmp/rtk-bench/src
bash benchmark/rtk/scripts/rtk-meas.sh read /tmp/rtk-bench/src/core/runner.rs
bash benchmark/rtk/scripts/rtk-meas.sh ls /tmp/rtk-bench/src
bash benchmark/rtk/scripts/rtk-meas.sh find /tmp/rtk-bench/src --name '*.rs'
bash benchmark/rtk/scripts/rtk-meas.sh gh pr view 2129 --repo rtk-ai/rtk
bash benchmark/rtk/scripts/rtk-meas.sh gh pr list --repo rtk-ai/rtk
```

The wrapper logs:
- `in_chars`: argv tail after `rtk`
- `out_chars`: stdout + stderr returned to the agent
- `elapsed_ms`, `exit`, current question number

Bare `rtk ...` is unmetered, so redo that question through the wrapper before recording it.

## Per-question loop

```bash
bash benchmark/rtk/scripts/set-q.sh <n>
# research with metered rtk commands
# write answer to /tmp/answer.md
bash benchmark/rtk/scripts/record.sh <n> "<model-id>" /tmp/answer.md
```

## Finalize

```bash
node benchmark/rtk/scripts/finalize.mjs benchmark/rtk/output/rtk
```

---

# Judge instructions

Use this section only if your assigned role is `judge`.

## Inputs

1. `benchmark/rtk/QUESTIONS.md`
2. `benchmark/rtk/output/octocode/output.md` + `summary.json`
3. `benchmark/rtk/output/rtk/output.md` + `summary.json`
4. Every `q<n>.md` and `q<n>.json` in both run directories

No expected-facts file. Independently verify against the live `rtk-ai/rtk` GitHub repo and local clone.

## Quality scoring (0–3 per question)

| Score | Meaning |
|---:|---|
| 3 | All load-bearing facts correct and complete — no missing sub-answers, no false claims |
| 2 | Mostly correct — one load-bearing sub-fact is missing or inaccurate |
| 1 | Partially correct — unsupported claim present, or a key fact missing |
| 0 | Wrong, empty, or `UNKNOWN` |

**Special scoring notes for this benchmark:**

- For questions testing comment preservation (Q3, Q4, Q5, Q19): if an answer misses information that only exists in source comments, score the answer according to how much of the requested fact pattern remains supported.
- For questions testing result limits (Q1, Q2, Q15): if an answer gives an incomplete count or is missing files, confirm via independent search whether the answer is exhaustive.
- For questions testing PR comments/labels (Q10, Q11, Q12, Q17): a missing label or a missing discussion point is scored as a missing load-bearing fact.

## Token-usage scoring

```text
effective_chars = in_chars + out_chars + amortized_mcp_init_chars
token_score     = quality / (effective_chars / 1000)
```

For octocode: `amortized_mcp_init_chars = (mcp_init.in_chars + mcp_init.out_chars) / N`
For rtk: `amortized_mcp_init_chars = 0`

A zero-quality answer scores zero regardless of char efficiency.

## Required output

Write one judge summary file: `benchmark/rtk/output/summary.md`

```markdown
# Benchmark summary — octocode vs rtk

## Per-question table

| Q | Category | Drift | Octo qual | rtk qual | Octo chars | rtk chars | Octo token score | rtk token score | Winner | Notes |

## Quality verdict (non-drift Qs only)

| Agent | Σ quality | Token-score wins | Token-score ties | Avg quality per Q |

## Quality-adjusted token-usage verdict

| Axis | octocode | rtk | ratio (octo/rtk) |
| Σ quality (non-drift) | | | |
| Σ calls | | | |
| Σ in_chars (per-Q) | | | |
| Σ out_chars (per-Q) | | | |
| MCP init chars | | 0 | |
| TOTAL chars (per-Q + init) | | | |
| Approx tokens (TOTAL chars / 4) | | | |
| Quality per 1k chars | | | |

## Capability Review

For each question where rtk scored lower than Octocode, cite the specific capability difference:
- Comment preservation
- Result completeness
- PR metadata coverage
- Remote content breadth
- Out-of-scope registry or LSP capability

## Verdict
```

---

## Common Run-Quality Issues

| Mistake | Fix |
|---|---|
| rtk researcher uses bare `rtk` without wrapper | Redo question through `rtk-meas.sh` |
| rtk researcher uses bare `rg`, `cat`, `gh` directly | Only `rtk` commands allowed |
| Octocode MCP not through `mcp-meas.mjs` | Reconfigure and rerun |
| Missing `_initialize` / `_tools/list` rows | MCP context cost not counted — rerun |
| Skipped `set-q.sh` | Tool calls attributed to a different Q |
| rtk researcher does not clone repo | Local commands have no target |

---

## Links

- Questions: [`benchmark/rtk/QUESTIONS.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/rtk/QUESTIONS.md)
- Prior art: [`benchmark/github/README.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/README.md) — same benchmark framework, `gh` CLI vs Octocode
