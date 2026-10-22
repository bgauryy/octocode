# Octocode tools reference

Every tool's fields, defaults, limits, results, and continuations, on MCP and the CLI. Schemas come from `@octocodeai/octocode-core` (published in-repo by `@octocodeai/config/schema`); `@octocodeai/octocode-native` runs them. Other owners: the shared envelope, `next.*` pages, and `hints` leads ([TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md)); tool choice and flows ([OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md)); MCP setup ([OCTOCODE_MCP.md](OCTOCODE_MCP.md)); the CLI ([OCTOCODE_CLI.md](OCTOCODE_CLI.md)); settings ([CONFIGURATION.md](CONFIGURATION.md)). The live schema is the final authority. Quality criteria: [TOOL_QUALITY.md](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md).

## How every tool call works

The CLI and MCP expose the same contracts. Every tool, `clasify` included, advertises a `queries[]` envelope and returns one row per input position. Native admission also runs one bare query object as one row on CLI, MCP and the direct addon; a bare array is rejected. Use the advertised envelope for tool calls and copy continuations unchanged. A failed row, `clasify` rows included, does not erase successful siblings.

### Base call envelope

| Field | Scope | Meaning |
| --- | --- | --- |
| `queries` | Required, outer | 1–5 queries for the same tool. Rows keep their zero-based input `index` and run concurrently (`ghCloneRepo` rows run in order). Batching creates no dependencies between rows. |
| `mainGoal`, `reasoning` | Optional, per query, ≤500 chars each | The research question, and why this row advances it. Only for multi-call research on an unknown; a blank one is dropped. Not an activation switch or proof. With `clasifyContext`, both are extra context for the classification provider. Rules: [briefs](OCTOCODE_WORKFLOWS.md#briefs-maingoal-and-reasoning). |
| `extras` | Optional, per query | Opt-in output kinds (`hints`, `details`, `stats`, `receipts`, `echo`, `diagnostics`): [minimal by default](TOOL_DATA_CONTRACT.md#minimal-by-default). |
| `responseLength` | Optional, outer | Whole-response page size, 1–50,000 chars; it replaces the fixed 20,000-character page, not a tool's own pagination. A response over the page pages through `responsePagination.next`, by whole rows unless `responseScope` says otherwise; `localFetch` and `ghGetFileContent` rows fit their line pages to it. |
| `responseOffset`, `responseSnapshot`, `responseScope` | Optional, outer | Copy the first two from `responsePagination.next`. `responseScope` is what an explicit `responseLength` pages: `rows` (whole rows, the default), `text` (rendered-text windows), or `structured` (serialized envelope). |

Every other field belongs to one tool variant. Fields from different `operation` branches cannot mix, a line range is one `ranges` entry (`"a-b"`), and mutually exclusive selectors cannot combine.

### Schema discovery, variants, defaults, and hints

```bash
npx octocode schema                                     # catalog: names, availability, instructions
npx octocode schema localSearch --view variants         # branches, required fields, minimal examples
npx octocode schema ghGetHistoryItem --view query --select operation=pullRequest
npx octocode schema ghGetHistoryItem --view full        # the whole contract
```

The contract lists `variants` (when a branch applies, its required fields, a minimal example) and `rules` (preparation and validation steps). `next.*` pages, `hints` leads and `hints` tips: [executable continuations](TOOL_DATA_CONTRACT.md#executable-continuations).

### Results, evidence, partial failures, and continuations

Row fields, `status: empty` versus `error`, minimal output and pagination layers: [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md#requests-and-result-rows). Missing output proves neither success nor absence; when a row is partial, run its `next.*` page.

Each `errorCode` is one camelCase name with one exit class (2, 3, 4, 5, or 7), declared once in core and shipped in the contract as `errorCodes`. A call rejected before any row runs (schema-invalid query, malformed or missing JSON) returns one `{"error","errorCode":"invalidInput","details"}` envelope: on CLI stdout in JSON mode, and as MCP `structuredContent` beside an `isError` text that ends `(errorCode: invalidInput)`. `error` is the first repair; `details` lists the others. When every rejected field has one exact fix (an array value the enum does not list dropped, a number clamped to its bound, an enum value mapped to the one listed value it means — case, separators, an on/off word, an abbreviation such as `entryType:"file"` → `"f"`, or a near spelling — an unknown `true` field an enum value names (`merged:true` → `state:"merged"`), a number or boolean sent as a string converted and a whole number or boolean sent to a text field quoted, a line range spelled `"3:6"` or `[3,6]` → `"3-6"`, `owner:"a/b"` split into `owner`/`repo`, a `.git` suffix dropped from `repo`, a field of another form given that form's omitted selector, a field that needs an absent sibling dropped, a bare array wrapped as `{"queries":[…]}`, a nested envelope's rows lifted), the envelope also carries `repair` in `hints`: the corrected call (`tool`, `query`) to run unchanged, repeated in the `isError` text as `repair: {…}`. Dropping any other unknown field (one with no near-miss name), or a field of another form when the row's own selector picks its form, is not exact; nor is a GitHub web URL in `path` read into `owner`/`repo`/`ref`/`path` when the ref (not a full SHA) may contain `/`: the lead still comes with `confidence:"low"`, and `hints` names the dropped or guessed fields. An isolated batch row that fails validation carries the same lead for its own row. A fix that needs a value only the caller knows (a missing field, a misspelled name, an enum value no rule maps to one listed value) has no lead; the message names it. CLI exit codes, including paging (exit `0`) and batch precedence: [CLI exit codes](OCTOCODE_CLI.md#exit-codes).

## Internal, external, and hybrid tools

"External" names the data or provider boundary, not the MCP transport. All fourteen contract tools share the MCP and CLI contracts; availability gates hide or reject a tool per surface.

| Boundary | Tools |
| --- | --- |
| External (GitHub) | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem` |
| External (package registries) | `artifactSearch` |
| Hybrid (provider credentials, then a local checkout) | `ghCloneRepo` |
| Internal (allowed local paths) | `localSearch`, `structureSearch`, `astSearch`, `localFetch` |
| Internal, with a language-server process | `lspSearch` |
| External classification provider (vendor: Jev) | `clasify` |

Availability:

- GitHub tools need credentials ([AUTHENTICATION.md](AUTHENTICATION.md)).
- Local tools are on unless `local.enabled` (`OCTOCODE_ENABLE_LOCAL`) is `false`.
- `ghCloneRepo` is CLI-only (MCP never registers it) and needs persistent storage.
- No MCP description or lead names `ghCloneRepo`.
- LSP needs a compatible server for the file language.
- Public `clasify` needs beta and a nonblank classification API credential; `clasifyContext` on four readers needs only the credential. See the [availability table](OCTOCODE_CLASIFY.md#availability).

## Text, AST, graph, and LSP: choose the evidence you need

Text (`localSearch`), structural AST (`astSearch` `match` with exactly one of `pattern` or `rule`), file graph (the CLI's `octocode graph`), and LSP (`lspSearch`) evidence complement one another; what each establishes and still needs: [evidence boundaries](TOOL_DATA_CONTRACT.md#evidence-boundaries). Proof order (orient, anchor, shape, topology, exact read, identity, run): [local research](OCTOCODE_WORKFLOWS.md#local-research). For file graphs, run `npx octocode graph ingest <dir>` (an unchanged tree reuses its snapshot), then `graph query` ([graph](OCTOCODE_CLI.md#graph--persisted-code-graph)): `deps` or `dependents <file> [--depth n]`, `path <from> <to>`, `cycles` (witness edges and suggested cuts: `issues --detector cycle`), `issues --detector unreachable-file,unused-export,test-only` for dead or test-only code (`--entry <ref,...>` names entrypoints instead of the inferred ones), and `drift --graph <head> --baseline <baseline>` between two snapshots. An answer over a cut scan carries `graph.partialReasons` and a `graph.reingest` command. Keep entrypoints and the ingested scope fixed when you compare runs; a change to either changes what "reachable" means.

## GitHub tools reference

Tokens, precedence, `GITHUB_API_URL` (GitHub Enterprise), OAuth login, and refresh: [AUTHENTICATION.md](AUTHENTICATION.md). Flows: [GitHub research](OCTOCODE_WORKFLOWS.md#github-research).

### Shared GitHub rules

- **Paging.** `page` (1–1000) and `pageSize`. When more remains, run the `next.*` call: `nextPage` or a content continuation (`continueWindow`, `continuePatch`, `nextFilePage`, …). At a cap that cannot expand, metadata reports `terminalLimitReached` and omits unusable continuations. Numeric page, offset, and cursor fields and a raw `nextQuery` are not executable alone. File chunks page a `matchString` view without changing the selector.
- **Out-of-range pages.** `ghStructure`, `ghSearchCode`, `ghSearchRepo` and `ghSearchHistory` answer a `page` past the end with row `status: outOfRange`, `pagination.outOfRange:true`, a `reason` (`page N of M`) and `next.restart` (page 1 of the same query). The CLI exits `1`, as for an empty row.
- **Renamed repositories.** `ghSearchCode`, `ghSearchHistory`, `ghStructure` and `ghGetFileContent` warn `Repository o/r was renamed to n/r; leads use the canonical name.`; the leads carry the new name.
- **Missing refs.** A `ref` that does not resolve (for example `main` in a repository whose default branch is `master`, or the reverse) is a `notFound` error that is never read from another branch. When the repository's default branch is a different ref, the message ends `; the default branch is "<branch>".` and `atDefaultBranch` in `hints` is the same call at that branch, beside `viewRefs`. This applies to `ghGetFileContent`, `ghStructure`, `ghSearchHistory`, `ghCloneRepo`, and a `ghGetHistoryItem` commit or comparison; a comparison names the missing side (`"main" (base) not found`) and offers no retry that would compare the default branch with itself. A full commit SHA, a ref equal to the default branch, or unreadable repository metadata leaves the error as it was.
- **History `path` filters commits only.** GitHub has no path filter for pull requests or issues, so `path` on `ghSearchHistory` is valid only with `operation:"commit"`, and the validation rejects it for a pull request or issue. The route: search commits with `path`, then run `readPullRequest` on a commit headline that ends in `(#N)`.
- **Previews.** Search match values and provider snippets are previews, not pages. Read exact bytes before you quote them.
- **Keywords** (`ghSearchRepo`, `ghSearchCode`, `ghSearchHistory`; ≤100). Literal terms, ANDed, each sent as a bare word or one quoted phrase. Interior double quotes and backslashes are dropped, so `"hello" NOT` becomes the phrase `"hello NOT"` and cannot negate or replace the `repo:` scope. No OR/NOT: use separate queries. `owner` and `repo` must be GitHub names; a value with spaces, quotes, colons, or operators is a validation error.
- **Errors.** Retry only transport failures, timeouts, and 5xx. HTTP 451 is `errorCode: "unavailable"` (blocked for legal reasons); other unmapped statuses are `errorCode: "httpStatus"` with the code in the message; neither is retryable. An oversized error body keeps its classification and adds `(error body exceeded limit)`.

### `ghSearchRepo`

<!-- tool: ghSearchRepo -->
```json
{"keywords": ["code research"], "language": "TypeScript"}
```

| Field | Values and meaning |
|---|---|
| `keywords`, `topics` | ≤100 each; keyword rule above; every topic must match. |
| `language`, `license` | License is an SPDX id (`mit`). |
| `owner` | User or org. Alone (optionally with `sort:"updated:desc"`) it lists the owner's repositories (below). |
| `repo` | With `owner` alone: that one repository's row from the repository endpoint, no search request and no page; the row adds `defaultBranch`. With any filter beside it, a search scoped to that repository (`repo:owner/name`). A renamed repository answers under its new name. |
| `stars` | `">100"`, `"10..50"`. |
| `pushed`, `created` | ISO date, range, or window: `">2025-01-01"`, `"2024-01..2024-06"`, `"30d"`. |
| `includeArchived` | `true` also lists archived repositories; omitted or `false` excludes them; `qualifiers:"is:archived"` lists only them. |
| `matchIn` | `name`, `description`, `readme` (broader, slower). |
| `qualifiers` | ≤200 chars of rarer filters (`forks:>50 size:<5000 good-first-issues:>2 is:public`). Allowlisted keys; no `repo:`/`org:`/`user:`, and no key that has a typed field. |
| `sort` | `best-match` (default) or `<field>:desc`: `stars:desc`, `forks:desc`, `help-wanted-issues:desc`, `updated:desc` (latest push). The form is `<field>:<direction>`, as in `ghSearchHistory`; `artifactSearch` has no sort. |
| `pageSize` | 1–100, default 50. |
| `fields` | Row keys to keep (gh `--json a,b`); each row keeps its id, `owner`+`repo`. |

Malformed counts and dates fail validation and name the accepted forms.

Rows: `owner`, `repo` (name only), `stars`, `language`, `pushedAt`, `archived: true` or `fork: true` when so, and the full `description`; `extras:["details"]` adds `createdAt`, `license`, and `topics`, and `extras:["stats"]` adds `forks`, `updatedAt`, and `pagination` (`totalItems`: count an org's repositories with `owner` + `sort:"stars"`); the field rows are sorted by always shows (`sort:"forks:desc"` keeps `forks`, `updated:desc` keeps `updatedAt`); the cursor is `next.nextPage`, and its warning counts what is left. Leads: `viewRepo` in `hints` (with `extras:["hints"]`: the top row's root listing). An empty search gets `next.broadenSearch`, one batch of up to 5 variants: the search once without each filter it sets, then once without each keyword (a lone keyword of an `owner`+`repo` search drops to that repository's own row); the variant that returns rows names what emptied it. A search inside a renamed repository (empty, or not found by GitHub search) reruns under the new name, with a warning that names it.

Owner listing (`owner` only): the REST listing ordered by latest push (`order: "pushed"`), archived repositories excluded; a page that skipped some names their count in `warnings` and offers `includeArchived` in `hints`. One call reads up to 5 provider pages, so it can return more than `pageSize` rows. `next.nextPage.query.page` is a provider cursor that follows GitHub's `Link` header. No `totalItems`.

### `ghSearchCode`

<!-- tool: ghSearchCode -->
```json
{"keywords": ["useReducer"], "owner": "vercel", "repo": "next.js"}
```

| Field | Values and meaning |
|---|---|
| `keywords` | ≤100; keyword rule above. |
| `owner` | Required: code search cannot span all of GitHub. |
| `repo` | One repository; its top hits carry line numbers. |
| `ref` | Tag, SHA, or PR head: a repo-scoped page reads its hit lines at this ref. Candidates still come from the default-branch index. |
| `path` | Repository directory prefix, not a file path. A file path matches nothing: the empty row says so, on owner-wide searches too, and leads to `filename` with the parent directory as `path`; `viewTree` lists that directory. A name with no extension (`LICENSE`) is not recognised as a file, so its empty row only says a file is `filename` plus its parent as `path`. |
| `extensions` | 1–5, e.g. `["ts","tsx"]`; each is its own indexed search on the same page number, sharing `pageSize` (each lists `ceil(pageSize/k)`), merged into one page. |
| `filename` | GitHub also matches names that contain it; check each path. Not a glob: one is refused before any request, and a glob whose only wildcards are a leading or trailing `*` (`*.test.js`) repairs to its text (`.test.js`) in `next.repair`. |
| `matchIn` | `file` (default: hits) or `path` (paths only). |
| `pageSize` | 1–100, default 50: files per page, not lines. Each file lists up to 5 hit lines; `matchCount` is the file's total and `next.readHits` reads the rest. GitHub serves the first 1,000 results, so the last page is `floor(1000/pageSize)` and `pagination.totalItems` is the reachable count. With `extras:["stats"]`, `contentReads` counts the files the page read through the contents API (core quota) for line numbers. |
| `fields` | Row keys to keep; each row keeps `path` (and `owner`/`repo` on owner-wide pages). Without `lines` no hit lines are read. |

A `matchIn:"file"` search reads each hit's lines at the commit the index matched (`commitSha`), repo-scoped or owner-wide; a `ref` search resolves that ref's commit during the index search and reads every row there. The page's files are read concurrently. Row shapes (`lines`, `matchCount`, `readHits`, `alsoAt`, `atRef`, `lineResolved`, `indexRef`) and the non-head `ref` warning with `viewRepo` in `hints`: [search result shapes](TOOL_DATA_CONTRACT.md#search-result-shapes). With `extras:["hints"]`, `read` in `hints` is a `ghGetFileContent` read pinned to the commit where the hit was verified: the declaration's head line as `matchString` with `block:true` when the hit opens a declaration, else a `ranges` read around the best hit; a file absent at the requested `ref` gets no read. A complete page whose files list fewer hit lines than they have is `isPartial` with `partialReasons:["hitLinesNotListed"]`; `next.readHits` reads the unlisted lines (a page with more pages behind it states its remainder through `pagination`). A page GitHub marks incomplete is `isPartial` with `partialReasons:["providerIncompleteResults"]`, empty or not. GitHub's code-search totals are estimates and its index moves between page reads (the tool description says so once): `totalItems`/`totalPages` can change and pages can repeat or omit files, and a later page drops files an earlier page of the same search listed (compared against the earlier pages still in the response cache, 60 s each), naming the drops and how many earlier pages it could read. A page that holds only repeats is not an empty search. A renamed repository is searched under its new name (an empty search, or one GitHub search cannot resolve), with a warning that names it. A file's `next.readHits` is a `ghGetFileContent` read of the hits the listing did not show: `ranges` are one-based line numbers from the line after the last shown hit to the file's last line (never byte offsets), and `matchString` returns only the matching lines inside them. An empty repository-wide search is unproven absence (the index covers the default branch and lags): `cloneRepo` in `hints` is the `ghCloneRepo` query for the searched repository; `localSearch` the clone to verify. A search with a filter or keyword to drop also gets `next.broadenSearch` (up to 2 variants, within the code-search window's room); one with none gets `viewTree` at the root instead. A path-scoped empty search leads to `ghStructure` on that path.

### `ghStructure`

<!-- tool: ghStructure -->
```json
{"owner": "vercel", "repo": "next.js", "path": "packages", "maxDepth": 2}
```

| Field | Values and meaning |
|---|---|
| `owner`, `repo` | Required; `repo` without the `owner/` prefix. |
| `operation` | `tree` (default); `refs`: `defaultBranch` with its head as `commitSha`, and `tags` as names (a tag name is a ref); with `ref`, only that ref's `commitSha` and `refKind` (`branch`, `tag`, or `commit`; a name that is both resolves to the kind whose ref points at the commit); `extras:["details"]` adds `branches` as a `{"<name>": "<sha>"}` map (the branch list is requested only then). Each page holds the same page number of each listed kind, `next.nextPage` while one has more; `languages`: bytes of code per language on the default branch, largest first. |
| `path` | Directory; `""` or `"."` is the root. |
| `ref` | Branch, tag, or SHA. A missing ref is an error, never a default-branch fallback. |
| `maxDepth` | 1–20; omitted: 1, or 20 with `include`. |
| `include` | 1–100 case-insensitive globs, ORed over paths relative to `path`, as the local tools match (`**/_exception_handler.py`, `src/**/*.ts`). Without `/` a glob matches entry names; a bare word matches names that contain it. An empty listing gets `next.broadenSearch`: the same commit without `include`. |
| `pageSize` | 1–500; default 300 entries (`refs`: default 100, up to 100). |
| `materialize` | Writes this page's listed files (≤50, ≤300 KiB each) to `location.localPath`. |
| `materializeOffset` | 0–500: index in this page; resumes capped blob writes. |

`path`, `maxDepth`, `include`, `materialize`, and `materializeOffset` apply only to `tree`; `ref` applies to `tree` and to `refs` (that one ref's `commitSha`). A directory at GitHub's 1000-entry listing limit is partial with `cloneRepo` in `hints`. A git tree holds only committed content, so every directory is listed and walked (no local prune names apply); `omitted` (first page only) names symlinks and submodules as `"<repository-relative path> (symlink|submodule)"` → 1, since they have no listable content; with `include`, only those its globs keep. Contributors are not listed; `ghSearchHistory` author qualifiers answer who changed a path.

Tree rows under `files` keep the `dir`/`files`/`folders` shape. A file entry is `"<name> (<bytes>[, <YYYY-MM-DD>])"` (the structureSearch form; the last `" ("` opens the fields); a dated folder is `"<name> (<YYYY-MM-DD>)"`. `resolvedRef` appears only when the default branch was resolved, and `commitSha` only when it differs from the requested ref. Page totals are in `pagination`; `summary` counts the whole listing's `files`, `dirs`, and `bytes` (never one page's share) on every page.

Freshness (listing pages with `extras:["details"]`; a default listing sends no date request): a dated entry ends with the day of its last commit (`"routing.py (30472, 2026-08-26)"`, `"middleware (2026-09-23)"`), and `commitDate` is the listed commit's day. A page dates its first 100 entries in one GraphQL request (1 rate-limit point, about 3 s; cached per commit and paths; needs a GitHub token). A larger page names the undated count in `warnings` and carries `next.expandDates`: the listing pinned to the commit SHA at `pageSize:100`, one row per page that holds an undated entry (it can overlap dated entries, never skip one), each dated whole. A failed date request leaves entries undated with one `warnings` line (count and reason); it never fails the listing.

Materialize: for cross-file grep at a ref over MCP (no `ghCloneRepo`), combine `include` with `materialize:true`, then run `localSearch` at `location.localPath` (the listed directory). Files land under `<home>/tmp/materialize/v3/<owner>/<repo>/<commitSha>/`; a manifest beside them records each written file and size, so a later materialize reuses those files and clears a directory it did not write. Folders at `maxDepth` are listed, not written.

Leads: with `extras:["hints"]`, a first listing page offers `read` in `hints`, an outline of its entry file. A missing `path` is a not-found error whose `viewTree` in `hints` lists the nearest existing directory (case-corrected).

### `ghGetFileContent`

Reads one GitHub file without a checkout. Directories: `ghStructure`; a local subtree: `ghCloneRepo` with `path`.

<!-- tool: ghGetFileContent -->
```json
{"owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/config.ts", "matchString": "export", "contextLines": 2}
```

| Field | Values and meaning |
|---|---|
| `owner`, `repo`, `path` | Required. `path` is repository-relative, exact case, no leading slash. |
| `ref` | Branch, tag, or SHA; omit for the default branch. For a PR's head source, use its `headSha`. |
| `fullContent` | Whole file in one page, for small files. Rejects `matchString`, `ranges`, and window controls. |
| `ranges` | 1–10 line spans (`["95-105"]`; a reversed span `"105-95"` reads the same lines). With `matchString`, only hits inside the spans count and their windows stay inside them (a region grep). |
| `matchString` | Every matching slice; a list matches any entry (a grep map). |
| `contextLines` | 0–100, default 3. A matched line over 2,000 bytes (minified source) shows 200-byte windows instead, with `next.wholeLines`. |
| `regex` | `literal` (default; pasted code stays literal), `rust` (linear-time), `pcre2` (lookaround, backreferences, 5 s deadline). |
| `caseMode` | `smart` (default: insensitive unless the pattern has an uppercase letter), `sensitive`, `insensitive`. |
| `wholeWord` | Boolean. A `matchString` hit must not touch a word character on either side; the same field and meaning as in `localSearch`. |
| `block`, `unit`, `offset`, `length`, `minify` | As [`localFetch`](#localfetch); `length` 1–50,000; omit `minify` for exact text; `standard` drops comments and indentation (the numbered gutter keeps source line numbers). |
| `forceRefresh` | Bypass the content cache. |

Reader rules (here and in `localFetch`):

- Choose one extraction intent: whole file, line range, matching slices, or symbol outline. Symbol outlines reject match and line selectors. Selection precedes minification, redaction, and pagination; offsets are zero-based in the selected view. A whole-file line page of the `minify:"standard"` view counts raw source lines (the unit of `totalLines`, the gutter and `ranges`): each page covers `offset+1` through `offset+length` and shows the lines of that span the view kept.
- `unit:"lines"` (default) fills the response window, up to 50,000 source lines per page. An explicit `length` is a maximum under that window; an oversized line can switch to bytes. `unit:"bytes"` defaults to 16384 UTF-8 bytes; a byte end can extend by up to three bytes to finish a code point.
- Both readers size ordinary line pages for serialized output, including escaping, UTF-16 characters and source-line gutters. Selected reads use the same response window from page 1. Copy `next.continueWindow` unchanged to retain the selection, snapshot, source coordinates and any selection plan. See [response paging](TOOL_DATA_CONTRACT.md#executable-continuations) for indivisible-page exceptions.
- `matchString` reads are always verbatim (redaction still applies). `matchRanges` cover all selected context windows (overlaps merged); `matchedLines` (with `extras:["details"]`) holds the matching anchors on the current page; `selectedMatchCount` (with `extras:["stats"]`) counts matching lines in the selected view. In a JSON file (`.json`, `.jsonc`, `.json5`), `matchKeys` names each hit line's key path (`{"221":"devDependencies.vite"}`; `[i]` is an array item; a key holding `.`, `[`, `]`, `"` or a space is quoted), so a window that hides its section header still says which section a hit belongs to. A tolerant line scan of the whole file finds the path: comments, trailing commas and broken nesting degrade to the nearest open container, never to an error.
- A `matchString` item that does not compile under `regex:"rust"` or `"pcre2"` fails with `invalidPattern`, and the error names the item (`matchString[2] "def post("`); `regex:"rust"` is the engine `localSearch` runs, so both tools give one reason and the same verdict at any file size; when every item compiles as PCRE2 (backreferences, lookaround) `next.repair` is the same read with `regex:"pcre2"`, else it escapes the metacharacter. A range stays bounded by its end line; a match continuation keeps its pattern and context. Lines inside a requested span are never elided. Line numbers and gap markers: [numbered source content](TOOL_DATA_CONTRACT.md#numbered-source-content).
- `totalLines` (and `sourceBytes`, with `extras:["stats"]`) describe the original file (`localFetch` reports it on every successful text read, including empty files and no matches); `pagination.totalLines` (when it differs: a selected or outline view) and `pagination.totalBytes` (every byte page) describe the selected view. `minifyFallback` gives the requested and applied modes and why, when an outline is unavailable.
- A non-regex `matchString` miss that contains a literal `|` gets its own tip: `|` is not alternation; pass a `matchString` list or a regex.
- `minify:"symbols"` returns a paged outline; read its gutter, then `ranges` with `minify` omitted. `standard` drops comments and indentation, with no JS/TS optimization or type-declaration removal, and keeps source line numbers in the gutter; `contentView` is omitted when the view is the tool's default. Cite either view by line number; omit `minify` for quotes and comment-sensitive evidence ([minification coverage](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md#minification--file-reads-and-search-fragments)).
- Run `next.continueWindow` unchanged; offsets are exact. `next` and `hints` queries omit default view fields (`fullContent`) and keep a chosen `minify`; a `fullContent` read cut into pages continues with `minify` omitted unless you chose a view, so every line is reached.
- A match window that stops inside a declaration emits no reread lead: ask for `block:true` or a `ranges` read when you need the rest; a small file the window mostly covers returns whole instead.
- `ranges` that all start past the file end fail with `invalidInput`; `repair` in `hints` is the same read of the file's last window, as wide as the widest requested span (on `ghGetFileContent`, pinned to the read commit). A span that reaches into the file is read, clamped to the file end.

GitHub specifics:

- `fullContent:true` asks for an unpaged view. A view over 50,000 bytes (or a source over 100 KB) returns its first line page with `pagination.hasMore` and `next.continueWindow` pinned to the resolved commit SHA (and to the `minify` you set), so the pages join to the whole file.
- File fields (`path`, `content`, `totalLines`, `commitSha`, …) are directly on `data`. With `extras:["receipts"]`, a first page (offset 0) adds `lastModified`/`lastModifiedBy` (the author login when GitHub links one, else the name) from the last commit that touched the path at the read commit: one more request, sent only then and only after the content read succeeds.
- A batch of reads shares the automatic response window: a row over its fair share returns a shorter line page with `pagination.hasMore` and its own `next.continueWindow`.
- A `matchString` that selects no line returns an empty row with a tip and leads (as [`localFetch`](#localfetch); `ignoreCase` at the resolved `ref`).
- A failed read keeps `errorCode`, and `retryable: true` when a retry can help (absent: do not retry unchanged); `httpStatus`, `requestId`, and `documentationUrl` only with `extras:["diagnostics"]`.
- A file too large for the `/contents/` API falls back to the Git tree/blob API; size alone is no reason to clone.

Approximate cost: `matchString` 50–300 tokens; a small range 100–500; a large range 1k–10k per page; `symbols` 5–20% of the file; `fullContent` can exceed 50k.

### `ghSearchHistory`

<!-- tool: ghSearchHistory -->
```json
{"operation": "pullRequest", "owner": "vercel", "repo": "next.js", "keywords": ["middleware"], "qualifiers": "in:title", "state": "merged"}
{"operation": "commit", "owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/", "since": "30d"}
```

| Field | `pullRequest` | `issue` | `commit` |
|---|---|---|---|
| `owner`, `repo` | Optional; omit both for all of GitHub (`repo` needs `owner`) | Required | Required |
| `keywords` (≤100) | ANDed terms | ANDed terms | Commit-message terms on the default branch; not with `path` or `ref` |
| `state` | `open`, `closed`, `merged` (implies closed) | `open`, `closed` | — |
| `since`, `until` | Created from / up to: ISO date or `"30d"` | Same | Date bounds |
| `sort` | `best-match`, `created:desc`, `created:asc`, `updated:desc`, `updated:asc`, `comments:desc` (rows keep `commentsCount`), `reactions:desc` | Same | — |
| `path`, `ref` | — | — | File or directory prefix; ref to walk (default branch) |
| `qualifiers` (≤200) | `"author:x reviewed-by:y review:approved is:draft in:title merged:>2024-01-01 status:success -author:dependabot[bot]"` | `"commenter:x label:bug -label:wontfix"` | Only `author:` and `committer:` |
| `fields` | Row keys to keep beside `number` (a picked `state` keeps `mergedAt`) | Beside `number` | Beside `commitSha` |

Omitted `sort`: best match with keywords, else newest first; `sort:"created:asc"` is oldest first. `pageSize` 1–100, default 50. Search returns candidates and identities only: it takes no single-item identity, and commit search returns no diffs; read details with `ghGetHistoryItem`.

Validation: `qualifiers` rejects `repo:`/`org:`/`user:` (scope comes from `owner`/`repo`), unknown keys, and a filter set twice. A leading `-` excludes on any PR or issue key (`-label:wontfix`, `-author:dependabot[bot]`; it routes a PR query through search); `is:` negates only as a PR's `-is:draft` (the `state` field owns open/closed/merged), and commit qualifiers take no negation. One name per filter: GitHub's `status:` and `merged:` (no `checks:`/`merged-at:`). `owner`, `repo`, and person qualifiers (`author:`, `committer:`, `assignee:`, `mentions:`, `commenter:`, `reviewed-by:`, `review-requested:`) must be GitHub logins, names, or commit emails; labels cannot contain quotes or backslashes; a range or state qualifier is one term (whitespace inside `> 5` is removed). Any other value is a validation error, never a changed scope. An `archived:` qualifier always routes a PR query through search, which enforces it. PR branch filters are `head:` and `base:`.

A commit list without `keywords` comes from GitHub's list endpoint, which returns no total: `pagination` has no `totalItems`, and a first page with more behind it warns so (page on until `hasMore` is false); a `keywords` search reports `totalItems`. A finished single page (the whole result) carries no `pagination` and no `next`; `extras:["echo"]` adds its `pagination` with `hasMore:false`. A search that names a login GitHub cannot find (`author:`, `assignee:`, `commenter:`, `mentions:`, `reviewed-by:`, `review-requested:`) is `notFound` with GitHub's own words and `withoutUsers` in `hints`, the same query without the person qualifiers; GitHub's commit search, by contrast, ignores an unknown login and returns the unfiltered commits, so a commit search confirms each `author:`/`committer:` login (not an email or `app/` name) once with `GET /users/{login}` and answers the same `notFound` and `withoutUsers` in `hints` for a missing one. A lookup that fails otherwise (rate limit, outage) does not block the search: a warning says the login was not confirmed. A squash-merge commit names its pull request only in its `messageHeadline` (`(#123)`), which is not copied into a field.

Results: rows are an index. Each row carries its identity; a PR search beyond one repository adds `owner` and `repo` to each row. With `extras:["hints"]`, `readPullRequest` in `hints` targets the first merged row (else the first); `readPullRequest`, `readIssue`, and `readCommit` read any row when you swap in its number or SHA. With `extras:["hints"]`, a bare issue number in `keywords` adds `readIssueLinks` in `hints`, the issue read whose `closedBy` names its fix PRs. Commit rows carry the author's login (else the git name), never an email; `readCommit` in `hints` (with `extras:["hints"]`) reads the full message. Search titles first (`in:title`); narrow commits by `path` and time (`since`/`until`, or `pageSize` for the last N) before a diff.

### `ghGetHistoryItem`

| Operation | Required identity | Detail |
|---|---|---|
| `pullRequest` | `owner`, `repo`, `number` | body, changed files, selected patches, comments, reviews, commits |
| `issue` | `owner`, `repo`, `number` | body and comments |
| `commit` | `owner`, `repo`, `ref` | metadata and optional diff (`matchString`/`contextLines` grep patches) |
| `compare` | `owner`, `repo`, `base`, `head` (SHAs for a stable comparison) | `baseSha`, `headSha` (the resolved commits), `aheadBy` and `behindBy` (the range's commits each way), and the commits and changed files; `sections` names one or both; `matchString`/`contextLines` grep patches |

Fields from another operation are rejected, not ignored.

<!-- tool: ghGetHistoryItem -->
```json
{"operation": "pullRequest", "owner": "vercel", "repo": "next.js", "number": 12345, "sections": ["body", "files"]}
{"operation": "pullRequest", "owner": "microsoft", "repo": "TypeScript", "number": 51387, "matchString": "esbuild", "include": ["*.json", "*.mjs"]}
{"operation": "compare", "owner": "vercel", "repo": "next.js", "base": "v14.0.0", "head": "v14.1.0"}
```

| Field | Operations | Values and meaning |
|---|---|---|
| `sections` | all | PR (1–8): `body`, `files`, `patches`, `comments`, `reviewComments`, `reviews`, `commits`, `commitFiles`; omit for a summary. Issue: `body`, `comments`. Commit: `files`, `patches`. Compare: `commits`, `files`, `patches` (omitted: commits and files). |
| `include` | PR, commit, compare | 1–100 paths, `dir/`, or globs, any-of: `*` one segment, `**` any depth, no `/` matches the file name (`"*.ts"`). |
| `status` | PR | 1–7 of `added`, `removed`, `modified`, `renamed`, `copied`, `changed`, `unchanged`. |
| `minChanges` | PR | additions + deletions ≥ n (≥1); files without counts pass. |
| `matchString` | PR, commit, compare | Literal. Keeps matching patch lines, implies patches on a PR, returns every hit file on one page. It searches each file's diff text, never its path, so a function name in a hunk header (`@@ -1,4 +1,4 @@ def url_for(`) matches and that hunk is shown. On a commit or comparison it filters the changed files before they are paged: page N is the N-th page of hit files. A commit whose `matchString` hits no file is `status: empty`, warns `matchString matched 0 of N changed files (…)`, and offers `withoutMatchString` in `hints` (the same read without it). A PR read whose `matchString` hits no patch line is `status: empty` and offers `withoutMatchString` in `hints`, then `readFiles` in `hints` (the full file inventory); an `include`, `status` or `minChanges` scope that matches no file offers `readFiles` in `hints`. |
| `contextLines` | PR, commit, compare | 0–100, default 3 lines around each patch hit. |
| `offset`, `length` | all | Body, comment, or patch text window; `length` 1–100,000 (omitted: 12,000 characters of text; patches fill the response page). |
| `pageSize` | all | PR, commit, compare 1–1000 (omitted: 100 for collections and patches; a patch-free inventory fills the page). Issue 1–100, default 100. |
| `page` | all | 1–1000: the page of the section a read lists; copy from `next` (each page continuation names the one section it pages). |
| `headSha` | PR | Set by `next.*` pages: pins them to the head the first page read. A head that moved since is `staleSnapshot` with `next.restart`. |

`include`, `status`, and `minChanges` keep files that match every field set; a scope that matches no changed file returns an empty row with an inventory hint.

**PR summary and menu.** Every PR row has its merge state: `mergedAt` (or `closedAt` when closed unmerged) and `baseBranch`; first-page labels (the first 20, `labelsClipped: true` past that); `updatedAt` only while open. The body is opt-in (`sections:["body"]`). A merged PR reports `mergeCommitSha` (GraphQL `mergeCommit`; read it with `operation:"commit"`); an open PR never reports GitHub's test-merge SHA. Commit summaries carry the full message. A summary offers at most `readFiles` (with the body), `readPatches` (every patch of a small PR; with the body when no file list is offered), and `readDiscussion` (comments and reviews). A merged PR's first patch read also offers `readAtMerge` when a changed code file (not a test) adds lines: the most-changed such file of all loaded files, at the merge commit, as one `ranges` read of each hunk's new side (10 lines of padding, ≤10 ranges). An inventory read offers `readSelectedPatches`: up to 30 source files (no tests, docs, lockfiles, generated, or binary files; tests by language layout and naming such as `_test.go`, `test_*.py`, `FooTest.java`, `_spec.rb`, or `test/`), those that fit one patch budget first, the rest through `next.continuePatch`. On a large PR, ask for selected patches; leave commit diffs off until you know the commit.

**File inventory** (`sections:["files"]`): rows `"M +3 -1 [!flag ]path[ <- old/path]"` (git status letter, `T` changed, `U` unchanged; line counts; path), two or more files of one directory share a `{"dir/": [rows]}` group (path = dir + name), and a lone file is a plain row with its full path. `!tooLarge` (counts known), `!binary`, or `!omitted` (no patch or counts, for example past the diff budget; 0/0 can still have changed) mark a file without a patch; `<- old/path` marks a rename. It pages by `page`; `pageSize` omitted fills the response page (100–1000 rows), explicit up to 1000. GitHub lists at most 3000 files; past that the row states `terminalLimit`, `partialReasons:["providerFileListLimit"]`, and `providerLimit {reason, listed, changedFilesCount}`. A section read's per-section cursors (`contentPagination`) are an `extras:["stats"]` extra: `next.*` holds what is left.

**Patches** (PR, commit, compare): rows `{path, stat: "M +3 -1", patch}`, keeping `patchUnavailable`, `previousPath`, and `patchPagination` when cut. Files not reached ride the continuation, not empty rows; a pure rename's patch is `""`. `@@ -a,b +c,d @@ heading` lines stay verbatim (the heading is the enclosing symbol when GitHub gives one). Each line is numbered `cat -n` style: kept and added lines with their new-file line (`86\t context`, `87\t+added`), removed lines with their old-file line (`85\t-removed`), a `\ No newline` marker with a bare tab; `path:87` is citable, and removing each gutter (to the first tab) gives the raw patch. Patch text is never minified.

- A patch read lists 100 changed files a page, so it covers every patch of a PR of up to 100 files. Rows of one call share one response page; an explicit `responseLength` (≤50,000) replaces the automatic page.
- A window ends at the last whole file; a larger file is cut at its last whole line. `next.continuePatch` is the same file page at the stream `offset` (same budget, nothing skipped or repeated); a page with unread patches offers only it, and the window that finishes them offers `next.nextFilePage`. Both ask for `responseScope:"rows"`: a row that still outgrows the page splits into structured `rowPart`s (follow `responsePagination.next` first) and the hop rides the last part. Run each unchanged.
- A PR patch read that does not fit one response opens with `fileSummary` (inventory rows with hunk counts, `M +10 -10 4 hunks rt_common.rs`); `warnings` names unfinished patches and files.
- `matchString` lists `!tooLarge`, `!binary`, and `!omitted` files as unsearched (a miss there is not absence). Narrowed or clipped files are re-read raw by the opt-in lead `readFullPatches` in `hints` (100 files per read, then `readFullPatches2`, …).

**Section reads** (any `sections`, or a later page) return a slim header unless `extras:["details"]` is set: `number`, `title`, `author`, `state`, `headSha`, and a merged PR's `mergeCommitSha`; the summary read (no `sections`) carries the rest.

**Text views.** Every read returns GitHub's exact text after redaction (no `minify`). Match-filtered reads keep anchors. A file page whose patch equals an earlier row's on the same page carries `samePatchAs` (that row's path) instead of the patch. Bodies and comment bodies window by `offset`/`length` (automatic 12,000 characters); a continuation reads one surface. `next.continueCommentBody` (PR or issue) lists only the comments it continues; an issue's hop pages exactly the comments its first window showed and keeps that window's `body` section for sizing without showing the body again; `next.continueBody` reads the issue body. Comment-item continuations reset the text offset. Body, comment body, item page, file, and patch windows are independent; follow each.

**Collections.** One provider batch per source and call: 100 discussion comments, inline comments, or reviews; 50 commit summaries. `pageSize` bounds displayed items per batch (default 100, max 100); `next.nextReviewPage` pages reviews independently (`sections:["reviews"]` with its `page`), and review bodies keep their text continuations. Follow `next.*` even after a filter empties a batch; it carries `collectionPages` positions and resets item and text windows, and a zero position marks an exhausted source. `countScope: "providerBatch"` counts describe one batch. `reviewComments` rows carry `path`, `side`, `startLine` (multi-line only), `line`, `commitSha` (read the code at that ref), `outdated:true` when anchored to the original diff, and `inReplyToId` on replies.

**Comparison order.** A default `compare` lists `files` before `commits`: `contentPagination.commits` pages the commits and `contentPagination.files` the files; `next.nextCommitPage` and `next.nextFilePage` (each names its list in `sections`) page them.

**Commits and comparisons.** `commitFiles` fetches one file batch per displayed commit (≤`pageSize` files each); its `next.nextFilePage` and `next.continuePatch` carry the exact SHA. Commit reads page files by `page` with independent file and patch windows. An explicit `length` sizes the patch window up to one response page (larger is clamped with a warning); omitted, it fits the page. `patchPagination` is per file; the page's patch-stream cursor rides `contentPagination.files` and `next.continuePatch`. A file `page` past the end (`contentPagination.files.outOfRange`) is an empty row with `reason: "page N of M"` and `next.restart`, like any page past the end. A commit read stopped at its file-batch cap reports `changedFilesCountScope: "partial"` (omitted when the count is complete), `countScope: "partial"`, and `terminalLimit`. A commit's `parents` and author/committer emails are `extras:["details"]`, its `ref` echo `extras:["echo"]`; `committer` is omitted when it repeats the author or is GitHub's `web-flow`. Without `sections:["patches"]`, files read as the pull-request inventory (`"M +3 -1 path"`, grouped by directory). With `extras:["hints"]`, a commit offers `findPullRequest` in `hints` (a `ghSearchHistory` query for the PR with that SHA). A comparison resolves `base` and `head` to SHAs; each page carries only its own data: `next.nextCommitPage` continues the commits (`sections:["commits"]`), `next.nextFilePage` the files (`sections:["files"]`), and `contentPagination.commits.totalItems` (with `extras:["stats"]`) is GitHub's commit total. `changedFilesCount` appears when the listed files are not all of the comparison. GitHub lists at most 300 compared files (`partialReasons:["providerFileLimit"]`); a capped comparison always warns, and `readCommitFiles` in `hints` or `listCommits` in `hints` reaches every changed file through the range's commits. An `include` scope gets `narrowScope` in `hints` (its commits from the merge base up to `head`). A cross-fork `head` (`owner:branch`) is pinned to its commit SHA, so every lead names one comparison. A commit's `readPullRequest` in `hints` also comes from a `PR-URL:` trailer. Provider caps and omitted patches stay terminal limits, not completeness.

**Issues.** An issue section read (or a later comment page) carries `number` and `state`; the summary read carries the title, author, labels, and dates. A wrong-operation read (a PR number read as an issue, or the reverse) fails with the corrected read as its lead (`readPullRequest` in `hints`/`readIssue` in `hints`, keeping the sections both operations take) and no restating tip. An issue read lists up to 25 closing PRs (`closedBy`, merged first) and, with `extras:["hints"]`, offers `readPullRequest` in `hints`: every patch of a small fix, else the body and inventory, or only the patches of files that `mainGoal` names (as `include`). Past 25, body and comment windows are `isPartial` with `partialReasons:["closingReferenceLimit"]`, and the fix is a medium-confidence candidate.

REST reads revalidate by ETag; a 304 costs no body.

### `ghCloneRepo`

Atomically clones a repository or sparse subtree into Octocode's managed cache (CLI-only; needs persistent `storage.mode`, the default). With local tools off, cloning works but omits `exploreClone` in `hints`. `bytes` is the checked-out size on disk, excluding `.git`. A sparse checkout of directories only also holds the repository root's files; once any `path` is a file, it holds only the listed paths. Repository symlinks are checked out as plain files holding the link target.

<!-- tool: ghCloneRepo -->
```json
{"owner": "microsoft", "repo": "TypeScript", "path": "src/compiler"}
```

| Field | Meaning |
|---|---|
| `owner`, `repo` | Required; `repo` without the `owner/` prefix. |
| `ref` | Branch, tag, or commit SHA (abbreviated SHAs resolve); omit for the default branch. |
| `path` | Up to 10 repository-relative files or directories for a sparse checkout; no absolute path, backslash, or `..`. Completeness covers only these subtrees. A missing path is `pathNotFound` (exit 3): when its first missing segment has one near-miss sibling at the commit, `repair` in `hints` is the same clone with that path corrected; otherwise `viewTree` lists the deepest existing directory with `ghStructure`, pinned to the commit. |
| `historyDepth` | Commits of history, 1–50 (default 1). |
| `forceRefresh` | Fetch current state; refuses to replace a checkout with local edits. |

Result: `location.localPath` (absolute; pass it as `path` to local tools), `location.kind` (`repo` or `tree`), `commitSha` (the actual HEAD), `resolvedRef`, `requestedPaths` (sparse), `symlinks` (repo-relative paths of the checked-out repository symlinks; absent when none), `historyDepth`, `cached`, and `location.clonedAt`/`expiresAt` (fresh clones and cache hits). `verified` appears only when false. `exploreClone` in `hints` (shown without `extras`) lists the checkout with `structureSearch operation:"tree"` (the subtree for one path, the root for several); a single sparse file is read with `localFetch` instead. Cache keys, reuse, and expiry: [Cache behavior](#cache-behavior).

- Directories use the lowercased owner and repository names: `<octocode-home>/tmp/clone/<owner>/<repo>/…`.
- A full clone suits exploration and LSP. A sparse clone is faster, but LSP cross-file resolution can be limited and dependencies can be missing.
- One `commits/{ref}` call (for no ref, the repository's default branch first) pins the commit with your credential before git runs; checkouts are keyed by that commit under the repository's canonical name, so a moved branch is cloned again and a branch, SHA, or old repository name naming one commit shares one checkout.
- That call is also the access check a cached checkout is served behind (cache keys do not include the credential). Any failure refuses the row before git: no access is `notFound`; a rejected token, a rate limit, or a network failure is the provider error.
- A checkout that contains the reserved root path `.octocode-clone-meta.json` returns `cacheUnavailable` before bookkeeping is written and is preserved; read that repository with `ghGetFileContent` or `ghStructure`.

### `artifactSearch`

Finds packages by capability, resolves a known dependency to registry metadata, or locates its upstream source. A repository link is metadata, not proof of implementation or of the published version.

<!-- tool: artifactSearch -->
```json
{"ecosystem": "npm", "packageName": "react"}
{"ecosystem": "crates", "keywords": ["async", "runtime"], "pageSize": 10}
{"ecosystem": "npm", "packageName": "@example/widget", "registryUrl": "https://registry.example.com/"}
```

| Field | Meaning |
|---|---|
| `ecosystem` | Required, one per query: `npm`, `pypi`, `crates`, `maven`, `nuget`, `go`, `packagist`, `rubygems`. Compare ecosystems with separate queries; there is no `ecosystem:"all"`. |
| `packageName` | Exact coordinate (`@scope/name`, `group:artifact`, a Go module path); exclusive with `keywords`. `name@version` and PyPI `name==version` stay valid. |
| `version` | 1–100 chars, exact lookups only. npm, PyPI, crates.io: exact, range (`^3`, `>=2.31,<3`, `v18`, resolved like the registry's installer), or tag (`latest`, `next`). Go, NuGet, Maven: exact or `latest`. A missing version is `versionNotFound` with the nearest published versions (final releases first). |
| `keywords` | 1–100 discovery terms. Not PyPI: a PyPI keyword query is `invalidInput`, with no fallback to a website or third-party service. |
| `page`, `pageSize` | Discovery only: page 1–1000 (copy `next.nextPage`), `pageSize` 1–100, default 50. |
| `registryUrl` | npm only: HTTP(S) registry override; omitted, npm lookups use registry.npmjs.org (set it for a private or scoped registry). Credentials are not tool inputs. |

Rows (`artifacts[]`): canonical identity, available version, `description`, license, `homepage` (omitted when it is the repository page; `extras:["details"]` keeps it, its only effect on this tool), and source metadata; `extras:["receipts"]` adds registry URLs; rows never restate `ecosystem`. Exact rows add, when the registry has them, `publishedAt`, `deprecated`, `yanked`, `dependencies`/`optionalDependencies`/`peerDependencies` lists (`name@range` strings; `dependencies` holds required ones only and `[]` declares none; `optionalDependencies` lists npm `optionalDependencies`, PyPI extras, and optional crate and Maven dependencies, absent when none; `peerDependencies` is absent on non-npm rows), and `engines` (npm: an object of every declared engine, e.g. `{"node":">=18","npm":">=9"}`), `requiresPython` (PyPI), or `rustVersion` (crates). npm discovery rows carry `downloadsMonthly`. Go package paths stay separate from module identities. Metadata can be absent; popularity is not comparable across registries. crates.io exact versions read the version record and crate metadata, not the whole version list.

Source leads: with `extras:["hints"]`, an exact GitHub-backed lookup offers one `ghStructure` lead at the package directory (plus the `main` directory of an npm package without a build step): `viewReleaseSource` in `hints` when the release has a ref, else `viewRepo` in `hints` (default branch). Release refs: npm `gitHead` or a provenance-attested commit; the Go version tag or pseudo-version commit; the Composer source reference; the NuGet nuspec commit; the crates.io packaging commit (`.cargo_vcs_info.json`, `path_in_vcs` as `repositoryDirectory`); for PyPI (and crates without VCS info) an upstream `v<version>` or `<version>` tag that exists on GitHub. A PyPI repository is the labeled source link (`Source`, `Repository`, `GitHub`, …), else the first project link or homepage on GitHub. A Go row states `modulePath`/`packagePath` only when it differs from `name`.

- `verification:"provenance"` only for a commit from an npm SLSA provenance attestation bound to this exact tarball (subject digest equals `dist.integrity`) and the manifest's repository. The registry verifies it at publish; Octocode does not re-verify signatures. Other refs are unchecked registry leads.
- An npm `gitHead` that GitHub reports missing (one commit check through the configured GitHub API) pins no lead: the row drops `sourceRef`, says `verification:"defaultBranch"`, offers `viewRepo` in `hints`, and `warnings` names the dead commit (`<name> <version>: release commit <sha> is not in <owner/repo>`). When the check cannot run, the ref stays a `registryRef` lead.
- Check a lead's resolved revision before you treat it as evidence. A ref missing upstream recovers on the default branch (the same query without `ref`), which is not evidence of the release. Match the published or installed version before a version-specific claim.

npm: an explicit `registryUrl` routes the lookup (`.npmrc` `@scope:registry` mappings are not read); authentication uses registry-scoped npm configuration (with environment interpolation); caches isolate registry and configuration identities; continuations keep the registry. A private registry needs npm's search endpoint for discovery. Other ecosystems use official public APIs.

Paging and errors: follow `next.nextPage`; unknown totals stay unknown, and an empty page can still continue. Auth, unsupported, rate-limit, and provider failures are errors; a missing exact package is `notFound`.

## Local code tools reference

### Shared local rules

- **Enable.** On by default. `local.enabled: false` (or `OCTOCODE_ENABLE_LOCAL=false`) turns the local surface off; `DISABLE_TOOLS` / `tools.disabled` hides single tools; `TOOLS_TO_RUN` is a strict allowlist. Removed compatibility names cannot come back through `TOOLS_TO_RUN` or `.octocoderc`. Settings: [CONFIGURATION.md](CONFIGURATION.md).
- **Paths.** Relative paths resolve against the workspace root (`WORKSPACE_ROOT`, default the cwd). Allowed roots: the workspace root, `ALLOWED_PATHS` / `local.allowedPaths`, and the Octocode home; the home directory only when listed. In every local tool (`localSearch`, `localFetch`, `structureSearch`, `astSearch`, `lspSearch`), a path outside the allowed roots (directly or through a symlink) fails with `outsideAllowedRoots`, and a missing path with `pathNotFound` (exit 3); the recovery hint is keyed on that code. Row paths are relative to the envelope `root` ([paths](TOOL_DATA_CONTRACT.md#paths-shared-fields-and-anchors)). With `extras:["receipts"]`, when the root's `.git` HEAD is readable, `localSearch`, `localFetch`, and `structureSearch` report it once as `shared.commitSha` (per row when rows read different repositories); uncommitted edits are not reflected.
- **Engines.** Native in-process text-search, walker, and structural engines; no external `rg`, `grep`, `find`, or `tree`. macOS, Linux, and Windows.
- **Filters.** `include` globs are ORed; a bare word (no `/` or glob character) matches names that contain it, and a plain path also matches everything under it. `exclude` (≤100) adds to the default prune: a bare name skips that file or directory at any depth, and `dir/**` skips a path. The default prune covers dependency, build, cache, VCS, and credential directories (`node_modules`, `target`, `dist`, `.git`, `.ssh`, …); every local tool prunes the same names. Inside a git work tree the prune applies only to listed directories that git ignores (a tracked or unignored `build/` is searched); outside one it applies to every listed name. `defaultExcludes: false` turns the prune off; `.gitignore` still hides ignored directories until `includeIgnored: true`, so reaching an ignored `build/` or `node_modules/` takes both flags, as `rg --no-ignore` does in one, and credential-store directories such as `.ssh/` and credential-named files such as `.env` or `*.pem` are never walked. `includeHidden` includes dot entries in every tool, tool-config directories such as `.github`, `.vscode`, `.config` and `.agents` among them: one flag reaches every dot directory that is not generated output (`.cache`, `.venv`, …) and that no ignore rule names. A dot directory a `.gitignore` rule names (`.octocode/` in this repository) is also behind `includeIgnored: true`; the result counts or names what was skipped and its `includeSkipped` lead carries the flag. A directory named as `path` is always searched, whatever its name. `maxDepth` is 1–20 levels below `path` (1 = its children).
- **Paging.** Result pages (`page` ≤1000, `pageSize`) in `localSearch`, `structureSearch`, `astSearch`; per-file match pages (`matchPage`, `matchPageSize`) in `localSearch` and `astSearch` `match`; content windows (`unit`, `offset`, `length`) in `localFetch`. Use whole-response windows only when one result is still too large.

### `localSearch`

Lexical search. Outlines and metadata: `structureSearch`; syntax: `astSearch`; file graphs: `octocode graph` (CLI).

`clasifyContext` optionally selects relevant matched-file evidence before returning it. It does not retrieve files outside the lexical scope. See [context selection](#context-selection).

| Field | Values and meaning |
|---|---|
| `path`, `matchString` | Required. A `ghCloneRepo` `localPath` is valid as-is. `matchString` is one text, or a list of up to 10 texts matching any entry, as in `localFetch`; each entry is literal unless `regex` is set, `caseMode:"smart"` judges each entry on its own, and a list runs as the one regex alternation that finds the same lines (the `next.*` leads carry that regex query). With `regex` unset, `matchString` is literal in every tool, whatever its shape (`a\|b`, `.*`, `matches[0]`); pass `regex:"rust"` or `"pcre2"` to run a regex. An empty literal search whose text has regex characters says the regex reading exists; one with hits whose text is an alternation (`a\|b`, every side non-empty) says so only when one count of the regex reading finds more matched lines. |
| `regex` | `literal`, `rust` (linear-time), `pcre2` (lookbehind, backreferences, wall-clock limit: the result keeps finished files and reports `capReason:"pcre2Deadline"`). |
| `caseMode` | `smart` (default; follows query casing), `sensitive`, `insensitive`. |
| `wholeWord`, `multiline` | Boolean; `off` (default), `on`, `dotall` (`.` crosses lines). |
| `invertMatch` | Non-matching lines. With `resultView:"files"` it lists files with any non-matching line; for files without the pattern use `filesWithout`. |
| `resultView` | `paginated` (default), `files`, `filesWithout`, `countLines`, `countMatches`, `matchOnly` (one `{line, column, value}` row per span: the 1-based UTF-16 `column` keeps equal spans on one line apart). |
| `unique` | `matchOnly` only: `off` (default), `list` (distinct values per file), `count` (frequencies). |
| `contextLines` | 0–100 per side; default 0. |
| `contextBefore`, `contextAfter` | 0–100; override only the named side of `contextLines`, including explicit 0. Applied to snippet views; `matchOnly` and file/count views ignore context. |
| `matchContentLength` | 1–100,000 characters per snippet, clipped around the hit (`matchOnly`: per span); default 200 × (before + after + 1), capped at 4000. |
| `language` | File type: `ts`, `js`, `py`, `go`, … |
| `fileHas`, `fileLacks` | File-level AND / NOT (≤100 terms each): keep a matched file only if it also contains every `fileHas` term and no `fileLacks` term, anywhere in the file. Each term is literal and smart-case on its own (all-lowercase ignores case). |
| `include`, `exclude`, `defaultExcludes`, `includeHidden`, `includeIgnored`, `maxDepth` | [Shared local rules](#shared-local-rules). Ripgrep precedence: an entry an `include` glob matches is searched even when an ignore rule or the dot rule would skip it, so a bare-word `include` searches matching ignored and hidden entries too. The path policy still withholds credential stores. |
| `sort`, `reverse` | `relevance` (default), `traversal`, `matchCount`, `path`, `modified`, `accessed`, `created`; `reverse` applies before pagination. |
| `pageSize` | 1–1000 files; omitted, fill the response window. Content pages retain at most 20 files; path and count views use adaptive file counts. |
| `matchPage`, `matchPageSize` | ≤1000; 1–100,000 rows per file; omitted, fill the response window from the first page. |
| `snapshot` | ≤200 chars; copy from `next`. |

`relevance`: for one bare identifier, declaring files first; generated files (a `generated` directory, a `.generated.`/`_pb2.`-style name, a `DO NOT EDIT`/`@generated`/`Automatically generated` header, or `linguist-generated`/`linguist-vendored` in `.gitattributes`, resolved as git does) after every hand-written file; for a word alternation (`retry|backoff|jitter`, optionally `\b(…)\b`), files holding more of its words first, so one common word repeated cannot outrank them; then match count, source paths before test, vendored, and bundled paths, declaration hits before code and comment/string hits, then path. `files`/`filesWithout` order by source paths first, then path. `relevance` and `matchCount` keep the 10,000 highest-ranked files (`capReason:"maxCollectedFiles"`; `stats`, with `extras:["stats"]`, count all matched files).

How it differs from `rg` (same engine family, measured hit-for-hit equal on shared cases):

| Need | `rg` | `localSearch` |
|---|---|---|
| Output size | every line, all of a minified line | lossless response-window pages; long lines clipped around the hit with an `expandValues` read |
| Secrets in hits | printed | redacted, with a `redactedMatches` warning; credential files never walked |
| Order | walk order | `relevance` (above); per-file counts sorted by count |
| Empty result | exit 1 | names what was skipped and why (ignore rule, hidden, pruned dir), other spellings (`findSimilar`), and the lead that searches them |
| File-level AND / NOT (rg #875) | two runs and a pipe | `fileHas` / `fileLacks` |
| CRLF files and `$` | needs `--crlf` | works by default |
| Hit context | lines | innermost declaration (`enclosing`, then `enclosingLine` on later rows inside it), `lspSearch` callers/references leads |
| Big results | one stream | totals, `viewFileCount` lead, pages that never drop a match |

Coverage:

- No readable file under `path`: `fileAccessFailed` (exit 5). Some unreadable paths: `isPartial`, `terminalLimit`, `fileErrors`; zero matches then prove nothing. Files open without following symlinks; a path replaced by a symlink or special file after the walk is a read error.
- A file binary from its leading bytes (a NUL before any text, or after a short single-line header such as a font or database magic that matched nothing) is skipped without making the result partial; its matching bytes are not hits. A row with hits counts them in `stats.binarySkipped`; an empty row names them in a warning with a `binarySkipped` lead. Page 1 carries a `binarySkipped` warning (counts by extension) and `binarySkipped` in `hints`, a `structureSearch` `files` query listing those files (extensionless ones by name; it can also list same-extension text files).
- A file with real text before its first NUL is searched up to that byte: a `binaryFileSkipped` warning names it (`dir/{a,b}` per directory), and the result is `isPartial` (`extras:["stats"]` adds `capped` and `capReason: binaryQuit`; `terminalLimit` when no other continuation exists).
- Values with secret-shaped text replaced by `[REDACTED…]` carry a `redactedMatches` warning; they are not verbatim.

Rows: `path` is relative to `root` (the queried directory, or a queried file's parent). A hit row in a JSON or TOML file carries `matchKeys` (`{"289":"devDependencies.typescript"}`, `{"12":"dependencies.serde"}`): the key path of each hit line. For the first 20 files on a page, a hit inside a declaration carries `enclosing: {symbolName, kind, line, endLine}`, the innermost declaration around it (a hit on a declaration's name line names the declaration around it): `symbolName` + `line` anchor `lspSearch`, and `line`–`endLine` is a `localFetch` range. Consecutive rows in one declaration name it once. A declaration search (`fn foo`, `class Foo`) names owners only on rows that declare the symbol. Every view with rows shows `matchCount` and `matchedLineCount` (rows a walk shows); `stats` holds the scan accounting (`filesScanned`, `fileCount`), and `pagination` holds `currentPage`, `matchPage` (past the first match page), `totalPages`, `pageSize`, `totalItems`, and `hasMore`. Size paging and `pagination.moreLines`: [search result shapes](TOOL_DATA_CONTRACT.md#search-result-shapes).

| Key | When and what |
|---|---|
| `next.nextPage`, `next.nextMatchPage` | Without `pageSize`/`matchPageSize`, `nextPage` walks the rest in pages sized for the response window. With either set, `nextMatchPage` finishes the shown files' remaining rows first (later match pages list only files with rows left), then `nextPage` advances to the next files and resets the match page. This gives one executable route without repeated file pages. A terminal match-cursor limit still allows advancing to later files. |
| `next.restart` | Stale snapshot: rerun from page 1. |
| `next.expandValues`, `expandValues2`, … | A clipped value. On a `pageSize`/`matchPageSize` page whose longest clipped value fits the `matchContentLength` maximum and the page's per-row share: the same page with a wider `matchContentLength`. Otherwise `localFetch` reads of the clipped lines, one row per file (whole-file values are not offered), or for a multiline match the file searched alone with a wider `matchContentLength`. Reads past one call's rows continue in `expandValues2`, `expandValues3`, …; a default page ends early so its reads fit one call. |
| `read` in `hints` | With `extras:["hints"]`, `localFetch` on the first page of a complete result: a shown declaration of the searched symbol, whole; else ±6 lines around the top file's hits (a `matchString` read past 10 ranges). |
| `findCallers` in `hints`, `findReferences` in `hints` | With `extras:["hints"]`, `lspSearch` when the search names one symbol (an identifier, or one after `fn`/`def`/`class`…) and a shown hit declares it: `callers` for a function or method, else `references`. Omitted without a language server for the file. |
| `includeSkipped` in `hints` | An empty result where ignored, hidden, or default-excluded paths hold matches: the same search with exactly the flags whose relaxation found them (`includeIgnored`, `includeHidden`, `defaultExcludes:false`), each verified by a probe. The hint names why (up to three): the deciding ignore rule (`gen/.gitignore:2 '*'`), `hidden .notes/`, or the pruned directory. Default-excluded directories that could not be probed are named, with `defaultExcludes:false`. |
| `findSimilar` in `hints` | An empty first page for a compound identifier whose other case styles match (`parseConfig` → `parse_config`, `ParseConfig`, `PARSE_CONFIG`, `parse-config`): the same search for those spellings, with the file count. A literal page whose text has regex characters (empty), or reads as an alternation that matches more lines as a regex (with hits): the same search as a regex. |
| `viewFileCount` in `hints` | A line view with 1,000 or more matches: the same search as `countMatches`, ranked by hits per file. |
| `repair` in `hints` | Invalid regex: the same search with the broken pattern escaped; when PCRE2 compiles the pattern (backreferences, lookaround), the same search with `regex:"pcre2"`. |

### `astSearch`

Structural search for shapes regex cannot express (an `await` inside a `for` loop, calls with N arguments, functions without `try/catch`). Code patterns do not mistake text inside comments or strings for executable syntax; explicit node-kind rules can select those nodes.

```bash
astSearch(operation="match", path="src", pattern="track($$$ARGS)")
# rule is a YAML string; \n is a newline in the JSON argument (on the CLI use $'...').
astSearch(operation="match", path="src", rule="rule:\n  pattern: await $C\n  inside:\n    kind: for_statement\n    stopBy: end")
```

| Field | Operations | Values and meaning |
|---|---|---|
| `operation` | all | `match`, `syntaxTree`, `symbols`. |
| `path` | all | File or directory (`syntaxTree`: one source file). |
| `pattern` / `rule` | `match` | Exactly one. `pattern`: a complete structural node with `$VARS` (an omitted trailing `;` or `,` still matches). `rule`: a structural rule as a YAML string, the rule object, or `{rule: …}`; all match alike. |
| `language` | all | Grammar name, alias, or `.ext`; `cpp` parses `.h` as C++ (default C). A directory `match` without it uses the one grammar under `path` that parses the query (`inferredLanguage`; continuations pin it); several candidates return `languageRequired`. `symbols`: single file only. |
| `languageGlobs` | `symbols`, `match` | Directory parser map, `{"cpp":["include/**/*.h"]}`; not for clangd. A directory `match` that skips files no grammar parses warns with the count and example paths. |
| `include`, `exclude`, `defaultExcludes`, `includeHidden`, `includeIgnored`, `maxDepth` | `match` (`exclude`, `defaultExcludes`, `includeIgnored`, `includeHidden` also `symbols`) | [Shared local rules](#shared-local-rules). `maxDepth: 1` includes root files; depth filtering runs before the scan cap. |
| `resultView` | `match` | `content` (default), `files`, `countMatches`. |
| `captureText` | `match` | Opt-in captures. |
| `matchContentLength` | `match` | 1–100,000, default 500 characters per hit. |
| `sort`, `reverse` | `match` | `path` (default) or `matchCount`. |
| `pageSize` | all | ≤1000 files (`match`), nodes (`syntaxTree`) or declarations (`symbols`); omitted, fill the response window. |
| `matchPage`, `matchPageSize` | `match` | ≤1000 matches per file page; omitted, fill the response window. |
| `maxFiles` | `match`, `symbols` | 1–100,000 files scanned. Omitted, the scan covers the scope within a 10 s time budget and a 100,000-file walk ceiling. |
| `scanOffset`, `snapshot` | | Copy from `next`. |
| `line` | `syntaxTree` | 1-based line: the tree starts at the smallest node covering that line's text (a statement, or the declaration its header opens); omitted, the whole file. A blank line adds a `syntaxTree.options.lineBlank` warning diagnostic; a line past the end is `invalidInput` with `repair` in `hints` (the tree without `line`). |
| `namedOnly` | `syntaxTree` | Default `true`: skip anonymous nodes. |
| `kinds` | `symbols` | ≤100 declaration kinds (`function`, `method`, `class`, `constant`, `module`, …). |
| `symbolName` | `symbols` | A name or a list (any matches): an entry equal to a declaration name matches only it, else a substring; a directory scan parses only files whose text contains an entry. |

Rules support `pattern` (text or `{context, selector}`), `kind`, `regex`, `all`, `any`, `not`, and the relations `has`, `inside`, `precedes`, and `follows`. A contextual pattern parses the supplied context and selects its first node of the named grammar kind in source order. Only that selected node is matched: a wrapper such as `class Example { … }` need not occur in searched files. The context must parse without syntax errors. `selector` is a grammar kind, not a text or CSS selector.

`kind` accepts a grammar node name or a selector expression. Child (`>`), descendant (space), adjacent (`+`) and following (`~`) relations, comma alternatives, `:has`, `:not`, `:is`, `:nth-child` and `:nth-last-child` lower into the same structural rule matcher. Unknown syntax and grammar kinds fail explicitly.

`nthChild` selects positions among named siblings, counting from 1. Use an integer or an `An+B` formula, or `{position, reverse, ofRule}` to count from the end or count only siblings matching another rule. Named comments count unless excluded by `ofRule`; only the selected sibling contributes filter captures. For example, `{kind:"identifier", inside:{kind:"arguments"}, nthChild:{position:2, ofRule:{kind:"identifier"}}}` selects the second identifier argument; other argument expression kinds require a broader filter. Within `all`, capture-dependent rules are order-sensitive: put the pattern that binds a metavariable before an ordinal filter that reuses it. Reversing those entries can change which siblings the filter counts.

`rule.range` matches exact node start/end lines and any supplied columns: `{start:{line:0,column:0},end:{line:0,column:3}}`. Positions are zero-based; columns count Unicode scalar values; the end is exclusive. Omitting a column checks that boundary's line only. A leading UTF-8 BOM counts as one source scalar for `rule.range`; `scope` and returned match anchors exclude that encoding prefix. This is an exact-node predicate, not a source window.

For a source window, use `scope:{start:{line,column},end:{line,column},mode}` on a single-file `match` query. These positions use 1-based lines and UTF-16 columns, matching returned match anchors; the end is exclusive. `mode:"contained"` (default) keeps only nodes wholly within the region; `mode:"overlap"` keeps complete nodes intersecting it. The engine parses the full file, so parent/sibling rules and captures retain their context. Counts, result views and every continuation retain the same scope. Invalid file positions, half-surrogate boundaries, reversed/empty scopes and directory paths fail explicitly. A changed scope invalidates an earlier snapshot.

`has` searches named child nodes and `inside` starts at the immediate parent by default (`stopBy: "neighbor"`). Sibling adjacency includes punctuation tokens. `stopBy: "end"` searches to the end of the direction. A rule in `stopBy` stops inclusively: its node can still satisfy the relation, but traversal does not continue past it. Stop checks do not contribute captures. `has.field` restricts the first child edge to a grammar field and then searches that subtree; `inside.field` requires the path to the matching ancestor to cross that ancestor's named field. Sibling relations stay within one parent and do not accept `field`.

For example, `{kind: "function_declaration", has: {kind: "identifier", regex: "^run$", field: "name"}}` selects functions named `run` without relying on their return types or modifiers. To relate statements, select statement nodes explicitly: `{pattern: {context: "start();", selector: "expression_statement"}, precedes: {kind: "expression_statement", regex: "^finish", stopBy: "end"}}`. A bare call pattern selects a call node, whose siblings differ from the surrounding statement's siblings.

Scans, limits, and errors:

- A cut scan gets a `structural.scan.truncated` warning and, on the last page, `next.expandScan`; pages plus `expandScan` list every file once. A cut at an explicit `maxFiles` (`partialReasons:["maxFiles"]`) continues with doubled `maxFiles` after the files already evaluated; a scan without `maxFiles` cut at its time budget (`partialReasons:["scanBudget"]`) resumes at `scanOffset` under the same budget. A cut at the `maxFiles` maximum or the walk ceiling is terminal (`terminalLimit`), but the last page names the files it never read: the walk is name-sorted and depth-first, so they are the directories that sort after the walk's stopping point at each level, and `narrowScope`, `narrowScope2`, … in `hints` are this query over each such directory (for a directory's own unread files: its subdirectories excluded with `/*/` and the candidates the cut scan already read, which sort first, skipped by `scanOffset`, so no file is read twice; `maxDepth` counted from the new `path`). Only candidates count: a file the scan cannot parse (no grammar for its name, no `languageGlobs` rule) is neither unread nor part of a directory's size. At most 8 are offered; the rest are counted in a warning with a `structureSearch` lead that lists them. A hinted scope that itself exceeds the ceiling ends with hints of its own, so the calls together reach every unread file. A single directory holding more candidate files than the ceiling has no narrower `path`: the row adds `partialReasons:[…,"flatDirectory"]` and a warning naming it and its size (split it with `include` or `exclude` globs). A query whose `include`, `exclude` or `languageGlobs` match by position (anchored, or with a `/` inside) offers no scopes and says why, because another `path` would change what they match. The hints repeat the ignore, prune, path-policy and glob rules of the scan.
- Parser or matcher exhaustion keeps completed files and reports staged `diagnostics`; zero matches in an incomplete result prove nothing. `inside` reads each ancestor chain once, so deep nesting does not hit the deadline on `stopBy: end`; a file over the deadline is a `structural.match.deadline` diagnostic, never zero matches.
- A `match` names what its answer does not cover, beside the matches it lists. Files whose recovered parse could hide a match (tree-sitter error recovery, such as a macro attribute before a C++ method) make the row partial (`partialReasons:["parseRecovered"]`) with one warning that counts them and names a few; the count is the scan's, so every page states it. Only a file whose recovered syntax can hold a match counts: with literal anchors in the query, one whose error sits around an anchor in code (not in a comment or string); with none (a bare `kind`, `$F($$$A)`), any file with recovered syntax. `syntaxTree` shows where. Files of the query's grammar behind the default walk limits (ignored, hidden, default-excluded) are named in a warning, with `next.includeSkipped` (the same search with exactly the flags that reach them), as an empty result already does.
- A directory scan that skips entries withheld by path policy (credential-store directories such as `.aws`, credential-named files such as `.env`) says so in `warnings` and is not `coverage.state:"complete"`.
- `invalidPattern` (invalid or uncompilable query), `languageUnsupported`, `fileTooLarge` (a typed terminal limit, not `executionFailed`); engine `structural.*` names stay diagnostic codes. YAML `kind` rules are checked against the source grammar first: an unknown kind is a typed compile diagnostic, not zero matches. YAML is the rule format, not a source grammar.

Patterns:

- A pattern matches exactly as written; use a rule for alternatives. Modifiers are exact: Rust `fn $N()` does not match `pub fn` (a named child): when the scope holds nodes of the pattern's declaration kind that it missed, the row adds a `structural.pattern.exact` diagnostic counting them and a `broadenSearch` rule lead in `hints` listing them; write `pub fn …` or a rule on the item kind.
- Java call patterns can omit the trailing semicolon: the compiler adds grammar-checked statement context, direct or nested in a rule; complete patterns keep their parse, ranges, and captures.
- Matches inside Rust macro invocations and `macro_rules!` bodies, and inside C, C++ and CUDA `#define` bodies, are listed (each body is re-parsed in place, in the file's own grammar, so positions are file positions). A body that is not code (`a ## b`, a type, a brace) adds no match, no error and no noise.

Results:

- One-based lines and UTF-16 columns (span ends exclusive). A `symbols` declaration's `symbolName` + `line`, or an identifier capture's `text` + `line`, is `lspSearch` `symbolName` + `line` as-is.
- `match` rows: `{line, column, endLine?, value}` (whitespace-normalized; `column` is 1-based, so two matches on one line stay apart); per-file `totalMatchRows`/`returnedMatchRows` only when a page is a subset. `captureText:true` returns `{line, column, value}` rows (`endLine`/`endColumn` for multi-line spans) with `metavarRanges`; `next.expandCaptures` offers it when a row hides something it returns (a match cut to its header, or capture text the value does not show). With `extras:["hints"]`, a match page offers `read` in `hints`: the top file's hits with 3 lines of context (a multi-line match of up to 400 lines whole), when they fit 5 ranges.
- `syntaxTree` pages one file's tree; `pagination.totalItems` counts nodes.
- `symbols`: a file returns `symbols`; a directory returns `files: [{path, symbols}]`. `snapshot` only on paged results. A JS/TS declaration is `exported` by its local binding. With `extras:["hints"]`, a single named declaration gets `callers` in `hints` (function or method) or `references` in `hints` when a language server serves the file.

Symbols outline (declaration objects, nested `members`, `shared`: [location rows](TOOL_DATA_CONTRACT.md#search-result-shapes)). The YAML text prints `=== symbols <path> (line[-endLine] kind name; + exported; indented = member; doc = comment above) ===`, then:

```text
105-107 struct NamedPipeServer + doc@43
109-891 impl NamedPipeServer
  130-136 function from_raw_handle + doc@110
```

- Every row is an object. `line` is the name line (it feeds `lspSearch` at any depth); `endLine` appears when the declaration spans lines.
- Fields appear only when informative: `exported: true` (` +`); `exportedAs` for another export name (`export { foo as bar }`; `export default function foo` gives `default`; ` as a,b`); `docStartLine` for a comment block above (JSDoc, `///`, Python `#`; ` doc` when it ends on the line above, else ` doc@N`); `startLine` when attributes or decorators start earlier (` from@N`); `column` (1-based) only when two declarations of one kind share a name and line (` col N`).
- A trait or interface implementation names its trait (`symbolName:"AsyncRead for NamedPipeServer"`, `kind:"impl"`); a `symbolName` filter on the type still matches it, and its `references` lead anchors on the type. Members declared inside a macro body (`cfg_io_util! { … }`) nest under their real container.
- A member whose container a `kinds`/`symbolName` filter dropped stays top level with `parent` (`parentLine` when two containers share name and kind); the outline indents members two spaces and ends a detached one with `(in Parent@line)`. `pagination.totalItems` and `pageSize` count top-level rows; a page carries each container with all its members.

Structural extensions: `asm`, `assembly`, `c`, `cc`, `cjs`, `cpp`, `cs`, `cts`, `cu`, `cuh`, `cxx`, `go`, `h`, `hh`, `hpp`, `hxx`, `java`, `js`, `jsx`, `mjs`, `mts`, `py`, `pyi`, `rs`, `s`, `sbt`, `sc`, `scala`, `ts`, `tsx`. The same 30 back signatures and graph facts in the default release build; query the compiled engine capability API when optional grammar features are off.

### `structureSearch`

Directory outlines (`tree`, the default) and file metadata (`files`), with no parser.

| Field | Operations | Values and meaning |
|---|---|---|
| `path` | both | Directory root. |
| `maxDepth`, `minDepth` | both; `files` | 1–20 (1 = children). `tree` omitted: 1, or 20 with `include`. |
| `include` | both | ORed globs (`["*.ts", "*.tsx"]`); with `/` a glob matches the path below `path` ([shared rules](#shared-local-rules)). |
| `entryType`, `extensions` | both | `f` or `d`; extensions without a dot. |
| `exclude`, `defaultExcludes`, `includeIgnored` | both | [Shared local rules](#shared-local-rules); `includeIgnored` lists `.gitignore`d entries. |
| `includeHidden` | both | Include dot entries (default off, as in every local walk); one flag for every dot directory no ignore rule names, as in `localSearch` ([shared local rules](#shared-local-rules)). |
| `pageSize` | both | ≤1000 rows; omitted, fill the response window. |
| `maxEntries` | both | 1–10,000 entries scanned before paging. |
| `scanOffset`, `walkOffset`, `snapshot` | both | Copy from `next.continueWalk`. |
| `nameRegex` | `files` | Rust regex over the basename. |
| `time.modifiedWithin`, `time.modifiedBefore`, `time.accessedWithin` | `files` | Ages `<n>m\|h\|d\|w` (`7d`). |
| `size.greater`, `size.less` | `files` | `<n>[b\|k\|m\|g\|t]`, KiB-based (`100k`). |
| `empty`, `permissions`, `access` | `files` | Only empty entries; octal (`"644"`); `executable`, `readable`, `writable` (held by the caller). |
| `extras:["details"]` | `files` | Adds `totalLines` and `modifiedMs` (Unix ms) to each entry; without it an entry carries its size in bytes, plus the key it is sorted by (`sort:"lines"`: `totalLines`, `sort:"modified"`: `modifiedMs`). |
| `sort` | `files` | `path` (default: walk order like `git ls-files`, stops at `maxEntries`), or `modified`, `name`, `size`, `lines` (rank the full walk). |

Rows are `{dir, files}` groups with a workspace-relative `dir` (`dir + "/" + name` is a local path); entry and cut fields (`clipped`, `atLeast`, `totalAvailable`): [search result shapes](TOOL_DATA_CONTRACT.md#search-result-shapes). Entries have one shape in both operations: `name (bytes[, symlink])`, `name/` for a directory, and `extras: ["details"]` adds `totalLines=N` and `modifiedMs=N` to each file. `tree` lists `path`'s own group first, then subdirectories in walk order. A bare `"name/"` entry is left out when that directory has its own group on any page; every other entry appears once. Later pages reuse the first page's walk while the listed entries are unchanged. `summary` counts what was left out: `.gitignore`d entries, policy-withheld entries (the warning counts only entries the filters could match), default-excluded directories (named, such as `build, node_modules`), and, without `includeHidden`, dot entries (pruned directories such as `.git` excluded).

- A listing cut at `maxEntries` offers `next.continueWalk` on its last page (doubled `maxEntries`, `scanOffset` = rows already listed); pages plus `continueWalk` list every entry once.
- A walk is bounded by time as well (10 s, the same budget as `astSearch`'s scan window). A listing that runs out of time returns the entries it read with `isPartial`, `partialReasons:["walkDeadline"]` and a warning naming the first unscanned entry (walk order is name-sorted, depth-first). It is never `empty`. The last page offers `next.continueWalk`: the same query with `walkOffset` = the walk entries inspected so far, which resumes at the first unscanned entry, so pages plus `continueWalk` list every entry of an unchanged tree once. A resumed walk continues where the window it resumes stopped when the process that ran that window still holds the position (two minutes at most, and none of the directories it was inside changed, nor their ignore rules), so a deep resume costs only its own entries. Otherwise it replays the leading `walkOffset` entries without listing them (it still reads their directories, so a deep resume costs time before its fresh budget starts). A resumed window with no rows is not `empty`. The skipped, withheld, hidden and ignored counts of a window cover the entries it inspected and no others: a window ended at its `maxEntries`-th row counts only through that row, and the window that resumes there counts the rest, so the windows of one walk count each entry once. A cut with nothing inspected (no progress), `walkOffset` past its maximum, or a window also cut at `maxEntries` or the walk ceiling is a terminal limit: `terminalLimit`, a warning, and `narrowScope` in `hints` to a narrower `tree`. A window cut by both `maxEntries` and time widens first: its `next.continueWalk` doubles `maxEntries`, and the widened walk offers a resuming `continueWalk` if time cuts it again. An unnarrowed whole-tree walk with every ignore rule lifted (`includeIgnored` with `defaultExcludes:false`) over a build directory is the usual cause.
- With `extras:["hints"]`, `files` page 1 offers `read` in `hints`: the `minify:"symbols"` outline of the first source or doc file.
- A filtered `tree` that skipped dot entries offers `includeHidden` in `hints`. An empty listing whose skipped entries (`.gitignore`, dot entries, the default prune) hold matches offers `next.includeSkipped` with only the flags that find them, and a warning naming the rule, entry or directory; `files` lists `.gitignore`d entries once `defaultExcludes:false`, so that one flag is its lead for a pruned directory.
- A missing `path`: `pathNotFound` with `viewTree` in `hints` (nearest existing parent). A file as `path`: `notADirectory` with `next.read`, its `minify:"symbols"` outline.

### Context selection

`localFetch`, `localSearch`, `ghGetFileContent` and `ghSearchCode` accept `clasifyContext:{classifiers:[...],longContext?:boolean}` (1–5 classifiers) when a classification credential is set. When the provider answers, only selected evidence returns, in source order, with a `selection` receipt of selected, excluded and unknown units; `next.unfiltered` recovers the complete original view, including selected and excluded evidence. When it cannot answer now (`classificationProviderUnavailable`, `classificationRateLimited`), nothing is withheld: the view comes back unfiltered with an `unavailable` receipt, a warning names the code, there is no `next.unfiltered`, and later pages replay without provider calls. Any other provider error (credential, quota, content, size) fails the row and offers `next.unfiltered`. A source that fails (missing path, unreadable file, failed request) reports its own error with an `unavailable` receipt whose reason is `sourceUnavailable`; only `ghSearchCode`'s failed page offers `next.unfiltered`, which reruns the search. When the reader can isolate the unknown evidence, `next.unfilteredUnknown` recovers only that; when unknown ranges do not fit one `localFetch`/`ghGetFileContent` call, `next.unfilteredUnknown2`, `next.unfilteredUnknown3`, … read the rest. Classifiers judge only each candidate's shown snippet, and a `localFetch` page marks left-out lines as `... [lines a-b not selected] ...`. Past the candidate budget, `next.nextCandidatePage` repeats the same classified call over the candidates it left unjudged; following the chain judges each candidate once. A call without `clasifyContext` makes no provider request.

<!-- tool: localFetch -->
```json
{"path":"docs/guide.md","clasifyContext":{"classifiers":["How does cancelling a queued request release its reserved capacity?"]}}
```

Scope, decisions, line windows, long context and recovery: [classifiers on search and fetch](OCTOCODE_CLASIFY.md#classifiers-on-search-and-fetch).

### `localFetch`

Reads a known local path; a path-only read returns exact source after redaction. `ranges` (`["a-b"]`, one-based inclusive, 1–10) and `matchString` select lines (together, hits inside the ranges); `fullContent:true` excludes both. Reader rules: [`ghGetFileContent`](#ghgetfilecontent).

| Field | Meaning |
| --- | --- |
| `unit`, `offset` | `lines` (default) or UTF-8 `bytes` of the selected view; zero-based offset (raw source lines for a whole-file `minify:"standard"` read, outline lines with `minify:"symbols"`; byte offsets on code-point boundaries), default 0. |
| `length` | 1–50,000, a maximum under the response window. Omitted, ordinary line pages fill that window; byte pages default to 16384 bytes (a narrow response window never shrinks one below 4096), and a byte `offset` reaches every byte of a line, however long, in any source size; only an offset past the view's last byte is out of range. A first bare read (no range, match or `fullContent`) of a view over one line page (16 KiB) returns its first 200 lines, and `next.continueWindow` reads on from there; a source above the in-memory limit previews 100 source lines, then continues through bounded windows. `fullContent:true` selects the whole view and can still return continuations. |
| `matchString` | Nonempty source text, or a list matching any entry; `regex`, `caseMode` and `wholeWord` as in `ghGetFileContent`. A multiline match keeps every line it touches, in original byte coordinates. |
| `contextLines` | 0–100, default 3 per side. A matched line over 2,000 bytes (minified source) shows 200-byte windows around each hit instead, with any `contextLines` but 0 (`0` returns the matched line itself, paged from its start); full-source redaction precedes byte matching, edges expand to whole code points, and disjoint windows are separated by `... [N bytes omitted] ...`. |
| `minify` | Omit for exact text; `standard` compacts it, `symbols` returns a whole-file outline. Match views keep source text. |
| `block` | Widens a range or match window to its enclosing declaration (≤400 lines). With `matchString`, hits on a declaration head widen to that declaration and other hits keep their window; when no hit is a head, each hit widens. A range, or a match on a declaration's first line, also takes the doc comments and attributes above. JS/TS functions assigned to a member (`res.redirect = function () {}`, `exports.x = () => {}`) are declarations. A window no declaration encloses keeps its lines with a `block:` warning. |
| `fullContent` | Select the complete view within resource and security limits; no window selectors. Follow emitted continuations when the view needs multiple pages. |
| `snapshot` | From a continuation; a file changed since is rejected, not mixed. |

- Size limit: a source over 1 GiB is never read, not even in `ranges`, by `matchString` or with `fullContent`. The row is `fileTooLarge` with `sourceBytes`, `terminalLimit:true` and no `next`: no `localFetch` query reaches the file. Split or cut it outside Octocode (for example `head -c`, `split -b` or `tail -c`), then read the part.
- Paging: a source over 10 MiB is read in line windows without loading it whole (several `ranges` in one read with not-requested markers; a range that needs more than one window page ends the page with its first page, then reads on alone through `next.continueWindow` while `next.readOtherRanges` reads the ranges after it; `unit:"lines"` continues to the end through `next.continueWindow`). Line pages keep whole lines within 16384 bytes. An offset at or past the end returns empty content with an offset-zero `next.restart` (`extras:["echo"]` adds `pagination.outOfRange:true`). Continuations stop at the selected range or matched view.
- Exact text: a leading UTF-8 BOM is not text. Every read (whole-file, range, window, streamed, pattern and minified) leaves it out, and byte offsets and `pagination.totalBytes` exclude it; `sourceBytes` is the file's size, so it counts the BOM's three bytes. A U+FEFF anywhere else is text and stays. Redaction and disclosed non-UTF-8 decoding still apply.
- Fields: `sourceBytes`, `returnedBytes`, `returnedLines`, and `returnedChars` (UTF-16 units) come with `extras:["stats"]`. Content and the `symbols` outline are numbered `cat -n` style ([numbered source content](TOOL_DATA_CONTRACT.md#numbered-source-content)).
- A default context window (no `contextLines`) that stops inside its declaration shows the rest inline when that costs about 200 bytes or less; otherwise ask for `block:true` or a `ranges` read of the rest. No automatic reread lead is emitted.
- A regex `matchString` scan stops after 100,000 matches: the read selects that prefix, sets `partialReasons:["regex-match-limit"]` with a warning naming the stop line, and offers `findText` (a `localSearch` of the file) to page every match.
- A `matchString` that selects no line: an empty row with one tip and leads `findText` (a `localSearch` of the file's directory) and, after a case-sensitive match, `ignoreCase` (`caseMode:"insensitive"`). A missing file: `pathNotFound` (exit 3).
- Redaction: private-key blocks are redacted across the whole file before selection, so a key split by a page or range boundary never leaks. A line page is scanned with 8 KiB of surrounding lines, so a multi-line secret at the edge still matches whole (one page scan per call). A byte page is scanned on its whole lines plus at least 8 KiB of surrounding whole lines (single-line secrets always whole); a secret the page end cuts extends the page to the secret's end, so its placeholder shows on the page it starts on and `next.continueWindow` resumes after it; an offset inside a secret returns its placeholder. Byte offsets stay in unredacted coordinates. `fullContent` is scanned whole; above the scanner's 10,000,000-byte limit, `contentSecurityLimit` offers a smaller source range when possible, else a terminal limit. A full-content view over 50,000 bytes supplies bounded executable recovery. Totals unavailable due to access or resource limits are reported as unavailable.

## LSP tools reference

`lspSearch` is the one semantic tool (operations below). It is a local tool ([shared rules](#shared-local-rules)) and needs a file on disk. For a remote repository, use a `ghCloneRepo` `location.localPath`. Anchor from `localSearch` or `astSearch`, confirmed by `localFetch` ([local research](OCTOCODE_WORKFLOWS.md#local-research)).

### `lspSearch`

<!-- tool: lspSearch -->
```json
{"path": "/workspace/src/run.ts", "operation": "references", "symbolName": "isOctokitDeprecation", "line": 27, "groupByFile": true}
{"operation": "workspaceSymbol", "symbolName": "ToolConfig", "workspaceRoot": "/workspace"}
```

| Field | Values | Meaning |
|---|---|---|
| `path` | file path or URI | Required except for `workspaceSymbol`, which needs `path` or `workspaceRoot`; `path` selects the language server. |
| `operation` | next table | Anchored requests default to `definition`; other requests need it. Keep it in durable examples and continuations. |
| `symbolName`, `line` | string; ≥1 | Exact symbol text on the observed 1-based line (name-anchored operations). `workspaceSymbol` takes `symbolName` as a fuzzy query. |
| `orderHint` | 0–100,000, default 0 | Picks among repeats of the name on that line; `resolvedSymbol.alternatives` lists each repeat's `orderHint`. |
| `workspaceRoot` | path | Overrides root detection; in mixed-language workspaces also pass `path`. |
| `rustContext` | object | Needs a `.rs` `path`, also for `workspaceSymbol`. See [Rust build context](#rust-build-context). |
| `contextLines`, `groupByFile` | 0–100; boolean | `definition`, `references`, `typeDefinition`, `implementation`: source lines around each location (keep `0` unless you need previews); per-file rollups. |
| `includeDeclaration` | default `true` | `references` only; turn it off before unused analysis. |
| `graphDepth` | 1–20, default 1 | Call and type hierarchy depth; 1 = direct edges. |
| `page`, `pageSize` | ≤1000; 1–1000 | Omitted, fill the response window. Copy `page` only from `next.nextPage` (with its snapshot), or set `pageSize`; a later page with neither is rejected. |
| `snapshot` | ≤128 chars | From `next.nextPage` or `next.nextImporterPage`; omit on page 1, required later. |
| `importerPage` | 1–1000 | TS/JS `references` and `callers` importer scan: first candidate window of the request; copy only from `next.nextImporterPage`. |
| `candidatePage` | 1–1000 | `subtypes` without a type hierarchy: the window of 200 implementation candidates of the anchor type a walk checks; copy only from `next.nextCandidatePage`. |

| `operation` | Use | Output (row shapes: [location rows](TOOL_DATA_CONTRACT.md#search-result-shapes)) |
|---|---|---|
| `definition` | Usage or import to declaration; unresolved provider locations kept unchanged. | `kind:"definition"`, `matches[]`. |
| `references` | Uses of a function, type, variable, constant, class. | `files[]` of `{path, matches}`; `value` is the first line, a recovered row has `source`; `includeDeclaration` keeps or drops the declaration site. `groupByFile:true` gives `{path, matchCount, lines}`; `groupByFile:false` or `contextLines` gives per-location `matches[]`. Total: `matchCount`. With `extras:["hints"]`, `read` in `hints` reads the sites. |
| `callers` | Static incoming calls. | `files[]` of the calling declarations (`symbolName` + `line` is the next anchor) with their call `sites` in that file; `detail` is a container name, or a signature only when listed callers share a name. With `extras:["hints"]`, `read` in `hints` reads the sites. Paged. |
| `callees` | Static outgoing calls. | Rows filed under the caller's file (where the sites are); `path` names the callee's file when it differs. Paged. |
| `hover` | Type, signature, docs. | `hover`; `null` is `empty` with `noHover`. |
| `documentSymbols` | File outline. | `symbols[]` in the [`astSearch`](#astsearch) outline format, `unlistedNested` (function locals, counted only), pagination. |
| `typeDefinition`, `implementation` | Declared type; implementations of an interface or abstract symbol (when supported). | `matches[]`. |
| `workspaceSymbol` | Symbols from one server (not merged across servers). A `workspaceRoot`-only query opens one representative source (tsconfig `include` root, then `src/`). | `matches[]` of `{name, kind, containerName?, path, displayRange}`, paged over the returned provider rows; coverage remains unknown. |
| `supertypes`, `subtypes` | Type hierarchy (recursive with `graphDepth`) when advertised. | `matches[]` of `{name, kind, detail?, path, displayRange, level, via?}`, or `capabilityUnavailable`. |
| `diagnostic` | Pull diagnostics when the server has a pull provider, else the bounded push cache. | Diagnostics, or `empty` (`noDiagnostics`, `diagnosticsNotPublished`). |

Response fields: `resolvedSymbol` (the anchor; `.name` is an optional discovery hint), `summary` (symbol and call-flow totals), `kind`, `matches`/`files`, `pagination`, `rustContext` (normalized settings and fingerprint, when supplied; also on native document-symbol results), `next` (`nextPage`, `nextImporterPage`, `restart`, `retry`, `continueWalk*`), and `hints` (`read`, `findText`, …). Opt-in: `operation` and `workspaceRoot` (`extras:["echo"]`); `lsp` (availability, `lsp.source`, `lsp.receipt`) with `extras:["receipts"]`, shown without it only when the source is degraded; `meta.evidence` and `meta.diagnostics` (`extras:["diagnostics"]`; partial markers stay on the row).

Empty and partial:

- Empty: `kind:"empty"`, row `status: "empty"` (CLI exit `1` when every row is empty), `category` `noLocations`, `noHover`, `noDiagnostics`, `diagnosticsNotPublished`, or `unsupportedOperation`. Read the category, not only the exit code.
- `workspaceSymbol` is partial with `workspaceSymbolCoverageUnknown`: the protocol supplies no completeness marker or provider cursor. Native pagination covers the returned rows; neither its final page nor an empty response proves exhaustive workspace coverage. An optional `documentSymbols` lead inspects one authorized file with different matching semantics.
- A server still loading the project: partial row, `partialReasons: ["languageServerIndexing"]`, a warning, and `next.retry`; zero results then prove nothing. Other empty results are not retried.
- An empty anchored answer (every operation except `documentSymbols`, `workspaceSymbol` and `diagnostic`) on a line where the server reports an error is partial with `anchorLineError`. The warning names that error (its first line and source, plus a count of other errors on the line), and `next.diagnostics` lists the file's diagnostics. Example: clang without the CUDA SDK drops every `kernel<<<…>>>(…)` launch (`cudaConfigureCall` is undeclared). The server's diagnostics are awaited for at most 1 s; without them the row stays as answered.
- Reference counts cover the server's returned set (`matchCount`, `fileCount` with `extras:["stats"]`); `coverage` appears only with a per-call fact (`textOnlyFiles`, an unnamed row's `reason`). `source` labels recovered rows: `recoveredAlias` (through an aliasing import; counted in `recoveredAliasReferences`), `recoveredImporter` (re-queried from a verified importer's import anchor), `recoveredFromReferences` (a caller). On `importerScanFailed`, or a TypeScript file with no project configuration (`inferredProject`), the row is partial with the reason in `partialReasons`, and a name-anchored request carries `findText` in `hints`, a `localSearch` of the files that spell the name outside the answer. A partial row with no page continuation keeps its leads in the default output, each with every row: they are its only route to what it withholds. Rust references under `cfg(feature …)` gates carry `partialReasons:["cfgGatedFiles"]` and `next.expandFeatures`.
- `pagination.snapshot` fingerprints the canonical query plus the full result set; it validates a recomputed set across processes and keeps no old rows. A changed set, or a later page without its snapshot, fails as `staleSnapshot` (no items, `next.restart`).

Importer scan (TS/JS `references` and `callers`): candidates are files under the search scope that spell the name and that the server's answer does not list, sorted by path; the scan verifies them 24 at a time (`importerPage` names the first window of a request). `pagination` pages the sites found so far and is the only page counter: the scan reports progress as candidate numbers ("Verified candidates 25-216 of 1878"), never as pages. A scan with candidates left is partial (`importerScanCapped`, not terminal), and the last page of sites carries `next.nextImporterPage` (the candidate-list digest as `snapshot`). Walk `next.nextPage` to the end of the sites, then `next.nextImporterPage`. Gitignored files are not candidates: when some spell the name (default-excluded directories such as `node_modules/` aside), the row is partial (`importerScanIgnored`), its warning names the deciding ignore rules, and `next.includeSkipped` is a `localSearch` with `includeIgnored` over them. The scan for them has a 3 s bound; one that stops there still makes the row partial (`importerScanIgnored`), lists the files it reached, says the scan did not finish, and `next.includeSkipped` then covers the whole scope.

- Each candidate file is verified in one window, and its recovered rows are listed only there, so every row appears once. A row recovered in a non-candidate file stays with the window that found it.
- The first request lists the server's answer, then the first window. A later request lists only the rows of the windows it verified, and skips windows with no verified importer in the same request (up to 8 windows or 10 s), so a page lists sites or ends the scan.
- A later request that still finds no site (the skip cap was reached, or the scan ended) is not `empty` (exit `0`): `reason` states the candidates checked and whether `next.nextImporterPage` continues the scan or the scan is complete.
- `references` has no path or scope field: it lists every site the scan reaches. To count or list sites under a directory, use `groupByFile:true` (one `{path, matchCount}` row per file) and keep the paths under that prefix, or run `localSearch` with `path` set to the directory and the name as `matchString`.
- A changed candidate list gives `staleSnapshot` with `next.restart` (importer page 1, location page 1).
- `callers` with `graphDepth` > 1 windows only level-1 callers; subtrees can repeat across importer windows.

Coordinates (one-based lines, UTF-16 columns = LSP `character + 1`): `resolvedSymbol.foundAtLine`/`foundAtCharacter` (only when the symbol sat off `line`), location and hover `displayRange {startLine, startCharacter, endLine}`, hierarchy and workspace-symbol `displayRange`, and call `sites`. Declaration rows add a `column` only when two symbols share a line. A hierarchy node's `displayRange` starts at the name (usable as `line`) and ends with the declaration.

Hierarchy walks: rows are edges. Type items carry `level` (`1` = direct) and, below level 1, `via {name, path, line, character}`; call rows below level 1 carry `via: {symbolName, line}` (plus `path` when another node shares that name and line), so the flat list rebuilds the tree. `callers` rows put sites in the caller's file, `callees` rows in the `via` (or anchor) file. Repeated (parent, node) pairs merge into one edge. The walk is breadth-first; a node (canonical path + selection range) expands once at its shallowest level, and a node reached again keeps its edge without re-expanding. Caps per walk: depth 20, 200 nodes, 50 results per node.

- Node cap: `clipped`, `isPartial`, `partialReasons: ["hierarchyNodeLimit"]`, and `next.continueWalk` on the first parent whose children were dropped, with the remaining depth; other such parents get `next.continueWalk2`…`N`, and `unexpandedParents` lists all (`name`, `path`, `line`/`character`, `remainingDepth`). Each re-anchors by `symbolName`, `line`, and `orderHint` when the name repeats on the line.
- Fan-out cap: `partialReasons: ["hierarchyFanOutLimit"]`, `terminalLimit: true`; use paged `references`.
- A provider failure after some results: `callHierarchyExpansionFailed` / `typeHierarchyExpansionFailed`, with `next.retry` only when a failure is transient (timeout, crashed or not-ready server); a relation the walk could not establish (an unresolved base, an ambiguous candidate) is partial without it. Items outside the allowed roots are omitted with a warning.

Errors: `outsideAllowedRoots`, `pathNotFound`, other path failures `fileAccessFailed`; `invalidInput`; `timeout` and `serverCrashed` (both `retryable: true`); `capabilityUnavailable` (method not implemented); `requestFailed` (`retryable: true` except rejected parameters); `serverUnavailable` (no configured or startable server); `anchorUnresolved` (`symbolName` not near `line`, exit 3). A name-anchored failure offers `read` in `hints` of the `symbolName` match windows; empty diagnostics offer no read.

### Rust build context

| `rustContext` field | Default in an explicit context | Meaning |
|---|---|---|
| `features` | `[]` | Cargo features, or `"all"`; deduplicated and sorted for identity. |
| `noDefaultFeatures` | `false` | Disable default features. |
| `target` | unset (the server's Cargo environment) | Target triple, 1–256 chars. |
| `cfgs` | `[]` | ≤100 cfg settings added to the effective list (`"custom"`, `"mode=fast"`); `"!custom"` disables one. |
| `buildScripts` | `false` | Run build scripts and load their cfg and generated sources. |
| `procMacros` | `false` | Expand procedural macros; requires `buildScripts: true`. |

- In a crate with a build script that did not run, an incoming answer missing files that spell the name is partial (`buildScriptsOff`) and offers `includeBuildScripts` in `hints`: the same query with `rustContext.buildScripts: true`, which runs the crate's build scripts.
- rust-analyzer loads only the package directory, so an `include!` of a file outside it never resolves: an answer missing files that spell the name through such an include is partial (`includeOutsidePackage`) with a warning naming the file; no `rustContext` recovers it; check those uses lexically with `localSearch`.
- A file that several files declare as a module (`#[path = "support/x.rs"] mod x;` in a bench and a test) is compiled once per declaring crate, and rust-analyzer answers for one copy: an incoming answer missing files that spell the name is partial (`sharedModuleFile`) with a warning naming the declaring files; check the other copies' uses lexically with `localSearch`.
- A context is an overlay: it changes only the fields it sets, and only where the value differs from the effective one (the configured server options, else rust-analyzer's default). Everything else, `cfg(test)` code included, stays as the default server analyzes it, so `{}` or a context equal to the defaults reuses the default server. Build scripts and proc macros run only when `buildScripts`/`procMacros` is true; enabled providers run workspace code, and the context is not a sandbox.
- rust-analyzer supplies cfg-selected definitions, declarative macro expansion, and enabled build-script or proc-macro results; `astSearch` gets no compiler expansion. A disabled provider can explain an empty answer for a declaration a normal Cargo build generates.
- Different effective settings use different pooled clients. `rustContext.fingerprint` identifies the normalized requested settings and is part of pagination, so a changed context restarts pagination. It does not pin sources, Cargo configuration, toolchain, environment, or generated artifacts, and is not a reproducible-build identifier. See the [engine lifecycle contract](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#rust-context) and [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html).

### Root selection

Without `workspaceRoot`, the root is the nearest ancestor of the file with a project marker (`package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, `pyproject.toml`), even inside `WORKSPACE_ROOT`; with no marker, the process cwd.

### Server fidelity and the no-fallback contract

`documentSymbols` falls back to a native outline (`lsp.source`: `native` via oxc for JS/TS, or `markdown` headings; syntax-only) only when no server is available; a server (`lsp`: type-aware, cross-file) always wins. Every other operation needs a real server. Without one, Octocode does not guess: `status:"error"`, `errorCode:"serverUnavailable"`, and a message that points to `localSearch` or `astSearch` + `localFetch`. There is no same-file-only `references` path. See [LSP server lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md).

### Subtypes and implementations without an answer in time

`subtypes` on a server with no type hierarchy asks `textDocument/implementation`; `implementation` on a class, interface, struct or trait asks it directly. For both, a syntax scan of the files that spell the name (declarations whose `extends`/`implements` clause names it) runs at the same time. For `subtypes` it adds no wait; for `implementation`, a server answer also waits for the scan, and the declarations it found that the server left out (a function-local impl, say) follow as rows labelled `source: "native-graph-facts"`, partial (`partialReasons: ["scanOnlyImplementations"]`, no retry) with a note that they are unverified. The server's rows win when they arrive: while the scan runs and for up to 5 s after it finishes. If the request fails (timeout, crash, rejected) or is still silent after that grace, the scan's rows are the answer, labelled `lsp.source: "native-graph-facts"`, partial (`partialReasons: ["serverUnanswered"]`) with `next.retry` to ask the server again, and a note that they are unverified (the base is matched by name, not resolved to the declaration) and direct only (run the operation on each to descend). Files the scan could not read are named in a warning, add `heritageUnread` to `partialReasons`, and get a `next.findText` text search over them. A server answer is labelled `lsp.source: "implementation"`; for `subtypes`, direct and transitive subtypes are not told apart. rust-analyzer answers a trait with its impl blocks: each trait impl (`impl Trait for Type`) is a direct subtype row with `kind: "impl"`, named by its self type, and has no subtypes of its own. A `subtypes` walk checks the anchor type's candidates 200 at a time: past the first window the row is partial (`hierarchyCandidateLimit`) and its last page offers `next.nextCandidatePage`; a deeper type with more than 200 candidates stays a terminal limit. `implementation` on a function, method or other member has no scan: a timeout stays an error with the retry tip and the astSearch/localSearch route.

### TypeScript backends

1. An `lsp-servers.json` entry for the extension ([custom servers](#custom--bring-your-own-servers)); it always wins and gets no substitute.
2. `tsgo`, for TypeScript (`.ts`, `.mts`, `.cts`, `.tsx`) only: the workspace's own `node_modules/.bin/tsgo` when `lsp.trustProjectConfig` (`OCTOCODE_TRUST_PROJECT_LSP_CONFIG=true`) is on, else the `@typescript/native-preview` platform executable in Octocode's install tree (`octocode-mcp` and the `octocode` CLI depend on it). JavaScript stays on `typescript-language-server`, because tsgo drops constructor functions and `this` expandos. tsgo returns at most 256 workspace symbols; a `workspaceSymbol` answer that reaches 256 warns that the server capped it (narrow `symbolName`, or route `.ts` to `typescript-language-server` in `lsp-servers.json`).
3. `typescript-language-server`, for JavaScript, and for TypeScript when no `tsgo` is found or it fails to start (the answer says so): an executable on `PATH`; else `node_modules/typescript-language-server/lib/cli.mjs` through the current Node, from Octocode's install tree, then the workspace's `node_modules` only when the workspace is trusted, then the start directory. Trusted here means `lsp.trustProjectConfig` or a workspace at or inside the start directory, so a scanned checkout such as a clone cannot supply the executable.

Definition locations stay server output; import targets are never rewritten. LSP reads minified `.js`, but original source gives far better results.

### Language servers

Built-in routes: TypeScript (`tsgo`, else `typescript-language-server`; see [TypeScript backends](#typescript-backends)), JavaScript (`typescript-language-server`), Python (`basedpyright-langserver` or `pyright-langserver` on `PATH`, else `pylsp`), Rust (`rust-analyzer`), Go (`gopls`), Java (`jdtls`), C, C++, and CUDA (`clangd`), C# (`csharp-ls`), Scala (`metals`). Assembly has no built-in server. Rust and C/C++ support managed downloads (`octocode lsp-server install <name>`); others use installed executables, and a missing one's error names its install command.

Built-in servers start headless and read-only (user arguments and options win): rust-analyzer runs no build scripts, proc macros, or `cargo check`; clangd runs `--background-index=false --clang-tidy=false --log=error --pch-storage=memory`; jdtls keeps `-data` under the Octocode home with `java.autobuild.enabled:false`; Metals starts with `isHttpEnabled:false`. Each server tree is capped at 25% of host RAM, between 4096 and 8192 MiB (a per-server `maxMemoryMb` in lsp-servers.json overrides it; `0` disables); on macOS and Linux an RSS watchdog enforces it on the whole server tree ("language server exceeded memory cap").

Lines end at `\r\n`, `\n`, or a lone `\r` (the LSP rule). A location in an allowed file that cannot be read (too large, not UTF-8) is kept, with `content` stating why.

#### Custom / bring-your-own servers

An `lsp-servers.json` entry adds an extension or replaces a built-in server (another `rust-analyzer`, `tsgo`, a pinned `clangd`). Load order: `lsp.configPath` / `$OCTOCODE_LSP_CONFIG`; `<workspace>/.octocode/lsp-servers.json` (only with `lsp.trustProjectConfig`); `~/.octocode/lsp-servers.json`.

<!-- example: configuration -->
```jsonc
{"languageServers": {".php": {"command": "intelephense", "args": ["--stdio"], "languageId": "php"}}}
```

`command` and `languageId` are required; `args` (default `[]`) and `initializationOptions` (sent verbatim in `initialize`) are optional. The server answers what it advertises; an extension with no entry and no built-in route returns `serverUnavailable`. See [`LSP_SERVER_LIFECYCLE.md`](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#custom-configuration).

## Semantic assessment reference

`clasify` answers keyed typed `questions` (`noul`, `choice`, `score`, `box`) over exactly one of literal `state` or a `source` reference (a `localFetch` or `ghGetFileContent` row with explicit `ranges` or `fullContent:true`, or an HTTPS `url`). A row returns typed `answers`, the answering `model` and its `usage`, and for a source row a `read` in `hints` lead to exactly what was judged; never a body. `{operation:"models"}` lists the provider's models. Fields: `npx octocode schema clasify --view query`.

Use [context selection](#context-selection) to choose evidence to read, and `clasify` only for a typed judgment that changes your next action. Primitives, sources, images, output, limits and availability: [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md). Answers guide inspection; verify deciding source.

## Clone and local tools workflow

`ghCloneRepo` ([fields](#ghclonerepo)) brings remote source to the local, AST, and LSP tools: pass `location.localPath` as `path`, and keep the resolved revision and requested scope. Flow: [external to local](OCTOCODE_WORKFLOWS.md#external-to-local). Clones live under `<octocode-home>/tmp/...`, and the validators add the Octocode home as an allowed root, so `location.localPath` is valid for every local and LSP tool outside your workspace. Example sparse path: `<octocode-home>/tmp/clone/microsoft/typescript/main__sp_a3f8c1__host_4dc11541e2f7a1d9` (`kind: tree`).

### Cache behavior

| Behavior | Details |
|---|---|
| Clone TTL | 24 hours (fixed). |
| Clone keys | Commit SHA, sparse scope, history depth, and host. Sparse clones use `{branch}__sp_{hash}__host_{hash}/` and coexist with a full clone. |
| Identity | Clones accept a branch, tag, or full SHA and return the actual HEAD as `location.commitSha`. File reads resolve an omitted branch; pass a SHA for reproducible reads. |
| Clone hit | Only with a clean working tree at the recorded HEAD, and only after your credential passes a `commits/{ref}` check; a modified cache returns `checkoutDirty` and keeps its files. Check `verified` before you trust cached bytes. |
| Clone expiry | Clone activity evicts clean expired entries under their locks; modified checkouts are kept; directory-age sweeps never remove clones. |
| Force refresh | `forceRefresh: true` bypasses the cache and re-clones or re-fetches. |
| GitHub responses | Memory and disk caches, freshness, ETag revalidation, limits, the 24-hour sweep, manual cleanup: [Cache storage and lifecycle](CONFIGURATION.md#cache-storage-and-lifecycle). |
| Response marker | With `extras:["receipts"]`, a `ghGetFileContent` row whose files all came from its content cache has `cache: 1` (no other tool sets it; same on CLI and MCP). |
| Live tools | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, and `lspSearch` read the workspace directly and cache no results. |
