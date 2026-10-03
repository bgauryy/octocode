# Verification checks (manual, per tool)

One markdown checklist per MCP tool for **manual runtime verification** —
pagination, scheme, quality, and token-effectiveness — to run against the live
tool when changing the response/pagination layer or shipping a release.

Run a built tool directly from the repository root:

```bash
node packages/octocode/out/octocode.js <toolName> '<query-or-queries-envelope-json>'
```

## Automated coverage (NOT here — lives under `tests/`)

These run with `npx vitest run` and gate every change:

| Concern | Test |
|---|---|
| Per-tool pagination declarations and no-silent-loss language | `packages/octocode-mcp/tests/tools/all-tools.pagination-contract.test.ts` |
| Bulk-envelope numeric bounds (`responseChar*`, ≤5 queries) | `packages/octocode-mcp/tests/scheme/bulk_envelope_bounds.test.ts` |
| Native catalog registration and execution boundary | `packages/octocode-mcp/tests/native/node-boundary.mjs` |
| Shared pagination engine and result continuations | `packages/octocode-native/crates/runtime/src/response/tests.rs`, `packages/octocode-native/crates/runtime/src/runtime/response.rs` |
| GitHub file and history pagination axes | `packages/octocode-native/crates/runtime/src/tools/gh_get_file_content`, `packages/octocode-native/crates/runtime/src/tools/gh_get_history_item` |
| Package and topology executable page unions | `packages/octocode-native/crates/runtime/src/tools/artifact_search`, `packages/octocode-native/crates/runtime/src/tools/ast_graph` |

The markdown here covers what a unit test can't cheaply assert: **live** cursor
walks to completion, real-result quality spot-checks, and concise-vs-basic token
comparisons.

## Tools

- [ghSearchRepo, ghSearchCode, ghStructure](./ghSearchTools.md)
- [ghGetFileContent](./ghGetFileContent.md)
- [ghSearchHistory and ghGetHistoryItem](./githubHistory.md)
- [artifactSearch](./artifactSearch.md)
- [ghCloneRepo](./ghCloneRepo.md)
- [localSearch](./localSearch.md)
- [localFetch](./localFetch.md)
- [astSearch](../../../docs/OCTOCODE_TOOLS.md#astsearch)
- [lspSearch](./lspSearch.md)

## Pagination acceptance

Treat a bounded result as complete only when the response is terminal or every
typed `next.*` continuation has been executed. A numeric page or cursor without
a runnable tool query is a contract failure. For fixtures with more than one
page, verify that the union contains every expected item without duplicates.
