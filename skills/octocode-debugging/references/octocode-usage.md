Load when using Octocode research tools through MCP or CLI. Source: `skills/common`; installed copies are generated.

## Choose the surface

| Available surface | Use |
|---|---|
| Connected Octocode MCP lists the needed tool | Call that tool. |
| MCP absent or tool unlisted | Prefer the built CLI from a known checkout; otherwise installed `octocode` (`octocode-cli` in Octocode Code agent shells, where `octocode` is the coding agent) or `npx -y octocode`. |
| This checkout | `node packages/octocode/out/octocode.js` |
| Another workspace | Absolute checkout CLI path; keep task cwd so relative inputs target that project. |

The CLI serves every public tool regardless of MCP exposure gates; path/content security still applies. Examples use this checkout's CLI; substitute the selected CLI. Never switch versions to hide a failing call.

Research, Chrome DevTools and agents-communication are different executables. Their skills (agents-communication: `octocode-agentic`) own their command envelopes and mutation rules.

## Discover and invoke

Discover the least you need: `--help` lists when to use each tool; `--view variants` gives operations, required fields and an example; `--view query` every field; `--view full` every tool.

```bash
node packages/octocode/out/octocode.js --help
node packages/octocode/out/octocode.js schema localSearch --view variants
node packages/octocode/out/octocode.js schema localSearch --view query
node packages/octocode/out/octocode.js schema --view full
node packages/octocode/out/octocode.js localFetch '{"queries":[{"path":"src/index.ts","ranges":[{"startLine":1,"endLine":40}]}]}'
```

New requests contain `queries` and optional `responseChars`; each query takes `mainGoal` (the question this query answers) and `reasoning` (evidence so far and why this query decides). Use one tool per batch and respect its schema's batch limit. Resume only with an emitted `{cursor}` input. CLI and MCP use the same envelope. For long requests, save JSON under the active skill's scratch directory and use CLI `--input <file>`.

Save long command output under the active skill's workspace scratch directory. Search known failure markers with `localSearch` and read deciding spans with `localFetch`. When exact search cannot split large unstructured evidence, use `clasify` with `source:{tool:"localFetch",query:{path:".octocode/<skill-folder>/out.log"}}` and a typed question. Read emitted `next.read` before citing its lines. Use `sections` when order or every occurrence matters. Files outside allowed workspace roots, such as `/tmp`, are refused.

For a pipe-only input, use `set -o pipefail; <cmd> 2>&1 | <cli> clasify --content - '<request-json>'`; save reusable requests and pass `--input <file>`. Classification may incur provider cost; discovery does not.

Use observed paths, lines, refs and identifiers. Shared fields have one form: `matchString` string arrays, `contextLines:{before,after}`, `ranges:[{startLine,endLine}]`, numeric `{min,max}`, dates `{from,to}`, `sort:{by,order}`. Live schemas own exact fields and limits. Send declared fields in the canonical `queries` envelope. Supported unambiguous repairs are disclosed in warnings that name the canonical input; unsupported shapes and fields are refused with guidance.

## Availability

Inspect schema availability, auth status and `lsp-server status <file>` as needed. Unavailable is a coverage gap, not an empty answer. `ghCloneRepo` requires persistent storage when invoked.

Follow [configuration](configuration.md) for credentials/settings. Inline `classification:{predicates:[...],context?,longContext?}` requires `OCTOCODE_CLASSIFICATION_API` or classification.api. Without the key, both surfaces leave out clasify and those fields; a CLI call answers `missingConfiguration`. Discovery makes no provider request. No setting change is implied by a missing capability.

## Read results and recover

Inspect every `results[]` row's index (its position in the request's `queries`), data, status and coverage facts. Success of the envelope does not mean all rows succeeded: a failed row has `status:"error"` with `errorCode` (and `httpStatus` for provider failures), and the exit code is not per row. Each row owns its evidence; there is no shared/root inheritance protocol.

| Result signal | Required action / meaning |
|---|---|
| `responsePagination.next`, else `next.nextPage` | Run in that order until neither remains. Send `{tool,input:{cursor}}`'s `input` whole to its tool. |
| `rowPart{part,of}` | Read every part of the split row and join them in order: `rowPart.continues` names the array element a part resumes, `rowPart.text` the string its value appends to. |
| `pagination{hasMore}` or `sectionPagination.<collection>` | Continue the same loop; each file, patch, source, diagnostic or history collection can remain incomplete independently. |
| Other `next.<name>` | Branch, not page. Run it before claiming what it covers. |
| Claim complete | No `hasMore:true` or `next` covering that claim, with or without a paging object. |
| `isPartial`, `countScope` other than `complete` | Incomplete: name the gap or narrow scope. `pagination.total` is then a floor; `capped` is the reachable count. |
| `warnings` | Read the factual disclosures; use the coverage fields to decide what remains unread. A warning alone does not mean incompleteness. |
| `staleSnapshot` | Discard old snapshot pages; run `next.restart`. |

Use literal emitted keys. Finish every continuation needed for the claim or name unread scope.

`related[]` contains `{tool,input:{queries:[...]}}` for optional fresh work; its calls may widen scope (hidden or ignored paths). Successful answers omit routine advice while preserving calls with otherwise unstated identity or required by a warning. `hints[]` is prose recovery for empty results or errors. Input errors name fields to correct; no repair call silently changes scope. Terminal limits require a narrower or different query, never an invented cursor.

Absence needs unfiltered pages: `includeHidden:true`, `includeIgnored:true`, `defaultExcludes:false` and no `classification`. `status:"empty"` with skipped paths or `unknownItems>0` is not absence.

Classification receipts describe candidate decisions, not corpus completeness. Selected, related, excluded and unknown evidence have different meanings. `next.nextCandidatePage` completes a selected list; use `next.unfilteredUnknown` to inspect unresolved candidates and `next.unfiltered` before an absence or count claim, including after an empty classified result; `next.read` is the read needed to cite clasify lines.

Cursor handles are bound to tool, access context and saved state. Memory-mode handles die with the runtime; persistent state may resume across processes. Expired or foreign handles fail without contacting the provider. Restart explicitly from the original request when required.

CLI exits: 0 success (possibly paged), 1 empty, 2 correctInput (a missing target included), 4 refused, 5 unservable, 6 restart, 7 transient; 3 is unused. Every error row states `errorClass`. Exit zero alone proves no completeness claim. Use the [evidence rules](evidence.md) for source claims; the research skill owns query recipes.
