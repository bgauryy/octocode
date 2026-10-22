# Tool data and handoff contract

This page owns the shared request and result envelope: rows, numbered content, result shapes, pages, leads, shared fields, and the handoffs between tools. Operation fields: [OCTOCODE_TOOLS.md](OCTOCODE_TOOLS.md). Which tool to call next: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md). Package ownership and the contract pipeline: [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md).

Before an unfamiliar request, read the live input schema. The full catalog (`schema --view full`)'s compact fields are a summary: one flat row per form, `*` required and `?` optional, alternatives joined by ` | `, and the field that tells forms apart written with its values (`[operation:"files", path*, …]` means `{"operation":"files","path":…}`). `--view query` lists the fields you type first, then `continuationFields` (paging, scan and snapshot controls such as `page`, `snapshot`, `matchPage`, `extras`: copy those from `next.*` queries) and `moreFields` (the rest by name). The full view (`schema <tool> --view full`) lists every field with its constraints. `schema <tool>` reports `availability`; an unavailable tool shows `enabled:false` and, when a setting gates it, its `envVar`. Enabling a tool does not install a language server or supply provider credentials.

```sh
npx octocode schema
npx octocode schema astSearch --view query
```

Input preparation can add documented defaults or trim text fields; invalid numeric values and unknown fields are rejected, never clamped or dropped. MCP leaves `outputSchema` out of discovery to save context; core and native still validate every result against it. Responses carry `structuredContent` and a matching text representation.

## Requests and result rows

Each call uses one tool and an outer `queries` array of 1–5 queries; `clasify` rows go in `queries` too. One bare query object runs as a one-row call on every surface; a bare array is rejected. Independent queries can batch; a query that needs a prior result waits for it. `mainGoal` and `reasoning` (optional, ≤500 characters each; [briefs](OCTOCODE_WORKFLOWS.md#briefs-maingoal-and-reasoning)) do not supply missing runtime fields. `extras` (optional, per query, `clasify` included) lists the opt-in output kinds the row adds back: see [Minimal by default](#minimal-by-default).

MCP returns the envelope under `structuredContent`; CLI JSON output is the envelope. Tool payloads and their follow-ups are row-local under `results[index].data`.

The text channel (YAML by default) is compacted further; `structuredContent` and JSON keep the envelope:
- A single-row response drops the `results: - index: 0 data:` wrapper; a batch keeps it.
- Path-only search rows (`resultView:"files"`) render as `path`, and count rows as `path (count)`.
- `localFetch` and `ghGetFileContent` print content verbatim after the metadata, under `content (source lines):` when numbered or `content (copy-safe):` otherwise. A batch, or a row with a `status`, starts each block with `result: <index>` (plus ` (<status>)`).

| Field | Meaning |
|---|---|
| `results[].index` | Zero-based input position. Keep it when a batch has mixed outcomes. |
| `results[].status` | Omitted on a successful nonempty row. `empty`, `error`, and `outOfRange` are distinct outcomes. `outOfRange` means a page past the last: its reason names the requested and final pages, `next.restart` restarts the read, and the CLI exits 1. Read the reason before interpreting an empty result. |
| `results[].cache` | Only with `extras:["receipts"]`. `1` marks a cached primary response; it does not prove source freshness. |
| `results[].meta.evidence` | With `extras:["diagnostics"]`. `kind` and `confidence`: evidence origin and strength, not complete coverage. |
| `results[].meta.diagnostics` | Optional diagnostic codes, hints, and partial state. |
| `results[].data` | Payload, pagination, coverage, errors, `next` pages, and `hints`. An empty or error row carries its recovery leads; `hints` adds a sentence only for a per-call fact (a count, a name, a ref) that no lead or error text states. A failed `clasify` evaluation row nests the same `{errorCode, error, hints:[string]}` under `error`. Section assessment preserves resource recovery and coverage alongside a row error; each section/question has its own typed outcome. |
| `root`, `shared` | Compression metadata; see [Paths, shared fields, and anchors](#paths-shared-fields-and-anchors). |
| `responsePagination` | Whole-response pages, restarts, snapshot changes, or an `oversized:true` disclosure for an indivisible value. `hasMore:false` with no `next` is terminal. |

An outer `isError:false` does not mean every row succeeded. Never infer success, absence, or completeness from a missing field. `answerReady` is not a universal member of `meta.evidence`; read the operation's pagination, `coverage`, `clipped`, and terminal-limit fields.

### Numbered source content

When a `localFetch` or `ghGetFileContent` row returns original source lines (line ranges, match windows, `fullContent`, and line pages), `content` is numbered like `cat -n`, without padding:

```text
95	        self._thread_sharing_count = 0
96	
--
255	    def close(self):
```

- Each line is `<line>` + TAB + source text. Strip everything up to the first TAB before you copy text into an edit or a `matchString`.
- Non-adjacent windows are separated by one unnumbered `--` line (grep's group separator); the numbers on either side state the skipped lines.
- The numbers state the returned lines, so a numbered row omits `sourceLineRanges` (and a `localFetch` row omits `startLine`, `endLine`, and `returnedLines`). `matchedLines` is omitted when every returned line matched.
- Size counts (`returnedChars`, `sourceBytes`, `returnedBytes`; all with `extras:["stats"]`) measure the source text, not the prefixes.
- `minify:"standard"` and `"symbols"` views carry the same gutter: each kept line cites its source line and dropped lines leave gaps. A compaction that would join source lines is not used: that window keeps every line whole under its own number (trailing whitespace and blank lines dropped). `contentView` names the view when it is not the tool's default. Cite these views by line number; copy or quote exact text only from a read with `minify` omitted or with `matchString`.
- Line windows of a whole-file `minify:"standard"` read count raw source lines, like the gutter: a page covers `offset+1` through `offset+length`, the lines the view dropped there included, so the pages reach every source line exactly once.
- Byte windows address the selected view: `offset`, `length` and `totalBytes` count the view's text, so a `unit:"bytes"` page of the standard view is numbered view text (the way a `fullContent` walk pages an oversized line), and a byte page with `minify` omitted is verbatim source with `sourceLineRanges`. Content whose lines no longer map one-to-one stays verbatim and keeps `sourceLineRanges`.
- `ghGetHistoryItem` patch text uses the same separator on the new side of each hunk: `87\t+added` and `86\t context` (new-file numbers), `85\t-removed` (old-file number), and a bare tab before a `\ No newline` marker. `@@` lines stay verbatim. Patch `offset` counts the numbered text.
- A `localSearch` row with `contextLines > 0` numbers its window (`matchedLines` still lists the matches; a truncated window stays verbatim). A repo-scoped `ghSearchCode` `matchIn:"file"` row lists keyword lines as `lines: ["<line>\t<text>", …]`.

### Search result shapes

- **`localSearch`** without `pageSize` or `matchPageSize` fills the response window from the first page. Content pages retain at most 20 files; path and count views use adaptive file counts. A fitting result is shown whole with no paging fields; a larger one continues through `next.nextPage`. On a default content page, `pagination` counts hit rows and `totalItems` counts the rows across all pages, never files or bytes; on a `pageSize`/`matchPageSize` page and in path and count views it counts files; a file's per-file `pagination.moreLines` lists up to 24 later line numbers, and `moreLinesUnlisted` counts the rest. Either field set restores the file-page × match-page grid: `next.nextMatchPage` finishes the current files (`pagination.hasMore` stays true while they have rows left) before `next.nextPage` advances to the next files, so following the emitted route never repeats a file page. A terminal match-cursor limit reports the unreachable suffix while still allowing later file pages. With `extras:["hints"]`, a complete result over at most three files carries `read` in `hints`: a `localFetch` `ranges` read (±6 lines) of the top file's hits, or a `matchString` read when they need more than 10 ranges. An invalid alternation (`a(|b`) gets `repair` in `hints`, which escapes each broken alternative and keeps regex mode; a single invalid anchor gets the literal repair.
- A `localSearch` first logical file/match page can disclose `skippedPaths: {entries, sampled}`. Each observed entry keeps its `path`, `cause` (`ignored`, `hidden`, or `default-excluded`), and optional deciding `reason`. These entries describe observed skipped paths, not every descendant of an unwalked directory; `sampled:true` marks bounded diagnostic samples or incomplete walks. Counts stay in short hints, and `includeSkipped` in `hints` retains the executable scope recovery.
- **`ghSearchCode`** reads every file of a `matchIn:"file"` page (repo-scoped or owner-wide) through the contents cache at the commit its index hit names, once per distinct blob (core API quota, no extra code-search calls; `contentReads` with `extras:["stats"]`). Each resolved row has `lines` in place of index fragments: every keyword line, or with several keywords the lines that hold all of them (each keyword's first line when none does), up to 5, with `matchCount` and a `readHits` read of the keyword lines when there are more: when the listed lines are the file's first keyword lines it continues after the last one shown (`ranges` from the next line to the end), otherwise it re-reads every keyword line. Rows with identical evidence are listed once, other paths in `alsoAt`. `data.commitSha` names the commit read, and `owner`/`repo` appear once on `data`. A row with no keyword line or an unreadable blob keeps its fragments and sets `lineResolved:false`. With `ref`, every row is read at that ref's commit (`data.commitSha`); the candidates still come from the default-branch index, and no row keeps default-branch text. A path or hit absent at the ref is `atRef:false`; an unreadable row is `lineResolved:false` with a `readHits` lead at the ref commit. When the ref is not the default-branch head, a warning names both commits, and `viewRepo` in `hints` (first, with `extras:["hints"]`) lists the ref with `ghStructure`: files only at the ref are not in the index. Fragment `matches` need `extras:["details"]`, and their `matchIndices` `extras:["diagnostics"]`. With `extras:["hints"]`, `read` in `hints` reads the first resolved file at `data.commitSha`: its declaration by `matchString` with `block:true` when the hit opens one, else by one `ranges` entry; an unresolved top file's read is pinned to that commit, and a file absent at the ref gets no read.
- **Location rows** are objects named like the inputs that take them. One packed string is allowed: `"<line>\t<value>"`.
  - An `astSearch` symbols outline (and an `lspSearch` `documentSymbols` one) lists declarations in source order, every one an object: `{symbolName, kind, line, endLine?, docStartLine?, exported?, exportedAs?, startLine?, column?, parent?, parentLine?, shared?, members?}`. A container nests its members in `members`, and they name no `parent`; its `shared` states the `kind` or `exported: true` that all of its 2+ members have. Only a member whose container a filter dropped stays top level with `parent` (and `parentLine`).
  - `pagination` counts top-level rows: `totalItems` is the number of top-level rows over all pages, `pageSize` takes that many, and a page never splits a container from its members.
  - `symbolName` + `line` anchor `lspSearch` as-is, and `line`–`endLine` is a `localFetch` range. The YAML text prints declarations as an indented outline.
  - A `match` row is `{line, endLine?, value}` (`value` preserves the matched source, or its bounded exact prefix with `clipped:true`); `captureText:true` adds `endColumn` and `metavarRanges`. Quoted whitespace, line breaks, indentation, and literal content are preserved; a match is not necessarily a whole source line. A `localSearch` hit inside a declaration carries `enclosing: {symbolName, kind, line, endLine}` on the first row of the file inside it; later rows inside it carry `enclosingLine` (that declaration's name line: the `enclosing.line` of the nearest earlier row of the file that has it).
  - `lspSearch` `references` and `callers`/`callees` group rows per file under `files` as `{path, matches}`. A reference row is `{line, column, endLine?, value}`; a call row is `{symbolName, kind, line, endLine, sites: [{line, column}], path?, detail?, via?, source?}`.
  - Columns are 1-based UTF-16 units everywhere. The YAML text prints each such row on one line.
- **`structureSearch`** fills the response window when `pageSize` is omitted. `tree` and `files` rows are `{"dir":"<workspace-relative dir>","files":["<name>[/][ (<fields>)]", …]}` groups, the listed path's own entries included. `/` marks a directory; the fields are the size in bytes (every non-directory), `symlink`, `totalLines=N`, and `modifiedMs=N`, opened by the last ` (`. A page that continues a group repeats its `dir`. `sort:"path"` gives each directory one group; other sorts keep their order. A `"name/"` entry is left out when that directory's group is on the page. `files` sorts by path by default (walk order) and stops once `maxEntries` is filled; that cut reports `clipped`, `partialReasons:["maxEntries"]`, `atLeast` (a lower bound), and, on the listing's last page, `next.continueWalk`, which widens `maxEntries` and resumes after the listed rows (`scanOffset`). Other sorts walk the whole scope and report `totalAvailable`. A walk cut by its time budget (`partialReasons:["walkDeadline"]`) offers `next.continueWalk` on its last page, which resumes at the first unscanned entry (`walkOffset`); only a cut that cannot resume is terminal.
- **`next.continueWalk`** has two meanings: in `structureSearch` it resumes a walk cut by its time budget (`walkOffset`); in `lspSearch` it expands a hierarchy node.
- **`next.expandScan`** resumes a cut `astSearch` scan: the query carries `scanOffset`, so append its rows. It rides the last page of the scan.

### Minimal by default

Every output field has one class, declared once in the core output schema (`fieldClass`): evidence (the default), next (continuations, cursors, snapshots), disclosure (partial, coverage, caps, warnings), or extra with a kind. The contract lists each tool's extra fields by kind (`extraPaths`); the runtime drops each kind the row's `extras` does not name, then removes fields that assert nothing.

- `extras` is an optional per-query array of opt-in kinds, on every tool including `clasify`. The MCP instructions name it once; published schemas leave it out, `schema <tool> --view query` lists it under `continuationFields`, and the full contract (`schema <tool> --view full`) shows it.

| Kind | Adds back |
|---|---|
| `hints` | Row-level `hints` leads on a successful row. |
| `details` | Secondary details and full rows: `ghSearchCode` fragment `matches`, file reads' `matchedLines`, `ghSearchRepo` `createdAt`/`license`/`topics`, full `ghGetHistoryItem` later-page headers. |
| `stats` | Scan stats and counters: char, byte, and line counts, `selectedMatchCount`, `filesScanned`, `modified`, forks, `updatedAt`, page counters. Inline classification adds per-call `selection.usage` with known/unknown provider usage and replay counts. `clasify` always returns its provider `model` and `usage`. |
| `receipts` | Provenance: `lastModified`/`lastModifiedBy` (`ghGetFileContent` makes the extra commits request only then), `cache`, `searchEngine`, the `lsp.receipt`, registry URLs. |
| `echo` | Request echoes (`effectiveQuery`, `workspaceRoot`, `operation`), the top-level `snapshot`, `false`/`0` defaults, and the pagination of a finished single page. |
| `diagnostics` | `meta` (evidence and diagnostics), info-level diagnostics, `matchIndices`, `httpStatus`, `requestId`, `documentationUrl`, `graphqlFallback`. |

- Identity fields stay in minimal rows: `owner`, `repo`, `path`, and `ref` on GitHub rows.
- `hints` is an ordered list: the recovery first, then each further finding in the order the tool wrote it, a repeat once, none dropped (a finding never replaces the first or hides in `warnings`). It shows only on empty or failed rows, and only for a per-call fact; general repairs (fewer keywords, a shorter literal, the exact anchor) live once in the tool description. A successful row states a fact it needs (a clamp, a skipped scope) as a `warnings` entry.
- A validation error (the whole-call envelope or an isolated batch row) carries `repair` in `hints`, the corrected call, only when each issue has one fix: an unknown field dropped (unless a known name is a near miss or it is a root `mainGoal`), an unlisted array value dropped, an unlisted scalar enum value mapped to the one listed value it means (never dropped), a number clamped to its bound (a size or count of zero or less is set to its minimum when that is the field's default, else dropped so the default applies, lossy and named; below the minimum of a field the row requires, no lead, since any bound would be a guess and the message names the allowed range), an unknown field named for a kind of value a row field takes (the field's `x-valueKinds` metadata: `branch:"main"` → `ref:"main"`) moved into that field, exactly, unless the row already sets it, a value the row spells another way read from the row (`owner:"a/b"`, a `.git` suffix, a GitHub web URL, a line-range typo). A lead that drops or guesses a value is `confidence:"low"` and names those fields. The fix is derived from the issues, never from a tool or field name, and the corrected call must pass the same admission before it is offered. Any issue that needs a value only the caller knows leaves the call without a lead; its message names the field's fix.
- On a successful row, row-level `hints` leads appear only with `extras:["hints"]`, except the contract's `defaultLeads` (`apply`, `exploreClone`, `viewFileCount`, `includeArchived`, `includeBuildScripts`, `includeHidden`, `includeSkipped`, `narrowScope`, `readDiagnostics`). Empty and failed rows keep all their leads. Item-level leads (a PR row's content menu) stay, and a `clasify` source row's `read` in `hints` (the read of exactly what was judged) always shows.
- A row whose `next` holds pages states the rest once, by its always-visible `pagination` block (counters and `hasMore`) and `next.*`; no `warnings` entry repeats the count. Only a page a split row moved to another response part is worded (`7 more changed files (responsePagination.next)`), because the part's own `next` no longer holds it. `partialReasons` say why a row is partial.
- Never dropped: every `next` page and every `hints` lead the lead policy keeps (with its own `snapshot`), warnings and errors, scan scope on an empty search, and row confidence signals such as topology `completeness` and `confidence`. Error rows keep every field that is not an extra.
- The `pagination` counter block (also each `contentPagination` section, a search's `matchCount`/`fileCount`, a tree `summary`) is never an extra: it shows on every page that has more to read or follows another page. Only a finished single page drops it, and `extras:["echo"]` restores it. It has the same keys on every page and tool: `currentPage` or `offset`, `pageSize` or `length`, `totalItems`/`totalPages` when known, and `hasMore`. The cursor (`nextPage`, `snapshot`, `resultId`) rides the `next` page, never the block. `isPartial` is not sent beside a row's own `pagination.hasMore` with its `next` page: they already say more is unread. It stays for any other partial cause (`partialReasons`, `terminalLimit`, a partial source with no page). A failed row keeps only what explains the failure: no empty arrays, no `snapshot`, no finished single-page `pagination`. A read window's `pagination` (`localFetch`, `ghGetFileContent`) is evidence, not a stat: its offset places the text.
- A continuation is `{tool, query}`: no `why` (the tool description says what each lead kind is for) and no `confidence` (the runtime ranks leads with it, then removes it). A registry row states its release ref's `verification`; its leads do not repeat it.
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

File reads (`localFetch`, `ghGetFileContent`) return exact content when `minify` is omitted; `minify:"standard"` drops comments and indentation (the gutter keeps source line numbers), and `matchString` reads are always verbatim. A small response does not prove fidelity or absence.

For LSP, the tool name does not prove semantic resolution: native document-symbol output is syntactic. Read `data.lsp.source` (with `extras:["receipts"]`), the evidence metadata, and completeness. An unavailable provider, an unsupported operation, a failed anchor, and a valid empty result each need a different recovery.

The CLI's `graph query` returns syntax-derived import, re-export, and call edges within the ingested scope ([graph](OCTOCODE_CLI.md#graph--persisted-code-graph)); the inferred entrypoints, or those `issues --entry` names, decide what is reachable. AST declaration identifiers describe source occurrences, not bindings that stay stable across edits; use exact source and LSP for identity and impact claims.

## Executable continuations

Follow-up calls use two channels. In every tool and both text encodings, a row's `pagination` and `<part>Pagination` blocks, then `next`, come before its evidence rows, so a client that cuts long output keeps the continuation. A stale snapshot ends the page (`hasMore:false`, `next.restart` only). One lead verb per kind: `continueWindow` (a file window), `continuePatch`, `nextPage`, `nextMatchPage`, `nextFilePage`. Core `continuationChannels.ts` owns the rule; the contract ships it as `continuationChannels`.
- **`next`** holds pages, coverage continuations and explicit recovery routes: `nextPage` and other `next*` pages, `continue*`, `expand*`, `restart`, `retry`, `searchUnpatchedFile`, and `responsePagination.next`. Follow every page the claim needs, including nested captures, diagnostics, history collections, and content windows. Missing coverage is reported through pagination or partial/completeness fields. `next.unfiltered` recovers the complete original view after `clasifyContext` selection, and `next.unfilteredUnknown` only the evidence judged unknown, each when offered. These recovery routes alone do not mark selected evidence incomplete ([context selection](OCTOCODE_TOOLS.md#context-selection)).
- **`hints`** is a list of optional guidance strings. Per-call facts are plain text; suggested calls use `<action>: <tool> <JSON input>`, with `Approximate:` before the tool when the suggestion has low confidence. The JSON is a complete, validated input. Duplicate text and calls appear once. Distinct suggestions are not limited by a fixed count. Skipping a suggestion never leaves the result incomplete. Search and fetch emit no automatic suggestion that rereads evidence they already returned.
- A read of evidence the row withholds (a capped file's `readHits`, a capped body's `readDeclaration`, a clipped line's `wholeLines` or `readBoundedLines`, the ranges after the span a streamed multi-range page stopped inside, `readOtherRanges`) is a `next` page, never a lead. A `matchString` patch view shows what was asked: its whole-patch re-read `readFullPatches` is an opt-in lead. Repeats of one kind merge into as few calls as the row limit allows.

| Returned location | Query shape | How to call it |
|---|---|---|
| `results[].data.next.<name>` | A whole tool input, `{"queries": [row, …]}`. | Pass `query` unchanged as the named tool's arguments. |
| `results[].data.hints[]` | Optional text, or `<action>: <tool> <JSON input>`. | When useful, call the named tool with the JSON input unchanged. |
| `responsePagination.next` | A complete outer request with its own `queries`. | Pass `query` as the arguments. Do not wrap it in another `queries` array. |

Every `next.*` query and suggested call in `hints` is complete: it carries the page, snapshot, and offset fields the published schemas leave out. Continuations drop the row's `mainGoal` and `reasoning`, except `clasifyContext` pages, which keep them for selection replay. A same-tool page or lead inherits the row's non-empty `extras`; a cross-tool lead inherits only its `hints` opt-in. The CLI accepts the returned query or envelope as `<tool> '<query JSON>'`. A numeric cursor alone is not a continuation. Keep the operation, scope, revision, filters, bounds, and unrelated page axes.

| Page layer | Controls | Identity and stopping rule |
|---|---|---|
| Collection | `page`, `pageSize`, `matchPage`, or operation cursors | Follow `next` until the collection is complete. Not every provider search has snapshot isolation. |
| Selected content | File reads: `unit`/`offset`/`length`; history text: `offset`/`length` | Use returned offsets; do not compute them from displayed text or byte lengths. |
| Snapshot-aware operation | The operation's `snapshot` | Keep it. On a restart, discard earlier pages and run the restart query. |
| Whole-response pages | `responseOffset`, `responseLength`, `responseSnapshot`, `responseScope` | Copy the saved token and window; saved-read continuations omit the original queries. Changing a row-page length changes its plan and requires a restart. `responsePagination.restart:true` means discard earlier pages and run its offset-zero continuation. |

`responseScope` selects what an explicit `responseLength` pages: `rows` (whole rows as complete envelopes, the default), `text` (windows of the rendered text), or `structured` (the serialized envelope, as `responseWindow` fragments that concatenate into the envelope JSON). Paging checks both the rendered text and structured content against the page: `responseLength`, else a fixed 20,000-character page. Unless `responseScope` is `text` or `structured`, an oversized response pages by rows, and both surfaces carry complete rows; the continuation names the scope it pages by. A row page, its `responsePagination` block included, fits `responseLength`; only a fragment that cannot be divided further exceeds it, and that page says `oversized:true`. Row pages each repeat the envelope's shared metadata, so their text does not concatenate into the full response (every row and fragment still arrives once, in order); only `text` and `structured` windows reassemble byte for byte. A partial `responseScope:"text"` page leaves `structuredContent.results` empty to avoid repeating the payload: empty rows there do not mean no results, so read the text and follow `responsePagination.next`. A single complete text page keeps its rows.

A row too large for one page carries `rowPart` markers. Standalone `clasify` partitions keyed answers between whole questions; every probability distribution stays intact, and an individually oversized answer is disclosed instead of truncated. Section assessment pages the declared resource, section and answer collections; coordinate pairs and answer distributions stay atomic. Its shared question definitions appear in the first display part. `next.nextSectionPage` performs more assessment work, while `responsePagination.next` only reads already settled output. Collect its evidence across every part: container descriptors can repeat while already-delivered record lists become empty. For example, an outline's later parts can carry `members:[]` while continuing `exportedAs`. Executable `next` and `hints` queries remain whole; their argument arrays are never evidence fragments. A call too large for the target stays executable and receives an oversized-page disclosure.

Ordinary `localFetch` and `ghGetFileContent` line pages fit the response page (`responseLength`, else the automatic page), accounting for serialized text, escaping and source-number gutters before rendering, for single reads as well as batches. Explicit line lengths remain maxima under the response window. A byte fragment, one oversized source line, or a completed ranked read without a reusable plan can remain indivisible and exceed the target. An indivisible whole-row page keeps `responsePagination.oversized:true` even when it is the only page, with no invented continuation. A failed sizing reread retains the original evidence. Copy every emitted continuation; sizing never authorizes dropping source or running classification again.

The response token names one captured response. While the runtime cache holds it, its pages replay the same output even if a file or provider changes; they do not rerun the query. To see current source, start a new query without response fields. If the capture is gone, the runtime returns `responseSnapshotExpired` without running the query. A deliberate new query can acquire source and run classification again. Saved responses are session-local in memory mode; persistent mode also stores them for continuations across CLI invocations. Both tiers have bounded retention. A memory-only CLI invocation returns the complete result with a warning instead of issuing a display token that a new process cannot read. GitHub-derived pages also expire when the current credential identity changes; checking that identity does not fetch source or refresh credentials. `responsePagination.replayed:true` marks a saved page; usage inside it is historical. The token does not freeze a provider or replace a tool's own snapshot. Page headers are presentation, not source text.

A typed terminal limit is a boundary that cannot page further. Narrow the scope, choose another evidence surface, or report the limit. Do not raise an unsupported bound again and again, invent a continuation, or turn a terminal result into an absence claim.

## Paths, shared fields, and anchors

Local results can replace absolute paths with a relative `path` and a top-level `root`. Join them before you build a follow-up by hand. A `structureSearch` group `dir` resolves against `root` the same way: join `dir`, `/`, and the entry name without its ` (<fields>)` suffix. Do not apply `root` to GitHub paths or URLs. Returned `next` and `hints` calls and clone `location` objects keep callable paths (local paths relative to the workspace root that `root` names); prefer them.

`shared` holds identical scalar fields removed from object entries in arrays directly inside row `data`. Apply them to those entries only, not to every nested object. Identity, path (including a row's `file` and a listing `dir`), anchor, kind, and reason fields stay per entry. A symbols container's own `shared` applies only to its `members`. Source text, snippets, and captures are evidence; do not rewrite them with `root`.

| Anchor | Units and scope |
|---|---|
| File-read `ranges`, LSP `line` | 1-based source lines. `line` must be the observed symbol line. |
| File-read `matchedLines` | The matched source lines. Context `matchRanges` can start earlier and are not the same anchors. |
| LSP `orderHint` | Picks among repeated names on the observed line. |

Anchored LSP operations need `path`, `symbolName`, and `line`. Document operations (`documentSymbols`, `diagnostic`) need only `path`. `workspaceSymbol` needs `symbolName` and `path` or `workspaceRoot`. Read the exact source first: minified output or a search snippet does not give a precise position.

## Connections between tools

| From | To | Carry forward and verify |
|---|---|---|
| `ghSearchCode` / `ghStructure` | `ghGetFileContent` | Owner, repository, path, and ref. Indexed code search has no reliable line identity; fetch the source. |
| `ghSearchHistory` | `ghGetHistoryItem` | Number or commit ref, owner and repository, and the singular operation. Prefer the emitted call. |
| `ghGetHistoryItem` | `ghGetFileContent` or another history read | The changed path and the right revision or diff side; continue each history surface on its own. |
| `artifactSearch` | Repository search or clone | Repository host, owner and name, and the package subdirectory. A repository link is metadata, not source. |
| `ghCloneRepo` | Local tools | `data.location.localPath` and checkout metadata; limits in [External to local](OCTOCODE_WORKFLOWS.md#external-to-local). |
| `localSearch`, `structureSearch`, `astSearch` | `localFetch` | Path (joined with `root`) and source line. |
| `localFetch` | `lspSearch` | Exact path, symbol, and source line. |
| `lspSearch` | Exact read or lexical recovery | Returned locations (`path` + 1-based `displayRange`) or the emitted `read` in `hints`, `findText` in `hints`, or `next.retry`; keep provider and completeness limits. |

Check these handoffs through the public interface, not only by asserting that a `next` or `hints` object exists. Acceptance levels: [TOOL_QUALITY.md](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md).
