# Octocode Interfaces

Load when transport, tool choice, authentication, diagnostics, or CLI syntax is unclear. Why: `references/algorithm.md` owns evidence and routing; this file owns only *how to call things*.

**Interface.** Use MCP tools when they are exposed — typed, no shell hop, same payloads; call them directly. Otherwise use `npx octocode tools <name>`: same 15-tool catalog, same schemas, same JSON — the CLI is a full substitute, not a degraded one. Check once at the start, then stop thinking about it. If neither interface exists, continue with stated degraded confidence; ask to install or authenticate only when protected GitHub data is essential.

**Two-step call rule.** Never guess fields: read the schema, then run.

```bash
npx octocode context --minimal                         # protocol + tool list (cheapest)
npx octocode auth status --json                        # GitHub reach
npx octocode tools <name> --scheme --json --compact     # 1. what fields exist
npx octocode tools <name> --queries '<json>' --compact  # 2. run it
npx octocode clone <owner/repo[/path][@branch]>         # sparse clone of a repo or subtree
npx octocode cache fetch <owner/repo> [path] --depth file|tree|clone
```

Unknown fields fail fast with a suggestion (`'depth' → did you mean 'maxDepth'?`) and exit `2` — a cheap correction, but the schema read is cheaper. There is no `search` command and no aliases (`grep`, `cat`, `ls`, `find`, `lsp`, `pr`, `pkg`, `repo`, `diff`): every research need routes through `tools <name>`; only `clone` and `cache` are shortcuts.

**Materialize.** `ghCloneRepo` is the in-tool path; the two shell commands above do the same job when you are already at a prompt. Both land content under the local Octocode cache and hand you a path the local tools run on unmodified. Materialize once when AST, LSP, multi-file regex, exact absence, or a third read into the same remote area is coming.

**Efficiency.** Batch up to five independent queries per call — one call with five beats five calls. Orient cheap (tree, discovery) before exact reads; they aim the expensive ones. Follow `next.*`, cursors, and match ranges from the payload instead of re-deriving them. Use `--compact` for agent output, `--pretty` for humans, `context --minimal` under tight budgets. Spend an extra angle on a *claim*; spend an extra query on a *lookup*. **The 15-tool catalog:**

| Need | Tool |
|---|---|
| local orient / find / search / read | `localViewStructure`, `localFindFiles`, `localSearchCode`, `localGetFileContent` |
| dead-code candidates | `localFindDeadCode` for repo-wide reachability clusters; prove deletes with LSP/search/tests |
| semantics | `lspGetSemantics` (`documentSymbols`, `definition`, `references`, `callers`, `callees`, `hover`, diagnostics, type/call hierarchy) |
| GitHub code / read / tree / repos | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos` |
| GitHub PRs / issues / commits | `ghSearchPullRequests`, `ghSearchIssues`, `ghSearchCommits` |
| materialize remote · packages | `ghCloneRepo` · `npmSearch` |

`ghListReleases` (release history, latest stable) is opt-in and not in the default catalog: it is absent from `tools --json` and from `context` until `ENABLE_RELEASES=1`, and returns a typed "disabled" error if called without the flag. Treat releases as a skipped surface unless you set it. **Gates and diagnostics:**

| Gate or signal | Effect → move |
|---|---|
| `ENABLE_LOCAL` | MCP server can gate local tools; the CLI enables them by default |
| `ENABLE_CLONE` | MCP clone needs it; CLI clone is on by default |
| `ENABLE_RELEASES=1` | required for `ghListReleases` on both surfaces |
| auth/rate | check auth; ask login only for protected data; narrow/retry and mark incomplete |
| local/clone disabled | check `ENABLE_LOCAL`/`ENABLE_CLONE`/`.octocoderc`; use remote proof |
| tool disabled error | check the gate rows above before assuming the tool does not exist |
| LSP unavailable | exact/AST fallback; check `lsp-server status <file>`; do not claim no usage |
| partial/warning/redaction | follow continuation; preserve warning; never reconstruct secrets |
| provider empty/approximate/stale | verify ref/path/filter, materialize or downgrade; force refresh only for freshness |

A disabled surface is a *skipped* surface — declare it, degrade confidence, do not fake it. Exit codes: `0` ok · `2` input · `3` not-found · `4` auth · `5` tool · `7` rate-limit.

Next: when a call returns empty or errors, read the failure signals in `references/algorithm.md` rather than concluding absence; when materializing to prove a remote claim load `references/workflow-combination.md`; otherwise return to the route you came from.
