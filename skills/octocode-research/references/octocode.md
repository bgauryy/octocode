# Octocode Interfaces

Load for transport, tool choice, authentication, diagnostics, or CLI syntax. `algorithm.md` owns evidence/routing.

## Interfaces
| Interface | Use |
|---|---|
| MCP tools | preferred when exposed; typed calls without shell hop |
| `npx octocode` | MCP missing or schema/explain/dry-run introspection needed |

Read the tool schema immediately before raw calls and `search --scheme --compact` before hand-written OQL. If neither interface exists, continue with stated degraded confidence or ask to install/auth only when protected GitHub data is essential.

## Tool Families
| Need | MCP | CLI search lane |
|---|---|---|
| local search/read/tree/find | `localSearchCode`, `localGetFileContent`, `localViewStructure`, `localFindFiles` | discovery/content/tree/path |
| semantics | `lspGetSemantics` | `--op documentSymbols|definition|references|callers|callees|hover` |
| GitHub code/read/tree/repos | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos` | repo/ref/path discovery/content/tree |
| history | `ghHistoryResearch` | `--target pullRequests|commits` |
| packages | `npmSearch` | `--target packages` |
| federated/diff/graph | `oqlSearch` | `--query <json>` / diff/research/graph targets |
| materialize | `ghCloneRepo` or directory fetch | clone/cache fetch; CLI clones by default, MCP server gates clone behind `ENABLE_CLONE` |


Batch up to five independent queries per tool call. Materialize when remote providers cannot prove AST/LSP/negative/many-file predicates.

## CLI Probes
```bash
npx octocode --help
npx octocode auth status --json
npx octocode context
npx octocode tools <name> --scheme
npx octocode search --scheme --compact
npx octocode lsp-server status <file>
```

Use `--json` for automation and `--compact` for orientation. Removed aliases (`grep`, `cat`, `ls`, `find`, `lsp`, `pr`, `pkg`, `repo`, `diff`) map to `search` lanes.

## Diagnostics
| Signal | Move |
|---|---|
| auth/rate | check auth; ask login only for protected data; narrow/retry and mark incomplete |
| local/clone disabled | check `ENABLE_LOCAL`/`.octocoderc`; clone needs `ENABLE_CLONE`; use remote proof |
| LSP unavailable | exact/AST fallback; check server status; do not claim no usage |
| partial/warning/redaction | follow continuation; preserve warning; never reconstruct secrets |
| provider empty/approximate/stale | verify ref/path/filter, materialize or downgrade; force refresh only for freshness |
