Load when using Octocode research tools through MCP or CLI. Source: `skills/common`; installed copies are generated.

## Choose the surface

| Available surface | Use |
|---|---|
| Connected Octocode MCP lists the needed tool | Call that tool. |
| MCP absent or tool unlisted | Prefer the built CLI from a known checkout; otherwise installed `octocode` or `npx -y octocode`. |
| This checkout | `node packages/octocode/out/octocode.js` |
| Another workspace | Absolute checkout CLI path; keep task cwd so relative inputs target that project. |

The CLI serves every public tool regardless of MCP exposure gates; path/content security still applies. Examples use `npx`; substitute the selected CLI. Never switch versions to hide a failing call.

Research, Chrome DevTools and agents-communication are different executables. Their skills (agents-communication: `octocode-agentic`) own their command envelopes and mutation rules.

## Discover and invoke

Discover the least you need: `--help` lists when to use each tool; `--view variants` gives operations, required fields and an example (about 1 KB); `--view query` every field (6–10 KB); `--view full` every tool (about 23 KB).

```bash
npx -y octocode --help
npx -y octocode schema localSearch --view variants
npx -y octocode schema localSearch --view query
npx -y octocode schema --view full
npx -y octocode localFetch '{"queries":[{"path":"src/index.ts","ranges":[{"startLine":1,"endLine":40}]}]}'
```

New requests contain `queries` and optional `responseChars`; each query takes `mainGoal` (the task) and `reasoning` (why this query). Use one tool per batch and respect its schema's maximum row count (at most five). Resume only with an emitted `{cursor}` input. CLI and MCP use the same envelope. For long requests, save JSON under the active skill's scratch directory and use CLI `--input <file>`.

Use observed paths, lines, refs and identifiers. Shared fields have one form: `matchString` string arrays, `contextLines:{before,after}`, `ranges:[{startLine,endLine}]`, numeric `{min,max}`, dates `{from,to}`, `sort:{by,order}`. Live schemas own exact fields and limits. There are no input aliases, bare queries or guessed repairs.

## Availability

Inspect schema availability, auth status and `lsp-server status <file>` as needed. Unavailable is a coverage gap, not an empty answer. `ghCloneRepo` requires persistent storage when invoked.

Follow [configuration](configuration.md) for credentials/settings. Inline `classification:{predicates:[...],context?,longContext?}` requires `OCTOCODE_CLASSIFICATION_API` or classification.api. Without the key, both surfaces leave out clasify and those fields; a CLI call answers `missingConfiguration`. Discovery makes no provider request. No setting change is implied by a missing capability.

## 17 public tools

The default catalog contains 12 tools over MCP and 16 in the CLI, securityCheck among them. A nonblank classification key adds clasify to both (beta and provider health do not gate it). MCP independently adds graph tools with beta, clone with `OCTOCODE_ENABLE_CLONE=true` and securityCheck with `OCTOCODE_SECURITY_CHECKS=true`; tool lists and the local switch can restrict MCP further. Discovery is authoritative for the configured surface.

- Graph, only when discovery lists it: graphBuild writes and prunes workspace snapshots; graphQuery explores them. Candidates need LSP and source verification.
- GitHub: ghSearchRepo, ghSearchCode, ghStructure, ghGetFileContent, ghSearchHistory, ghGetHistoryItem, ghCloneRepo.
- Local: localSearch, structureSearch, astSearch, localFetch, lspSearch.
- Other: artifactSearch, clasify.

## Read results and recover

Inspect every `results[]` row's index (its position in the request's `queries`), data, status and coverage facts. Success of the envelope does not mean all rows succeeded: a failed row has `status:"error"` with `errorCode` (and `httpStatus` for provider failures), and the exit code is not per row. Each row owns its evidence; there is no shared/root inheritance protocol.

| Result signal | Required action / meaning |
|---|---|
| `responsePagination.next`, else `next.nextPage` | Run in that order until neither remains. Send `{kind:"continue",tool,input:{cursor}}`'s `input` whole to its tool. |
| `rowPart{part,of}` | Read every part of the split row. |
| `pagination{hasMore}` or `sectionPagination.<collection>` | Continue the same loop; each file, patch, source, diagnostic or history collection can remain incomplete independently. |
| Other `next.<name>` | Branch, not page. Run it before claiming what it covers. |
| Claim complete | No `hasMore:true` or `next` covering that claim, with or without a paging object. |
| `isPartial`, `warnings`, `countScope` other than `complete` | Incomplete: name the gap or narrow scope. `pagination.total` is then a floor; `capped` is the reachable count. |
| `staleSnapshot` | Discard old snapshot pages; run `next.restart`. |

Use literal emitted keys. Finish every continuation needed for the claim or name unread scope.

`related[]` contains `{kind:"related",tool,input:{queries:[...]}}` for optional fresh work; its calls may widen scope (hidden or ignored paths). `hints[]` is prose only. Input errors name fields to correct; no repair call silently changes scope. Terminal limits require a narrower or different query, never an invented cursor.

Absence needs unfiltered pages: `includeHidden:true`, `includeIgnored:true`, `defaultExcludes:false` and no `classification`. `status:"empty"` with skipped paths or `unknownItems>0` is not absence.

Classification receipts describe candidate decisions, not corpus completeness. Selected, related, excluded and unknown evidence have different meanings. `next.nextCandidatePage` completes a selected list; use `next.unfilteredUnknown` to inspect unresolved candidates and `next.unfiltered` before an absence or count claim, including after an empty classified result; `next.read` is the read needed to cite clasify lines.

Cursor handles are bound to tool, access context and saved state. Memory-mode handles die with the runtime; persistent state may resume across processes. Expired or foreign handles fail without contacting the provider. Restart explicitly from the original request when required.

CLI exits: 0 success (possibly paged), 1 empty, 2 input, 3 not found, 4 authentication, 5 execution, 7 rate limit. Exit zero alone proves no completeness claim. Use the [evidence rules](evidence.md) for source claims; the research skill owns query recipes.
