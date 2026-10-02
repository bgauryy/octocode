# Octocode interfaces

Load when invocation, availability, or recovery is unclear. Live `scheme` output is authoritative.

```bash
node packages/octocode/out/octocode.js scheme --compact          # or: npx -y octocode scheme --compact
node packages/octocode/out/octocode.js scheme localSearch        # variants with runnable examples
node packages/octocode/out/octocode.js scheme localSearch --view query --compact
```

Every query needs `goal` and `reasoning`; `debug:true` adds diagnostics and receipts. MCP takes `{ "queries": [query] }` (≤5 rows); the CLI also accepts one bare query. On validation failure fix the named field.

## Public tools

| Evidence question | Tool |
|---|---|
| GitHub code / tree / repositories | `ghSearchCode` / `ghStructure` / `ghSearchRepo` |
| GitHub file / history search / history item / clone | `ghGetFileContent` / `ghSearchHistory` / `ghGetHistoryItem` / `ghCloneRepo` |
| Local text / layout / syntax / content / identity | `localSearch` / `structureSearch` / `astSearch` / `localFetch` / `lspSearch` |
| Topology / rewrite / packages / classification | `astTopology` / `astRewrite` / `artifactSearch` / `clasify` |

`ghCloneRepo` and `astRewrite` are CLI-only; MCP never lists them. `ghCloneRepo` needs persistent storage (the default). `astTopology` and `astRewrite` need `OCTOCODE_BETA=true`; `clasify` needs a classification key (`OCTOCODE_CLASSIFICATION_API`). Check reach with `scheme` (`availability`), `auth status`, and `lsp-server status <file>`; report an unavailable tool as a gap, not as empty.

## Output and recovery
- Per-row `status`: `error` is failure (follow its hint or `next.repair`/`next.restart`), `empty` is scoped absence; exit 0 does not mean every row succeeded. Check hoisted `shared` before calling a field missing.
- Follow `next.*` in rows and nested payloads, unchanged; they carry fields schemas omit. Coverage claims run every page.
- Result, match, content, patch, and diagnostic pagination are independent; inspect limits even when `hasMore:false`. `responsePagination` pages an oversized batch response (`responseScope`: text, structured, or rows); finish it before following a row continuation. A terminal limit calls for a narrower query, never an invented cursor.
- GitHub empty or unindexed is a provider blind spot: verify the path or materialize and search locally.

Exit codes: `0` ok · `1` empty · `2` input (any rejected row) · `3` not-found · `4` auth · `5` execution · `6` partial (run `next`) · `7` rate-limit.

Next: a query shape → `tool-examples.md`; otherwise return to the route that sent you.
