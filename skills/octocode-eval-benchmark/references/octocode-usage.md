Load when using Octocode research tools through MCP or CLI. Source: `skills/common`; installed copies are generated.

## Choose the surface

Use the connected Octocode MCP tools when available, and call only the tools your MCP host lists. The CLI is a separate surface: it serves every public tool and ignores MCP exposure gates, while path and content security still apply. When Octocode MCP is not connected, or does not list a tool the task needs, prefer the built local CLI from a known Octocode checkout; otherwise use an installed `octocode` or `npx -y octocode`. In the checkout, the local command is `node packages/octocode/out/octocode.js`. From another workspace, use its absolute path and keep the task’s working directory so relative inputs still target that project. Examples use the portable `npx` prefix; substitute the selected CLI. Do not switch versions to hide a failing call.

Research, Chrome DevTools and agents-communication are different executables. Their skills own their command envelopes and mutation rules.

## Discover and invoke

```bash
npx -y octocode schema
npx -y octocode schema localSearch --view query
npx -y octocode schema localSearch --view variants
npx -y octocode schema --view full
npx -y octocode localFetch '{"queries":[{"path":"src/index.ts","ranges":[{"startLine":1,"endLine":40}]}]}'
```

New requests contain `queries` and optional `responseChars`. Use one tool per batch and respect its schema's maximum row count (at most five). Resume only with an emitted `{cursor}` input. CLI and MCP use the same envelope. For long requests, save JSON under the active skill's scratch directory and use CLI `--input <file>`.

Use observed paths, lines, refs and identifiers. Shared fields have one form: `matchString` string arrays, `contextLines:{before,after}`, `ranges:[{startLine,endLine}]`, numeric `{min,max}`, dates `{from,to}`, `sort:{by,order}`. Live schemas own exact fields and limits. There are no input aliases, bare queries or guessed repairs.

## Availability

Inspect schema availability, auth status and `lsp-server status <file>` as needed. Unavailable is a coverage gap, not an empty answer. `ghCloneRepo` requires persistent storage when invoked.

Follow [configuration](configuration.md) for credentials/settings. Inline `classification:{predicates:[...],context?,longContext?}` requires `OCTOCODE_CLASSIFICATION_API` or classification.api. CLI always exposes clasify and checks credentials/provider readiness when invoked. Discovery makes no provider request. No setting change is implied by a missing capability.

## 16 public tools

The default catalog contains 12 tools over MCP and 16 in the CLI. MCP independently adds graph tools with beta, clone with `OCTOCODE_ENABLE_CLONE=true`, and clasify with a nonblank classification key (beta and provider health do not gate it); tool lists and the local switch can restrict MCP further. Discovery is authoritative for the configured surface.

- Graph: graphBuild writes and prunes workspace snapshots; graphQuery explores them. Candidates need LSP and source verification.
- GitHub: ghSearchRepo, ghSearchCode, ghStructure, ghGetFileContent, ghSearchHistory, ghGetHistoryItem, ghCloneRepo.
- Local: localSearch, structureSearch, astSearch, localFetch, lspSearch.
- Other: artifactSearch, clasify.

## Read results and recover

Inspect every `results[]` row's index (its position in the request's `queries`), data, status and coverage facts. Success of the envelope does not mean all rows succeeded: a failed row has `status:"error"` with `errorCode` (and `httpStatus` for provider failures), and the exit code is not per row. Each row owns its evidence; there is no shared/root inheritance protocol.

Pagings use literal keys: `responsePagination{hasMore,next}` splits the response; `rowPart{part,of}` marks one row split across pages, so read every part first; a row's `pagination{hasMore}` continues in `next.<name>` (such as `next.nextPage`); `contentPagination.<collection>` pages one `ghGetHistoryItem` list. Execute `responsePagination.next` before advancing a row's `next.<name>`. A continue call is `{kind:"continue",tool,input:{cursor}}`; send its input whole to its tool. A claim about what a `next.<name>` covers needs that call. Complete for a claim: no `hasMore:true` and no `next` covering it, with or without a paging object. `isPartial`, `warnings` or `totalItemsCapped` (then `totalItems` is the reachable count) means incomplete: name the gap or narrow the scope. On `staleSnapshot`, discard pages read from the old snapshot and run `next.restart`. Finish every continuation needed for the claim or name the unread scope. Independent file, patch, source and diagnostic collections can each remain incomplete.

`related[]` contains `{kind:"related",tool,input:{queries:[...]}}` for optional fresh work; its calls may widen scope (hidden or ignored paths). `hints[]` is prose only. Input errors name fields to correct; no repair call silently changes scope. Terminal limits require a narrower or different query, never an invented cursor.

Absence needs unfiltered pages: `includeHidden:true`, `includeIgnored:true`, `defaultExcludes:false` and no `classification`. `status:"empty"` with skipped paths or `unknownItems>0` is not absence.

Classification receipts describe candidate decisions, not corpus completeness. Selected, related, excluded and unknown evidence have different meanings. `next.nextCandidatePage` completes a selected list; `next.unfiltered` or `next.unfilteredUnknown` is needed before an absence or count claim, including after an empty classified result; `next.read` is the read needed to cite clasify lines.

Cursor handles are bound to tool, access context and saved state. Memory-mode handles die with the runtime; persistent state may resume across processes. Expired or foreign handles fail without contacting the provider. Restart explicitly from the original request when required.

CLI exits: 0 success (possibly paged), 1 empty, 2 input, 3 not found, 4 authentication, 5 execution, 7 rate limit. Exit zero alone proves no completeness claim. Use the [evidence rules](evidence.md) for source claims; the research skill owns query recipes.
