# Octocode Interfaces

Load for transport, tool choice, authentication, diagnostics, or CLI syntax. `algorithm.md` owns evidence/routing.

## Interfaces
| Interface | Use |
|---|---|
| MCP tools | preferred when exposed; typed calls without shell hop |
| `npx octocode tools <name>` | MCP missing; same tool catalog via raw CLI calls |

Read the tool schema (`--scheme`) immediately before any unfamiliar raw call. If neither interface exists, continue with stated degraded confidence or ask to install/auth only when protected GitHub data is essential.

## Active Tool Families (15)
| Need | MCP / CLI tool |
|---|---|
| local orient/find/search/read | `localViewStructure`, `localFindFiles`, `localSearchCode`, `localGetFileContent` |
| dead-code candidates | `localFindDeadCode` for repo-wide reachability clusters; prove deletes with LSP/search/tests |
| semantics | `lspGetSemantics` (`documentSymbols`, `definition`, `references`, `callers`, `callees`, `hover`, diagnostics, type/call hierarchy) |
| GitHub code/read/tree/repos | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos` |
| GitHub PRs/issues/commits | `ghSearchPullRequests`, `ghSearchIssues`, `ghSearchCommits` |
| materialize remote | `ghCloneRepo`; CLI clone is on by default, MCP clone needs `ENABLE_CLONE` |
| packages | `npmSearch` |

Batch up to five independent queries per tool call. Materialize when remote providers cannot prove AST/LSP/negative/many-file predicates.

## CLI Probes
```bash
npx octocode --help
npx octocode auth status --json
npx octocode context --minimal
npx octocode context
npx octocode tools --json
npx octocode tools <name> --scheme --json --compact
npx octocode tools <name> --queries '<json>' --compact
npx octocode lsp-server status <file>
```

Use `--compact` for agent output, `--pretty` with compact JSON for humans, and `context --minimal` under tight budgets. There is no `search` command or aliases (`grep`, `cat`, `ls`, `find`, `lsp`, `pr`, `pkg`, `repo`, `diff`) — every research need routes through `tools <name>`.

## Diagnostics
| Signal | Move |
|---|---|
| auth/rate | check auth; ask login only for protected data; narrow/retry and mark incomplete |
| local/clone disabled | check `ENABLE_LOCAL`/`.octocoderc`; clone needs `ENABLE_CLONE`; use remote proof |
| LSP unavailable | exact/AST fallback; check server status; do not claim no usage |
| partial/warning/redaction | follow continuation; preserve warning; never reconstruct secrets |
| provider empty/approximate/stale | verify ref/path/filter, materialize or downgrade; force refresh only for freshness |
