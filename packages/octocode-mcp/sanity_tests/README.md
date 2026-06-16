# Sanity checks (manual, per tool)

One markdown checklist per MCP tool for **manual runtime sanity checks** —
pagination, scheme, quality, and token-effectiveness — to run against the live
tool when changing the response/pagination layer or shipping a release.

Run a tool out-of-band with the metering smoke wrapper:

```bash
node benchmark/github/scripts/call-tool.mjs <toolName> '<queries-json>'
```

## Automated coverage (NOT here — lives under `tests/`)

These run with `npx vitest run` and gate every change:

| Concern | Test |
|---|---|
| Per-tool pagination-knob declaration + no silent-loss language | `tests/tools/all-tools.pagination-contract.test.ts` |
| pagination-cursor uniformity | `tests/tools/all-tools.pagination.test.ts` |
| Bulk-envelope numeric bounds (`responseChar*`, ≤5 queries) | `tests/scheme/bulk_envelope_bounds.test.ts` |
| Catalog registration + bulk-schema existence | `tests/tools/directToolCatalog.test.ts` |
| Pagination engine (within-item slicing, oversized items) | `tests/utils/structuredPagination*.test.ts` |

The markdown here covers what a unit test can't cheaply assert: **live** cursor
walks to completion, real-result quality spot-checks, and concise-vs-basic token
comparisons.

## Tools

- [ghSearchCode](./ghSearchCode.md)
- [ghGetFileContent](./ghGetFileContent.md)
- [ghViewRepoStructure](./ghViewRepoStructure.md)
- [ghSearchRepos](./ghSearchRepos.md)
- [ghSearchPRs](./ghSearchPRs.md)
- [npmSearch](./npmSearch.md)
- [ghCloneRepo](./ghCloneRepo.md)
- [localSearchCode](./localSearchCode.md)
- [localViewStructure](./localViewStructure.md)
- [localFindFiles](./localFindFiles.md)
- [localGetFileContent](./localGetFileContent.md)
- [lspGotoDefinition](./lspGotoDefinition.md)
- [lspFindReferences](./lspFindReferences.md)
- [lspCallHierarchy](./lspCallHierarchy.md)

## Open pagination gaps (tracked)

- `lspFindReferences` — over-budget `locations` tail-dropped, not char-paged (issue #1 / task #6).
- `npmSearch` — no result-count page cursor for `searchLimit>1` (issue #2).
- `localFindFiles` — `maxFiles` caps discovery before pagination (issue #3).
- `lspCallHierarchy` — per-node `content` clipped to 500 chars (issue #4).
