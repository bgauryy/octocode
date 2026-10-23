Load when using Octocode research tools through MCP or the CLI. Source: `skills/common`; installed copies are generated.

## Choose the surface

Use the connected Octocode MCP tools when available. For a CLI-only capability or when MCP is unavailable, use the task's selected local CLI; otherwise use `npx -y octocode`. In the Octocode checkout, development checks use `node packages/octocode/out/octocode.js` after the relevant build. Do not silently switch versions to hide a failing call.

The research CLI, Chrome DevTools, and agents-communication are different executables. Their command envelopes are not interchangeable; browser and coordination skills own those packages' `/cli`, session, and mutation rules.

## Discover and invoke

Inspect live discovery before relying on a tool or field:

```bash
npx -y octocode schema
npx -y octocode schema localSearch --view query
npx -y octocode schema localSearch --view full
```

MCP and CLI accept the research-tool envelope `{ "queries": [query, ...] }`. Batch up to five independent rows of the same tool; wait for a result before constructing a dependent query. The CLI also accepts a bare query as one row. Preserve the documented envelope when transferring calls between surfaces.

```bash
npx -y octocode localFetch '{"queries":[{"path":"src/index.ts","ranges":["1-40"]}]}'
```

Use observed paths, lines, refs, and identifiers in real calls. For long requests, use the CLI's `--input <file>` with a request saved under the active skill's scratch directory. Live schemas own fields and limits; do not reconstruct a continuation from the compact query schema.

## Availability

Check `schema` availability, `auth status`, or `lsp-server status <file>` for the capability you need. Unavailable is a coverage gap, not an empty result. `ghCloneRepo` is CLI-only. The connected catalog determines what MCP exposes.

For credentials and settings, follow [configuration](configuration.md). GitHub uses `GH_TOKEN` or `GITHUB_TOKEN`. Inline `clasifyContext` needs `OCTOCODE_CLASSIFICATION_API` (or `classification.api`); public `clasify` additionally needs beta (`OCTOCODE_BETA=1`). Calls without classification perform ordinary retrieval; there is no separate selection switch.

`ghCloneRepo` needs persistent `OCTOCODE_STORAGE_MODE`; exploring the clone (`exploreClone`, local tools) needs `OCTOCODE_ENABLE_LOCAL`. `ENABLE_CLONE` is ignored. Inspect live configuration before selecting a capability. No setting change is implied by a missing capability.

## 14 public tools

The default catalog contains 12 tools over MCP and 13 in the CLI; the full schema inventory includes beta tools. Live discovery remains authoritative for the selected environment.

- GitHub: `ghSearchCode`, `ghStructure`, `ghSearchRepo`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo`.
- Local: `localSearch`, `structureSearch`, `astSearch`, `localFetch`, `lspSearch`.
- Other: `artifactSearch`, `clasify`.
- File graphs (CLI, not a tool): `octocode graph ingest <dir>`, then `graph query deps|dependents|path|cycles|issues|drift`.

## Read results and recover

- Inspect each row, not only the process exit or MCP envelope. A successful envelope can contain failed or partial queries; check hoisted `shared`/`root` fields before calling data missing.
- On validation failure, repair the named field. For a row error, use its returned repair or restart instruction; classification can place the error under `queries[].error`.
- Copy required `next.*` calls unchanged, retaining snapshots, selection, offsets, and scope. Finish `responsePagination` before a row continuation. Match, content, patch, and diagnostic pages can be independent even when one `hasMore` is false.
- A coverage claim must include every page in its stated view or name the unread part. A classification selection describes candidate decisions, not complete corpus coverage; keep selected pages, unjudged candidates, unknown inspection, and the original view distinct.
- `hints` are optional leads, not continuations or proof. Set `extras:["hints"]` on each `queries[]` row for next-step leads on successful rows (empty and failed rows show them); other opt-ins are `details`, `stats`, `receipts`, `echo`, and `diagnostics`. Copy a suggested tool's JSON unchanged when following it. `Approximate:` marks low confidence.
- For a terminal limit, narrow the query; never invent a cursor. For repeated empty results, change scope, filters, ref/index, or source. GitHub may be unindexed: verify the path or search a local checkout.
- CLI exit codes: `0` success, `1` empty, `2` input, `3` not found, `4` authentication, `5` execution, `7` rate limit. Exit `0` alone proves neither row success nor complete coverage.

For source and runtime claims, use the [evidence rules](evidence.md). Research-specific query recipes and classification recovery remain in `octocode-research`.
