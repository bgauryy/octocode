# Octocode interfaces

Load when tool selection, transport, availability, or recovery is unclear. The live catalog and schemas are authoritative; this file is an invocation guide, not a second schema.

## Discover and invoke
Prefer exposed Octocode MCP tools with current public contracts. If unavailable, use the built checkout CLI; an installed skill can use `npx -y octocode`. These share tools-core runners. Do not substitute a legacy tool with different fields.

```bash
node packages/octocode/out/octocode.js tools --json --compact
node packages/octocode/out/octocode.js tools localSearch --scheme --json --compact
node packages/octocode/out/octocode.js tools localSearch --queries '{"operation":"text","path":"/ABS/repo/src","searchText":"needle","maxFiles":10}' --compact
```

Use `context --minimal` only for protocol orientation. Inspect an unfamiliar schema once, including relations and operation variants; reuse it until the tool/version changes. Use full schema JSON when compact fields do not resolve a condition. Explicit commands above work in Bash and zsh without splitting a command stored in a scalar.

## 10 public tools
| Need | Tool |
|---|---|
| Local text, AST, files, tree | `localSearch` with the corresponding operation |
| Exact local content | `localGetFileContent` |
| File dependencies, dependents, paths, cycles, reachability, dead-code candidates | `localAnalyzeGraph` |
| Symbol identity, references, call/type relationships, capabilities | `lspGetSemantics` |
| GitHub code / tree / repositories | `ghSearch` |
| Exact remote file or directory materialization | `ghGetFileContent` |
| PR, issue, commit discovery | `ghSearchHistory` |
| PR, issue, commit, comparison detail | `ghGetHistoryItem` |
| Cached shallow checkout | `ghCloneRepo` |
| Exact npm metadata or keyword discovery | `npmSearch` |

The default catalog contains 9 tools; the full discovery catalog includes opt-in `ghCloneRepo`. Local access, clone, storage, and tool filters determine availability. Check the live catalog before using a follow-up. Check auth only when needed. If the current interface is unavailable, state the fallback and its coverage; do not present an unsupported call as an empty result.

## Output and recovery
- CLI default is YAML; `--compact` gives structured data; `--json` gives the full MCP-style envelope. MCP returns text plus structured data. Inspect per-row status: error is failure, empty is scoped absence, and exit 0 alone does not establish success for every row.
- Compact output can hoist repeated values into top-level `shared`. Inspect those values before declaring a row field missing, and retain shared identity when interpreting its files or directories.
- Follow executable `next.*` calls relevant to the claim in row data and nested payloads. Each supplies a tool and query; a label is not a tool name. Optional follow-up suggestions are not mandatory workflow steps.
- Result, match, metadata, content, and diagnostic pagination can be independent. Inspect partial/limit state even when `hasMore:false`; a limit may have another continuation or be terminal.
- `responsePagination` windows `content[].text`; structured content can remain complete. Avoid fetching the same evidence again solely to recover an envelope text window.
- Copy a continuation query unchanged before adapting a new search. For coverage claims, execute all relevant pages and check their union. For a lookup, stop at sufficient evidence and state material limits.
- An incomplete response never proves absence. A terminal limit calls for a narrower scope/query or an explicit gap, not an invented cursor. Preserve redaction and never reconstruct secrets.
- Batch independent probes within the interface's current limit; sequence dependent probes. Respect provider rate-limit/retry guidance rather than repeatedly issuing the same failing request.

Exit codes: `0` command completed · `2` input · `3` not-found · `4` auth · `5` tool · `7` rate-limit. Inspect row errors as well.

Next: route local evidence with `references/workflow-local.md`, remote evidence with `references/workflow-external.md`, or materialization with `references/workflow-combination.md`; examples live in `references/tool-examples.md`.
