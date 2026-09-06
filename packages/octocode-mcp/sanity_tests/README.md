# Verification checks (manual, per tool)

One markdown checklist per MCP tool for **manual runtime verification** —
pagination, scheme, quality, and token-effectiveness — to run against the live
tool when changing the response/pagination layer or shipping a release.

Run a built tool directly from the repository root:

```bash
node packages/octocode/out/octocode.js tools <toolName> --queries '<queries-json>' --compact
```

## Automated coverage (NOT here — lives under `tests/`)

These run with `npx vitest run` and gate every change:

| Concern | Test |
|---|---|
| Per-tool pagination declarations and no-silent-loss language | `packages/octocode-mcp/tests/tools/all-tools.pagination-contract.test.ts` |
| Bulk-envelope numeric bounds (`responseChar*`, ≤5 queries) | `packages/octocode-mcp/tests/scheme/bulk_envelope_bounds.test.ts` |
| Catalog registration and bulk-schema existence | `packages/octocode-mcp/tests/tools/directToolCatalog.test.ts` |
| Shared pagination engine and bulk result continuations | `packages/octocode-tools-core/tests/utils/pagination.test.ts`, `bulk.pagination.test.ts` |
| GitHub file and history pagination axes | `packages/octocode-tools-core/tests/github/fileContentPagination.test.ts`, `historyPaginationAxes.test.ts` |
| npm and graph executable page unions | `packages/octocode-tools-core/tests/tools/package_search/pagination.test.ts`, `local_analyze_graph/localAnalyzeGraph.pagination.test.ts` |

The markdown here covers what a unit test can't cheaply assert: **live** cursor
walks to completion, real-result quality spot-checks, and concise-vs-basic token
comparisons.

## Tools

- [ghSearch](./ghSearch.md)
- [ghGetFileContent](./ghGetFileContent.md)
- [ghSearchHistory and ghGetHistoryItem](./githubHistory.md)
- [npmSearch](./npmSearch.md)
- [ghCloneRepo](./ghCloneRepo.md)
- [localSearch](./localSearch.md)
- [localGetFileContent](./localGetFileContent.md)
- [localAnalyzeGraph](../../../docs/OCTOCODE_TOOLS.md#localanalyzegraph)
- [lspGetSemantics](./lspGetSemantics.md)

## Pagination acceptance

Treat a bounded result as complete only when the response is terminal or every
typed `next.*` continuation has been executed. A numeric page or cursor without
a runnable tool query is a contract failure. For fixtures with more than one
page, verify that the union contains every expected item without duplicates.
