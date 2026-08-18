# Octocode Interfaces

Load for transport, tool choice, authentication, diagnostics, or CLI syntax. `algorithm.md` owns evidence and routing; this file only owns *how to call things*.

## Pick the interface

**Use MCP tools when they are exposed.** They are typed, need no shell hop, and return the same payloads. Call them directly.

**Otherwise use `npx octocode tools <name>`.** Same 15-tool catalog, same schemas, same JSON. Nothing in this skill requires MCP — the CLI is a full substitute, not a degraded one.

Check once at the start, then stop thinking about it:

```bash
npx octocode context --minimal   # protocol + tool list (cheapest)
npx octocode auth status --json  # GitHub reach
```

If neither interface exists, continue with stated degraded confidence. Ask to install or authenticate only when protected GitHub data is essential.

## The two-step call rule

Never guess fields. Read the schema, then run:

```bash
npx octocode tools <name> --scheme --json --compact   # 1. what fields exist
npx octocode tools <name> --queries '<json>' --compact # 2. run it
```

Unknown fields fail fast with a suggestion (`'depth' → did you mean 'maxDepth'?`) and exit `2`. That is a cheap correction, but the schema read is cheaper.

## Tools (15)

| Need | Tool |
|---|---|
| local orient / find / search / read | `localViewStructure`, `localFindFiles`, `localSearchCode`, `localGetFileContent` |
| dead-code candidates | `localFindDeadCode` for repo-wide reachability clusters; prove deletes with LSP/search/tests |
| semantics | `lspGetSemantics` (`documentSymbols`, `definition`, `references`, `callers`, `callees`, `hover`, diagnostics, type/call hierarchy) |
| GitHub code / read / tree / repos | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos` |
| GitHub PRs / issues / commits | `ghSearchPullRequests`, `ghSearchIssues`, `ghSearchCommits` |
| materialize remote | `ghCloneRepo` |
| packages | `npmSearch` |

Opt-in, not in the default catalog: `ghListReleases` (release history and latest stable) requires `ENABLE_RELEASES=1`. It is absent from `tools --json` and from `context` until enabled, and returns a typed "disabled" error if called without the flag. Treat releases as a skipped surface unless you set it.

## Materialize: shell shortcuts

`ghCloneRepo` is the in-tool path. Two shell commands do the same job when you are already at a prompt:

```bash
npx octocode clone <owner/repo[/path][@branch]>          # sparse clone of a repo or subtree
npx octocode cache fetch <owner/repo> [path] --depth file|tree|clone
```

Both land content under the local Octocode cache and hand you a path the local tools run on unmodified. Materialize once when AST, LSP, multi-file regex, exact absence, or a third read into the same remote area is coming.

## Efficiency

- **Batch up to five independent queries per call.** One call with five queries beats five calls.
- **Orient cheap, then read exact.** Tree and discovery cost little and aim the expensive reads.
- **Follow `next.*`, cursors, and match ranges** that come back in the payload instead of re-deriving them.
- **Use `--compact` for agent output**, `--pretty` for humans, `context --minimal` under tight budgets.
- **Spend an extra angle on a *claim*; spend an extra query on a *lookup*.**

There is no `search` command and no aliases (`grep`, `cat`, `ls`, `find`, `lsp`, `pr`, `pkg`, `repo`, `diff`). Every research need routes through `tools <name>`; only `clone` and `cache` are shortcuts.

## Environment gates

| Gate | Effect |
|---|---|
| `ENABLE_LOCAL` | MCP server can gate local tools; the CLI enables them by default |
| `ENABLE_CLONE` | MCP clone needs it; CLI clone is on by default |
| `ENABLE_RELEASES=1` | required for `ghListReleases` on both surfaces |

A disabled surface is a *skipped* surface — declare it, degrade confidence, do not fake it.

## Diagnostics

| Signal | Move |
|---|---|
| auth/rate | check auth; ask login only for protected data; narrow/retry and mark incomplete |
| local/clone disabled | check `ENABLE_LOCAL`/`ENABLE_CLONE`/`.octocoderc`; use remote proof |
| tool disabled error | check the gate table above before assuming the tool does not exist |
| LSP unavailable | exact/AST fallback; check `lsp-server status <file>`; do not claim no usage |
| partial/warning/redaction | follow continuation; preserve warning; never reconstruct secrets |
| provider empty/approximate/stale | verify ref/path/filter, materialize or downgrade; force refresh only for freshness |

Exit codes: `0` ok · `2` input · `3` not-found · `4` auth · `5` tool · `7` rate-limit.

Validate: `node scripts/eval-research.mjs --case degraded-transport`.
