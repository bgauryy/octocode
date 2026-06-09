# GitHub Research Benchmark

This directory contains a benchmark for comparing GitHub research agents by **semantic answer quality per total chars spent on the task** (Factory.ai framing): every metered call records `in_chars + out_chars`, and the denominator is the sum across *all* calls made to answer a question — not just the first call. A tool that retrieves incomplete information and needs three additional calls pays for all four. Elapsed time is recorded for context only; it does not decide the winner.

Both agents use **CLI tools only** — no MCP server, no schema-loading overhead:

- **`octocode` researcher**: calls Octocode tools via `octocode tools <name> --queries '<json>'`, routed through `scripts/octo-meas.sh`.
- **`gh` researcher**: calls GitHub CLI via `gh`, routed through `scripts/gh-meas.sh`.

The benchmark has two phases:

1. **Research:** two blind researchers answer `benchmark/github/QUESTIONS.md` sequentially, one question at a time.
2. **Judge:** a separate judge reads the completed `octocode` and `gh` outputs, independently fact-checks each answer against GitHub repositories/PRs, assigns semantic quality scores, combines those scores with measured character usage, and writes `benchmark/github/output/summary.md`.

Blind runs use judge evidence notes instead of a static answer-key file. Publication-quality runs should include those evidence notes, fixed repository refs for non-drift questions where possible, and the raw run artifacts so another reviewer can reproduce the score.

---

## Question Categories And Capability Dimensions

The 20 questions are grouped by the capability dimension each one probes. The goal is to compare how each toolset handles the same research task, then report any tradeoffs plainly.

| Category | Tag | Octocode tool | gh comparison surface | Questions |
|---|---|---|---|---|
| Code search completeness | `SEARCH` | `githubSearchCode` | Result limits and multi-query workflows | Q1–Q4 |
| File content completeness | `CONTENT` | `githubGetFileContent` | Large-file retrieval and targeted reads | Q5–Q8 |
| Repo tree navigation | `STRUCTURE` | `githubViewRepoStructure` | Tree shape, filtering, and metadata extraction | Q9–Q11 |
| PR intelligence | `PR` | `githubSearchPullRequests` | PR comments, reviews, commits, body text, and changed files | Q12–Q15, Q20 |
| Repository search | `REPOS` | `githubSearchRepositories` | Search filters, counts, pagination, and org-scoped queries | Q16–Q17, Q19 |
| Package registry | `PACKAGE` | `packageSearch` | No gh CLI equivalent — documents exclusive capability | Q18 |

Questions tagged `[drift]` are time-sensitive. The judge scores them loosely and reports them in the **Drift verdict** section, separate from the main quality tally.

---

## If You Are An Agent: Choose Your Role First

Start by confirming which role you were assigned.

| Assigned role | What you do | Fact-checking mode | Output directory/file |
|---|---|---|---|
| `researcher: octocode` | Answer all questions using only metered `octocode tools` calls | Blind research only | `benchmark/github/output/octocode/` |
| `researcher: gh` | Answer all questions using only metered `gh` CLI calls | Blind research only | `benchmark/github/output/gh/` |
| `judge` | Compare completed `octocode` and `gh` runs semantically and by efficiency | Independent verification | `benchmark/github/output/summary.md` |

If your assigned role is unclear, ask whether you are `researcher: octocode`, `researcher: gh`, or `judge`.

---

## Dependencies

Metering is **character-only** and dependency-free. The scripts count Unicode codepoints directly; tokenizer libraries are outside this benchmark's ruler.

---

See [benchmark/README.md](../README.md) for the publication-quality run standard.

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

Both agents use the same ruler — no init overhead for either side.

| Agent | Hook | `in_chars` | `out_chars` |
|---|---|---|---|
| `octocode` | `octo-meas.sh` delegates to `octo-meas.mjs`, which spawns `octocode tools` and captures stdout | Unicode codepoints of the queries JSON string passed to `--queries` | Unicode codepoints of exact stdout decoded as UTF-8 |
| `gh` | `gh-meas.sh` delegates to `gh-meas.mjs`, which spawns `gh` and captures the subprocess output | Unicode codepoints of the argv tail, excluding literal `gh ` | Unicode codepoints of exact `stdout + stderr` decoded as UTF-8 |

JSON-RPC envelopes and the literal `gh` command word are excluded so both agents are measured on meaningful payload only. Character counts are produced in-process with JavaScript codepoint counting (`[...text].length`).

---

## Script reference

| Script | Who uses it | Purpose | Why it is needed |
|---|---|---|---|
| `scripts/init-run.sh <agent>` | operator/researcher | Creates `output/<agent>/`, exports `$SESSION`, `$RUN`, `$LOG`, `$Q` | Establishes one clean, isolated run directory and derives the question count from `QUESTIONS.md` instead of hard-coding it. |
| `scripts/set-q.sh <n>` | researcher | Sets current question and starts Q wall-clock timer | Prevents cross-question metric leakage by giving metering wrappers a single current-Q sentinel. |
| `scripts/octo-meas.sh <tool> '<queries-json>'` | octocode researcher | Thin shell wrapper that delegates to `octo-meas.mjs` | Gives the octocode researcher a simple command shape while keeping metering in one Node script. |
| `scripts/octo-meas.mjs <tool> '<queries-json>'` | octocode researcher via wrapper | Spawns `octocode tools`; logs queries/stdout char I/O | Applies the character ruler to every Octocode CLI call and captures the exact output returned to the agent. |
| `scripts/gh-meas.sh <gh args>` | gh researcher | Thin shell wrapper that delegates to `gh-meas.mjs` | Gives the `gh` researcher a simple command shape while keeping metering implementation in one Node script. |
| `scripts/gh-meas.mjs <gh args>` | gh researcher via wrapper | Spawns `gh`; logs argv/stdout/stderr char I/O | Applies the same character ruler to all GitHub CLI calls and captures exact output returned to the agent. |
| `scripts/record.sh <n> <model> /tmp/answer.md` | researcher | Aggregates Q metrics and writes `q<n>.md` + `q<n>.json` | Couples the final answer with the measured rows for that exact question and fails loud if no metered calls were captured. |
| `scripts/finalize.mjs <run-dir>` | researcher/operator | Writes per-run `output.md` + `summary.json`; reports missing expected questions | Produces the machine-readable totals the judge needs and prevents incomplete runs from looking complete. |
| `scripts/chars.mjs` | metering scripts | Counts Unicode codepoints | Provides a dependency-free, tokenizer-independent ruler shared by every wrapper. |
| `scripts/aggregate.mjs` | internal/operator | Sums `log.jsonl` rows for one question; fails on zero rows | Provides the single source of truth for per-question calls/chars/time and catches bypassed metering. |
| `scripts/cross-run.mjs <run...>` | optional analysis | Reports medians across saved repeated runs | Summarizes repeated same-agent runs without pretending one stochastic run is definitive. |
| `scripts/report-variance.mjs [--csv] <run...>` | optional analysis | Reports variance/CV across saved repeated runs of the same agent | Quantifies run-to-run spread so benchmark claims can disclose instability instead of hiding it. |
| `scripts/validate-pipeline.mjs [--strict-cmds] <run...>` | optional analysis | Checks deterministic metering fields across same-agent runs | Regression-tests the metering pipeline itself, separate from normal agent stochasticity. |
| `scripts/score-token-usage.mjs <octocode-run> <gh-run> <quality.json>` | optional judge aid | Combines judge-supplied quality scores with measured chars; it does not score quality itself | Makes the arithmetic reproducible while keeping semantic quality judgment evidence-based and reviewable. |

---

# Researcher instructions: `octocode`

Use this section only if your assigned role is `researcher: octocode`.

Follow [`benchmark/OCTOCODE_RESEARCHER.md`](../OCTOCODE_RESEARCHER.md) with `<BENCHMARK>=github`.

Setup:

```bash
rm -rf benchmark/github/output/octocode
source benchmark/github/scripts/init-run.sh octocode
```

Finalize:

```bash
node benchmark/github/scripts/finalize.mjs benchmark/github/output/octocode
```

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

Use [`benchmark/judge/prompt.md`](../judge/prompt.md) with:

```
AGENTS:    octocode, gh
RUNS:      benchmark/github/output/octocode, benchmark/github/output/gh
QUESTIONS: benchmark/github/QUESTIONS.md
OUTPUT:    benchmark/github/output/summary.md
```

---

## Common Run-Quality Issues

| Issue | Impact | Fix |
|---|---|---|
| Researcher reads the other agent's output or final judge summary before finishing | The run is no longer blind | Discard and rerun |
| Bare `octocode tools` instead of `octo-meas.sh` | Tool call is unmetered | Redo the question through wrapper |
| Bare `gh` instead of `gh-meas.sh` | CLI call is unmetered | Redo the question through wrapper |
| Skipped `set-q.sh` | Tool calls are attributed to a different Q or Q0 | Redo the question correctly |
| `record.sh --allow-zero` | Broken metering is hidden | Keep it disabled for benchmark runs |
| Parallel questions | Metrics can leak across questions | Run sequentially |

---

## Links

- Questions: [`benchmark/github/QUESTIONS.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/QUESTIONS.md)
- Researcher prompt: [`benchmark/github/prompts/researcher.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/prompts/researcher.md)
- Judge prompt: [`benchmark/github/prompts/judge.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/prompts/judge.md)
