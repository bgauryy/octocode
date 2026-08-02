# Results — octocode-mcp-vs-cli

> Tracked results ledger. Latest scored run first; full artifacts in the (gitignored) `output/<run>/` dir it names. Refresh this file after every scored run (see BENCHMARK.md § Results ledger).

## Run: compare-run-20260802-b (first scored run of this suite)

- **Time of check:** 2026-08-02 ~15:30–15:55 IDT (solves) · judged same session
- **Verdict: correctness TIE 1.00 / 1.00 (same engine, as designed). The findings are the result: 1 surface bug, 2 surface gaps, 1 engine limitation.**
- Provenance: repo SHA `e166f62d` (+8 dirty files from same-day fixes; identical tree both arms) · MCP arm = local `packages/octocode-mcp/dist/index.js` via MCP gateway (14 tools) · CLI arm = `node packages/octocode/out/octocode.js` via shell · k=1, single-agent, not blind · **no runner L3 tokens available** (bytes estimator)
- Artifacts: `output/compare-run-20260802-b/` — `octocode-mcp-vs-cli.md`, `kpi.json`, `logs/octocode-mcp-vs-cli/{mcp,cli}/calls.jsonl`

### Performance comparison matrix

| Metric | MCP surface | CLI surface | Note |
|---|---:|---:|---|
| Correctness (10 Q) | **1.00** | **1.00** | tie by design (same engine) |
| Quality (judge 1–5) | 4.8 | 5.0 | YAML render readable; CLI reads leaner |
| L0 preamble (cold) | ~183 KB | **4.2 KB** | full-catalog injection proxy vs one on-demand `--scheme` |
| L2 result payload | ~73 KB, **unfilterable** | 53.2 KB raw / **~18 KB read** | CLI filtered via jq/python |
| Tool calls | **19** | 33 | CLI paid quoting/field-retry turns |
| Retry turns lost | 2 (silent-wrong, worse) | 2 (loud schema errors) | see finding 1 |
| Data parity | ✅ byte-identical on 10/10 | ✅ | after canonical fields |

### Per-question matrix

| Q | Topic | MCP corr | CLI corr | MCP calls | CLI calls | CLI bytes (raw stdout) | Parity |
|---|---|---:|---:|---:|---:|---:|---|
| 1 | npm→structure | 1.0 | 1.0 | 2 | 2 | 1,834 | ✅ |
| 2 | zod repo+anchor | 1.0 | 1.0 | 5 | 4 | 3,818 | ❌ with `keywordsToSearch` (bug), ✅ after canonical field |
| 3 | checker region | 1.0 | 1.0 | 1 | 1 | 1,080 | ✅ byte-identical |
| 4 | fastify lib PR | 1.0 | 1.0 | 1 | 7 | 15,317 | ✅ |
| 5 | flask issue+commit | 1.0 | 1.0 | 2 | 4 | 7,032 | ✅ |
| 6 | clone+structural | 1.0 | 1.0 | 2 | 4 | 5,084 | ✅ engine-exact; MCP lacks clone (gap) |
| 7 | structure+sizes | 1.0 | 1.0 | 2 | 4 | 2,442 | ✅ |
| 8 | symbols outline | 1.0 | 1.0 | 1 | 2 | 2,030 | ✅ identical outline |
| 9 | LSP references | 1.0 | 1.0 | 1 | 3 | 3,357 | ✅ identical (incl. identical undercount) |
| 10 | dead exports | 1.0 | 1.0 | 2 | 2 | 11,213 | ✅ |

### Findings matrix (the payload of this suite)

| # | Kind | Finding | Surface | Severity |
|---|---|---|---|---|
| 1 | **Bug** | Unknown fields **silently dropped** → `ghSearchCode keywordsToSearch` ran an *unfiltered* repo search (434 junk matches, no error, deterministic). CLI alias-folds the same input and answers correctly. | MCP only | **High** — confidently-wrong data |
| 2 | Gap | No `ghCloneRepo` without `ENABLE_CLONE`; `type:"directory"` materializer is 1-level-deep (Q6 workflow impossible in bounded calls) | MCP only | Medium |
| 3 | Gap | `localFindDeadCode` `entrypointsResolved` = 161 entries injected wholesale (~6 KB L2 tax); CLI filtered to ~300 B | MCP (structural) | Low |
| 4 | Engine | LSP `references` warmup misses tests dir (`warmedFiles: 2`) — identical undercount **both** surfaces, proving engine-not-surface | Both | Known (tool audit) |

### Conclusion

Correctness parity confirms surface isolation worked. **MCP = fewer calls, zero shell friction, but pays cold-start preamble and everything the tool emits; CLI = pays plumbing turns but reads only what it chooses.** The decisive item is finding 1: the MCP path must adopt the direct-catalog alias folding + unknown-field rejection (`toolInputPreparation.ts`) — silent-wrong beats loud-error as the worst failure mode observed across all three suites today. Next run: ≥3 solvers, blind judge, harness with L3 runner tokens (cold/warm cache split) to settle the L0 amortization question quantitatively.

## Prior runs

| Run | Date | Verdict | Notes |
|---|---|---|---|
| compare-run-20260802-b | 2026-08-02 | TIE + findings | first scored run; promoted suite from `draft` |
