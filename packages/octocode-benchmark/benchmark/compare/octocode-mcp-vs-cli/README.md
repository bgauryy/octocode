# Octocode MCP vs Octocode CLI — Surface Self-Eval

Same engine, two delivery surfaces. Arm **B-mcp** solves with the
`mcp__octocode__*` MCP tools; Arm **B-cli** solves with
`node packages/octocode/out/octocode.js tools <name> --queries '<json>'` via a
shell. Both run the *same tool implementations underneath* — so **correctness
and answer quality should tie**, and the suite isolates what the surface itself
costs and affords.

## What this check answers

| Dimension | MCP surface | CLI surface |
|---|---|---|
| How results enter context | tool_result injected **wholesale** — the agent cannot filter before reading | stdout can be piped through `jq`/`head` first — the agent reads a **chosen subset** |
| Schema discovery | tool schema pre-loaded by the client (paid once per session) | `tools <name> --scheme` on demand (paid when used) |
| Batching | one call, `queries[]` array | same via `--queries '[...]'` |
| Plumbing overhead | none | shell quoting, temp files, `wc -c` bookkeeping turns |

## The 10 questions cover all 15 active tools

| Q | Target tools | Q | Target tools |
|---|---|---|---|
| Q1 | npmSearch → ghViewRepoStructure | Q6 | ghCloneRepo → localSearchCode (structural) |
| Q2 | ghSearchRepos → ghSearchCode | Q7 | localViewStructure → localFindFiles |
| Q3 | ghGetFileContent (matchString + minify) | Q8 | localGetFileContent (minify:"symbols") |
| Q4 | ghSearchPullRequests | Q9 | lspGetSemantics (references/callers) |
| Q5 | ghSearchIssues → ghSearchCommits | Q10 | localFindDeadCode → localSearchCode |

## Measuring token usage — the point of this suite

MCP token cost is layered; measuring only tool_result bytes systematically
flatters MCP (it hides the schema preamble) and measuring only runner totals
flatters the CLI (it hides plumbing turns). Report **all four layers** per arm:

| Layer | What | MCP arm | CLI arm |
|---|---|---|---|
| **L0 — surface preamble** | one-time context cost of making tools callable | chars of ALL injected `mcp__octocode__*` tool definitions/schemas (paid at session start whether used or not; measure once, report both raw and amortized per question, and note cache_read status after turn 1) | chars of every `tools <name> --scheme` output actually fetched (paid only on use) |
| **L1 — call envelope** | per-call overhead | `tool_use` block JSON (name + serialized args) per call | the shell command string + any bookkeeping turns (`wc -c`, temp files, quoting retries) |
| **L2 — result payload** | what enters context per call | full `tool_result` content chars — injected **wholesale**, unfilterable | chars the solver actually read (post `jq`/`head`), AND separately raw stdout bytes (what a plumbing-free agent would have paid) |
| **L3 — runner totals (authoritative)** | ground truth from the harness | input / output / cache_read / cache_write tokens for the whole solve | same |

Rules:

1. **L3 is the primary tokens-usage KPI** when available; L0–L2 explain *why*
   it differs between arms. chars/4 is the fallback estimator — state which
   was used.
2. **Cache-aware accounting**: MCP's L0 preamble is typically cache_read after
   the first turn — report both cold (first-question) and warm (amortized)
   cost, never conflate them.
3. **Calls and turns are first-class metrics**: report per question `calls`
   (tool invocations) and `turns` (assistant turns), including CLI plumbing
   turns that produced no answer content. Fewer calls at equal correctness =
   better surface ergonomics.
4. **Parity check per call**: for each question, the two arms' underlying tool
   payloads must byte-compare equal modulo envelope — any data divergence is a
   surface bug and is reported regardless of scores.

## Oracles

Same-engine parity: for every question the two arms' *data* must agree (same
counts, same file:line anchors) — a data divergence is a **surface bug**, the
most valuable possible finding. Facts about this repo are computed at run time
at the frozen SHA; remote facts re-verified by the judge outside both arms
(curl to api.github.com / raw.githubusercontent.com). Judge is blind to the arm
and scores the shared rubric: correctness 1.0/0.5/0 + quality 1–5.

Method, metrics, decision rule: [`../README.md`](../README.md). Output contract:
[`../../schemas/`](../../schemas/) — emit `kpi.json` per
[`kpi.schema.json`](../../schemas/kpi.schema.json).

## Status

`draft` — questions frozen, not yet scored. First scored run should pin: this
repo's SHA, the remote repos' SHAs, and identical model + step budget per arm.
