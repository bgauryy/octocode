# RTK vs Octocode Benchmark

This directory benchmarks two approaches to LLM-assisted code research on the same codebase: **rtk CLI filtering** vs **Octocode CLI tools**. The metric is **semantic answer quality per total chars spent on the task** (Factory.ai framing): every metered call records `in_chars + out_chars`, and the denominator is the sum across *all* calls made to answer a question — not just the first call. A tool that needs three follow-up calls because the first call returned incomplete data pays for all four. Elapsed time is recorded for context only.

Both agents use **CLI tools only** — no MCP server, no schema-loading overhead:

- **`octocode` researcher**: calls Octocode tools via `octocode tools <name> --queries '<json>'`, routed through `scripts/octo-meas.sh`.
- **`rtk` researcher**: runs `rtk` CLI commands, routed through `scripts/rtk-meas.sh`.

---

## What is being compared

| Dimension | rtk researcher | octocode researcher |
|---|---|---|
| **How it works** | Runs native CLI tools (`rg`, `ls`, `find`, `cat`, `gh`) through rtk's output filter, which compresses text before the LLM sees it | Calls GitHub API and local filesystem directly via structured Octocode CLI tools |
| **Code search** | `rtk rg <pattern> <path>` — has configured result limits and long-line compression | `localSearchCode` — full results, explicit pagination, no line truncation |
| **File content** | `rtk read <file>` — language-aware filter **strips comments by default** (`Minimal` level) | `localGetFileContent` — full fidelity, char-offset pagination, `matchString` anchor |
| **Directory listing** | `rtk ls <path>` / `rtk tree <path>` — applies its configured directory filters | `localViewStructure` — full tree, structured metadata, configurable depth |
| **File finding** | `rtk find <path>` — same hidden dirs, no size/mtime metadata | `localFindFiles` — size, mtime, extension filters, structured output |
| **GitHub PR research** | `rtk gh pr view <n>` — drops labels, comments, assignees, file change list | `githubSearchPullRequests` — all metadata, comments option, diff access |
| **GitHub file content** | `rtk gh api repos/.../contents/path` — 2000-char passthrough window | `githubGetFileContent` — full content with char-offset pagination |
| **Package lookup** | Out of scope for rtk's CLI filtering model | `packageSearch` — npm registry API: version, downloads, homepage |
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
- The **octocode researcher** answers via Octocode CLI tools — both local (after cloning) and remote GitHub tools.

Cloning for the rtk researcher:
```bash
git clone https://github.com/rtk-ai/rtk /tmp/rtk-bench
```

---

## If you are an agent: choose your role first

Start by confirming which role you were assigned.

| Assigned role | What you do | Output directory |
|---|---|---|
| `researcher: octocode` | Answer all questions using only metered `octocode tools` calls | `benchmark/rtk/output/octocode/` |
| `researcher: rtk` | Answer all questions using only metered `rtk` CLI commands | `benchmark/rtk/output/rtk/` |
| `judge` | Compare completed runs semantically and by efficiency | `benchmark/rtk/output/summary.md` |

If your assigned role is unclear, ask before starting.

---

## Dependencies

- **rtk researcher**: `rtk` ≥ 0.28 installed, `git`, `node` (for metering script), repo cloned at `/tmp/rtk-bench`
- **octocode researcher**: `octocode` CLI installed (`brew install bgauryy/octocode/octocode` or `npm install -g octocode-cli`), `node` (for `octo-meas.mjs`)
- Metering is character-only. Tokenizer libraries are outside this benchmark's ruler.

---

See [benchmark/README.md](../README.md) for the publication-quality run standard.

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

Both agents use the same ruler — no init overhead for either side.

| Agent | Hook | `in_chars` | `out_chars` |
|---|---|---|---|
| `octocode` | `scripts/octo-meas.sh` delegates to `octo-meas.mjs`, which spawns `octocode tools` and captures stdout | Unicode codepoints of the queries JSON string passed to `--queries` | Unicode codepoints of exact stdout decoded as UTF-8 |
| `rtk` | `scripts/rtk-meas.sh` delegates to `rtk-meas.mjs`, which spawns `rtk` and captures the subprocess output | Unicode codepoints of the full rtk command argv (no `rtk ` prefix) | Unicode codepoints of exact stdout + stderr |

---

## Script reference

| Script | Who uses it | Purpose |
|---|---|---|
| `scripts/init-run.sh <agent>` | operator | Creates `output/<agent>/`, exports `$SESSION`, `$RUN`, `$LOG` |
| `scripts/set-q.sh <n>` | researcher | Sets current question sentinel, starts Q wall-clock |
| `scripts/octo-meas.sh <tool> '<queries-json>'` | octocode researcher | Thin wrapper → `octo-meas.mjs`; logs queries/stdout char I/O |
| `scripts/octo-meas.mjs <tool> '<queries-json>'` | octocode researcher via wrapper | Spawns `octocode tools`; measures char I/O |
| `scripts/rtk-meas.sh <rtk args>` | rtk researcher | Wraps `rtk`; logs argv/stdout/stderr |
| `scripts/rtk-meas.mjs <rtk args>` | rtk researcher via wrapper | Spawns `rtk`; measures char I/O |
| `scripts/record.sh <n> <model> /tmp/answer.md` | researcher | Writes `q<n>.md` + `q<n>.json` |
| `scripts/finalize.mjs <run-dir>` | researcher | Writes `output.md` + `summary.json` |
| `scripts/aggregate.mjs` | internal | Sums log rows for one Q |
| `scripts/chars.mjs` | metering | Counts Unicode codepoints |

---

# Researcher instructions: `octocode`

Use this section only if your assigned role is `researcher: octocode`.

Follow [`benchmark/OCTOCODE_RESEARCHER.md`](../OCTOCODE_RESEARCHER.md) with `<BENCHMARK>=rtk`.

Setup:

```bash
git clone https://github.com/rtk-ai/rtk /tmp/rtk-bench
rm -rf benchmark/rtk/output/octocode
source benchmark/rtk/scripts/init-run.sh octocode
```

Finalize:

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
- Keep research inside the metered `rtk` wrapper; Octocode tools, bare `rg`, bare `cat`, bare `gh`, and web search are outside this run.
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

Use [`benchmark/judge/prompt.md`](../judge/prompt.md) with:

```
AGENTS:    octocode, rtk
RUNS:      benchmark/rtk/output/octocode, benchmark/rtk/output/rtk
QUESTIONS: benchmark/rtk/QUESTIONS.md
OUTPUT:    benchmark/rtk/output/summary.md
```

**Special scoring notes for this benchmark:**

- Comment preservation (Q10, Q11, Q12): if an answer misses information that only exists in source comments or doc comments, score according to how much of the requested fact pattern remains supported.
- Comment-as-target search (Q3, Q4): the comment text IS the search match — credit full marks if all matching lines are found.
- Result limits (Q1, Q2, Q16): confirm via independent search whether a count is exhaustive.
- PR metadata (Q18, Q19, Q20): a missing label, body section, or motivation statement is a missing load-bearing fact.

---

## Common Run-Quality Issues

| Mistake | Fix |
|---|---|
| rtk researcher uses bare `rtk` without wrapper | Redo question through `rtk-meas.sh` |
| rtk researcher uses bare `rg`, `cat`, `gh` directly | Only `rtk` commands allowed |
| Octocode researcher uses bare `octocode tools` without wrapper | Redo question through `octo-meas.sh` |
| Skipped `set-q.sh` | Tool calls attributed to a different Q |
| `record.sh --allow-zero` used | Broken metering is hidden |
| rtk researcher does not clone repo | Local commands have no target |

---

## Links

- Questions: [`benchmark/rtk/QUESTIONS.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/rtk/QUESTIONS.md)
- Prior art: [`benchmark/github/README.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/README.md) — same benchmark framework, `gh` CLI vs Octocode
