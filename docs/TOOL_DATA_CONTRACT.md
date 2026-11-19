# Tool data and continuations

The CLI and MCP take the same JSON input and return the same indexed results. Use `--json` in scripts; in MCP, parse the JSON in the tool result's text block.

## Requests

Send `{"queries": [...]}`. Each row is one independent query to one tool; most tools take up to five rows, and `graphBuild` takes one. Each result row's `index` is its position in `queries`; resumed work keeps it. Fields and operations come from `octocode schema <tool> --view query` (add `--select operation=<value>` for one operation; `--view variants` shows an example, `--view output` what each result field means).

### Example: a localFetch request

```json
{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}
```

**Input repairs.** The validator fixes unambiguous input, runs the repaired call and adds one warning per repair (for example `read matchString: "x" as matchString: ["x"]`). Ambiguous input fails with a field error. An invalid envelope fails before any row runs. Exit codes: [CLI exit codes](OCTOCODE_CLI.md#exit-codes).

## Result rows

| `status` | Meaning |
|---|---|
| none (`data` only) | Evidence. Check pagination and coverage warnings. |
| `empty` | Nothing in the stated scope. Read `hints`. |
| `outOfRange` | The page is past the end. `pagination.total` gives the size. A range that starts past the end is an `error` (`invalidInput`) instead. |
| `error` | Read `data.errorCode`, `data.errorClass`, `data.error`, `hints` and recovery calls. Valid sibling rows still succeed. |

## Text and structure forms

| Form | Where | Example |
|---|---|---|
| Numbered source: `N`, tab, text. One-based; ranges include both ends | `localFetch` `content` | `"1\t# Title\n"` |
| Hit line: `N`, tab, text | `ghSearchCode` `files[].matches[]` | `"12\tconst x = 1"` |
| Ref: `name sha` (7 to 64 hex characters) | `ghStructure` `branches[]`, `tags[]` | `"main 3f2a9c1"` |
| Outline: `N`, tab, indent (two spaces per depth), kind, name, optional tags | `localSearch`, `astSearch`, `lspSearch` | `"12\tfunction run (exported)"` |

A `matchString` read's `matchedLines` lists the page's real hits; an empty list means context only. Without it, every returned line is a hit. Search results group matches by `files[].path`. Relative paths resolve against the configured workspace; allowed external paths stay absolute.

## Shared facts

A response states each repeated fact once. To read a field a row lacks, look in this order: the nearest holder (the object that holds the row's list), the row's `data`, `shared.data`, then the request. For example, `shared.data.coverage` describes every row. Warnings shared by all rows appear once at the top level, hints shared by every empty or failed row once in `hints`, and related calls of one tool merge into one `related` call.

Paths, lines and executable calls never move: a `{tool, input}` call, `next`, `related` and a cursor run unchanged. Use `includeProvenance: true` (where a tool offers it) for byte spans and source identity. A default result still discloses source transformations, such as redaction, and incomplete parses.

## Executable continuations

Finish response pages before tool pages: call `responsePagination.next`, then `next.nextPage`, until the rows on each page are read.

| Channel | What to do |
|---|---|
| `responsePagination.next` | Finish response pages first. One continuation can carry several rows or sections. |
| `next.nextPage` | The next tool page of this row's walk. |
| Other `next.*` | Follow the branch your claim depends on: `next.unfiltered` (classification left out), `next.nextCandidatePage` (billed), `next.readHits`, `next.restart`, and similar. |
| `related[]` | Optional fresh work; its scope may differ. Required paging uses `next`, never `related`. |
| `hints[]` | Guidance for an empty result or an error. |
| `warnings[]` | Evidence and coverage disclosures, including on successful rows. |

Pass only the complete `input` of a continuation to its `tool`:

```json
{"tool":"localFetch","input":{"cursor":"<issued cursor>"}}
```

Cursors are bound to the tool, configuration, access context and saved state. Replaying a saved page can show historical usage; expired saved work fails explicitly. In memory storage mode a CLI call cannot use a saved continuation: use persistent storage or keep the MCP session open ([storage and limits](STORAGE_AND_LIMITS.md)).

## Errors and recovery

| `errorClass` | Do this |
|---|---|
| `correctInput` | Fix the input the error names, then retry. |
| `refused` | Stop. A policy, permission, auth or quota limit blocks the call. |
| `unservable` | Use another tool or source. |
| `transient` | Retry when `retryable` is true. |
| `restart` | Drop the pages you hold and follow `next.restart`. The source changed (`staleSnapshot`) or the saved answer expired. |

`ssoUrl` on a 403 is a GitHub SAML authorization link: open it, authorize the token, then retry.

## Pages and size

`responseChars` is an optional page-size target (default 20,000 characters). It sizes reads (`localFetch`, `graphQuery`) and remote pages (the GitHub tools, `clasify`, `artifactSearch`, `securityCheck`). Local walks (`localSearch`, `structureSearch`, `astSearch`, `lspSearch`) use an automatic window shared by their pages; there it sizes only the split parts of `responsePagination`. A line over 4 KB is served in byte windows, reassembled by `offset`; a matched window can arrive before the bytes before it. `oversized: true` appears only when one indivisible value is longer than the target. Large responses split at row boundaries where possible, then inside long strings (`rowPart`); a page can exceed the target by its fixed parts (envelope, `responsePagination`, repeated context).

| Field | How to read it |
|---|---|
| `pagination` | `unit`, zero-based `offset`, `length`, `hasMore`, `total` when known. A one-page result can omit it. |
| `countScope` | `loaded`, `partial` or `capped`. Before the last page, or when capped, the total is a floor. |
| `rowPart` | Join parts in order. Keep repeated identities and scalars once; append array elements. Append `rowPart.text` at its JSON pointer, not as the pointer text. |

Programmatic readers take `collectionPaths` from `octocode schema <tool> --view output --json` (plus `collectionRefs` where present). Append each declared collection, keep repeated context once, and rebuild each `rowPart` before you treat it as a complete row. Consume every row of a response continuation before you advance tool continuations.

## Evidence boundaries

A complete page can belong to a partial search. Before you report a total or an absence, check `isPartial`, `partialReasons`, skipped paths, unassessed candidates, warnings and terminal limits.

- `ghSearchCode` covers the default branch's index. A row with `atRef: false` is not at the requested ref.
- A `clasify` result covers the candidates it assessed.
- An empty `securityCheck` result means no known advisory matched. A failed check is a separate error.

Cite the exact revision, path, lines or package version a result returned. If a source is unreachable, name the missing scope. Agent workflows: [protocol](OCTOCODE_PROTOCOL.md).
