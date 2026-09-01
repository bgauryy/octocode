# Octocode Interfaces

Load when transport, tool choice, authentication, diagnostics, or CLI syntax is unclear. Why: `references/algorithm.md` owns evidence and routing; this file owns only *how to call things*.

**Interface.** Use MCP tools when they are exposed — typed and with no shell hop; call them directly. In an Octocode checkout use the built local CLI (`node packages/octocode/out/octocode.js`); from a standalone installed skill use `npx octocode`. Both expose the same 10 input contracts and runtime row data, and 10 tools are enabled by default. Their transports differ: MCP returns a `CallToolResult`, while CLI default, compact, and JSON modes render different views of it. Neither interface publishes output schemas. Check once at the start, then stop thinking about it. If neither interface exists, continue with stated degraded confidence; ask to install or authenticate only when protected GitHub data is essential.

**Two-step call rule.** Never guess fields: read the schema, then run.

```bash
OCTO='node packages/octocode/out/octocode.js'           # checkout; use npx octocode outside it
$OCTO context --minimal                                 # protocol + tool list (cheapest)
$OCTO auth status --json                                # GitHub reach
$OCTO tools <name> --scheme --json --compact            # 1. fields + conditional relations
$OCTO tools <name> --queries '<json>' --compact         # 2. run it
$OCTO cache fetch <owner/repo> [path] --depth file|tree|clone
```
Unknown fields fail fast with a suggestion (`'depth' → did you mean 'maxDepth'?`) and exit `2` — a cheap correction, but the schema read is cheaper. There is no `search` command and no aliases (`grep`, `cat`, `ls`, `find`, `lsp`, `pr`, `pkg`, `repo`, `diff`): every research need routes through `tools <name>`; only `clone` and `cache` are shortcuts.

**Materialize.** `ghCloneRepo` is the tool path; `cache fetch` is the shell shortcut when you are already at a prompt. Both land content under the local Octocode cache and hand you a path the local tools run on unmodified. Materialize once when AST, LSP, multi-file regex, exact absence, or a third read into the same remote area is coming.

**Efficiency.** Batch up to five independent queries per call — one call with five beats five calls. Orient cheap (tree, discovery) before exact reads; they aim the expensive ones. Follow `next.*`, cursors, and match ranges from the payload instead of re-deriving them. Use `--compact` for agent output, `--pretty` for humans, `context --minimal` under tight budgets. Compact schemas include a `relations` list for mode-specific required and mutually exclusive fields; obey it before composing a query. Spend an extra angle on a *claim*; spend an extra query on a *lookup*. **The 10-tool discovery catalog:**

| Need | Tool |
|---|---|
| local orient / find / search / read | `localSearch` with `operation:"tree"|"files"|"text"|"structural"`, then `localGetFileContent` |
| repository file graph | `localAnalyzeGraph`: `dependencies`, `dependents`, `path`, `cycles`, `reachability`, `deadCode`; follows relative imports and exact workspace `package.json` exports, never inferred aliases; edges are syntactic candidates, and deletes need LSP/search/tests |
| semantics | `lspGetSemantics` (`documentSymbols`, `definition`, `references`, `callers`, `callees`, `hover`, diagnostics, type/call hierarchy) |
| GitHub code / tree / repositories | `ghSearch` with `operation:"code"|"tree"|"repositories"` |
| GitHub exact read | `ghGetFileContent` |
| GitHub PR / issue / commit search | `ghSearchHistory` with `operation:"pullRequests"|"issues"|"commits"` |
| GitHub PR / issue / commit / compare detail | `ghGetHistoryItem` with `operation:"pullRequest"|"issue"|"commit"|"compare"` |
| materialize remote · packages | `ghCloneRepo` · `npmSearch` |
Remote code, repository, and tree discovery route through `ghSearch`; local text, structural, file, and tree discovery route through `localSearch`. **Gates and diagnostics:**

| Gate or signal | Effect → move |
|---|---|
| `ENABLE_LOCAL` | MCP server can gate local tools; the CLI enables them by default |
| `ENABLE_CLONE` | clone is on by default for CLI and MCP; `false` disables it |
| auth/rate | check auth; ask login only for protected data; narrow/retry and mark incomplete |
| local/clone disabled | check `ENABLE_LOCAL`/`ENABLE_CLONE`/`.octocoderc`; use remote proof |
| tool disabled error | check the gate rows above before assuming the tool does not exist |
| LSP unavailable | exact/AST fallback; check `lsp-server status <file>`; do not claim no usage |
| partial/warning/redaction | follow continuation; preserve warning; never reconstruct secrets |
| provider empty/approximate/stale | verify ref/path/filter, materialize or downgrade; force refresh only for freshness |

A disabled surface is a *skipped* surface — declare it, degrade confidence, do not fake it. Exit codes: `0` ok · `2` input · `3` not-found · `4` auth · `5` tool · `7` rate-limit.
Next: when a call returns empty or errors, read the failure signals in `references/algorithm.md` rather than concluding absence; when materializing to prove a remote claim load `references/workflow-combination.md`; otherwise return to the route you came from.
