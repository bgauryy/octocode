# Benchmark Suite

Benchmarks for AI-assisted code research. The suite measures which research
tooling helps an agent produce the best verified answer for the measured
character budget.

Raw command speed is context only. Winners are decided by answer quality,
research depth, and measured character cost. Optional real token counters can
be reported separately when every compared runner supports them.

## Directory Structure

```text
benchmark/
├── README.md                  # Map, runbook, and output contract
├── COMPARISON.md              # Scoring methodology and comparison rules
├── prompts/
│   ├── octocode-researcher.md # Paste-ready Octocode researcher prompt
│   ├── rtk-gh-researcher.md   # Paste-ready RTK + gh researcher prompt
│   └── judge.md               # Paste-ready judge prompt
├── questions/
│   ├── README.md              # Detailed Next.js benchmark notes
│   └── nextjs.md              # Default 10-question benchmark (Section 1, remote only)
├── scripts/
│   ├── init-run.sh            # Creates output/<agent>/ and exports RUN/LOG
│   ├── set-q.sh               # Selects the active question
│   ├── *-meas.sh              # Metered wrappers for octocode, rtk, gh
│   ├── record.sh              # Records one answer and per-Q metrics
│   ├── finalize.mjs           # Writes output.md and summary.json
│   └── score-comparison.mjs   # Combines judge scores with measured metrics
└── output/
    ├── octocode/              # Completed Octocode researcher run
    ├── rtk-gh/                # Completed RTK + gh researcher run
    └── summary.md             # Judge output
```

The default benchmark uses
[`benchmark/questions/nextjs.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/questions/nextjs.md).
It covers **10 remote-only questions** (Section 1). The shared scripts can run any question file that uses the existing
`### Q<n> — <question>` heading format.

## Agents

| Role | Reads | Writes | Rule |
|---|---|---|---|
| `researcher: octocode` | `benchmark/questions/nextjs.md`, `benchmark/prompts/octocode-researcher.md` | `benchmark/output/octocode/` | Use only metered Octocode wrapper calls. |
| `researcher: rtk-gh` | `benchmark/questions/nextjs.md`, `benchmark/prompts/rtk-gh-researcher.md` | `benchmark/output/rtk-gh/` | Use only metered `rtk` and `gh` wrapper calls. |
| `judge` | completed run dirs, questions, `benchmark/prompts/judge.md` | `benchmark/output/summary.md` | Fact-check independently; do not count judge research in researcher totals. |

Researchers must stay blind: do not read another agent's output or
`benchmark/output/summary.md` before finalizing the current run.

## Run A Researcher

From the repository root:

```bash
# Required: point to the local build of the Octocode CLI.
# Always use the built binary — never rely on the globally installed octocode.
export OCTOCODE_CLI_BIN="/Users/guybary/Documents/octocode-mcp/packages/octocode-cli/out/octocode-cli.js"

# Required for local-tool questions in the default Next.js benchmark.
rm -rf /tmp/nextjs-bench
git clone --depth 1 https://github.com/vercel/next.js /tmp/nextjs-bench
export ALLOWED_PATHS="/tmp/nextjs-bench"

# Start one fresh run.
rm -rf benchmark/output/octocode
source benchmark/scripts/init-run.sh octocode
```

For each question:

```bash
bash benchmark/scripts/set-q.sh <n>

# Octocode researcher:
bash benchmark/scripts/octo-meas.sh <tool-name> '<queries-json>'

# RTK + gh researcher:
bash benchmark/scripts/rtk-meas.sh <rtk-subcommand-and-args>
bash benchmark/scripts/gh-meas.sh <gh-subcommand-and-flags>

bash benchmark/scripts/record.sh <n> "<model-id>" /tmp/answer.md
```

Finalize the run:

```bash
node benchmark/scripts/finalize.mjs "$RUN"
```

Every research command must go through a metering wrapper. Bare `octocode`,
`rtk`, or `gh` calls make the run invalid because they do not appear in
`log.jsonl`.

## Run The Judge

Wait until both researcher runs contain `output.md` and `summary.json`, then use
[`benchmark/prompts/judge.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/prompts/judge.md)
with:

```text
AGENTS:    octocode, rtk-gh
RUNS:      /Users/guybary/Documents/octocode-mcp/benchmark/output/octocode,
           /Users/guybary/Documents/octocode-mcp/benchmark/output/rtk-gh
QUESTIONS: /Users/guybary/Documents/octocode-mcp/benchmark/questions/nextjs.md
OUTPUT:    /Users/guybary/Documents/octocode-mcp/benchmark/output/summary.md
```

The judge assigns quality/depth scores after independently verifying source
facts. Judge tool calls are outside the measured researcher runs.

## Make A New Benchmark

1. Add a question bank under `benchmark/questions/<name>.md`.
2. Use headings like `### Q1 — <category>: <question>` so scripts can count and extract questions.
3. Decide the agents to compare, then create or update prompts in `benchmark/prompts/`.
4. Start each run with a custom questions file:

```bash
export QUESTIONS_FILE="$(pwd)/benchmark/questions/<name>.md"
source benchmark/scripts/init-run.sh <agent-slug>
```

5. Require every researcher to use `benchmark/scripts/*-meas.sh`.
6. Record every answer with `benchmark/scripts/record.sh`.
7. Finalize each run with `benchmark/scripts/finalize.mjs`.
8. Judge the completed outputs and write the comparison to `benchmark/output/summary.md`.

Keep benchmark questions independently verifiable. Good questions require exact
file paths, lines, source quotes, PR metadata, package facts, or exhaustive
search results. Mark changing questions with `[drift]`.

## Measurement

Canonical cost:

```text
total_chars_to_answer = sum(in_chars + out_chars) across every metered call
```

Approximate tokens are display-only:

```text
approx_tokens = ceil(total_chars_to_answer / 4)
```

### octocode — Q1–Q10 token usage (latest run)

| Q | Topic | Calls | ~Tokens | Total Chars |
|---|-------|------:|--------:|------------:|
| Q1 | `notFound()` propagation | 4 | 4,341 | 17,363 |
| Q2 | Bulk symbol lookup | 2 | 2,892 | 11,567 |
| Q3 | `revalidatePath` call sites | 7 | 13,116 | 52,463 |
| Q4 | Files with both routers | 2 | 3,636 | 14,541 |
| Q5 | `redirect()` end-to-end | 4 | 4,103 | 16,410 |
| Q6 | `renderToHTMLOrFlight` signature | 4 | 3,399 | 13,593 |
| Q7 | `revalidateTag` invalidation | 4 | 3,917 | 15,665 |
| Q8 | Server Action routing | 7 | 7,039 | 28,153 |
| Q9 | Agent eval benchmark | 5 | 3,218 | 12,869 |
| Q10 | Turbopack benchmark workflow | 11 | 5,917 | 23,668 |
| **Σ** | | **50** | **51,573** | **206,292** |

For the full scoring model, clean-win rules, drift handling, and character/token
policy, see
[`benchmark/COMPARISON.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/COMPARISON.md).

## Output Contract

Each completed agent run directory must contain:

```text
benchmark/output/<agent>/
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

After the judge writes a quality/depth JSON file, combine measured run metrics
with judge scores:

```bash
node benchmark/scripts/score-comparison.mjs \
  --questions benchmark/questions/nextjs.md \
  --scores benchmark/output/quality-depth.json \
  --markdown \
  octocode=benchmark/output/octocode \
  rtk-gh=benchmark/output/rtk-gh
```
