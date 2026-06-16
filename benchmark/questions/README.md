# Next.js Benchmark

This directory benchmarks two approaches to LLM-assisted code research on `vercel/next.js`: **Octocode CLI** vs **RTK + `gh` CLI**. The metric is **semantic answer quality per total chars spent on the task**: every metered call records `in_chars + out_chars`, and the denominator is the sum across *all* calls made to answer a question — not just the first. A tool that makes three follow-up calls pays for all three.

**20 questions in one file.** Q1–Q10 can be answered from remote GitHub repositories. Q11–Q20 require a local clone.

---

## What is being compared

| Dimension | rtk-gh researcher | octocode researcher |
|---|---|---|
| **How it works** | Runs `rtk` for local/filtered access and `gh` for raw GitHub API | Calls GitHub API and local filesystem via structured Octocode CLI tools |
| **Code search** | `rtk rg <pattern> <path>` — compressed output + long-line limits | `localSearchCode` — full results, explicit pagination |
| **File content** | `rtk read <file>` — language-aware comment stripping | `localGetFileContent` — full fidelity, char-offset pagination |
| **Directory listing** | `rtk ls` / `rtk tree` | `localViewStructure` — full tree, structured metadata |
| **File finding** | `rtk find` — no size/mtime metadata | `localFindFiles` — size, mtime, extension filters |
| **GitHub API** | `gh api` / `gh search code` — raw JSON | `ghGetFileContent`, `ghSearchCode` — structured with pagination |
| **PR research** | `gh pr view` — raw JSON | `ghSearchPRs` — all metadata, comments, diff access |
| **LSP navigation** | Out of scope | `lspGetSemantics` — definition, references, call hierarchy |

---

## If you are an agent: choose your role first

| Assigned role | What you do | Output directory |
|---|---|---|
| `researcher: octocode` | Answer all 20 questions using only metered `octocode` calls | `benchmark/output/octocode/` |
| `researcher: rtk-gh` | Answer all 20 questions using only metered `rtk` + `gh` calls | `benchmark/output/rtk-gh/` |
| `judge` | Compare completed runs semantically and by efficiency | `benchmark/output/summary.md` |

If your assigned role is unclear, ask before starting.

---

## Dependencies

- **rtk-gh researcher**: `rtk` ≥ 0.28, `gh` CLI authenticated, `node` ≥ 18, `git`
- **octocode researcher**: Octocode CLI (see path below), `node` ≥ 18, `git`
- Both: clone at `/tmp/nextjs-bench` before starting Q11–Q20

### Octocode CLI path

Set `OCTOCODE_CLI_BIN` once to use the local development build:

```bash
export OCTOCODE_CLI_BIN="/Users/guybary/Documents/octocode-mcp/packages/octocode-cli/out/octocode-cli.js"
```

The metering wrapper reads this env var and runs `node $OCTOCODE_CLI_BIN tools ...` automatically. Unset = falls back to the global `octocode` binary.

---

## Output layout

```text
benchmark/output/
├── octocode/
│   ├── log.jsonl
│   ├── q1.md · q1.json
│   ├── ...
│   ├── q20.md · q20.json
│   ├── output.md
│   └── summary.json
├── rtk-gh/
│   ├── log.jsonl
│   ├── q1.md · q1.json
│   ├── ...
│   ├── q20.md · q20.json
│   ├── output.md
│   └── summary.json
└── summary.md              # judge output
```

Start fresh:

```bash
rm -rf benchmark/output/octocode benchmark/output/rtk-gh
```

---

## How metering works

Every tool call goes through a wrapper that logs:

```json
{"ts": "...", "q": 3, "agent": "octocode", "cmd": "localSearchCode", "in_chars": 210, "out_chars": 1840, "elapsed_ms": 88, "exit": 0}
```

Both researchers use the same ruler — no init overhead for either side.

| Agent | Hook | `in_chars` | `out_chars` |
|---|---|---|---|
| `octocode` | `octo-meas.sh` → `octo-meas.mjs` | Unicode codepoints of the `--queries` JSON string | Unicode codepoints of stdout |
| `rtk` | `rtk-meas.sh` → `rtk-meas.mjs` | Unicode codepoints of argv tail (no `rtk ` prefix) | Unicode codepoints of stdout + stderr |
| `gh` | `gh-meas.sh` → `gh-meas.mjs` | Unicode codepoints of argv tail (no `gh ` prefix) | Unicode codepoints of stdout + stderr |

The rtk-gh researcher routes all calls through `rtk-meas.sh` or `gh-meas.sh` — both append to the same `log.jsonl`.

---

## Script reference

| Script | Who uses it | Purpose |
|---|---|---|
| `benchmark/scripts/init-run.sh <agent>` | operator | Creates `benchmark/output/<agent>/`, exports `$RUN`, `$LOG`, `$QUESTIONS_FILE` |
| `benchmark/scripts/set-q.sh <n>` | researcher | Sets question sentinel, starts Q wall-clock |
| `benchmark/scripts/octo-meas.sh <tool> '<queries-json>'` | octocode researcher | Wraps `octocode tools`; logs char I/O |
| `benchmark/scripts/rtk-meas.sh <rtk args>` | rtk-gh researcher | Wraps `rtk`; logs char I/O |
| `benchmark/scripts/gh-meas.sh <gh args>` | rtk-gh researcher | Wraps bare `gh`; logs char I/O |
| `benchmark/scripts/record.sh <n> <model> /tmp/answer.md` | researcher | Aggregates Q metrics, writes `q<n>.md` + `q<n>.json` |
| `benchmark/scripts/finalize.mjs <run-dir>` | researcher | Writes `output.md` + `summary.json` |
| `benchmark/scripts/aggregate.mjs` | internal | Sums log rows for one Q |
| `benchmark/scripts/chars.mjs` | metering | Counts Unicode codepoints |

---

# Researcher instructions: `octocode`

Use this section only if your assigned role is `researcher: octocode`.

## Validity requirements

- Read [`nextjs.md`](./nextjs.md) before starting.
- Keep the run blind: leave `benchmark/output/rtk-gh/` and `benchmark/output/summary.md` unread during the run.
- Route every tool call through `benchmark/scripts/octo-meas.sh`. Bare `octocode tools` is unmetered.
- For Q11–Q20: set `ALLOWED_PATHS` to include the clone so local tools can access `/tmp/nextjs-bench`.
- Run questions sequentially — finish and record Q`n` before starting Q`n+1`.

## Setup

```bash
export OCTOCODE_CLI_BIN="/Users/guybary/Documents/octocode-mcp/packages/octocode-cli/out/octocode-cli.js"
export ALLOWED_PATHS="/tmp/nextjs-bench"
git clone --depth 1 https://github.com/vercel/next.js /tmp/nextjs-bench
rm -rf benchmark/output/octocode
source benchmark/scripts/init-run.sh octocode
```

## Available tools

Use any of the 12 Octocode tools. Every call must go through the wrapper.

| Tool | When to use |
|---|---|
| `ghSearchCode` | Search code across GitHub by keyword |
| `ghGetFileContent` | Fetch a specific file or path from GitHub |
| `ghViewRepoStructure` | Browse the repository tree |
| `ghSearchRepos` | Search for repositories |
| `ghSearchPRs` | Search PRs, read body, comments, diffs |
| `ghCloneRepo` | Clone a repo subtree for local + LSP use |
| `npmSearch` | npm package version, downloads, repo URL |
| `localSearchCode` | ripgrep search on the local clone |
| `localGetFileContent` | Read a local file with optional pagination |
| `localViewStructure` | Browse local directory tree |
| `localFindFiles` | Find files by name, extension, size, mtime |
| `lspGetSemantics` | Semantic navigation: definition, references, callHierarchy, documentSymbols, hover, typeDefinition, implementation |

## How to call Octocode tools

Every tool call uses the wrapper:

```bash
bash benchmark/scripts/octo-meas.sh <tool-name> '<queries-json>'
```

**Required fields on every query:** `mainResearchGoal`, `researchGoal`, `reasoning`. Missing any of these will fail schema validation.

Examples:

```bash
# Search code on GitHub (Q1–Q10)
bash benchmark/scripts/octo-meas.sh ghSearchCode \
  '{"keywordsToSearch":["notFound"],"owner":"vercel","repo":"next.js","mainResearchGoal":"trace notFound propagation","researchGoal":"find notFound definition","reasoning":"need exact declaration file and line"}'

# Get a file from GitHub (Q1–Q10)
bash benchmark/scripts/octo-meas.sh ghGetFileContent \
  '{"owner":"vercel","repo":"next.js","path":"packages/next/src/server/app-render/app-render.tsx","mainResearchGoal":"read renderToHTMLOrFlight signature","researchGoal":"read app-render.tsx","reasoning":"need return type and parameters"}'

# Browse repo tree (Q1–Q10)
bash benchmark/scripts/octo-meas.sh ghViewRepoStructure \
  '{"owner":"vercel","repo":"next.js","path":"packages/next/src/server","mainResearchGoal":"list server subdirectories","researchGoal":"browse server dir","reasoning":"need subdirectory names"}'

# Search local clone (Q11–Q20)
bash benchmark/scripts/octo-meas.sh localSearchCode \
  '{"path":"/tmp/nextjs-bench/packages/next/src/server","pattern":"TODO|FIXME|HACK","mainResearchGoal":"find all annotation comments","researchGoal":"search for TODO FIXME HACK","reasoning":"need exhaustive list with file and line"}'

# LSP call hierarchy (Q11–Q20)
bash benchmark/scripts/octo-meas.sh lspGetSemantics \
  '{"type":"callHierarchy","uri":"/tmp/nextjs-bench/packages/next/src/server/app-render/app-render.tsx","symbolName":"renderToHTMLOrFlight","lineHint":42,"mainResearchGoal":"find all callers of renderToHTMLOrFlight","researchGoal":"incoming call hierarchy","reasoning":"need direct callers with file and line"}'
```

## Per-question loop

Run `cat "$RUN/.q-count"` to see the total number of questions (20). For each `n` from 1 to 20:

```bash
# 1. Advance the question sentinel
bash benchmark/scripts/set-q.sh <n>

# 2. Research using metered octocode calls (repeat as needed)
bash benchmark/scripts/octo-meas.sh <tool> '<queries-json>'

# 3. Write your answer to a file
cat > /tmp/answer.md << 'EOF'
- <fact 1>
- <fact 2>
EOF

# 4. Record the answer and metrics
bash benchmark/scripts/record.sh <n> "<model-id>" /tmp/answer.md
```

Answer format:
- Start directly with bullets — no `## Answer` header.
- State exact file paths, line numbers, PR numbers, verbatim quotes.
- If you cannot answer after appropriate research: `UNKNOWN — <one-line reason>`.
- Keep command transcripts and reasoning out of the recorded answer.

## Finalize

```bash
node benchmark/scripts/finalize.mjs "$RUN"
```

---

# Researcher instructions: `rtk-gh`

Use this section only if your assigned role is `researcher: rtk-gh`.

## Validity requirements

- Read [`nextjs.md`](./nextjs.md) before starting.
- Keep the run blind: leave `benchmark/output/octocode/` and `benchmark/output/summary.md` unread during the run.
- Route every `rtk` call through `benchmark/scripts/rtk-meas.sh` and every `gh` call through `benchmark/scripts/gh-meas.sh`. Bare `rtk` or bare `gh` is unmetered.
- For Q11–Q20: run `rtk` commands against the clone at `/tmp/nextjs-bench`.
- Run questions sequentially.

## Setup

```bash
git clone --depth 1 https://github.com/vercel/next.js /tmp/nextjs-bench
rm -rf benchmark/output/rtk-gh
source benchmark/scripts/init-run.sh rtk-gh
```

## How to call rtk

```bash
bash benchmark/scripts/rtk-meas.sh <rtk-subcommand-and-args>
```

Examples:

```bash
bash benchmark/scripts/rtk-meas.sh rg 'notFound' /tmp/nextjs-bench/packages/next/src
bash benchmark/scripts/rtk-meas.sh read /tmp/nextjs-bench/packages/next/src/server/base-server.ts
bash benchmark/scripts/rtk-meas.sh ls /tmp/nextjs-bench/packages/next/src/server
bash benchmark/scripts/rtk-meas.sh find /tmp/nextjs-bench/packages/next/src --name '*.ts'
```

## How to call gh

```bash
bash benchmark/scripts/gh-meas.sh <gh-subcommand-and-flags>
```

Examples:

```bash
bash benchmark/scripts/gh-meas.sh api repos/vercel/next.js/contents/packages/next/src/server
bash benchmark/scripts/gh-meas.sh search code 'notFound repo:vercel/next.js' --json repository,path,textMatches
bash benchmark/scripts/gh-meas.sh pr list --repo vercel/next.js --search 'partial prerendering' --state merged --json number,title
bash benchmark/scripts/gh-meas.sh pr view 12345 --repo vercel/next.js --json title,body,files,comments,reviews
```

## Per-question loop

```bash
# 1. Advance the question sentinel
bash benchmark/scripts/set-q.sh <n>

# 2. Research using metered rtk / gh commands (repeat as needed)
bash benchmark/scripts/rtk-meas.sh rg '<pattern>' /tmp/nextjs-bench/...
bash benchmark/scripts/gh-meas.sh search code '...'

# 3. Write your answer to a file
cat > /tmp/answer.md << 'EOF'
- <fact 1>
- <fact 2>
EOF

# 4. Record the answer and metrics
bash benchmark/scripts/record.sh <n> "<model-id>" /tmp/answer.md
```

If `record.sh` reports zero rows, redo the question through the metered wrappers before moving on.

## Finalize

```bash
node benchmark/scripts/finalize.mjs "$RUN"
```

---

# Judge instructions

Use this section only if your assigned role is `judge`.

Wait for both researcher runs to be finalized before scoring.

```
AGENTS:    octocode, rtk-gh
RUNS:      benchmark/output/octocode
           benchmark/output/rtk-gh
QUESTIONS: benchmark/questions/nextjs.md
OUTPUT:    benchmark/output/summary.md
```

## Scoring rubric

For each question, assign each agent a **depth score D ∈ {0, 1, 2, 3}**:

| D | Meaning |
|---|---|
| 3 | Complete and correct: all sub-answers present, verbatim quotes match source, file:line citations verified |
| 2 | Mostly correct: one sub-answer missing or slightly off, core finding is right |
| 1 | Partial: found the general area but missing key evidence (e.g. stopped at re-export, missed inline comments, fabricated a quote) |
| 0 | Wrong or `UNKNOWN` with no meaningful finding |

For each question also record the **efficiency ratio**: `total_chars_agent_A / total_chars_agent_B` (from `q<n>.json`). Lower chars for the same D score is better.

## How to score

For each Q, read:
- `benchmark/output/octocode/q<n>.md` — the octocode answer
- `benchmark/output/rtk-gh/q<n>.md` — the rtk-gh answer
- The question text in `nextjs.md`

Verify factual claims (file paths, line numbers, PR numbers, verbatim quotes) against the source repo directly — do not accept them at face value.

Write one row per question to `benchmark/output/summary.md`:

```markdown
| Q | octocode D | rtk-gh D | octocode chars | rtk-gh chars | Notes |
|---|---|---|---|---|---|
| Q1 | 3 | 2 | 4,210 | 9,840 | rtk-gh missed catch site |
...
```

Then add a totals row and a brief qualitative verdict.

**Special scoring notes:**

- Code archaeology (Q1, Q5, Q7, Q8, Q11, Q13): full D=3 only when all three sub-answers include exact verbatim quotes with correct file:line citations.
- Benchmark/eval discovery (Q9, Q10, Q12, Q14, Q16): full D=3 requires the named benchmark/eval artifact, its file path, and the concrete source lines or README lines proving how it is structured or run.
- Symbol definitions (Q2, Q17, Q18): deduct for import sites returned instead of declaration sites.
- Search completeness (Q3, Q4, Q12, Q14, Q19): verify total count independently from a fresh clone or the cited GitHub repository.
- Call hierarchy (Q20): verify every direct caller — agents that miss one or include indirect callers score D≤1.

---

## Common run-quality issues

| Mistake | Fix |
|---|---|
| Bare `octocode tools` without `octo-meas.sh` | Redo question through wrapper |
| Bare `rtk` without `rtk-meas.sh` | Redo question through wrapper |
| Bare `gh` without `gh-meas.sh` | Redo question through wrapper |
| Skipped `set-q.sh` | Tool calls attributed to wrong Q |
| `record.sh --allow-zero` used | Broken metering is hidden |
| Q11–Q20 without cloning | Local tools have no target |
| Reading other agent's output during run | Run is no longer blind — discard and rerun |
| Parallel questions | Metrics can leak across questions |

---

## Links

- Questions: [`benchmark/questions/nextjs.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/questions/nextjs.md)
- Benchmark framework: [`benchmark/README.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/README.md)
