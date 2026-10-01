# Octocode interfaces

Load when invocation, availability, or recovery is unclear. Live `scheme` output is authoritative.

```bash
node packages/octocode/out/octocode.js scheme --compact          # or: npx -y octocode scheme --compact
node packages/octocode/out/octocode.js scheme localSearch        # variants with runnable examples
node packages/octocode/out/octocode.js scheme localSearch --view query --compact
```

Every query needs `goal` and `reasoning`. MCP takes `{ "queries": [query] }`; the CLI also accepts one query. On validation failure fix the named field.

## 16 public tools

| Evidence question | Tool |
|---|---|
| GitHub code / tree / repositories | `ghSearchCode` / `ghStructure` / `ghSearchRepo` |
| GitHub file / history search / history item / clone | `ghGetFileContent` / `ghSearchHistory` / `ghGetHistoryItem` / `ghCloneRepo` |
| Local text / layout / syntax / content / identity | `localSearch` / `structureSearch` / `astSearch` / `localFetch` / `lspSearch` |
| Topology / rewrite / packages / classification | `astTopology` / `astRewrite` / `artifactSearch` / `clasify` |

The default catalog contains 12 tools; `ghCloneRepo` (opt-in), `astTopology` and `astRewrite` (`OCTOCODE_BETA=1`), and `clasify` (`OCTOCODE_CLASSIFICATION_API`) complete it. `ghCloneRepo` and `astRewrite` are CLI-only. Check reach with `scheme`, `auth status`, and `lsp-server status <file>`; report an unavailable tool as a gap, not as empty.

## Output and recovery
- Per-row `status`: `error` is failure (follow its hint), `empty` is scoped absence; exit 0 does not mean every row succeeded. Check hoisted `shared` before calling a field missing.
- Follow `next.*` in rows and nested payloads, unchanged; coverage claims run every page.
- Result, match, content, and diagnostic pagination are independent; inspect limits even when `hasMore:false`. `responsePagination` windows only `content[].text`. A terminal limit calls for a narrower query, never an invented cursor.
- GitHub empty or unindexed is a provider blind spot: verify the path or materialize and search locally.

Exit codes: `0` ok · `1` empty · `2` input · `3` not-found · `4` auth · `5` execution · `6` partial · `7` rate-limit.

Next: a query shape → `tool-examples.md`; otherwise return to the route that sent you.
