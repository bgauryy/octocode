# Octocode live tool benchmark

This folder is the durable question catalog for live Octocode CLI, raw tool, and OQL evaluation. It splits live checks into local, external, and workflow lanes.

These docs define what to ask and how to score it. Reported runs still write artifacts under `packages/octocode-benchmark/output/<benchmark-name>-<YYYYMMDDTHHMMSSZ>/` and must follow the agent benchmark runbook at https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/recipes/agent-benchmark-runbook.md.

## Automated Smoke

Use the live smoke runner when a benchmark claim needs durable artifacts instead of a manual transcript:

```bash
node packages/octocode-benchmark/benchmark/octocode/run-live-smoke.mjs
node packages/octocode-benchmark/benchmark/octocode/run-live-smoke.mjs --skip-network
node packages/octocode-benchmark/benchmark/octocode/validate-output-runs.mjs
node packages/octocode-benchmark/benchmark/octocode/validate-output-runs.mjs --repair-legacy
```

`run-live-smoke.mjs` writes the full runbook layout (`README.md`, `manifest.json`, `summary.json`, `commands.ndjson`, `results.md`, `reflection.md`, `ratings.json`, `raw/`, `schemes/`, `artifacts/`) and validates the new `summary.json` before exiting. The default mode exercises GitHub/npm/cache flows; `--skip-network` keeps only local deterministic lanes. `validate-output-runs.mjs` checks every recorded run with a `summary.json`; `--repair-legacy` converts older loose summaries from their existing `results.json` and `commands.ndjson` evidence.

## Layout

| Folder | Focus | Main question |
|---|---|---|
| `local/` | Local tools, local quick commands, local OQL targets | Can local search, file reads, structure, LSP, AST, ripgrep, binary, and archive paths produce complete, paginated, followable evidence? |
| `external/` | GitHub, npm, clone/materialize, PRs, commits, remote OQL | Can external provider rows preserve repo/package/PR/commit identity and expose exact follow-up proof? |
| `flows/` | Local and external workflows | Can an agent move from orientation to exact proof with low token cost, clear continuations, timings, and reflection? |

## Inventory Gate

Run this before scoring any row. A missing command, stale schema, or bad route blocks the run.

```bash
NODE_BIN="${NODE_BIN:-node}"
CLI=("$NODE_BIN" packages/octocode/out/octocode.js)

"${CLI[@]}" --help --no-color > "$BENCH_OUT/schemes/help.txt"
"${CLI[@]}" tools --compact --no-color > "$BENCH_OUT/schemes/tools.txt"
"${CLI[@]}" search --scheme --compact --no-color > "$BENCH_OUT/schemes/search.json"

for tool in \
  ghSearchCode ghGetFileContent ghViewRepoStructure ghSearchRepos \
  ghHistoryResearch ghCloneRepo npmSearch localSearchCode \
  localViewStructure localFindFiles localGetFileContent \
  localBinaryInspect lspGetSemantics oqlSearch
do
  "${CLI[@]}" tools "$tool" --scheme --compact --no-color \
    > "$BENCH_OUT/schemes/$tool.txt"
done

NO_COLOR=1 "$NODE_BIN" packages/octocode-benchmark/benchmark/cli/check-cli-metadata.mjs
```

If the embedded app Node cannot load `octocode-engine.darwin-arm64.node`, rerun with a system Node such as `/opt/homebrew/bin/node` and record the Node path in `manifest.json`.

## Universal Scoring

| Score | Meaning |
|---:|---|
| `2` | Pass: correct data, no unexpected errors, explicit pagination/continuation, and enough anchors to continue without guessing. |
| `1` | Partial: data is useful, but one shape, hint, pagination field, match anchor, timing, or continuation is missing or ambiguous. |
| `0` | Fail: wrong/empty result, silent lossy mapping, schema drift, unusable continuation, false proof of absence, or unexpected tool error. |
| `N/A` | Environment gated: auth, rate limit, clone disabled, LSP server unavailable, fixture missing, or provider unavailable with an honest diagnostic. |

Also record output quality on a `1..5` scale:

| Quality | Meaning |
|---:|---|
| `5` | Exact anchors, concise output, valid evidence grade, runnable `next.*`, no stale hints. |
| `4` | Correct result with minor verbosity or formatting friction. |
| `3` | Works, but a human has to infer the next command. |
| `2` | Command exits 0, but evidence grade, pagination, or continuation is misleading. |
| `1` | Fails, uses stale schema, hides missing fields, or offers no repair path. |

## Required Measurements

Every row result should include these fields in `results.md` or a companion table, and command executions should still be logged in `commands.ndjson`.

| Column | Meaning |
|---|---|
| `id` | Row ID from `local/`, `external/`, or `flows/`. |
| `surface` | `quick-cli`, `raw-tool`, `oql`, or `flow`. |
| `command_or_query` | Exact command or JSON query used. |
| `exit_code` | CLI/tool exit code. |
| `duration_ms` | Wall-clock time for the command. Use median after warmup for local-only rows; one marked live run is acceptable for network rows. |
| `stdout_chars` | Raw stdout character count. |
| `stderr_chars` | Raw stderr character count. |
| `tokens_out_est` | Approximate output tokens, `ceil(stdout_chars / 4)`. |
| `tokens_in_est` | Approximate prompt/command tokens when measured by an agent harness; otherwise `N/A`. |
| `cache_mode` | `cold`, `warm`, `mixed`, or `not-applicable`. |
| `data_complete` | `yes`, `partial`, or `no`; partial must include the next page/range query. |
| `pagination_ok` | Whether page, match page, char window, PR content page, archive page, or string offset continuation works. |
| `followup_ok` | Whether the result exposes a runnable `next.*`, `location.*`, `matchRanges[]`, path, line, PR number, commit SHA, package repo, or localPath. |
| `diagnostics_ok` | Whether empty, unsupported, rate-limited, partial, or approximate behavior is honest. |
| `score` | `2`, `1`, `0`, or `N/A`. |
| `quality` | `1..5` output quality score. |
| `notes` | One concise, actionable observation. |

## Universal Checks

Apply these checks to every row, even when a table does not repeat them.

| Check | Pass condition |
|---|---|
| Schema shape | `--scheme` or help documents the exact fields used by the row; raw tool calls never guess field names. |
| Data completeness | Rows include enough owner/repo/path/file/line/symbol/PR/commit/package/artifact context to continue. |
| No unexpected errors | Exit code and diagnostics match the expected state. Provider/auth gates are explicit. |
| Pagination | `hasMore` or partial content includes a concrete next query/window/page and page 2 preserves filters. |
| Match anchors | Search rows can feed content fetch through `matchString`, `matchRanges[]`, `line`, `lineHint`, PR `matchString`, or selected patch files. |
| Minification | Exact, standard/compact, and symbols views are distinct; partial content is marked. |
| Search CLI | `octocode search --scheme`, shorthand, full `--query`, `--explain`, and `--dry-run` agree on target, backend route, and diagnostics. |
| OQL proof | OQL rows include evidence semantics and proof grade/status when applicable; candidate rows are not scored as proof until upgraded. |
| Hints | Output tells the next action when results are empty, unsupported, partial, approximate, or candidate-grade. |

## Corpus

| Alias | Corpus | Use |
|---|---|---|
| `LOCAL` | This monorepo root | Local search, LSP, AST, binary/archive, OQL research, search CLI shorthand. |
| `LCJS` | `langchain-ai/langchainjs` | TypeScript code search, PR archaeology, commit history, streaming APIs. |
| `LCPY` | `langchain-ai/langchain` | Large Python repo pagination and cross-language comparison. |
| `LGJS` | `langchain-ai/langgraphjs` | LangGraph.js comparison, tree pagination, docs/examples split. |
| `LGPY` | `langchain-ai/langgraph` | Python graph/runtime comparison. |
| `ZUSTAND` | `pmndrs/zustand` | Small TypeScript package, npm-to-source, content/minify proof. |
| `HERMES_AGENT` | `NousResearch/hermes-agent` | Mixed Python/package/web assets. |
| `HERMES_ENGINE` | `facebook/hermes` | Native/runtime repo, CMake/content, non-TypeScript browsing. |
| `OPENCLAW` | `Gen-Verse/OpenClaw-RL` plus repo-search candidates | Discovery quality and honest fallback behavior. |

## Reflection

Every reported run must include `reflection.md` with these sections in order:

| Section | Required content |
|---|---|
| What worked | Fast exact anchors, runnable continuations, best local flow, best external flow. |
| What did not work | Failures, friction, misleading/noisy output, environment gates. |
| Missing | Benchmark command gaps, schema/help gaps, missing proof continuation, missing hint. |
| Possible improvements | Product/code reason, benchmark automation reason, documentation reason. |
| Good flows | Reusable command chain and why it should be preserved. |
| Ratings | Raw/MCP tools, OQL search, quick CLI, remote-as-local, data quality, schema quality, research quality, output quality. |
| Next fix | One concrete product fix or benchmark automation task. |
