# GitHub Research Benchmark

31 questions answered by an agent restricted to one tool. Measures cost on the same ruler regardless of toolset.

## Agents

| Slug | Instruction |
|------|-------------|
| `octocode` | Use Octocode MCP tools only. Agent's MCP client is pointed at `mcp-meas.mjs` (a transparent stdio proxy); every `tools/call` is logged automatically. |
| `gh` | Use the `gh` CLI only. Invoke via `gh-meas.sh`. |
| `none` | No tools. Answer from training knowledge only. (Baseline — anchors per-question difficulty.) |

The questions in [`QUESTIONS.md`](./QUESTIONS.md) are tool-agnostic. Nothing else is communicated to the agent.

## Metrics

Per call (logged to `<run>/log.jsonl`):

| Field | Source | Deterministic |
|-------|--------|---------------|
| `calls` | count of log rows for that question | yes |
| `in_chars` | codepoints in the command/request | yes |
| `out_chars` | codepoints in the response body | yes |
| `elapsed_ms` | `Date.now()` end − start, around the live JSON-RPC exchange | yes |
| `quality` | judge score 0–3 | semi (see Scoring) |

No composite. Report all axes. Both sides are fully automated: the Octocode agent talks to `mcp-meas.mjs` (a stdio proxy spawning the real MCP server), so every `tools/call` pair is logged identically to a `gh` shell call.

## Run

```bash
source docs/dev/benchmark/github/scripts/init-run.sh octocode  # or gh, none
export Q=1
# … agent answers Q1, logs each call …
bash docs/dev/benchmark/github/scripts/record.sh "$Q" "<model>" answer.md
export Q=2
# … repeat through Q31 …
node docs/dev/benchmark/github/scripts/finalize.mjs "$RUN"
```

## Scoring

```bash
node docs/dev/benchmark/github/scripts/judge.mjs <run_a> <run_b>
node docs/dev/benchmark/github/scripts/verify-facts.mjs   # weekly drift check
```

`judge.mjs` is heuristic — backticked identifiers and CamelCase tokens from [`EXPECTED_FACTS.md`](./EXPECTED_FACTS.md). Treat it as a starting point; manual review of close calls is expected. `verify-facts.mjs` re-fetches every cited file path against GitHub and fails on 404.

## Multi-run

A single run reflects one rollout. Run each agent ≥3 times; compare medians:

```bash
node docs/dev/benchmark/github/scripts/cross-run.mjs output/*-octocode
```

## Layout

```
github/
├── QUESTIONS.md            ← agent input
├── EXPECTED_FACTS.md       ← judge-only
├── scripts/                ← measurement + aggregation
└── output/<ts>-<agent>/    ← one run per directory
    ├── log.jsonl
    ├── q1/output.md … q31/output.md
    └── output.md           ← finalize.mjs summary
```

## Conflict of interest

Designed by the Octocode team. Questions were chosen for code-research realism; independent replication with a different question set is welcomed.
