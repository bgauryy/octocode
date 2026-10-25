Load when using Octocode research tools through MCP or CLI. Source: `skills/common`; installed copies are generated.

## Choose the surface

Use the connected Octocode MCP tools when available. For CLI-only capabilities or unavailable MCP, use the task's selected local CLI; otherwise use `npx -y octocode`. In this checkout, use `node packages/octocode/out/octocode.js` after building. Do not switch versions to hide a failing call.

Research, Chrome DevTools and agents-communication are different executables. Their skills own their command envelopes and mutation rules.

## Discover and invoke

```bash
npx -y octocode schema
npx -y octocode schema localSearch --view query
npx -y octocode schema localSearch --view variants
npx -y octocode schema --view full
npx -y octocode localFetch '{"queries":[{"path":"src/index.ts","ranges":[{"startLine":1,"endLine":40}]}]}'
```

New requests contain `queries` (up to five independent rows of one tool) and optional `responseChars`. Resume only with an emitted `{cursor}` input. CLI and MCP use the same envelope. For long requests, save JSON under the active skill's scratch directory and use CLI `--input <file>`.

Use observed paths, lines, refs and identifiers. Shared fields have one form: `matchString` string arrays, `contextLines:{before,after}`, `ranges:[{startLine,endLine}]`, numeric `{min,max}`, dates `{from,to}`, `sort:{by,order}`. Live schemas own exact fields and limits. There are no input aliases, bare queries or guessed repairs.

## Availability

Inspect schema availability, auth status and `lsp-server status <file>` as needed. Unavailable is a coverage gap, not an empty answer. `ghCloneRepo` is CLI-only and needs persistent storage; local analysis also needs local tools enabled.

Follow [configuration](configuration.md) for credentials/settings. Inline `classification:{predicates:[...],context?,longContext?}` requires `OCTOCODE_CLASSIFICATION_API` or classification.api. Public clasify also requires a successful provider ping; beta is not required. No setting change is implied by a missing capability.

## 14 public tools

The default catalog contains 12 tools over MCP and 13 in the CLI; the full inventory includes clasify when its provider is configured and its ping succeeds. Discovery is authoritative for the configured environment.

- GitHub: ghSearchRepo, ghSearchCode, ghStructure, ghGetFileContent, ghSearchHistory, ghGetHistoryItem, ghCloneRepo.
- Local: localSearch, structureSearch, astSearch, localFetch, lspSearch.
- Other: artifactSearch, clasify.
- Graphs are CLI commands: `octocode graph ingest <dir>`, then graph query operations.

## Read results and recover

Inspect every `results[]` row's index, data, status and coverage facts. Success of the envelope does not mean all rows succeeded. Each row owns its evidence; there is no shared/root inheritance protocol.

Execute `responsePagination.next` before advancing tool-level `next.*`. A continue call is `{kind:"continue",tool,input:{cursor}}`; copy input unchanged. Finish every continuation needed for the claim or name the unread scope. Independent file, patch, source and diagnostic collections can each remain incomplete.

`related[]` contains `{kind:"related",tool,input:{queries:[...]}}` for optional fresh work. `hints[]` is prose only. Input errors name fields to correct; no repair call silently changes scope. Terminal limits require a narrower or different query, never an invented cursor.

Classification receipts describe candidate decisions, not corpus completeness. Selected, related, excluded and unknown evidence have different meanings. Read selected pages or emitted unfiltered/source continuations according to the question.

Cursor handles are bound to tool, access context and saved state. Memory-mode handles die with the runtime; persistent state may resume across processes. Expired or foreign handles fail without contacting the provider. Restart explicitly from the original request when required.

CLI exits: 0 success (possibly paged), 1 empty, 2 input, 3 not found, 4 authentication, 5 execution, 7 rate limit. Exit zero alone proves no completeness claim. Use the [evidence rules](evidence.md) for source claims; the research skill owns query recipes.
