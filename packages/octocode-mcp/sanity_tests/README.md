# Verification checks (manual, per tool)

One markdown checklist per MCP tool for **manual runtime verification** —
pagination, scheme, quality, and token-effectiveness — to run against the live
tool when changing the response/pagination layer or shipping a release.

Run a built tool directly from the repository root:

```bash
node packages/octocode/out/octocode.js <toolName> '{"queries":[…]}'
```

## Automated coverage (NOT here — lives under `tests/`)

These gate every change:

| Concern | Test |
|---|---|
| Per-tool pagination declarations, schema bounds and no-silent-loss wording | core schema tests (`octocode-core`, `src/__tests__/`) |
| Launcher start-up, delegation and failure exits | `packages/octocode-mcp/tests/launcher/` |
| Native response pagination over stdio (`yarn test:boundary`) | `packages/octocode-benchmark/local/e2e/harness/transport.mjs` |
| Shared pagination engine and result continuations | `packages/octocode-native/crates/runtime/src/response/row_pages`, `packages/octocode-native/crates/runtime/src/response/rows.rs` |
| GitHub file and history pagination axes | `packages/octocode-native/crates/runtime/src/tools/gh_get_file_content`, `packages/octocode-native/crates/runtime/src/tools/gh_get_history_item` |
| Package and topology executable page unions | `packages/octocode-native/crates/runtime/src/tools/artifact_search`, `packages/octocode-native/crates/runtime/src/tools/ast_graph` |

The markdown here covers what a unit test can't cheaply assert: **live** cursor
walks to completion, real-result quality spot-checks, and default-vs-`fields`
row-size comparisons.

## Tools

- [ghSearchRepo, ghSearchCode, ghStructure](./ghSearchTools.md)
- [ghGetFileContent](./ghGetFileContent.md)
- [ghSearchHistory and ghGetHistoryItem](./githubHistory.md)
- [artifactSearch](./artifactSearch.md)
- [localSearch](./localSearch.md)
- [localFetch](./localFetch.md)
- [astSearch](../../../docs/OCTOCODE_TOOLS.md#astsearch)
- [lspSearch](./lspSearch.md)

`ghCloneRepo` is CLI-only; MCP never registers it.

## Pagination acceptance

Treat a bounded result as complete only when the response is terminal or every
typed `next.*` continuation has been executed. A numeric page or cursor without
a runnable tool query is a contract failure. For fixtures with more than one
page, verify that the union contains every expected item without duplicates.
