# Tool data and continuations

The CLI and MCP take the same JSON input and return the same indexed results. Use `--json` in scripts; in MCP, parse the JSON in the tool result's text block.

With `output.mcpResult: text` (`OCTOCODE_MCP_RESULT=text`) the MCP text block is the CLI's text view instead: content lines, then `# key: value` lines. The text view adds no fact the JSON lacks; its one path layout rule states the directory a row's paths share once (`# root: <dir>`, joined with the row's own `root` when it states one) and prints those paths relative to it, so root, `/` and a printed path are the JSON path. Do not parse it as JSON. Each continuation line (`# next:`, the reply's one continuation; a row's `# next.*`; `# related[i]:`) holds the call as compact JSON, `{"tool":"…","input":{…}}`: send its `input` unchanged. Fractions are exact, and the notes the CLI writes to stderr follow the content in the same block. A result without a text view arrives as JSON.

## Requests

Send `{"queries": [...]}`. Each row is one independent query to one tool; most tools take up to five rows, and `graphBuild` takes one. Each result row's `index` is its position in `queries`; resumed work keeps it. Fields and operations come from `octocode schema <tool> --view query` (add `--select operation=<value>` for one operation; `--view variants` shows an example, `--view output` what each result field means).

### Example: a localFetch request

```json
{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}
```

**Strict input.** A request is validated as sent. It is read in another form only where the schema alone gives the value one typed reading: one value where a list is declared, a text holding the declared JSON, an enum literal in another letter case, an object missing a member whose default the object states (`sort` without `by`), a bare row or list of rows without `queries`, and `responseChars` beside a `cursor`; each adds one warning (for example ``read `exclude: "x"` as `exclude: ["x"]`; send `exclude: ["x"]` next time.``), and a call that bills (clasify) is never read leniently. Everything else is refused before the row runs: an undeclared field gets one line naming the declared field to use (`Unknown field 'branch' in queries[0]; use 'ref'.`) or the allowed fields, a field in the wrong object is told where it goes, and an example shows the request with the fix applied. An invalid envelope fails before any row runs. Exit codes: [CLI exit codes](OCTOCODE_CLI.md#exit-codes).

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
| Hit text: `value`, the hit's line alone, or numbered lines when it carries context | `localSearch`, `astSearch`, `lspSearch` `files[].matches[].value` | `"const x = 1"` |
| Listing entry: `name`, `/` after a directory, then ` (fields)`; a ranked listing names the entry by its row path | `structureSearch`, `ghStructure` `files[]` | `"src/big.rs (9120)"` |
| Ref: `name`, or `name sha` (7 to 64 hex characters) when `ref` is a glob | `ghStructure` `branches[]`, `tags[]` | `"main"`, `"main 3f2a9c1"` |
| Repository commit: `repo sha`, for rows that omit `commitSha` | `ghSearchCode` `repoCommits[]` | `"api 3f2a9c1"` |
| Outline: `N`, tab, indent (two spaces per depth), kind, name, optional tags | `localSearch`, `astSearch`, `lspSearch` | `"12\tfunction run (exported)"` |

A `matchString` read's `matchedLines` lists the page's real hits; an empty list means context only. Without it, every returned line is a hit. Search results group matches by `files[].path`. One path form per family: a local row's path is workspace-relative, or relative to the `root` the row states when its evidence lies outside the workspace; a GitHub row's path is repo-relative. A row's path, a listing group's `dir + "/" + name` and a ranked listing's entry path all run unchanged as another tool's `path`. Relative input paths resolve against the configured workspace; allowed external paths stay absolute.

## Shared facts

A response states each repeated fact once. To read a field a row lacks, look in this order: the nearest holder (the object that holds the row's list), the row's `data`, `shared.data`, then the request. For example, `shared.data.coverage` describes every row. Warnings shared by all rows appear once at the top level, hints shared by every empty or failed row once in `hints`, and related calls of one tool merge into one `related` call.

Paths, lines and executable calls never move: a `{tool, input}` call, `next`, `related` and a cursor run unchanged. Use `includeProvenance: true` (where a tool offers it) for byte spans and source identity. A default result still discloses source transformations, such as redaction, and incomplete parses.

## Executable continuations

One rule: a reply that carries a top-level `next` is unfinished. Run it unchanged until a reply carries none; then every row is read to its end for what was scanned. A reply holds the rows it has room for: in a batch an unfinished row can first appear on a later reply, under its query's `index`. A row's own `next.*` branch (a `ghGetHistoryItem` section such as `next.patches`, a recovery call) reads evidence the reply does not hold: such a reply has no top-level `next` and is still not everything the item has. `replayed: true` marks any reply served from the saved output of an earlier call, which includes every later page of a paged response; its usage numbers are historical. In the text view a page states its place, first item to last: `lines 166-247 of 247`, or `lines 166-247` when no total is stated.

| Channel | What to do |
|---|---|
| Top-level `next` | The reply's one continuation: one call that continues every unfinished row, each under its query's `index` (append its rows to that row). `expiresAt` beside it says when it stops answering; `replayed: true` marks a page read back from saved output. |
| A row's `next.*` | Branches, never the row's own page walk. Follow the one your claim depends on: `next.readHits`, `next.restart`, clasify's `next.read` (the lines an answer cites), `next.confirm` and `next.retry` (billed), and similar. |
| `related[]` | Optional fresh work; its scope may differ. Paging uses the reply's `next`, never `related`. |
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

`responseChars` is an optional page-size target (default 20,000 characters). It sizes reads (`localFetch`, `graphQuery`) and remote pages (the GitHub tools, `clasify`, `artifactSearch`, `securityCheck`). Local walks (`localSearch`, `structureSearch`, `astSearch`, `lspSearch`) use an automatic window shared by their pages; there it sizes only the parts a row is split into. A line over 4 KB is served in byte windows, reassembled by `offset`; a matched window can arrive before the bytes before it. A byte window keeps the numbered format: each row that starts a source line has its `N<TAB>` gutter, a window that opens inside a line states `continuesLine` (that line's number) and its first row has no gutter, and `offset` and `length` count the source text, never the gutters. Only a window of match pieces joined by `... [bytes A-B …] ...` markers stays unnumbered, beside `sourceLineRanges`. `oversized: true` appears only when one indivisible value is longer than the target. Large responses split at row boundaries where possible, then inside long strings (`rowPart`); a page can exceed the target by its fixed parts (envelope, `next`, repeated context). A split row's first part may hold only the fields the row states once, with its items starting on the next part.

| Field | How to read it |
|---|---|
| `pagination` | Where a text window sits (`unit`, zero-based `offset`, `length`) or an exact count (`total`). A stated `total` is exact: it appears once the walk has counted everything (which can be before the last reply), and a provider's estimate or a capped count is never stated as one. A row's units are the rows it serves: an outline counts members as rows. It never says whether more follows: the reply's `next` does. A row that states neither has no `pagination`. |
| `rowPart` | A row too large for one reply arrives in parts over the replies `next` reaches; the marker is `{part, of}`. Join parts in order. Keep repeated identities and scalars once; append array elements. Append `rowPart.text` at its JSON pointer, not as the pointer text. |

Programmatic readers take `collectionPaths` from `octocode schema <tool> --view output --json` (plus `collectionRefs` where present). Append each declared collection, keep repeated context once, and rebuild each `rowPart` before you treat it as a complete row. Follow the reply's `next` until none is left before you treat any row as complete.

## Evidence boundaries

A complete page can belong to a partial search. Before you report a total or an absence, check `isPartial`, `partialReasons`, skipped paths, unassessed candidates, warnings and terminal limits.

- `ghSearchCode` covers the default branch's index. A row with `atRef: false` is not at the requested ref.
- A `clasify` result covers the candidates it assessed.
- An empty `securityCheck` result means no known advisory matched. A failed check is a separate error.

Cite the exact revision, path, lines or package version a result returned. If a source is unreachable, name the missing scope. Agent workflows: [protocol](OCTOCODE_PROTOCOL.md).
