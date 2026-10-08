# Tool data and handoff contract

This page owns the shared request and result envelope: rows, numbered content, result shapes, pages, leads, shared fields, and the handoffs between tools. Operation fields: [OCTOCODE_TOOLS.md](OCTOCODE_TOOLS.md). Which tool to call next: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md). Package ownership and the contract pipeline: [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md).

Before an unfamiliar request, read the live input schema. The catalog's compact fields are a summary; `--view query` and the full view keep nested and conditional constraints. `schema <tool>` reports `availability`; a disabled tool shows `enabled:false` and its gating `envVar`. Enabling a tool does not install a language server or supply provider credentials.

```sh
node packages/octocode/out/octocode.js schema
node packages/octocode/out/octocode.js schema astSearch --view query
```

Input preparation can add documented defaults or trim text fields; invalid numeric values and unknown fields are rejected, never clamped or dropped. MCP leaves `outputSchema` out of discovery to save context; core and native still validate every result against it. Responses carry `structuredContent` and a matching text representation.

## Requests and result rows

Each call uses one tool and an outer `queries` array of 1–5 queries; `clasify` matrices go in `queries` too. A bare array is rejected; MCP requires `queries`, and the CLI runs one bare query object as one query. Independent queries can batch; a query that needs a prior result waits for it. `mainGoal` and `reasoning` are optional (at most 500 characters each): set them only in multi-call research on an unknown, where `mainGoal` is the research question and `reasoning` says why this call advances it. They do not supply missing runtime fields. A blank brief is dropped.

MCP returns the envelope under `structuredContent`; CLI JSON output is the envelope. Tool payloads and their follow-ups are row-local under `results[index].data`.

The text channel (YAML by default) is compacted further; `structuredContent` and JSON keep the envelope:
- A single-row response drops the `results: - index: 0 data:` wrapper; a batch keeps it.
- Path-only search rows (`resultView:"files"`) render as `path`, and count rows as `path (count)`.
- `localFetch` and `ghGetFileContent` print content verbatim after the metadata, under `content (source lines):` when numbered or `content (copy-safe):` otherwise. A GitHub batch labels each block `=== [index] path content (…) ===`.

| Field | Meaning |
|---|---|
| `results[].index` | Zero-based input position. Keep it when a batch has mixed outcomes. |
| `results[].status` | Omitted on a successful nonempty row. `empty` and `error` are distinct outcomes; read the reason before you interpret either. |
| `results[].cache` | Debug only. `1` marks a cached primary response; it does not prove source freshness. |
| `results[].meta.evidence` | `kind` and `confidence`: evidence origin and strength, not complete coverage. |
| `results[].meta.diagnostics` | Optional diagnostic codes, hints, and partial state. |
| `results[].data` | Payload, pagination, coverage, errors, `next` pages, and `hints`. An empty or error row carries one recovery tip in `hints.text` (at most 120 characters). A `clasify` answer, page, or resource failure nests the same `{errorCode, error, hints:{text}}` under `error`. |
| `root`, `shared` | Compression metadata; see [Paths, shared fields, and anchors](#paths-shared-fields-and-anchors). |
| `responsePagination` | Pages of the whole response, separate from row pages; present only while something remains (`hasMore`, a restart, or a changed snapshot). |

An outer `isError:false` does not mean every row succeeded. Never infer success, absence, or completeness from a missing field. `answerReady` and `complete` are not universal members of `meta.evidence`; read the operation's pagination, coverage, truncation, and terminal-limit fields.

### Numbered source content

When a `localFetch` or `ghGetFileContent` row returns original source lines (line ranges, match windows, `fullContent`, and line pages, with no transformed `contentView`), `content` is numbered like `cat -n`, without padding:

```text
95	        self._thread_sharing_count = 0
96	
... [lines 97-254 not requested] ...
255	    def close(self):
```

- Each line is `<line>` + TAB + source text. Strip everything up to the first TAB before you copy text into an edit or a `matchString`.
- Gap markers between windows (`... [lines A-B not requested] ...`) are not numbered. In `ghGetFileContent`, two or more gaps that only single lines separate share one marker at the first gap: `... [N gaps in lines A-B not requested] ...`.
- The numbers state the returned lines, so a numbered row omits `sourceLineRanges` (and a `localFetch` row omits `startLine`, `endLine`, and `returnedLines`). `matchedLines` is omitted when every returned line matched.
- Size counts (`returnedChars`, and the debug-only `sourceBytes` and `returnedBytes`) measure the source text, not the prefixes.
- `minify:"standard"` and `"symbols"` views (`contentView`) carry the same gutter: each kept line cites its source line, dropped lines leave gaps, and a joined line cites its first line. Copy exact text from a `minify:"none"` read.
- Byte windows (`contextBytes`, long minified lines), `unit:"bytes"` pages, and content whose lines no longer map one-to-one stay verbatim and keep `sourceLineRanges`.
- Both text encodings render from the same numbered content (`crates/runtime/src/runtime/numbered.rs`); other tools reuse it.
- `ghGetHistoryItem` patch text uses the same separator on the new side of each hunk: `87\t+added` and `86\t context` (new-file numbers), `85\t-removed` (old-file number), and a bare tab before a `\ No newline` marker. `@@` lines stay verbatim. Patch `offset` counts the numbered text.
- A `localSearch` row with `contextLines > 0` numbers its window (`matchedLines` still lists the matches; a truncated window stays verbatim). A repo-scoped `ghSearchCode` `match:"file"` row lists keyword lines as `lines: ["<line>\t<text>", …]`.

### Search result shapes

- **`localSearch`** without `pageSize` or `matchPageSize` pages by size. A result within one page of about 24 KB is shown whole, with no paging fields. A larger one opens with up to 10 rows from each of its first 20 files, and `next.nextPage` walks the rest in pages of about 24 KB; a file's per-file `pagination.moreLines` lists up to 24 later line numbers, and `moreLinesUnlisted` counts the rest. Either field set restores the file-page × match-page grid with `next.nextMatchPage`. A complete result over at most three files carries `hints.read`: a `localFetch` `ranges` read (±6 lines) of the top file's hits, or a `matchString` read when they need more than 10 ranges. An invalid alternation (`a(|b`) gets `hints.repair`, which escapes each broken alternative and keeps regex mode; a single invalid anchor gets the literal repair.
- **`ghSearchCode`** reads the top 5 files of a repo-scoped `match:"file"` page through the contents cache (core API quota, no extra code-search calls). Each resolved row has `lines` in place of index fragments: every keyword line, or with several keywords the lines that hold all of them (each keyword's first line when none does), up to 20, with `hitCount` and a `readHits` read when there are more. Rows with identical evidence are listed once, other paths in `alsoAt`. `data.commitSha` names the commit read, and `owner`/`repo` appear once on `data`. A row with no keyword line or an unreadable blob keeps its fragments and sets `lineResolved:false`. With `ref`, every row is read at that ref (`data.ref`), and `data.indexRef:"defaultBranch"` labels the candidates as index output; no row keeps default-branch text. A path or hit absent at the ref is `atRef:false`; an unreadable row is `lineResolved:false` with a `readHits` lead at the ref commit. When the ref is not the default-branch head, a warning names both commits, and `hints.viewRepo` (first) lists the ref with `ghStructure`: files only at the ref are not in the index. Fragment `matchIndices` need `debug:true`. `hints.read` reads the first resolved file by one `ranges` entry at `data.commitSha`; an unresolved top file's read is pinned to that commit, and a file absent at the ref gets no read.
- **Location rows** are objects named like the inputs that take them (X1). Two packed strings are allowed: `"<line>\t<value>"` and the outline entry (P1).
  - An `astSearch` symbols outline (and an `lspSearch` `documentSymbols` one) lists declarations in source order. A declaration without members is an entry string in the `structureSearch` entry grammar: `"<symbolName> (<line>[-<endLine>][, <kind>][, doc <docStartLine>][, exported][, <key>=<value>]…)"`. The last ` (` opens the fields, so a name can hold ` (` or `, `. `<key>=<value>` carries `exportedAs`, `startLine`, `column`, `parent`, and `parentLine`; the value is bare words, or JSON when it holds a space, comma, paren, quote, or `=` (a ` (` inside it is `(`).
  - A container stays an object: `{symbolName, kind, line, endLine?, docStartLine?, exported?, exportedAs?, startLine?, column?, parent?, parentLine?, shared?, members}`. Members nest in `members` and name no `parent`; a container's `shared` states the `kind` or `exported: true` that all of its 2+ members have. Only a member whose container is off the page stays top level with `parent=` (and `parentLine=`).
  - `symbolName` + `line` (an entry's first number) anchor `lspSearch` (`lineHint` = `line`), and `line`–`endLine` is a `localFetch` range.
  - Parse an entry with `lastIndexOf(" (")` and split its fields on `, ` outside JSON strings (`symbol_outline::parse_entry` in native, `parseOutlineEntry` in `octocode-local-testing/harness/mcp-client.mjs`). Every outline round-trips losslessly. The YAML text prints declarations as an indented outline.
  - A `match` row is `{line, column, endLine?, value}` (`value` is whitespace-normalized match text, not a source line); `captureText:true` adds `endColumn` and `metavarRanges`. A `localSearch` hit inside a declaration carries `enclosing: {symbolName, kind, line, endLine}`.
  - `lspSearch` `references` and `callers`/`callees` group rows per file under `payload.files` as `{path, matches}`. A reference row is `{line, column, endLine?, value}`; a call row is `{symbolName, kind, line, endLine, sites: [{line, column}], path?, detail?, via?, source?}`.
  - Columns are 1-based UTF-16 units everywhere. The YAML text prints each such row on one line.
- **`structureSearch`** pages by size when `pageSize` is omitted (about 24 KB of rows). `tree` and `files` rows are `{"dir":"<workspace-relative dir>","files":["<name>[/][ (<fields>)]", …]}` groups, the listed path's own entries included. `/` marks a directory; the fields are the size in bytes (every non-directory), `symlink`, `lineCount=N`, and `modifiedMs=N`, opened by the last ` (`. A page that continues a group repeats its `dir`. `sort:"path"` gives each directory one group; other sorts keep their order. A `"name/"` entry is left out when that directory's group is on the page (`structureFiles` in `mcp-client.mjs` expands rows). `files` sorts by path by default (walk order) and stops once `maxEntries` is filled; that cut reports `truncated`, `partialReasons:["maxEntries"]`, `atLeast` (a lower bound), and, on the listing's last page, `next.expandScan`, which resumes after the listed rows (`scanOffset`). Other sorts walk the whole scope and report `totalAvailable`.
- **`next.expandScan`** has two meanings; its query says which. In `structureSearch` and `astSearch` it resumes: the query carries `scanOffset`, so append its rows. In `astTopology` it replaces: more files can retract, merge, or reorder a whole-graph row, so the query carries `supersedes` (the cut scan's `maxFiles`), page 1 echoes it with a warning, and you discard the earlier rows. Both ride the last page of their scan. An `astTopology` cut that more files cannot lift (the edge cap, skipped files, or `maxFiles` at its maximum) offers no `expandScan` and sets `terminalLimit:true`.

### Minimal by default

Every output field has one class, declared once in the core output schema (`fieldClass`): evidence (the default), next (continuations, cursors, snapshots), disclosure (partial, coverage, caps, warnings), or verbose. The contract lists each tool's verbose fields as `verbosePaths`; the verbose stage drops them unless the row asked for `debug: true`, and then `minimize_row` removes what asserts nothing.

- Prose `hints.text` shows only on empty or failed rows. A successful row states a tip it needs (a clamp, a regex trap, a skipped scope) as a `warnings` entry.
- A row whose `next` holds pages gets one `warnings` entry per page with what it has left, when known (for example `7 more changed files: follow next.nextFilePage`, or `more: follow next.nextPage`). `warnings` then leads the row. `partialReasons` say why a row is partial and never replace that entry; only a tool warning that already names the page does.
- `debug: true` adds the verbose fields (`meta`, `cache`, scan and provider fields such as `searchEngine`, `filesScanned`, `modified`, byte counts, `effectiveQuery`, the `lsp.receipt` and `workspaceRoot`, forks, update dates, page counters), info-level diagnostics, the top-level `snapshot`, some request echoes (for example `operation`), `false`/`0` defaults, and the pagination of a finished single page. Identity fields stay in minimal rows: `owner`, `repo`, `path`, and `ref` on GitHub rows.
- Never dropped: open pagination (`hasMore`, or a page after the first), every `next` page and `hints` lead (with its own `snapshot`), warnings and errors, scan scope on an empty search, and row confidence signals such as topology `completeness` and `confidence`. Error rows keep every non-verbose field.
- Every pagination block has the same keys on every page and tool: `currentPage` or `offset`, `pageSize` or `length`, `totalItems`/`totalPages` when known, and `hasMore`. The cursor (`nextPage`, `snapshot`, `resultId`) rides the `next` page, never the block.
- Continuations carry no `confidence`: a page is an exact replay, and the runtime ranks leads before it removes the field. A registry row states its release ref's `verification`; its leads do not repeat it.
- A stale snapshot fails every tool the same way: `errorCode:"staleSnapshot"`, one error text, and `next.restart` (page 1 of the same query on the current source).

## Evidence boundaries

| Evidence kind | Supports | Still requires |
|---|---|---|
| `lexical` | Text and regex matches in the scanned scope. | Exact source and semantic checks for identity or usage. |
| `structural` | Syntax matches and captures. | Symbol resolution and runtime checks when those are the claim. |
| `syntactic` | Parsed declarations, syntax trees, and file topology. | Project-aware semantics; graph roots and exclusions limit reachability. |
| `exact` | Returned source or file metadata in the selected scope. | Coverage checks; redaction and explicit transformations still apply. |
| `semantic` | Language-server results for its project and capabilities. | Completeness checks, and runtime verification for runtime claims. |
| `provider` | Registry or repository-provider data. | Revision, index, result-cap, and materialization checks. |

Local file reads and path-only GitHub file reads default to exact content (`minify:"none"`). A small response does not prove fidelity or absence.

For LSP, the tool name does not prove semantic resolution: native document-symbol output is syntactic. Read `data.lsp.source` (debug only), the evidence metadata, and completeness. An unavailable provider, an unsupported operation, a failed anchor, and a valid empty result each need a different recovery.

`astTopology` returns candidate import and re-export edges within the scanned scope. `transitiveEdge:true` marks a direct condensation edge that also has another path, not an indirect import. `topologicalLayer` follows the query's direction, so dependents and dependencies can assign different layers; it is not an architecture label. `includeTests:false` stops tests from acting as retention roots; it does not evaluate conditional compilation or remove every edge into test code. Keep edge kinds, coverage diagnostics, configuration, and snapshots when you combine results. AST declaration identifiers describe source occurrences, not bindings that stay stable across edits; use exact source and LSP for identity and impact claims.

## Executable continuations

Follow-up calls use two channels. The rule lives in core `continuationChannels.ts`, exported as `CONTINUATION_CHANNELS` from `@octocodeai/config/schema`.
- **`next`** holds pages and coverage continuations: `nextPage` and other `next*` pages, `continue*`, `expand*`, `restart`, `retry`, `searchUnpatchedFile`, the `next.clasify` walk, and `responsePagination.next`. The response is incomplete without them. A row that carries one (other than `restart` or `next.clasify`) reports `complete: false` or `isPartial: true`. Follow every one the claim needs, including nested captures, diagnostics, history collections, and content windows.
- **`hints`** holds optional guidance: `hints.text` lists prose tips, and every other entry is a lead `{tool, query}`, such as `hints.read`, `hints.readPullRequest`, `hints.viewRepo`, `hints.textSearch`, `hints.narrowScope`, or `hints.clasify`. Skipping a lead never leaves the result incomplete.
- A read of evidence the row withholds (a capped file's `readHits`, a capped body's `readDeclaration`, a clipped line's `wholeLines` or `readBoundedLines`, narrowed patches' `readFullPatches`) is a `next` page, never a lead. Repeats of one kind merge into as few calls as the row limit allows.

| Returned location | Query shape | How to call it |
|---|---|---|
| `results[].data.next.<name>` | A whole tool input, `{"queries": [row, …]}`. | Pass `query` unchanged as the named tool's arguments. |
| `results[].data.hints.<name>` | A whole tool input. | Pass `query` unchanged when the lead is useful. |
| `responsePagination.next` | A complete outer request with its own `queries`. | Pass `query` as the arguments. Do not wrap it in another `queries` array. |

Every `next.*` and `hints.*` query is complete: it carries the page, snapshot, and offset fields the published schemas leave out. A same-tool page or lead inherits the row's `mainGoal` and `reasoning` only when the source query sent them; a cross-tool lead takes no brief, but keeps one it already carries. Every continuation inherits `debug: true`. The CLI accepts the returned query or envelope as `<tool> '<query JSON>'`. A numeric cursor alone is not a continuation. Keep the operation, scope, revision, filters, bounds, and unrelated page axes.

PR menus are exact reads: `readSelectedPatches` names its files (a ranking guess, `confidence: "high"`), and a literal search of every patch needs the caller's literal, so it is never offered as a placeholder. An issue read adds `closedBy` (`{number, state, mergedAt?}`, merged first, at most 25; more set `isPartial`, `terminalLimit`, and `partialReasons:["closingReferenceLimit"]`) and `hints.readPullRequest` (every patch when the fix is small, otherwise its body and file inventory). `ghSearchHistory`'s `hints.readPullRequest` targets one row; read any other row's number the same way.

| Page layer | Controls | Identity and stopping rule |
|---|---|---|
| Collection | `page`, `pageSize`, `matchPage`, or operation cursors | Follow `next` until the collection is complete. Not every provider search has snapshot isolation. |
| Selected content | File reads: `unit`/`offset`/`length`; history text: `offset`/`length` | Use returned offsets; do not compute them from displayed text or byte lengths. |
| Snapshot-aware operation | The operation's `snapshot` | Keep it. On a restart, discard earlier pages and run the restart query. |
| Whole-response text | `responseOffset`, `responseLength`, `responseSnapshot`, `responseScope` | Keep the token. `responsePagination.restart:true` means discard earlier text pages and run its offset-zero continuation. |

`responseScope` selects what an explicit window pages: `text` (default), `structured` (the serialized envelope, as `responseWindow` fragments that concatenate into the envelope JSON), or `rows` (whole rows as complete envelopes). A response larger than `output.pagination.defaultCharLength` (default 50,000) with no explicit window pages automatically by rows, and both surfaces carry complete rows. A partial text page leaves `structuredContent.results` empty to avoid repeating the payload: empty rows there do not mean no results, so read the text and follow `responsePagination.next`. A single complete text page keeps its rows.

The response token names one captured response. While the runtime cache holds it, its pages replay the same output even if a file or provider changes; they do not rerun the query. To see current source, start a new query without response fields. If the capture is gone, the runtime can run the query again; a different response gives restart metadata, not mixed pages. The token does not freeze a provider or replace a tool's own snapshot. Page headers are presentation, not source text.

A typed terminal limit is a boundary that cannot page further. Narrow the scope, choose another evidence surface, or report the limit. Do not raise an unsupported bound again and again, invent a continuation, or turn a terminal result into an absence claim.

## Paths, shared fields, and anchors

Local results can replace absolute paths with a relative `path` and a top-level `root`. Join them before you build a follow-up by hand. A `structureSearch` group `dir` resolves against `root` the same way: join `dir`, `/`, and the entry name without its ` (<fields>)` suffix. An `astTopology` row groups files under the scanned directory: join `root`, the row `path`, and each `file`. Do not apply `root` to GitHub paths or URLs. Returned `next` and `hints` calls and clone `location` objects keep callable paths (local paths relative to the workspace root that `root` names); prefer them.

`shared` holds identical scalar fields removed from object entries in arrays directly inside row `data`. Apply them to those entries only, not to every nested object. Identity, path (including a listing `dir`), anchor, kind, and reason fields stay per entry. A symbols container's own `shared` applies only to its `members`. Source text, snippets, and captures are evidence; do not rewrite them with `root`.

| Anchor | Units and scope |
|---|---|
| File-read `ranges`, LSP `lineHint` | 1-based source lines. `lineHint` must be the observed symbol line. |
| File-read `matchedLines` | The matched source lines. Context `matchRanges` can start earlier and are not the same anchors. |
| LSP `orderHint` | Picks among repeated names on the observed line. |

Anchored LSP operations need `path`, `symbolName`, and `lineHint`. Document operations (`documentSymbols`, `diagnostic`) need only `path`. `workspaceSymbol` needs `symbolName` and `path` or `workspaceRoot`. Read the exact source first: minified output or a search snippet does not give a precise position.

## Connections between tools

| From | To | Carry forward and verify |
|---|---|---|
| `ghSearchCode` / `ghStructure` | `ghGetFileContent` | Owner, repository, path, and ref. Indexed code search has no reliable line identity; fetch the source. |
| `ghSearchHistory` | `ghGetHistoryItem` | Number or commit ref, owner and repository, and the singular operation. Prefer the emitted call. |
| `ghGetHistoryItem` | `ghGetFileContent` or another history read | The changed path and the right revision or diff side; continue each history surface on its own. |
| `artifactSearch` | Repository search or clone | Repository host, owner and name, and the package subdirectory. A repository link is metadata, not source. |
| `ghCloneRepo` | Local tools | `data.location.localPath` and checkout metadata; limits in [External to local](OCTOCODE_WORKFLOWS.md#external-to-local). |
| `localSearch`, `structureSearch`, `astSearch`, `astTopology` | `localFetch` | Path (joined with `root`) and source line. |
| `localSearch` / `ghSearchCode` (wide page) | `clasify` | The `hints.clasify` lead, unchanged; present only while `clasify` is available. It asks the query's `mainGoal` when sent, else the searched phrase. |
| `localFetch` | `lspSearch` | Exact path, symbol, and source line. |
| `lspSearch` | Exact read or lexical recovery | Returned locations (`path` + 1-based `displayRange`) or the emitted `hints.read`, `hints.textSearch`, or `next.retry`; keep provider and completeness limits. |

Check these handoffs through the public interface, not only by asserting that a `next` or `hints` object exists. Acceptance levels: [TOOL_QUALITY.md](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md).
