# GitHub Research Benchmark

This directory contains a benchmark for comparing GitHub research agents by **semantic answer quality per measured character**. Characters are the deterministic token-usage proxy: every metered call records `in_chars + out_chars`, and the Octocode run also pays the one-time MCP context cost from `initialize` and `tools/list` because tool schemas and server instructions enter the agent context. Elapsed time is recorded for context only; it does not decide the winner.

The benchmark has two phases:

1. **Research:** two blind researchers answer `benchmark/github/QUESTIONS.md` sequentially, one question at a time:

   - `octocode` researcher: uses only Octocode MCP tools, routed through `scripts/mcp-meas.mjs`.
   - `gh` researcher: uses only GitHub CLI, routed through `scripts/gh-meas.sh`.

2. **Judge:** a separate judge reads the completed `octocode` and `gh` outputs, independently fact-checks each answer against GitHub repositories/PRs, assigns semantic quality scores, combines those scores with measured character usage, and writes `benchmark/github/output/summary.md`.

Blind runs use judge evidence notes instead of a static answer-key file. Publication-quality runs should include those evidence notes, fixed repository refs for non-drift questions where possible, and the raw run artifacts so another reviewer can reproduce the score.

---

## Question Categories And Capability Dimensions

The 17 questions are grouped by the capability dimension each one probes. The goal is to compare how each toolset handles the same research task, then report any tradeoffs plainly.

| Category | Tag | Octocode tool | gh comparison surface | Questions |
|---|---|---|---|---|
| Code search completeness | `SEARCH` | `githubSearchCode` | Result limits and multi-query workflows | Q1–Q4 |
| File content completeness | `CONTENT` | `githubGetFileContent` | Large-file retrieval and targeted reads | Q5–Q8 |
| Repo tree navigation | `STRUCTURE` | `githubViewRepoStructure` | Tree shape, filtering, and metadata extraction | Q9–Q11 |
| PR intelligence | `PR` | `githubSearchPullRequests` | PR comments, reviews, commits, and changed files | Q12–Q15 |
| Repository search | `REPOS` | `githubSearchRepositories` | Search filters, counts, and pagination metadata | Q16–Q17 |

Questions tagged `[drift]` are time-sensitive. The judge scores them loosely and reports them in the **Drift verdict** section, separate from the main quality tally.

---

## If You Are An Agent: Choose Your Role First

Start by confirming which role you were assigned.

| Assigned role | What you do | Fact-checking mode | Output directory/file |
|---|---|---|---|
| `researcher: octocode` | Answer all questions using only metered Octocode MCP calls | Blind research only | `benchmark/github/output/octocode/` |
| `researcher: gh` | Answer all questions using only metered `gh` CLI calls | Blind research only | `benchmark/github/output/gh/` |
| `judge` | Compare completed `octocode` and `gh` runs semantically and by efficiency | Independent verification | `benchmark/github/output/summary.md` |

If your assigned role is unclear, ask whether you are `researcher: octocode`, `researcher: gh`, or `judge`.

---

## Dependencies

Metering is **character-only** and dependency-free. The scripts count Unicode codepoints directly; tokenizer libraries are outside this benchmark's ruler.

---

## Publication-Quality Run Standard

A run is considered publication-ready when it includes:

- Raw `octocode` and `gh` run directories with `log.jsonl`, every `q<n>.md`, every `q<n>.json`, `output.md`, and `summary.json`.
- A judge summary with evidence notes for every score below 3, plus clear treatment of drift questions.
- A completed `RUN_MANIFEST.template.md` copy with model IDs, tool versions, refs, and retrieval dates.
- Repository refs or retrieval dates for facts that can drift over time.
- At least three same-agent runs when stochastic agent behavior is being compared, with variance reported through `scripts/report-variance.mjs`.
- The exact model IDs, tool versions, authentication source, and benchmark commit SHA used for the run.

---

## Output layout

Fresh benchmark outputs go directly under `benchmark/github/output/`:

```text
benchmark/github/output/
├── octocode/
│   ├── log.jsonl
│   ├── q1.md
│   ├── q1.json
│   ├── ...
│   ├── output.md
│   └── summary.json
├── gh/
│   ├── log.jsonl
│   ├── q1.md
│   ├── q1.json
│   ├── ...
│   ├── output.md
│   └── summary.json
└── summary.md              # judge output
```

Start a fresh benchmark by removing the two run directories:

```bash
rm -rf benchmark/github/output/octocode benchmark/github/output/gh
```

---

## How metering works

Every tool call must go through the correct wrapper so the benchmark can log:

```json
{"q", "agent", "cmd", "in_chars", "out_chars", "elapsed_ms", "exit"}
```

### Character ruler

| Agent | Hook | `in_chars` | `out_chars` |
|---|---|---|---|
| `octocode` | `mcp-meas.mjs` proxies MCP stdio and pairs each JSON-RPC request/response by `id` | Unicode codepoints of `JSON.stringify(params.arguments)` for `tools/call` | Unicode codepoints of the exact text payload returned to the agent: `result.content[].text` joined in order |
| `gh` | `gh-meas.sh` delegates to `gh-meas.mjs`, which spawns `gh` and captures the subprocess output | Unicode codepoints of the argv tail, excluding literal `gh ` | Unicode codepoints of exact `stdout + stderr` decoded as UTF-8 |

JSON-RPC envelopes and the literal `gh` command word are excluded so both agents are measured on meaningful payload only. Character counts are produced in-process with JavaScript codepoint counting (`[...text].length`).

For Octocode MCP init/context rows, `mcp-meas.mjs` counts the full JSON-RPC `result` for `initialize` and `tools/list`, because those server instructions and tool schemas are loaded into the agent context.

### MCP init/context cost

The Octocode MCP client receives server instructions and tool schemas during MCP startup:

- `initialize`
- `tools/list`

`mcp-meas.mjs` logs these as `q=0` rows with:

- `cmd="_initialize"`
- `cmd="_tools/list"`

These rows represent the one-time context cost of loading the MCP system instructions and tool schemas into the agent. `finalize.mjs` exposes them as `mcp_init` in `output/octocode/summary.json`; the judge must include them in total Octocode chars. The `gh` run has no equivalent schema-loading cost.

---

## Script reference

| Script | Who uses it | Purpose | Why it is needed |
|---|---|---|---|
| `scripts/init-run.sh <agent>` | operator/researcher | Creates `output/<agent>/`, exports `$SESSION`, `$RUN`, `$LOG`, `$Q` | Establishes one clean, isolated run directory and derives the question count from `QUESTIONS.md` instead of hard-coding it. |
| `scripts/set-q.sh <n>` | researcher | Sets current question and starts Q wall-clock timer | Prevents cross-question metric leakage by giving metering wrappers a single current-Q sentinel. |
| `scripts/mcp-meas.mjs <server-cmd>` | octocode researcher MCP config | Transparent MCP proxy; logs MCP init and every `tools/call` char I/O | Ensures every Octocode tool call, plus MCP schema/context startup cost, is measured without changing the agent's research workflow. |
| `scripts/gh-meas.sh <gh args>` | gh researcher | Thin shell wrapper that delegates to `gh-meas.mjs` | Gives the `gh` researcher a simple command shape while keeping metering implementation in one Node script. |
| `scripts/gh-meas.mjs <gh args>` | gh researcher via wrapper | Spawns `gh`; logs argv/stdout/stderr char I/O | Applies the same character ruler to all GitHub CLI calls and captures exact output returned to the agent. |
| `scripts/octo-meas.sh <tool> <req-file> <res-file> [ms]` | fallback/operator | Manual Octocode metering fallback when MCP proxying is impossible; prefer `mcp-meas.mjs` | Keeps a last-resort metered path available, but marks it as less reliable because it depends on operator discipline. |
| `scripts/record.sh <n> <model> /tmp/answer.md` | researcher | Aggregates Q metrics and writes `q<n>.md` + `q<n>.json` | Couples the final answer with the measured rows for that exact question and fails loud if no metered calls were captured. |
| `scripts/finalize.mjs <run-dir>` | researcher/operator | Writes per-run `output.md` + `summary.json`; reports missing expected questions | Produces the machine-readable totals the judge needs and prevents incomplete runs from looking complete. |
| `scripts/chars.mjs` | metering scripts | Counts Unicode codepoints | Provides a dependency-free, tokenizer-independent ruler shared by every wrapper. |
| `scripts/aggregate.mjs` | internal/operator | Sums `log.jsonl` rows for one question; fails on zero rows | Provides the single source of truth for per-question calls/chars/time and catches bypassed metering. |
| `scripts/call-tool.mjs <tool> '<queries-json>'` | smoke test/operator | Sends one MCP tool call through `mcp-meas.mjs` without a full agent run | Verifies the MCP proxy path and logging before spending a full benchmark run. |
| `scripts/cross-run.mjs <run...>` | optional analysis | Reports medians across saved repeated runs | Summarizes repeated same-agent runs without pretending one stochastic run is definitive. |
| `scripts/report-variance.mjs [--csv] <run...>` | optional analysis | Reports variance/CV across saved repeated runs of the same agent | Quantifies run-to-run spread so benchmark claims can disclose instability instead of hiding it. |
| `scripts/validate-pipeline.mjs [--strict-cmds] <run...>` | optional analysis | Checks deterministic metering fields across same-agent runs | Regression-tests the metering pipeline itself, separate from normal agent stochasticity. |
| `scripts/score-token-usage.mjs <octocode-run> <gh-run> <quality.json>` | optional judge aid | Combines judge-supplied quality scores with measured chars; it does not score quality itself | Makes the arithmetic reproducible while keeping semantic quality judgment evidence-based and reviewable. |

---

# Researcher instructions: `octocode`

Use this section only if your assigned role is `researcher: octocode`.

## Validity Requirements

- Read `benchmark/github/QUESTIONS.md`.
- Keep the run blind: leave the other agent's output and `benchmark/github/output/summary.md` unread during the run.
- You may use **any Octocode MCP tool** needed to answer the questions when every call is routed through `scripts/mcp-meas.mjs`.
- Keep research inside the metered Octocode path; direct/unmetered Octocode tools, `gh`, web search, `curl`, `wget`, `git clone`, and local repository files are outside this run.
- Run questions sequentially: finish and record Q`n` before starting Q`n+1`.
- Leave `record.sh --allow-zero` unused for benchmark runs.

## Setup

From the repository root:

```bash
rm -rf benchmark/github/output/octocode
source benchmark/github/scripts/init-run.sh octocode
```

Configure your MCP client so Octocode is launched through the metering proxy, not directly:

```text
command: node
args: [benchmark/github/scripts/mcp-meas.mjs, <octocode-server-cmd>]
env: { RUN, LOG }
```

Example server command options:

```text
<octocode-server-cmd> = octocode-mcp
```

or a local built server path if the operator provides one.

Before Q1, verify the MCP handshake was logged:

```bash
grep '"cmd":"_initialize"' "$RUN/log.jsonl"
grep '"cmd":"_tools/list"' "$RUN/log.jsonl"
```

If either row is missing, pause the run and fix the MCP configuration before answering Q1. The MCP context/tool-schema cost was not captured.

## Per-question loop

For each question number `n` from 1 to `cat "$RUN/.q-count"`:

```bash
bash benchmark/github/scripts/set-q.sh <n>
```

Read exactly that question from `QUESTIONS.md`. Research using any Octocode MCP tool that helps, but only through the metered MCP path.

After your first Octocode tool call for that question, verify the call was attributed to the current question. Replace `<n>` with the current question number:

```bash
grep '"q":<n>' "$RUN/log.jsonl"
```

If no row exists for that Q after a tool call, pause and fix the metered path before recording an answer.

Write the answer to `/tmp/answer.md`:

- Start directly with bullets; no `## Answer` header.
- Use concise facts while preserving required sub-answers.
- Use one bullet per fact/sub-question/repository when helpful.
- Put file paths, repo slugs, function names, PR numbers, version strings, APIs, and important identifiers in backticks when practical.
- If you cannot answer after appropriate metered research, write `UNKNOWN — <one-line reason>`.
- Keep process notes and tool transcripts out of the recorded answer.

Record the answer:

```bash
bash benchmark/github/scripts/record.sh <n> "<model-id>" /tmp/answer.md
```

If `record.sh` reports zero rows, redo the question through the metered path before moving on.

## Finalize octocode run

After the last question:

```bash
node benchmark/github/scripts/finalize.mjs benchmark/github/output/octocode
```

This writes:

- `benchmark/github/output/octocode/output.md`
- `benchmark/github/output/octocode/summary.json`

---

# Researcher instructions: `gh`

Use this section only if your assigned role is `researcher: gh`.

## Validity Requirements

- Read `benchmark/github/QUESTIONS.md`.
- Keep the run blind: leave the other agent's output and `benchmark/github/output/summary.md` unread during the run.
- You may use **any `gh` CLI command** needed to answer the questions when every call is routed through `scripts/gh-meas.sh`.
- Keep research inside the metered `gh` wrapper; bare `gh`, Octocode tools, web search, `curl`, `wget`, `git clone`, and local repository files are outside this run.
- Run questions sequentially: finish and record Q`n` before starting Q`n+1`.
- Leave `record.sh --allow-zero` unused for benchmark runs.

## Setup

From the repository root:

```bash
rm -rf benchmark/github/output/gh
source benchmark/github/scripts/init-run.sh gh
```

## How to call GitHub CLI

Every GitHub CLI call must use the wrapper:

```bash
bash benchmark/github/scripts/gh-meas.sh <gh-subcommand-and-flags>
```

Examples:

```bash
bash benchmark/github/scripts/gh-meas.sh api repos/facebook/react/contents/packages
bash benchmark/github/scripts/gh-meas.sh search code 'renderToReadableStream repo:vercel/next.js' --json repository,path,textMatches
bash benchmark/github/scripts/gh-meas.sh pr view 27733 --repo facebook/react --json title,body,files,comments,reviews
```

The wrapper logs:

- input chars: argv tail after `gh`
- output chars: stdout + stderr
- elapsed time
- current question number from `$RUN/.current-q`

Bare `gh ...` is unmetered, so redo that question through the wrapper before recording it.

## Per-question loop

For each question number `n` from 1 to `cat "$RUN/.q-count"`:

```bash
bash benchmark/github/scripts/set-q.sh <n>
```

Read exactly that question from `QUESTIONS.md`. Research using any `gh` CLI command that helps, but only through `gh-meas.sh`.

Write the answer to `/tmp/answer.md`:

- Start directly with bullets; no `## Answer` header.
- Use concise facts while preserving required sub-answers.
- Use one bullet per fact/sub-question/repository when helpful.
- Put file paths, repo slugs, function names, PR numbers, version strings, APIs, and important identifiers in backticks when practical.
- If you cannot answer after appropriate metered research, write `UNKNOWN — <one-line reason>`.
- Keep process notes and command transcripts out of the recorded answer.

Record the answer:

```bash
bash benchmark/github/scripts/record.sh <n> "<model-id>" /tmp/answer.md
```

If `record.sh` reports zero rows, redo the question through the metered path before moving on.

## Finalize gh run

After the last question:

```bash
node benchmark/github/scripts/finalize.mjs benchmark/github/output/gh
```

This writes:

- `benchmark/github/output/gh/output.md`
- `benchmark/github/output/gh/summary.json`

---

# Judge instructions

Use this section only if your assigned role is `judge`.

## Inputs

Read:

1. `benchmark/github/QUESTIONS.md`
2. `benchmark/github/output/octocode/output.md`
3. `benchmark/github/output/octocode/summary.json`
4. `benchmark/github/output/gh/output.md`
5. `benchmark/github/output/gh/summary.json`
6. Every `q<n>.md` and `q<n>.json` in both run directories.

Blind runs use independent judge verification instead of an expected-facts file. The judge verifies the relevant facts for each question before assigning scores. Judge research/tool calls are outside the measured researcher runs and are excluded from token-usage totals.

## Quality scoring

Score each answer semantically from 0 to 3:

| Score | Meaning |
|---:|---|
| 3 | All load-bearing facts present, no false claims, all requested repos/trace steps/PR sub-questions answered |
| 2 | Mostly correct, but one load-bearing sub-fact is missing or inaccurate |
| 1 | Partially correct, or an unsupported claim is present |
| 0 | Wrong, empty, or `UNKNOWN` |

Rules:

- Avoid checklist-only scoring.
- Score against the exact question wording and facts you independently verify from GitHub repositories, files, PRs, comments, reviews, and release/source history as needed.
- Accept equivalent identifiers, moved/renamed files, paraphrases, and extra correct context.
- Penalize missing required facts, unsupported claims, or contradictions.
- For multi-part questions, score parts separately and average.
- For every score below 3, cite a specific missing or inaccurate file path, identifier, PR discussion point, or agent claim.

## Token-usage scoring

Use char fields only. Characters are the canonical tokenizer-independent proxy for token usage; optional `approx_tokens` is `chars / 4` for display only.

```text
effective_chars = in_chars + out_chars + amortized_mcp_init_chars
token_score     = quality / (effective_chars / 1000)
```

For Octocode:

```text
amortized_mcp_init_chars = (mcp_init.in_chars + mcp_init.out_chars) / N
```

`N` is the number of comparable, non-drift, non-excluded questions scored for both agents. Amortize MCP init only over questions included in the token-usage verdict.

For `gh`:

```text
amortized_mcp_init_chars = 0
```

A zero-quality answer has zero token score even if it is character-cheap. If the token-usage winner has materially lower raw quality, state that tradeoff explicitly. Wall-clock time is context only and must not decide the winner.

## Required judge output

Write one judge summary file:

```text
benchmark/github/output/summary.md
```

Use these sections:

```markdown
# Benchmark summary — octocode vs gh

## Per-question table

| Q | Category | Drift | Octo qual | gh qual | Octo calls | gh calls | Octo chars | gh chars | Octo token score | gh token score | Winner | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|

## Quality verdict (non-drift Qs only)

| Agent | Σ quality | Token-score wins | Token-score ties | Avg quality per Q |
|---|---|---|---|---|

## Drift verdict (reported separately)

| Q | Category | Octo qual | gh qual | Notes |
|---|---|---|---|---|

## Quality-adjusted token-usage verdict

| Axis | octocode | gh | ratio (octo/gh) |
|---|---|---|---|
| Σ quality (non-drift) | | | |
| Σ calls | | | |
| Σ in_chars (per-Q) | | | |
| Σ out_chars (per-Q) | | | |
| MCP init chars | | 0 | |
| TOTAL chars (per-Q + init) | | | |
| Approx tokens (TOTAL chars / 4) | | | |
| Quality per 1k chars = Σ quality / (TOTAL chars/1000) | | | |
| Σ tool_elapsed_ms (context only) | | | |
| Σ q_elapsed_ms (context only) | | | |
| Σ reasoning_ms (context only) | | | |

## Capability Review

For each question where `gh` scored lower than Octocode, cite the specific capability difference:
- **SEARCH result limit** — code search result ceilings affected completeness
- **SEARCH bulk workflow** — `gh` needed separate calls where Octocode used bulk queries
- **CONTENT large file path** — large files required extra retrieval steps
- **CONTENT pagination** — one tool returned a broad blob while the other used a targeted window
- **CONTENT directory** — directory metadata shape affected filtering/grouping work
- **STRUCTURE tree shape** — tree output shape affected parsing and counts
- **PR inline comments** — inline review threads required additional retrieval
- **PR diff completeness** — changed-file or diff details required additional retrieval
- **REPOS pagination** — repository enumeration depended on result limits and pagination

## Verdict
```

Write the judge summary only.

---

## Scoring model summary

Evaluation is semantic and evidence-based. The judge independently fact-checks the answers instead of relying on keyword matching. The benchmark winner is based on quality-adjusted token/character usage: answer quality per measured character, with MCP init/context chars charged to Octocode. Elapsed time is reported as context only.

The 17 questions are grouped into five categories (`SEARCH`, `CONTENT`, `STRUCTURE`, `PR`, `REPOS`), each probing a distinct research capability. The category tag appears in the judge's per-question table so capability patterns are visible across the full run.

Drift questions (heading suffix `[drift]` in `QUESTIONS.md`, or questions the judge flags as date-sensitive) are scored loosely and reported in the **Drift verdict** section, separate from the main quality tally.

---

## Common Run-Quality Issues

| Issue | Impact | Fix |
|---|---|---|
| Researcher reads the other agent's output or final judge summary before finishing | The run is no longer blind | Discard and rerun |
| Octocode MCP not routed through `mcp-meas.mjs` | Tool calls and MCP context are unmetered | Reconfigure MCP client and rerun |
| Missing `_initialize` / `_tools/list` rows | MCP system prompt/tool schema context was not counted | Reconfigure MCP client and rerun |
| Bare `gh` instead of `gh-meas.sh` | CLI call is unmetered | Redo the question through wrapper |
| Skipped `set-q.sh` | Tool calls are attributed to a different Q or Q0 | Redo the question correctly |
| `record.sh --allow-zero` | Broken metering is hidden | Keep it disabled for benchmark runs |
| Parallel questions | Metrics can leak across questions | Run sequentially |

---

## Links

- Questions: [`benchmark/github/QUESTIONS.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/QUESTIONS.md)
- Researcher prompt: [`benchmark/github/prompts/researcher.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/prompts/researcher.md)
- Judge prompt: [`benchmark/github/prompts/judge.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/prompts/judge.md)
