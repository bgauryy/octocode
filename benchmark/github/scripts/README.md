# Scripts

14 utilities. Full operator flow and agent prompts live in [`../README.md`](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/github/README.md).

## Pipeline at a glance

```
init-run ──→ set-q ──→ (agent works) ──→ record ──→ (repeat) ──→ finalize ──→ judge
              │                            │                       │
              ↓                            ↓                       ↓
         .current-q              log.jsonl + q<N>.json       summary.json
         .q-start                                             output.md
```

## 1 · Setup

**`init-run.sh <agent-slug>`** — Creates fixed `output/<agent>/` with empty `log.jsonl`, `.current-q=0`, and `.q-count=N` (auto-counted from `QUESTIONS.md`). Exports `$SESSION=output`, `$RUN`, `$LOG`, `$Q=0`. Slug = any `[a-z0-9_-]+`. Remove the existing `output/<agent>/` before starting a fresh run.

## 2 · Per-question routing

**`set-q.sh <n>`** — Writes `<n>` to `.current-q` and `Date.now()` to `.q-start`. Run **before every Q's first tool call** — the metering scripts read `.current-q` to attribute the call to the right Q; `record.sh` later reads `.q-start` to compute Q wall-clock.

## 3 · Metering (one log row per tool call)

**`mcp-meas.mjs <server-cmd> [args]`** — Transparent MCP stdio proxy. Spawns the real MCP server (e.g. `octocode-mcp`), forwards every JSON-RPC line in both directions, and appends one row to `$LOG` per `tools/call`. The agent's MCP client points at this script instead of the server.

**`gh-meas.sh <gh args>`** — Wrapper for `gh`. Runs the command, captures stdout+stderr, appends one row to `$LOG`. Agent must call every `gh` through this — bare `gh` is unmetered.

**`octo-meas.sh <tool> <req-file> <res-file> [ms]`** — Manual fallback when the MCP proxy can't be inserted. Agent supplies request + response payloads as files; the script logs them. Fragile (operator discipline) — prefer `mcp-meas.mjs`.

**`chars.mjs [--file P | --text S | <stdin>]`** — Character counter using Unicode codepoints. Dependency-free.

Log row schema (jsonl):

```json
{"ts","q","agent","cmd","in_chars","out_chars","elapsed_ms","exit"}
```

## 4 · Recording (per Q, after the agent finishes)

**`aggregate.mjs <log> <q> [--allow-zero] [--json]`** — Sums rows where `q==<n>`. Prints `calls in_chars out_chars elapsed_ms` (or JSON with per-call breakdown). **Fails loud on zero rows** unless `--allow-zero`.

**`record.sh <q> <model> <answer-file>`** — Aggregates `$LOG` for `<q>`, computes `q_elapsed_ms = now − .q-start`, writes `q<n>.json` + `q<n>.md` flat in `$RUN`. Strips a leading `## Answer` header from the answer file. `DETERMINISTIC=1` zeroes timestamps for byte-stable golden tests.

## 5 · Rollup (after all Qs)

**`finalize.mjs <run>`** — Reads flat `q<n>.json` files, writes `<run>/output.md` (human summary) and `<run>/summary.json` (machine totals + per-Q breakdown).

## 6 · Stability & integrity (≥2 runs)

**`cross-run.mjs <run...>`** — Median per metric across same-agent runs. Reads `summary.json`. Descriptive.

**`report-variance.mjs [--csv] <run...>`** — Coefficient of variation (CV=stdev/mean) per metric. Flags unstable Qs (CV ≥ 0.3). Descriptive — no pass/fail.

**`validate-pipeline.mjs [--strict-cmds] <run...>`** — Asserts `calls`, `in_chars`, `out_chars` are byte-identical across runs (wall-clock excluded). For regression-checking the metering code, not agent behaviour. Exit 0 only on full match.

## 7 · Scoring

Primary scoring is done by the judge agent — paste `prompts/judge.md` with the two run paths. The agent reads both `q<n>.md` answer files, independently fact-checks the answers against GitHub source/PR facts, scores semantic quality, and picks the winner by **quality-adjusted token/character usage**.

Winner axis:

```text
effective_chars = in_chars + out_chars + amortized_mcp_init_chars
token_score     = quality / (effective_chars / 1000)
```

Elapsed time is context only; it must not decide the winner.

**`score-token-usage.mjs <octocode-run> <gh-run> <quality-scores.json>`** — Deterministically combines judge quality scores with metered chars. It does not judge quality itself; it expects a JSON score file and outputs per-Q effective chars, token scores, and the aggregate token-usage winner.

## 8 · Smoke-testing

**`call-tool.mjs <tool-name> '<queries-json>'`** — One-shot MCP client routed through `mcp-meas.mjs`. Spawns proxy+server, sends a single `tools/call`, prints the result text, exits. Useful for verifying the MCP path works without writing a full run. Server resolution: `$OCTOCODE_MCP_SERVER` → monorepo `dist/index.js` → `octocode-mcp` on PATH.

## Ruler — same for both agents

Characters are counted as Unicode codepoints.

| | octocode (`mcp-meas.mjs`) | gh (`gh-meas.sh`) |
|---|---|---|
| `in_chars`  | chars of `JSON.stringify(params.arguments)` | chars of argv tail (no `gh ` prefix) |
| `out_chars` | chars of `result.content[].text` joined | chars of stdout + stderr |

JSON-RPC envelope and `gh` command prefix are excluded so neither agent pays for transport overhead.

## Pass/fail vs descriptive

| Script | Asks | Exits non-zero on… |
|---|---|---|
| `aggregate.mjs` | Did metering capture this Q? | missing log / zero rows / missing char fields |
| `validate-pipeline.mjs` | Is the metering code deterministic? | metric diff across runs |
| judge agent | How well did the agent answer, and who wins by quality per token/char? | semantic — no pass/fail |
| `score-token-usage.mjs` | Given judge scores, who wins by quality per measured char? | missing/invalid score file or run summaries |
| `cross-run.mjs`, `report-variance.mjs` | What's the spread / median? | descriptive only |
