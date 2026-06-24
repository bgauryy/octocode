# Advanced MCP Tool Verification

This playbook verifies that every Octocode MCP tool works as a research tool, not just as a callable function. Use it before releases, after schema changes, after response-shape changes, and after changes to pagination, hints, security, LSP behavior, provider mapping.

## Source Of Truth

The active MCP tool catalog is defined in [packages/octocode-tools-core/src/tools/toolConfig.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/tools/toolConfig.ts). Local schema helpers live in [packages/octocode-tools-core/src/scheme/fields.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/scheme/fields.ts); each GitHub/package/LSP tool owns its independent `scheme.ts` beside the tool implementation, for example [packages/octocode-tools-core/src/tools/github_search_pull_requests/scheme.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/tools/github_search_pull_requests/scheme.ts) and [packages/octocode-tools-core/src/tools/lsp/semantic_content/scheme.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/tools/lsp/semantic_content/scheme.ts).

Response behavior is shared through [packages/octocode-tools-core/src/utils/response/bulk.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/utils/response/bulk.ts), [packages/octocode-tools-core/src/utils/response/structuredPagination.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/utils/response/structuredPagination.ts), [packages/octocode-tools-core/src/utils/pagination/hints.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/utils/pagination/hints.ts), and [packages/octocode-tools-core/src/scheme/responseEnvelope.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/scheme/responseEnvelope.ts).

Existing contract tests that this playbook extends include [packages/octocode-mcp/tests/tools/all-tools.pagination.test.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/tests/tools/all-tools.pagination.test.ts), [packages/octocode-mcp/tests/tools/hints/all-tools.lean-contract.test.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/tests/tools/hints/all-tools.lean-contract.test.ts), [packages/octocode-mcp/tests/tools/response_structure.test.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/tests/tools/response_structure.test.ts), and [packages/octocode-mcp/tests/tools/executionBoundaries.flows.test.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/tests/tools/executionBoundaries.flows.test.ts).

## Verification Goals

Every tool must pass the same top-level contract:

| Area | Required checks |
| --- | --- |
| Registration | Tool is present in `ALL_TOOLS`, has a direct execution definition, has MCP input and output schema, and registers with the expected security wrapper. |
| Bulk envelope | `queries` accepts 1 to 5 items, preserves order, rejects duplicate `id`, isolates per-query errors, and does not let one bad query block siblings. |
| Output shape | Responses are structured YAML/JSON `results[]` (single source of truth in `content[0].text` and `structuredContent`). Lean hoists apply: `base` relativizes absolute `path`/`uri`, `shared` collapses constants identical across leaves (identity keys `owner`/`repo`/`name`/`id` are never hoisted). |
| Pagination | Native page fields, query-level `charOffset`/`charLength`, and top-level `responseCharOffset`/`responseCharLength` work independently and together. Pagination hints appear only when `hasMore=true`. |
| Hints | Tool `hints.ts` files expose only `empty` and `error`. Empty hints are conditional and filter-aware. Error hints classify the failure and stay short. Success path hints are limited to data-bearing signals such as pagination or warnings. |
| Empty results | Successful no-match responses are not errors. They must include a clear empty signal, preserve query identity, and provide recovery hints only when the query context makes a concrete next step possible. |
| Errors | Provider, validation, path, auth, rate-limit, timeout, LSP-unavailable, and command failures return structured errors with recovery context and without leaking secrets. |
| Evidence | Tools that can report evidence should set `evidence.kind`, `answerReady`, `confidence`, and `complete` consistently. Aggregated evidence should downgrade confidence and completeness when any query is partial or fallback-based. |
| Security | Local tools respect path validation and command allow-lists. Remote tools sanitize errors and redact secrets. Clone and directory fetch do not write outside the intended cache or checkout root. |

## Global Scenario Matrix

Run these scenarios for every tool before adding tool-specific edge cases:

| Scenario | What to verify |
| --- | --- |
| Minimal valid query | Tool accepts the smallest useful input and returns non-error `CallToolResult`. |
| Full valid query | Every public schema field is accepted, survives mapping, and affects execution as documented. |
| Unknown fields | MCP schema strips or rejects extra fields according to the overlay. Hidden fields must not reach execution from MCP calls. |
| Invalid field type | The call fails validation cleanly, with the invalid path named. |
| Invalid enum | The response names the allowed values or a usable correction. |
| Empty array | Bulk `queries:[]` is rejected by schema or returns an error before execution. |
| Six queries | Bulk schema rejects more than five queries. |
| Mixed success, empty, error | One response contains all three states, preserves input order, dedupes top-level hints, and sets `isError=false` unless all entries failed. |
| All errors | Tool sets `isError=true`, preserves one error per query, and includes no misleading success evidence. |
| Lean output | `base` relativizes absolute `path`/`uri` against a common root; `shared` hoists scalar fields identical across all leaves; both are lossless and reconstructable, and identity keys (`owner`/`repo`/`name`/`id`) stay per-item. |
| Query pagination | `charLength` creates query-level pagination metadata and a next cursor; re-calling with the cursor continues without duplicating content. |
| Response pagination | `responseCharLength` pages the outer multi-query response and leaves native per-query pagination intact. |
| Final page | No pagination hint appears when `hasMore=false`. |
| Auth unavailable | Remote tools return actionable auth errors without exposing token names beyond approved env var guidance. |
| Rate limit | Remote tools preserve rate-limit reset/retry metadata when provider data includes it. |
| Secret redaction | Responses and errors redact tokens, keys, and credentials from content, paths, and provider errors. |

## Tool Checklist

### `ghSearchCode`

Primary code: [src/tools/github_search_code/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/github_search_code). Schema: `GitHubCodeSearchQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `keywordsToSearch`, owner/repo scoping, path/name/extension filters, match mode, `page`, `limit`, `charOffset`, `charLength`, and bulk response pagination. |
| Implementation | Provider query is built with exact filters, default branch context is preserved for single-repo hits, results are grouped by `owner/repo`, and match values are sanitized. |
| Pagination | Upstream provider pagination, per-query `outputPagination`, and top-level `responsePagination` can all appear without overwriting each other. |
| Empty | No-match queries appear in `emptyQueries` with query id and concrete recovery hints. Empty groups are not silently dropped in mixed bulk calls. |
| Warnings | `match-value-truncated` includes group id, path, full length, truncation point, and recovery. |
| Research quality | A hit must include enough path and snippet evidence to justify a follow-up `ghGetFileContent` call. Each result must carry `owner`, `repo`, and per-match `path` and `value`. |

### `ghGetFileContent`

Primary code: [src/tools/github_fetch_content/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/github_fetch_content). Schema: `FileContentQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify owner, repo, path, branch/ref, file versus directory mode, `fullContent`, `matchString`, `startLine`/`endLine`, `matchStringContextLines`, `charOffset`, `charLength`. |
| Mutex | `fullContent`, `matchString`, and line ranges are mutually exclusive. Invalid combinations produce per-query errors in bulk calls. |
| File mode | Line ranges are accurate, `totalLines` is correct, branch fallback/resolution is reported, large files page by character cursor, and partial content sets `isPartial=true`. |
| Directory mode | Requires local and clone support. Returns `localPath`, file count, total size, cached state, and resolved branch. Follow-up local tools must work against `localPath`. |
| Empty | `matchString` with no matches returns empty, not provider error, and does not fabricate content. |
| Warnings | `content-truncated` includes group id, path, full content length, truncation point, and recovery. |
| Research quality | File content should be answer-ready when the query requested a line range or match. Directory mode should be treated as setup evidence for local and LSP follow-ups. |

### `ghViewRepoStructure`

Primary code: [src/tools/github_view_repo_structure/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/github_view_repo_structure). Schema: `GitHubViewRepoStructureQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify owner, repo, branch/ref, path, depth, folder/file limits, `entriesPerPage`, `entryPageNumber`. |
| Implementation | Tree keys are stable, files and folders are separated, branch fallback details are preserved, and provider errors retain owner/repo/path context. |
| Pagination | Entry pagination uses `entryPageNumber=N+1` hints only while more entries exist. Page counts must match total entries, not only visible folders. |
| Empty | Empty repository paths or filters return empty with precise path/branch context. Missing paths return error. |
| Research quality | Structure should support choosing the next content or search query without guessing. Entries must expose `path` and `type`. |

### `ghSearchRepos`

Primary code: [src/tools/github_search_repos/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/github_search_repos). Schema: `GitHubReposSearchSingleQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify query text, topics, language, owner/user/org qualifiers, stars/forks/created/pushed filters, sort/order, `page`, `limit`. |
| Implementation | Keyword and topic searches merge deterministically, duplicate repos collapse, partial variant failures are reported, and language maps to GitHub's `language:` qualifier. |
| Pagination | Pagination is preserved when exactly one provider result set succeeds. It is omitted or explained when multiple result sets are merged. |
| Empty | Empty results name active filters so the agent can broaden language, topics, or pushed-date constraints. |
| Research quality | Results must include repository identity, description, URL, default branch, pushed date, language, stars, topics, and enough metadata to choose follow-up search or structure calls. |

### `ghHistoryResearch`

Primary code: [src/tools/github_search_pull_requests/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/github_search_pull_requests). Schema: `GitHubPullRequestSearchQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify owner/repo, query, PR number, author, state `open|closed|merged`, `matchScope`, sort `created|updated|best-match`, page, limit, diff/content options, `charOffset`, `charLength`. |
| Implementation | `state:"merged"` maps to merged search, approximate-title archaeology works with `matchScope:["title"]` and `sort:"best-match"`, and PR-number fetch returns full body when requested. |
| Pagination | Provider page metadata and output-size pagination coexist. Large diffs or many file changes should emit a targeted follow-up hint instead of dumping unusable data. |
| Empty | Empty responses name state, owner/repo, match scope, and query terms when present. |
| Research quality | A PR result should expose title, state, author, timestamps, branches, SHAs when present, changed-file counts, comments/diffs when requested, and enough evidence to explain why the PR matters. |

### `npmSearch`

Primary code: [src/tools/package_search/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/package_search). Schema: `NpmSearchQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `name`, default `ecosystem:"npm"`, explicit non-npm ecosystem rejection, `limit`, `searchLimit`, metadata fetch options. |
| Implementation | `limit` maps to `searchLimit`, npm registry metadata is normalized, repository URLs are parsed into owner/repo when possible, deprecated packages add warning context, and package-not-found is empty. |
| Pagination | Limit controls search breadth. Top-level `responseCharLength` still pages large metadata responses. |
| Empty | Empty search returns package-specific recovery without pretending the package exists. |
| Research quality | Results must include package identity, version, description, repository URL or owner/repo, homepage, weekly downloads if fetched, license, keywords, and freshness metadata when available. |

### `ghCloneRepo`

Primary code: [src/tools/github_clone_repo/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/github_clone_repo). Schema: `CloneRepoQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify owner, repo, branch/ref, path/subtree options, depth or sparse checkout options if exposed, and bulk ids. Clone is side-effecting and has no verbosity field. |
| Implementation | Requires clone/local enablement, resolves branch fallback, reuses cache when valid, refreshes expired cache, and returns a safe `localPath`. |
| Pagination | No native pagination is expected, but bulk response pagination must still work. |
| Empty | Not applicable. A missing repository, branch, or path is an error with recovery context. |
| Data management | Verify cache TTL, cache invalidation, concurrent clone locking, cleanup on failed clone, and no writes outside the tmp materialization roots. |
| Research quality | Returned `localPath` should be immediately usable by `localSearchCode`, `localViewStructure`, `localGetFileContent`, and LSP tools. |

### `localSearchCode`

Primary code: [src/tools/local_ripgrep/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/local_ripgrep). Schema: `RipgrepQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `path`, `pattern`, search mode, `fixedString`, `perlRegex`, `wholeWord`, `caseSensitive`, type/include/exclude/excludeDir, hidden/noIgnore, `filesOnly`, `filesWithoutMatch`, `count`, `countMatches`, `contextLines`, `matchContentLength`, `filesPerPage`, `matchesPerPage`, `filePageNumber`, `charOffset`, `charLength`. |
| Hidden fields | MCP schema must not expose hidden performance or diagnostic knobs such as threads, multiline, binary, encoding, sort, debug, passthru, or symlink following. |
| Mutex | `filesOnly` conflicts with `filesWithoutMatch`; `fixedString` conflicts with `perlRegex`. Violations become per-query errors. |
| Implementation | Runs ripgrep in-process through the native engine. No external `rg` binary and no grep fallback. Invalid regex, path errors, and no-permission paths are structured errors. |
| Pagination | File and match pagination work independently. `line` values are stable 1-indexed `lineHint` inputs for LSP tools. |
| Empty | Empty hints name active filters such as type, include, exclude, excludeDir, or path. No-filter empty stays silent. |
| Research quality | Results must include file path, match count, line, column, snippet value, and enough context to drive precise `lspGetSemantics` queries such as `type="definition"`, `type="references"`, `type="callers"`, or `type="callees"`. |

### `localViewStructure`

Primary code: [src/tools/local_view_structure/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/local_view_structure). Schema: `ViewStructureQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `path`, pattern filters, `extensions`, exclude filters, `depth`, `limit`, `entriesPerPage`, `entryPageNumber`, `charOffset`, `charLength`. |
| Hidden fields | `extension` singular and unbounded `recursive` are not exposed in MCP. Use `extensions` and bounded `depth` instead. |
| Implementation | Directory walk respects path validation, depth cap, ignored directories, sorting, and entry typing. Symlink and permission cases are explicit. |
| Pagination | Entry pagination uses stable ordering so page 2 continues page 1 without duplicates or missed entries. |
| Empty | Empty directories are empty, not errors. Missing paths are errors. Filtered empties name the active filter. |
| Research quality | Entries should identify name, path, type, size, modified time, and depth so the next search or content call can be scoped. |

### `localFindFiles`

Primary code: [src/tools/local_find_files/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/local_find_files). Schema: `FindFilesQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `path`, name/pattern filters, file type, size filters, modified/accessed/created filters, permissions if exposed, `limit`, `filesPerPage`, `filePageNumber`, `charOffset`, `charLength`. |
| Implementation | Uses the safe file-discovery path, respects allowed paths, handles large trees without unbounded output, and returns stable metadata. |
| Pagination | File pagination and char pagination both work. Cap notices must not replace next-page cursors. |
| Empty | Empty hints quote active filters such as `name`, `modifiedWithin`, or `sizeGreater`. No-filter empty stays silent. |
| Research quality | Results should support targeted follow-ups by path, type, size, permissions, and timestamps. |

### `localGetFileContent`

Primary code: [src/tools/local_fetch_content/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/local_fetch_content). Schema: `FetchContentQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `path`, `fullContent`, `matchString`, `startLine`, `endLine`, `matchStringContextLines`, `charOffset`, `charLength`. |
| Mutex | `fullContent`, `matchString`, and line ranges are mutually exclusive, with per-query errors inside bulk calls. |
| Implementation | Handles UTF-8 files, large files, minified content, binary/unreadable files, no trailing newline, and out-of-range line requests. |
| Pagination | Character pagination continues exact content without overlap. Match extraction plus pagination should preserve `matchRanges`. |
| Empty | A missing `matchString` result returns empty with no fake content. Missing file and invalid path are errors. |
| Research quality | Returned content must include path, line range, total lines, `isPartial`, and enough source text to cite or reason from. Partial line-range reads emit a `startLine=N` continuation hint. |

### `lspGetSemantics`

Primary code: [src/tools/lsp/semantic_content/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-mcp/src/tools/lsp/semantic_content). Schema: `LspGetSemanticsQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `uri`/`filePath`, `type`, `symbolName`, `lineHint`, `orderHint`, `includeDeclaration`, `groupByFile`, `depth`, `page`, `contextLines`, and output pagination. |
| Implementation | Requires a `lineHint` for symbol-anchored types, resolves exact code occurrences while ignoring string/comment hits, uses pooled LSP clients, and reports capability gaps explicitly. |
| Pagination | Large semantic payloads page without losing target identity. |
| Empty | Symbol-not-found, unsupported capability, and LSP-unavailable paths are explicit. |
| Semantic quality | Definition/reference/call/hover/symbol outputs identify URI, range, symbol identity, completeness, and static-vs-dynamic limits where applicable. |

## Cross-Tool Research Quality Suites

These suites verify that tools compose into reliable research workflows.

| Suite | Steps | Pass criteria |
| --- | --- | --- |
| Local semantic navigation | `localSearchCode` for a symbol, then `lspGetSemantics` with `type="definition"`, `type="references"`, and `type="callers"`/`type="callees"` using returned line hints. | LSP tools resolve the same symbol, references include the definition when requested, call direction is correct, and fallback mode is explicit if used. |
| Remote to local deep dive | `ghSearchCode` or `ghSearchRepos`, then `ghCloneRepo`, then local search and LSP tools on `localPath`. | Remote identity, branch, clone path, and local path all line up. No result requires guessing a path or branch. |
| Structure to content | `ghViewRepoStructure` or `localViewStructure`, then content fetch on selected entries. | Paths emitted by structure tools are directly accepted by content tools. Empty directories and missing files are differentiated. |
| Package provenance | `npmSearch`, then `ghViewRepoStructure` or `ghSearchCode` on parsed repo owner/name. | Package repo metadata is normalized enough to drive GitHub tools, and missing/ambiguous repo URLs are represented as missing evidence. |
| PR archaeology | `ghHistoryResearch` with title search, then PR number fetch and file-content or code search follow-up. | Approximate search finds candidates; PR-number path returns full body/diff data requested; large diffs guide targeted follow-up. |
| Empty-result recovery | Run over-constrained queries across GitHub, local, and LSP tools. | Each tool either stays silent when no concrete advice exists or names exactly which filter to relax. |
| Pagination chain | Force small `limit`, `entriesPerPage`, `filesPerPage`, `matchesPerPage`, `referencesPerPage`, `callsPerPage`, `charLength`, and `responseCharLength`. | Every next cursor continues the same result set without duplicates, missing entries, or final-page chatter. |
| Verbosity chain | Run the same broad task with `concise`, drill down with `compact`, and confirm with `basic`. | `concise` is tiny and lossy, `compact` is enough to choose a target, and `basic` provides citeable evidence. |

## Data Management And Reliability

Verify these behaviors whenever touching cache, clone, local files, pagination, or response shaping:

| Area | Checks |
| --- | --- |
| Clone cache | TTL is honored, branch/ref changes do not return stale checkout content, concurrent clone requests do not corrupt cache, and failed clones clean up partial directories. |
| Local reads | Large files, binary files, permission-denied files, hidden files, symlinks, and paths outside allowed roots are handled explicitly. |
| Provider cache | Cached HTTP responses do not hide auth/rate-limit failures and do not merge responses from different owners, repos, branches, pages, or query filters. |
| Response sizes | Raw source chars, transformed chars, output chars, and char-savings stats remain best-effort and never break responses. |
| Sanitization | File content, provider errors, clone paths, and local command output pass through secret redaction. |
| Concurrency | Bulk concurrency preserves input order, isolates timeouts, and sets `isError` only when every query failed. |

## Semantic Improvement Backlog

Use this list to turn verification failures into focused improvements:

| Topic | Improvement target |
| --- | --- |
| Schema visibility | Add tests that snapshot MCP JSON schemas for hidden fields, defaults, enum values, and max bounds. |
| Evidence quality | Enforce evidence metadata for every tool family and downgrade confidence for fallback, partial, or paginated results. |
| Empty states | Add per-tool tests for over-constrained filters so empty hints stay specific and do not become generic workflow prose. |
| Pagination | Add end-to-end cursor replay tests for every pagination dimension, not only generator unit tests. |
| LSP semantics | Add fixtures covering same-symbol multiple occurrences, import/export chains, dynamic imports, generated files, and non-TypeScript language fallbacks. |
| Remote semantics | Add provider-mapper tests for branch fallback, merged PR state, repository language filtering, topic/query merging, and package repo URL normalization. |
| Lean-output contract | Add tests for `base`/`shared` hoisting across tools with warnings, empty queries, and mixed results — including identity-key exclusion and exact reconstruction. |
| Direct CLI parity | Verify direct CLI schema help, auto-filled metadata, JSON/YAML output, and direct execution match MCP behavior. |

## Suggested Command Sets

From `packages/octocode-mcp/`, run the focused suites first:

```bash
yarn test tests/tools/all-tools.pagination.test.ts
yarn test tests/tools/hints/all-tools.lean-contract.test.ts
yarn test tests/tools/response_structure.test.ts
yarn test tests/tools/executionBoundaries.flows.test.ts
yarn test tests/tools/local_mcp_response_pagination.test.ts
yarn test tests/tools/remote-tools.contract.test.ts
```

Then run broader tool coverage:

```bash
yarn test tests/tools
yarn lint
yarn typecheck
yarn test
```

For dead-code and dependency drift, run from the repository root:

```bash
npx knip
```

## Release Gate

Do not mark a tool-surface change complete until these are true:

1. All 14 tools still register with input and output schemas.
2. All public schema defaults, caps, hidden fields, and mutex rules have tests.
3. Every tool has success, empty, error, mixed-bulk, pagination, lean-output (`base`/`shared`), and verbosity coverage.
4. Remote tools cover auth, rate limit, provider error, no results, and provider-mapper edge cases.
5. Local tools cover path validation, large output, hidden/ignored files, empty results, and command allow-list behavior.
6. LSP tools cover semantic success, fallback mode, symbol-not-found, wrong line hint, `orderHint`, pagination, and context snippets.
7. Hints are short, contextual, deduped, and absent on final pages.
8. Evidence metadata correctly represents whether the result is answer-ready, complete, and high-confidence.
9. No response leaks secrets, raw tokens, internal stack traces, or unrelated query metadata.
10. `yarn lint`, `yarn typecheck`, and `yarn test` pass in the package environment.
