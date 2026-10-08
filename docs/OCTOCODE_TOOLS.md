# Octocode tools reference

Every tool's fields, defaults, limits, results, and continuations, on MCP and the CLI. Schemas come from `@octocodeai/octocode-core` (published in-repo by `@octocodeai/config/schema`); `@octocodeai/octocode-native` runs them. Other owners: the shared envelope, `next.*` pages, and `hints.*` leads ([TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md)); tool choice and flows ([OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md)); MCP setup ([OCTOCODE_MCP.md](OCTOCODE_MCP.md)); the CLI ([OCTOCODE_CLI.md](../packages/octocode/docs/OCTOCODE_CLI.md)); settings ([CONFIGURATION.md](CONFIGURATION.md)). The live schema is the final authority. Quality criteria: [TOOL_QUALITY.md](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md).

## How every tool call works

The CLI and MCP expose the same contracts. Every tool, `clasify` included, takes a `queries[]` envelope and returns one row per input position. MCP requires `queries`; the CLI runs one bare query object as one query; a bare array is rejected. A failed row or `clasify` page does not erase successful siblings.

### Base call envelope

| Field | Scope | Meaning |
| --- | --- | --- |
| `queries` | Required, outer | 1–5 queries for the same tool. Rows keep their zero-based input `index` and run concurrently, except `ghCloneRepo` and `astRewrite` rows (in order); `clasify` schedules its own provider work. Batching creates no dependencies between rows. |
| `mainGoal`, `reasoning` | Optional, per query, ≤500 chars each | The research question, and why this row advances it. Only for multi-call research on an unknown; a top-level value is not inherited, and a blank one is dropped. Not a ranking instruction or proof; no secrets, hidden chain-of-thought, or required runtime data. Carry-over rules: [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md#executable-continuations). `clasify` sends them in each page's evidence state and `mainGoal` with every question. |
| `debug` | Optional, per query | `true` adds the fields core classes as verbose (scan stats, receipts, echoes, diagnostics). Paging snapshots stay in `next`. |
| `responseLength` | Optional, outer | Whole-response text window, 1–50,000 chars; it does not replace a tool's own pagination. Omitted: a response over `output.pagination.defaultCharLength` (default 50,000) pages automatically through `responsePagination.next`, and a page with rows left opens with an `incomplete — …` warning. |
| `responseOffset`, `responseSnapshot`, `responseScope` | Optional, outer | Copy the first two from `responsePagination.next`. `responseScope` is what an explicit window pages: `text` (default), `structured` (serialized envelope), or `rows` (whole rows). |

Every other field belongs to one tool variant. Fields from different `operation` branches cannot mix, a line range is one `ranges` entry (`"a-b"`), and mutually exclusive selectors cannot combine.

### Schema discovery, variants, defaults, and hints

```bash
npx octocode schema                                     # catalog: names, availability, instructions
npx octocode schema localSearch --view variants         # branches, required fields, minimal examples
npx octocode schema ghGetHistoryItem --view query --select variant=pullRequest
npx octocode schema ghGetHistoryItem --view full        # the whole contract
```

The contract lists `variants` (when a branch applies, its required fields, a minimal example), `defaults` (values the runtime adds per branch), and `rules` (preparation and validation steps). A row has at most one `hints` object: `hints.text` holds prose tips (an empty or error row gets one recovery tip, ≤120 characters), and every other entry is an executable lead `{tool, query}`. Tips and leads never prove absence or success. Pages and completeness recovery calls are never hints: they stay in `next`, and the response is incomplete until you follow them.

### Results, evidence, partial failures, and continuations

Row fields are `index`, optional `status`, `data`, and, with `debug: true`, `meta` and `cache`. `status: empty` means no usable result (unsupported capability, unresolved anchor, or no match; read the tool diagnostics); `status: error` means the row failed. Missing output proves neither. When a row is partial, run its `next.*` page; a numeric cursor or a bounded first page is not complete. Minimal output, text rendering, evidence kinds, and pagination layers: [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md).

CLI exit codes: `0` success, `1` every row empty, `2` invalid input (including any rejected batch row), `3` not found, `4` authentication or permission, `5` execution error, `6` results with a re-runnable `next.*` continuation or a partial source read, `7` rate limited, `130` interrupted.

Each `errorCode` is one camelCase name with one exit class (2, 3, 4, 5, or 7). Core declares them once (`errorCodes.ts`) and ships them in the contract as `errorCodes`; an undeclared code fails the output contract. A call rejected before any row runs (schema-invalid query, malformed or missing JSON) returns one `{"kind":"octocode.toolError","version":1,"tool","error","errorCode":"invalidInput","details"}` envelope: on CLI stdout in JSON mode, and as MCP `structuredContent` beside an `isError` text that ends `(errorCode: invalidInput)`. `details` lists the repairs.

## Internal, external, and hybrid tools

"External" names the data or provider boundary, not the MCP transport. All sixteen catalog entries share the MCP and CLI contracts; availability gates can hide or reject an entry on one surface.

| Boundary | Tools |
| --- | --- |
| External (GitHub) | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem` |
| External (package registries) | `artifactSearch` |
| Hybrid (provider credentials, then a local checkout) | `ghCloneRepo` |
| Internal (allowed local paths) | `localSearch`, `structureSearch`, `astSearch`, `localFetch`, `astTopology`, `astRewrite` |
| Internal, with a language-server process | `lspSearch` |
| External classification provider (vendor: Jev) | `clasify` |

Availability:

- GitHub tools need provider runtime and credentials ([AUTHENTICATION.md](AUTHENTICATION.md)).
- Local tools are on unless `OCTOCODE_ENABLE_LOCAL` / `local.enabled` is `false`.
- `ghCloneRepo` is CLI-only (MCP never registers it) and needs persistent storage.
- `astRewrite` and `astTopology` are CLI-only and need `OCTOCODE_BETA=true` (or `local.beta: true`), their only gate (for `astRewrite` it covers preview and apply). Without it, `schema` and root help omit them, and `schema <name>` reports `availability.enabled:false` with `envVar:"OCTOCODE_BETA"`. MCP never registers them, and no MCP instruction, description, or lead names them.
- LSP needs a compatible server for the file language.
- `clasify` needs a resolved classification key ([OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md)). Without one, MCP omits it and a CLI call returns an actionable missing-key error.

## Text, AST, graph, and LSP: choose the evidence you need

Text (`localSearch`), structural AST (`astSearch` `match` with exactly one of `pattern` or `rule`), file graph (`astTopology`), and LSP (`lspSearch`) evidence complement one another; what each establishes and still needs: [evidence boundaries](TOOL_DATA_CONTRACT.md#evidence-boundaries). Proof order (orient, anchor, shape, topology, exact read, identity, run): [local research](OCTOCODE_WORKFLOWS.md#local-research). For graph results, keep `entrypoints`, `includeTests`, exclusions, scan caps, diagnostics, and `rustWorkspace` fixed; a change to any of them changes what "reachable" means.

## GitHub tools reference

Tokens, precedence, `GITHUB_API_URL` (GitHub Enterprise), OAuth login, and refresh: [AUTHENTICATION.md](AUTHENTICATION.md). Flows: [GitHub research](OCTOCODE_WORKFLOWS.md#github-research).

### Shared GitHub rules

- **Paging.** `page` (1–1000) and `pageSize`. When more remains, run the `next.*` call: `nextPage` or a content continuation (`continue`, `continuePatch`, `nextFilePage`, …). At a cap that cannot expand, metadata reports `terminalLimitReached` and omits unusable continuations. Numeric page, offset, and cursor fields and a raw `nextQuery` are not executable alone. File chunks page a `matchString` view without changing the selector.
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
| `owner` | User or org. Alone (optionally with `sort:"updated"`) it lists the owner's repositories (below). |
| `stars` | `">100"`, `"10..50"`. |
| `pushed`, `created` | ISO date, range, or window: `">2025-01-01"`, `"2024-01..2024-06"`, `"30d"`. |
| `archived` | `true` also lists archived repositories; omitted or `false` excludes them; `qualifiers:"is:archived"` lists only them. |
| `match` | `name`, `description`, `readme` (broader, slower). |
| `qualifiers` | ≤200 chars of rarer filters (`forks:>50 size:<5000 good-first-issues:>2 is:public`). Allowlisted keys; no `repo:`/`org:`/`user:`, and no key that has a typed field. |
| `sort` | `best-match` (default), `stars`, `forks`, `help-wanted-issues`, `updated` (latest push). |
| `pageSize`, `concise` | 1–100, default 20; `concise` gives flat `owner/repo` rows. |

Malformed counts and dates fail validation and name the accepted forms.

Rows: `owner`, `repo` (name only), `stars`, `language`, `license`, `pushedAt`, `createdAt`, `archived: true` or `fork: true` when so, the full `description`, and every `topic`; `debug: true` adds `forks` and `updatedAt`. `pagination` holds `totalItems` (count an org's repositories with `owner` + `archived:false`), `hasMore`, `currentPage`, and `totalPages`; the cursor is `next.nextPage`. Leads: `hints.viewRepo` (the top row's root listing); `hints.includeArchived` (page 1 of a search that excluded archived repositories: the same search with `archived:true`); `hints.findRepository` (an empty filtered search: the keywords alone).

Owner listing (`owner` only): the REST listing ordered by latest push (`order: "pushed"`), archived repositories excluded; a page that skipped some names their count in `warnings` and offers `hints.includeArchived`. One call reads up to 5 provider pages, so it can return more than `pageSize` rows. `next.nextPage.query.page` is a provider cursor that follows GitHub's `Link` header. No `totalItems`.

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
| `path` | Repository path prefix, not a file path. |
| `extensions` | 1–5, no dot; each is its own indexed search, merged into one page. |
| `filename` | GitHub also matches names that contain it; check each path. |
| `language` | For example `typescript`. |
| `match` | `file` (default: hits) or `path` (paths only). |
| `pageSize`, `concise` | 1–100, default 20; `concise` gives flat `owner/repo:path` rows. |

A repo-scoped `match:"file"` search resolves its hit-line commit during the index search, then reads the page's files concurrently. Row shapes (`lines`, `hitCount`, `readHits`, `alsoAt`, `atRef`, `lineResolved`, `indexRef`) and the non-head `ref` warning with `hints.viewRepo`: [search result shapes](TOOL_DATA_CONTRACT.md#search-result-shapes). `hints.read` is a `ghGetFileContent` `ranges` read pinned to the commit where the hit was verified; a file absent at the requested `ref` gets no read. A page GitHub marks incomplete is `isPartial` with `partialReasons:["providerIncompleteResults"]`, empty or not. A renamed repository is searched under its new name, with a warning that names it.

### `ghStructure`

<!-- tool: ghStructure -->
```json
{"owner": "vercel", "repo": "next.js", "path": "packages", "maxDepth": 2}
```

| Field | Values and meaning |
|---|---|
| `owner`, `repo` | Required; `repo` without the `owner/` prefix. |
| `operation` | `tree` (default); `refs`: `branches` and `tags` as `{"<name>": "<sha>"}` maps plus `defaultBranch`, the same page of both lists per page, `next.nextPage` while either has more; `languages`: bytes of code per language on the default branch, largest first. |
| `path` | Directory; `""` or `"."` is the root. |
| `ref` | Branch, tag, or SHA. A missing ref is an error, never a default-branch fallback. |
| `maxDepth` | 1–20; omitted: 1, or 20 with `include`. |
| `include` | 1–100 case-insensitive globs, ORed over repository-relative paths at any ref (`**/_exception_handler.py`, `src/**/*.ts`). Without `/` a glob matches entry names; a bare word matches names that contain it. |
| `pageSize` | 1–500; default 300 entries (`refs`: default 30, up to 100). |
| `materialize` | Writes this page's listed files (≤50, ≤300 KiB each) to `location.localPath`. |
| `materializeOffset` | 0–500: index in this page; resumes capped blob writes. |

`ref`, `path`, `maxDepth`, `include`, `materialize`, and `materializeOffset` apply only to `tree`. Contributors are not listed; `ghSearchHistory` author qualifiers answer who changed a path.

Tree rows under `entries` keep the `dir`/`files`/`folders` shape. A file entry is `"<name> (<bytes>[, <YYYY-MM-DD>])"` (the structureSearch form; the last `" ("` opens the fields); a dated folder is `"<name> (<YYYY-MM-DD>)"`. `resolvedRef` appears only when the default branch was resolved, and `commitSha` only when it differs from the requested ref. Page totals are in `pagination`; `debug: true` adds the per-page `summary`.

Freshness (listing pages only): a dated entry ends with the day of its last commit (`"routing.py (30472, 2026-08-26)"`, `"middleware (2026-09-23)"`), and `commitDate` is the listed commit's day. A page dates its first 100 entries in one GraphQL request (1 rate-limit point, about 3 s; cached per commit and paths; needs a GitHub token). A larger page names the undated count in `warnings` and carries `next.expandDates`: the listing pinned to the commit SHA at `pageSize:100`, one row per page that holds an undated entry (it can overlap dated entries, never skip one), each dated whole. A failed date request leaves entries undated with one `warnings` line (count and reason); it never fails the listing.

Materialize: for cross-file grep at a ref over MCP (no `ghCloneRepo`), combine `include` with `materialize:true`, then run `localSearch` at `location.localPath` (the listed directory; `hints.exploreClone` lists it). Files land under `<home>/tmp/materialize/v2/<owner>/<repo>/<commitSha>/`; a manifest beside them records each written file and size, so a later materialize reuses those files and clears a directory it did not write. Folders at `maxDepth` are listed, not written.

Leads: a first listing page offers `hints.read`, an outline of its entry file. A missing `path` is a not-found error whose `hints.viewTree` lists the nearest existing directory (case-corrected).

### `ghGetFileContent`

Reads one GitHub file without a checkout. Directories: `ghStructure`; a local subtree: `ghCloneRepo` with `path`.

<!-- tool: ghGetFileContent -->
```json
{"owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/config.ts", "matchString": "export", "contextLines": 2}
```

| Field | Values and meaning |
|---|---|
| `owner`, `repo`, `path` | Required. `path` is repository-relative, exact case, no leading slash. |
| `ref` | Branch, tag, or SHA; omit for the default branch. For a PR's head source, use its `sourceSha`. |
| `fullContent` | Whole file, for small files. Rejects `matchString`, `ranges`, and window controls. |
| `ranges` | 1–10 line spans (`["95-105"]`); the only line selector. |
| `matchString` | Every matching slice; a list matches any entry (a grep map). |
| `contextLines` / `contextBytes` | 0–100, default 10 / 0–16384. Mutually exclusive; `contextBytes` requires `matchString`. |
| `regex` | `literal` (default; pasted code stays literal), `rust` (linear-time), `pcre2` (lookaround, backreferences, 5 s deadline). |
| `caseMode` | `smart` (default: insensitive unless the pattern has an uppercase letter), `sensitive`, `insensitive`. |
| `block`, `unit`, `offset`, `length`, `minify` | As [`localFetch`](#localfetch); `length` 1–50,000; `minify` defaults to `none`. |
| `forceRefresh` | Bypass the content cache. |

Reader rules (here and in `localFetch`):

- Choose one extraction intent: whole file, line range, matching slices, or symbol outline. Symbol outlines reject match and line selectors. Selection precedes minification, redaction, and pagination; offsets are zero-based in the selected view.
- `unit:"lines"` (default) uses `length:2000`, but the 16384-byte page budget usually ends the page first; an oversized line switches to bytes. `unit:"bytes"` defaults to 16384 UTF-8 bytes; a byte end can extend by up to three bytes to finish a code point.
- `matchString` forces exact content (redaction still applies). `matchRanges` cover all selected context windows (overlaps merged); `matchedLines` holds the matching anchors on the current page; `selectedMatchCount` (debug-only on GitHub) counts matching lines in the selected view. A range stays bounded by its end line; a match continuation keeps its pattern and context. Lines inside a requested span are never elided. Line numbers and gap markers: [numbered source content](TOOL_DATA_CONTRACT.md#numbered-source-content).
- `totalLines` (and debug-only `sourceBytes`) describe the original file (`localFetch` reports it on every successful text read, including empty files and no matches); `pagination.totalLines` (when it differs) and `pagination.totalBytes` (every byte page) describe the selected view. `minifyFallback` gives the requested and applied modes and why, when a match forces exact content or an outline is unavailable.
- `minify:"symbols"` returns a paged outline; read its gutter, then `ranges` with `minify:"none"`. `standard` removes comments and rewrites formatting, with no JS/TS optimization or type-declaration removal; use `none` for quotes and comment-sensitive evidence ([minification coverage](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md#minification--file-reads-and-search-fragments)).
- Run `next.continue` unchanged; offsets are exact. `next` and `hints` queries omit default view fields (`fullContent`, `minify:"none"`) and keep a chosen `minify`.
- A match window that stops inside a declaration offers `hints.readBlock`; a small file the window mostly covers returns whole instead.

GitHub specifics:

- `fullContent:true` asks for an unpaged view. A view over 50,000 bytes (or a source over 100 KB) returns its first line page, `partialReasons:["full-content-size-limit"]`, and `next.continue` pinned to the resolved commit SHA.
- File fields (`path`, `content`, `totalLines`, `commitSha`, …) are directly on `data`. A first page (offset 0) adds `lastModified`/`lastModifiedBy` (the author login when GitHub links one, else the name) from the last commit that touched the path at the read commit: one more request, sent only after the content read succeeds.
- A batch of reads shares the automatic response window: a row over its fair share returns a shorter line page plus its own `next.continue`, and the envelope opens with `warnings: ["incomplete — N of M rows partial …"]`.
- A `matchString` that selects no line returns an empty row with a tip and leads (as [`localFetch`](#localfetch); `ignoreCase` at the resolved `ref`).
- A failed read keeps `errorCode`, and `retryable: true` when a retry can help (absent: do not retry unchanged); `httpStatus`, `requestId`, and `documentationUrl` only with `debug: true`.
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
| `sort`, `order` | `created`, `updated`, `best-match`, `comments`, `reactions`; `asc`/`desc` | Same | — |
| `path`, `ref` | — | — | File or directory prefix; ref to walk (default branch) |
| `qualifiers` (≤200) | `"author:x reviewed-by:y review:approved is:draft in:title merged:>2024-01-01 comments:>5"` | `"commenter:x label:bug"` | Only `author:` and `committer:` |
| `concise` | Compact rows | Compact rows | — |

Omitted `sort`: best match with keywords, else newest first; `order:"asc"` + `sort:"created"` is oldest first. `pageSize` 1–100, default 30. Search returns candidates and identities only: it takes no single-item identity, and commit search returns no diffs; read details with `ghGetHistoryItem`.

Validation: `qualifiers` rejects `repo:`/`org:`/`user:` (scope comes from `owner`/`repo`), unknown keys (with a suggestion), negation other than a PR's `-is:draft`, and a filter set twice. `owner`, `repo`, and person qualifiers (`author:`, `committer:`, `assignee:`, `mentions:`, `commenter:`, `reviewed-by:`, `review-requested:`) must be GitHub logins, names, or commit emails; labels cannot contain quotes or backslashes; a range or state qualifier is one term (whitespace inside `> 5` is removed). Any other value is a validation error, never a changed scope. An `archived:` qualifier always routes a PR query through search, which enforces it. PR branch filters are `head:` and `base:`.

Results: rows are an index. Each row carries its identity; a PR search beyond one repository adds `repository`. `hints.readPullRequest` targets the first merged row (else the first); `readPullRequest`, `readIssue`, and `readCommit` read any row when you swap in its number or SHA. A bare issue number in `keywords` adds `hints.readIssueLinks`, the issue read whose `closedBy` names its fix PRs. Commit rows carry the author's login (else the git name), never an email; `hints.readCommit` reads the full message. Search titles first (`in:title`); narrow commits by `path` and time (`since`/`until`, or `pageSize` for the last N) before a diff.

### `ghGetHistoryItem`

| Operation | Required identity | Detail |
|---|---|---|
| `pullRequest` | `owner`, `repo`, `number` | body, changed files, selected patches, comments, reviews, commits |
| `issue` | `owner`, `repo`, `number` | body and comments |
| `commit` | `owner`, `repo`, `ref` (+ `base` to compare `base...ref`) | metadata and optional diff |
| `compare` | `owner`, `repo`, `base`, `head` (SHAs for a stable comparison) | ahead/behind counts and commits between refs |

Fields from another operation are rejected, not ignored.

<!-- tool: ghGetHistoryItem -->
```json
{"operation": "pullRequest", "owner": "vercel", "repo": "next.js", "number": 12345, "sections": ["body", "files"]}
{"operation": "pullRequest", "owner": "microsoft", "repo": "TypeScript", "number": 51387, "matchString": "esbuild", "include": ["*.json", "*.mjs"]}
{"operation": "compare", "owner": "vercel", "repo": "next.js", "base": "v14.0.0", "head": "v14.1.0"}
```

| Field | Operations | Values and meaning |
|---|---|---|
| `sections` | all | PR (1–8): `body`, `files`, `patches`, `comments`, `reviewComments`, `reviews`, `commits`, `commitFiles`; omit for a summary. Issue: `body`, `comments`. Commit, compare: `files`, `patches`. |
| `include` | PR, commit, compare | 1–100 paths, `dir/`, or globs, any-of: `*` one segment, `**` any depth, no `/` matches the file name (`"*.ts"`). |
| `status` | PR | 1–7 of `added`, `removed`, `modified`, `renamed`, `copied`, `changed`, `unchanged`. |
| `minChanges` | PR | additions + deletions ≥ n (≥1); files without counts pass. |
| `matchString` | PR | Literal. Keeps matching patch lines, implies patches, returns every hit file on one page. |
| `contextLines` | PR | 0–100, default 10 lines around each patch hit. |
| `patchRanges` | PR | 1–100 `{file, additions?, deletions?}` (≤100 lines per side): patch lines to keep per file; reads patches. |
| `minify` | PR | `standard` (default) or `none` (Text views). |
| `includeBots` | PR, issue | Keep bot comments. |
| `offset`, `length` | all | Body, comment, or patch text window; `length` 1–100,000. |
| `pageSize` | all | PR 1–1000 (omitted: 30; a patch-free inventory fills the page). Others 1–100, default 30. |
| `filePage`, `commentPage`, `commitPage`, `reviewPage`, `page` | per operation | 1–1000; copy from `next`. |
| `path` | commit, compare | File or directory prefix. |

`include`, `status`, and `minChanges` keep files that match every field set; a scope that matches no changed file returns an empty row with an inventory hint.

**PR summary and menu.** Every PR row has its merge state: `mergedAt` (or `closedAt` when closed unmerged) and `targetBranch`; first-page labels (the first 20, `labelsTruncated: true` past that); `updatedAt` only while open. The body is opt-in (`sections:["body"]`). A merged PR reports `mergeCommitSha` (GraphQL `mergeCommit`; read it with `operation:"commit"`); an open PR never reports GitHub's test-merge SHA. Commit summaries carry the full message. A summary offers at most `readFiles` (with the body), `readPatches` (every patch of a small PR; with the body when no file list is offered), and `readDiscussion` (comments and reviews). A merged PR's first patch read also offers `readAtMerge` when a changed code file (not a test) adds lines: the most-changed such file of all loaded files, at the merge commit, as one `ranges` read of each hunk's new side (10 lines of padding, ≤10 ranges). An inventory read offers `readSelectedPatches`: up to 30 source files (no tests, docs, lockfiles, generated, or binary files; tests by language layout and naming such as `_test.go`, `test_*.py`, `FooTest.java`, `_spec.rb`, or `test/`), those that fit one patch budget first, the rest through `next.continuePatch`. On a large PR, ask for selected patches; leave commit diffs off until you know the commit.

**File inventory** (`sections:["files"]`): rows `"M +3 -1 [!flag ]path[ <- old/path]"` (git status letter, `T` changed, `U` unchanged; line counts; path), grouped as `{"dir/": [rows]}` (path = dir + name). `!tooLarge` (counts known), `!binary`, or `!omitted` (no patch or counts, for example past the diff budget; 0/0 can still have changed) mark a file without a patch; `<- old/path` marks a rename. It pages by `filePage`; `pageSize` omitted fills the response page (100–1000 rows), explicit up to 1000. GitHub lists at most 3000 files; past that: `countScope: "partial"`, `providerLimit.reason: "providerFileListLimit"`.

**Patches** (PR, commit, compare): rows `{path, stat: "M +3 -1", patch}`, keeping `patchUnavailable`, `previousPath`, and `patchPagination` when cut. Files not reached ride the continuation, not empty rows; a pure rename's patch is `""`. `@@ -a,b +c,d @@ heading` lines stay verbatim (the heading is the enclosing symbol when GitHub gives one). Each line is numbered `cat -n` style: kept and added lines with their new-file line (`86\t context`, `87\t+added`), removed lines with their old-file line (`85\t-removed`), a `\ No newline` marker with a bare tab; `path:87` is citable, and removing each gutter (to the first tab) gives the raw patch. Patch text is never minified.

- A patch read lists 100 changed files a page, so it covers every patch of a PR of up to 100 files. Rows of one call share one response page; an explicit `responseLength` (≤50,000) replaces the automatic page.
- A window ends at the last whole file; a larger file is cut at its last whole line. `next.continuePatch` is the same file page at the stream `offset` (same budget, nothing skipped or repeated); a page with unread patches offers only it, and the window that finishes them offers `next.nextFilePage`. Both ask for `responseScope:"rows"`: a row that still outgrows the page splits into structured `rowPart`s (follow `responsePagination.next` first) and the hop rides the last part. Run each unchanged.
- A PR patch read that does not fit one response opens with `fileSummary` (inventory rows with hunk counts, `M +10 -10 4 hunks rt_common.rs`); `warnings` names unfinished patches and files.
- `matchString` lists `!tooLarge`, `!binary`, and `!omitted` files as unsearched (a miss there is not absence). Narrowed or clipped files are re-read raw by `next.readFullPatches` (100 files per read, then `readFullPatches2`, …).

**Later pages** (an inventory, a patch read, a file/comment/commit/review page after the first, or a nonzero body offset) return a slim header unless `debug: true`: `number`, `state`, `sourceSha`, `mergedAt`/`closedAt`/`targetBranch`; first pages also keep labels, inventories title, author, and counts.

**Text views.** PR `minify:"standard"` compacts the Markdown of the body, comments, inline comments, and reviews; `none` keeps them after redaction. Dropped text sets `bodyView:"minified"` with `hints.readRawBody`. Match-filtered reads keep anchors. Issue, commit, and compare reads take no `minify` and return exact text after redaction. Bodies and comment bodies window by `offset`/`length` (automatic 12,000 characters); a continuation reads one surface. `next.continueCommentBody` (PR or issue) lists only the comments it continues; an issue's hop pages exactly the comments its first window showed and keeps that window's `body` section for sizing without showing the body again; `next.continueBody` reads the issue body. Comment-item continuations reset the text offset. Body, comment body, item page, file, and patch windows are independent; follow each.

**Collections.** One provider batch per source and call: 100 discussion comments, inline comments, or reviews; 50 commit summaries. `pageSize` bounds displayed items per batch (default 30, max 100); `reviewPage` pages reviews independently, and review bodies keep their text continuations. Follow `next.*` even after a filter empties a batch; it carries `collectionPages` positions and resets item and text windows, and a zero position marks an exhausted source. `countScope: "providerBatch"` counts describe one batch. `reviewComments` rows carry `path`, `side`, `startLine` (multi-line only), `line`, `commitSha` (read the code at that ref), `outdated:true` when anchored to the original diff, and `inReplyToId` on replies.

**Commits and comparisons.** `commitFiles` fetches one file batch per displayed commit (≤`pageSize` files each); its `next.nextFilePage` and `next.continuePatch` carry the exact SHA. Commit reads page files by `filePage` with independent file and patch windows. An explicit `length` sizes the patch window up to one response page (larger is clamped with a warning); omitted, it fits the page. `patchPagination` is per file; the page's patch-stream cursor rides `filePagination` and `next.continuePatch`. A commit read stopped at its file-batch cap reports `changedFilesCountScope: "partial"`, `countScope: "partial"`, and `terminalLimit`, never `complete`. A commit offers `hints.findPullRequest` (a `ghSearchHistory` query for the PR with that SHA). A comparison resolves `base` and `head` to SHAs; each page carries only its own data; its commit list rides the first window, and `pagination.totalItems` is GitHub's `totalCommits`. GitHub lists at most 300 compared files (`partialReasons:["providerFileLimit"]`); a `path` past them gets a warning and `hints.narrowScope` (the path's commits up to `head`). Provider caps and omitted patches stay terminal limits, not completeness.

**Issues.** An issue read lists up to 25 closing PRs (`closedBy`, merged first) and offers `hints.readPullRequest`: every patch of a small fix, else the body and inventory, or only the patches of files that `mainGoal` names (as `include`). Past 25, body and comment windows are `isPartial` with `partialReasons:["closingReferenceLimit"]`, and the fix is a medium-confidence candidate.

REST reads revalidate by ETag; a 304 costs no body.

### `ghCloneRepo`

Atomically clones a repository or sparse subtree into Octocode's managed cache (CLI-only; needs persistent `storage.mode`, the default). With local tools off, cloning works but omits `hints.exploreClone`.

<!-- tool: ghCloneRepo -->
```json
{"owner": "microsoft", "repo": "TypeScript", "path": "src/compiler"}
```

| Field | Meaning |
|---|---|
| `owner`, `repo` | Required; `repo` without the `owner/` prefix. |
| `ref` | Branch, tag, or full commit SHA; omit for the default branch. |
| `path` | Up to 10 repository-relative files or directories for a sparse checkout; no absolute path, backslash, or `..`. Completeness covers only these subtrees. A missing path is not-found (exit 3). |
| `historyDepth` | Commits of history, 1–50 (default 1). |
| `forceRefresh` | Fetch current state; refuses to replace a checkout with local edits. |

Result: `location.localPath` (absolute; pass it as `path` to local tools), `location.kind` (`repo` or `tree`), `source`, `commitSha` (the actual HEAD), `resolvedRef`, `requestedPaths` (sparse), `historyDepth`, `cached`, and `location.clonedAt`/`expiresAt` (fresh clones and cache hits). A hit's commit can lag the branch until `expiresAt`; pass `forceRefresh` for the current head. `verified` appears only when false. `hints.exploreClone` lists the checkout with `structureSearch operation:"tree"` (the subtree for one path, the root for several); a single sparse file is read with `localFetch` instead. Cache keys, reuse, and expiry: [Cache behavior](#cache-behavior).

- Directories use the lowercased owner and repository names: `<octocode-home>/tmp/clone/<owner>/<repo>/…`.
- A full clone suits exploration and LSP. A sparse clone is faster, but LSP cross-file resolution can be limited and dependencies can be missing.
- No GitHub API call precedes git: a branchless cache hit finds its entry through a recorded default-branch alias, and a fresh clone lets git resolve the remote HEAD. The API is asked only after git fails, to name a missing or inaccessible repository.
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
| `version` | 1–100 chars, exact lookups only. npm, PyPI, crates.io: exact, range (`^3`, `>=2.31,<3`, resolved like the registry's installer), or tag (`latest`, `next`). Go, NuGet: exact or `latest`. A missing version is `versionNotFound` with the nearest published versions. |
| `keywords` | 1–100 discovery terms. Not PyPI: a PyPI keyword query is `invalidInput`, with no fallback to a website or third-party service. |
| `page`, `pageSize` | Discovery only: page 1–1000 (copy `next.nextPage`), `pageSize` 1–100, default 10. |
| `registryUrl` | npm only: HTTP(S) registry override. Omit for npm environment and `.npmrc` routing. Credentials are not tool inputs. |

Rows (`artifacts[]`): canonical identity, available version, `description`, license, `homepage` (omitted when it is the repository page), and source metadata; `debug:true` adds registry URLs; rows never restate `ecosystem`. Exact rows add, when the registry has them, `publishedAt`, `deprecated`, `yanked`, `dependencies`/`peerDependencies` counts, and `engines` (npm), `requiresPython` (PyPI), or `rustVersion` (crates). npm discovery rows carry `downloadsMonthly`. Go package paths stay separate from module identities. Metadata can be absent; popularity is not comparable across registries. crates.io exact versions read the version record and crate metadata, not the whole version list.

Source leads: an exact GitHub-backed lookup offers one `ghStructure` lead at the package directory (plus the `main` directory of an npm package without a build step): `hints.viewReleaseSource` when the release has a ref, else `hints.viewRepo` (default branch). Release refs: npm `gitHead` or a provenance-attested commit; the Go version tag or pseudo-version commit; the Composer source reference; the NuGet nuspec commit; the crates.io packaging commit (`.cargo_vcs_info.json`, `path_in_vcs` as `repositoryDirectory`); for PyPI (and crates without VCS info) an upstream `v<version>` or `<version>` tag that exists on GitHub.

- `verification:"provenance"` only for a commit from an npm SLSA provenance attestation bound to this exact tarball (subject digest equals `dist.integrity`) and the manifest's repository. The registry verifies it at publish; Octocode does not re-verify signatures. Other refs are unchecked registry leads.
- An npm `gitHead` that GitHub reports missing (one commit check through the configured GitHub API) pins no lead: the row drops `sourceRef`, says `verification:"defaultBranch"`, offers `hints.viewRepo`, and `warnings` names the dead commit. When the check cannot run, the ref stays a `registryRef` lead.
- Check a lead's resolved revision before you treat it as evidence. A ref missing upstream recovers on the default branch (the same query without `ref`), which is not evidence of the release. Match the published or installed version before a version-specific claim.

npm: scoped names honor `@scope:registry`; an explicit `registryUrl` wins; authentication uses registry-scoped npm configuration (with environment interpolation); caches isolate registry and configuration identities; continuations keep the registry. A private registry needs npm's search endpoint for discovery. Other ecosystems use official public APIs.

Paging and errors: follow `next.nextPage`; unknown totals stay unknown, and an empty page can still continue. Auth, unsupported, rate-limit, and provider failures are errors; a missing exact package is `notFound`.

## Local code tools reference

### Shared local rules

- **Enable.** On by default. `local.enabled: false` (or `OCTOCODE_ENABLE_LOCAL=false`) turns the local surface off; `DISABLE_TOOLS` / `tools.disabled` hides single tools; `TOOLS_TO_RUN` is a strict allowlist. Removed compatibility names cannot come back through `TOOLS_TO_RUN` or `.octocoderc`. Settings: [CONFIGURATION.md](CONFIGURATION.md).
- **Paths.** Relative paths resolve against the workspace root (`WORKSPACE_ROOT`, default the cwd). Allowed roots: the workspace root, `ALLOWED_PATHS` / `local.allowedPaths`, and the Octocode home; the home directory only when listed. In every local tool (`localSearch`, `localFetch`, `structureSearch`, `astSearch`, `astTopology`, `lspSearch`, `astRewrite`), a path outside the allowed roots (directly or through a symlink) fails with `outsideAllowedRoots`, and a missing path with `pathNotFound` (exit 3); the recovery hint is keyed on that code. Row paths are relative to the envelope `root` ([paths](TOOL_DATA_CONTRACT.md#paths-shared-fields-and-anchors)). When the root's `.git` HEAD is readable, `localSearch`, `localFetch`, and `structureSearch` report it once as `shared.commitSha` (per row when rows read different repositories); uncommitted edits are not reflected.
- **Engines.** Native in-process ripgrep, walker, and structural engines; no external `rg`, `grep`, `find`, or `tree`. macOS, Linux, and Windows.
- **Filters.** `include` globs are ORed; a bare word (no `/` or glob character) matches names that contain it, and a plain path also matches everything under it. `exclude` (≤100) adds to the default prune: a bare name skips that file or directory at any depth, and `dir/**` skips a path. The default prune covers dependency, build, cache, VCS, and credential directories (`node_modules`, `target`, `dist`, `.git`, `secrets`, …); `localSearch` also prunes editor, CI, and package-manager config (`.github`, `.vscode`, `.config`, …), which the structure and AST tools keep visible. `defaultExcludes: false` turns the prune off; `.gitignore` still hides ignored directories until `noIgnore: true`, and sensitive directories such as `secrets/` are never walked. `hidden` includes dot entries. `maxDepth` is 1–20 levels below `path` (1 = its children).
- **Paging.** Result pages (`page` ≤1000, `pageSize`) in `localSearch`, `structureSearch`, `astSearch`, `astTopology`; per-file match pages (`matchPage`, `matchPageSize`) in `localSearch` and `astSearch` `match`; content windows (`unit`, `offset`, `length`) in `localFetch`. Use whole-response windows only when one result is still too large.

### `localSearch`

Lexical search. Outlines and metadata: `structureSearch`; syntax: `astSearch`; file graphs: `astTopology`.

| Field | Values and meaning |
|---|---|
| `path`, `matchString` | Required. A `ghCloneRepo` `localPath` is valid as-is. With `regex` unset, `matchString` is literal unless it has a regex operator (`\|`, `\`, `[`, `]`, `*`, `+`, `?`, `^`, `$`, `{`, `}`); `warnings` disclose the inferred mode when the readings differ. |
| `regex` | `literal`, `rust` (linear-time), `pcre2` (lookbehind, backreferences, wall-clock limit: the result keeps finished files and reports `capReason:"pcre2Deadline"`). |
| `caseMode` | `smart` (default; follows query casing), `sensitive`, `insensitive`. |
| `wholeWord`, `multiline` | Boolean; `off` (default), `on`, `dotall` (`.` crosses lines). |
| `invertMatch` | Non-matching lines. With `resultView:"files"` it lists files with any non-matching line; for files without the pattern use `filesWithout`. |
| `resultView` | `paginated` (default), `detailed`, `content`, `files`, `filesWithout`, `countLines`, `countMatches`, `matchOnly`. |
| `unique` | `matchOnly` only: `off` (default), `list` (distinct values per file), `count` (frequencies). |
| `contextLines` | 0–100; default 0 (`detailed`: 3). |
| `matchContentLength` | 1–100,000 characters per snippet, clipped around the hit (`matchOnly`: per span); default 200 × (2·contextLines + 1), capped at 4000. |
| `language` | ripgrep type: `ts`, `js`, `py`, `go`, … |
| `include`, `exclude`, `defaultExcludes`, `hidden`, `noIgnore`, `maxDepth` | [Shared local rules](#shared-local-rules). |
| `sort`, `reverse` | `relevance` (default), `traversal`, `matchCount`, `path`, `modified`, `accessed`, `created`; `reverse` applies before pagination. |
| `pageSize` | 1–1000 files; omitted: about 24 KB pages (path and count views: 100 files). |
| `matchPage`, `matchPageSize` | ≤1000; 1–100,000 rows per file; omitted: every row when the result fits one page, else 10 per file on page 1. |
| `snapshot` | ≤200 chars; copy from `next`. |

`relevance`: for one bare identifier, declaring files first; generated files (a `generated` directory, a `.generated.`/`_pb2.`-style name, or a `DO NOT EDIT`/`@generated`/`Automatically generated` header) after every hand-written file; then match count, source paths before test, vendored, and bundled paths, declaration hits before code and comment/string hits, then path. `files`/`filesWithout` order by source paths first, then path. `relevance` and `matchCount` keep the 10,000 highest-ranked files (`capReason:"maxCollectedFiles"`; debug `stats` count all matched files).

Coverage:

- No readable file under `path`: `fileAccessFailed` (exit 5). Some unreadable paths: `isPartial`, `terminalLimit`, `stats.errorCount`/`firstError`; zero matches then prove nothing. Files open without following symlinks; a path replaced by a symlink or special file after the walk is a read error.
- A file binary from its leading bytes (a NUL before any text, or after a short single-line header such as a font or database magic that matched nothing) is skipped, as rg does, without making the result partial; its matching bytes are not hits. Page 1 carries a `binarySkipped` warning (counts by extension) and `hints.binarySkipped`, a `structureSearch` `files` query listing those files (extensionless ones by name; it can also list same-extension text files).
- A file with real text before its first NUL is searched up to that byte: a `binaryFileSkipped` warning names it (`dir/{a,b}` per directory), and the result is `isPartial` (debug: `capped`, `capReason: binaryQuit`; `terminalLimit` when no other continuation exists).
- Values with secret-shaped text replaced by `[REDACTED…]` carry a `redactedMatches` warning; they are not verbatim.

Rows: `path` is relative to `root` (the queried directory, or a queried file's parent). On a page of at most 50 hit rows, a hit inside a declaration carries `enclosing: {symbolName, kind, line, endLine}`, the innermost declaration around it (a hit on a declaration's name line names the declaration around it): `symbolName` + `line` anchor `lspSearch`, and `line`–`endLine` is a `localFetch` range. Consecutive rows in one declaration name it once. A declaration search (`fn foo`, `class Foo`) names owners only on rows that declare the symbol. While pages remain, `stats` holds `matchCount`, `matchedLineCount` (rows a walk shows), and `fileCount`, and `pagination` holds `currentPage`, `matchPage` (past the first match page), `totalPages`, `pageSize`, `totalItems`, and `hasMore`. Size paging and `pagination.moreLines`: [search result shapes](TOOL_DATA_CONTRACT.md#search-result-shapes).

| Key | When and what |
|---|---|
| `next.nextPage`, `next.nextMatchPage` | Without `pageSize`/`matchPageSize`, `nextPage` walks the rest in about 24 KB pages. With either set, `nextPage` moves to the next files and `nextMatchPage` to each shown file's next rows (later match pages list only files with rows left). |
| `next.restart` | Stale snapshot: rerun from page 1. |
| `next.expandValues` | A clipped value. On a `pageSize`/`matchPageSize` page: the same page with a wider `matchContentLength`. On a default page: one `localFetch` of the clipped lines per file (`expandValues2`, …), or for a multiline match the file searched alone with a wider `matchContentLength`. |
| `hints.read` | `localFetch` on the first page of a complete result: a shown declaration of the searched symbol, whole; else ±6 lines around the top file's hits (a `matchString` read past 10 ranges). |
| `hints.callers`, `hints.references` | `lspSearch` when the search names one symbol (an identifier, or one after `fn`/`def`/`class`…) and a shown hit declares it: `callers` for a function or method, else `references`. Omitted without a language server for the file. |
| `hints.includeIgnored` | An empty result where ignored, hidden, or default-excluded paths match, or default-excluded directories (named) could not be checked: the same search with `noIgnore`, `hidden`, `defaultExcludes:false`. |
| `hints.repair` | Invalid regex: the same search with the broken pattern escaped. |
| `hints.clasify` | Wide pages with a multi-word phrase, while `clasify` is available ([OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md)). |

### `astSearch`

Structural search for shapes regex cannot express (an `await` inside a `for` loop, calls with N arguments, functions without `try/catch`). Comments and strings never match.

```bash
astSearch(operation="match", path="src", pattern="track($$$ARGS)")
# rule is a YAML string; \n is a newline in the JSON argument (on the CLI use $'...').
astSearch(operation="match", path="src", rule="rule:\n  pattern: await $C\n  inside:\n    kind: for_statement\n    stopBy: end")
```

| Field | Operations | Values and meaning |
|---|---|---|
| `operation` | all | `match`, `syntaxTree`, `symbols`. |
| `path` | all | File or directory (`syntaxTree`: one source file). |
| `pattern` / `rule` | `match` | Exactly one. `pattern`: a complete ast-grep node with `$VARS` (an omitted trailing `;` or `,` still matches). `rule`: a YAML string, the rule object (the `AstRule` shape of `astRewrite`), or `{rule: …}`; all match alike. |
| `language` | all | Grammar name, alias, or `.ext`; `cpp` parses `.h` as C++ (default C). A directory `match` without it uses the one grammar under `path` that parses the query (`inferredLanguage`; continuations pin it); several candidates return `languageRequired`. `symbols`: single file only. |
| `languageGlobs` | `symbols` | Directory parser map, `{"cpp":["include/**/*.h"]}`; not for clangd. |
| `include`, `exclude`, `defaultExcludes`, `hidden`, `noIgnore`, `maxDepth` | `match` (`exclude`, `defaultExcludes` also `symbols`) | [Shared local rules](#shared-local-rules). `maxDepth: 1` includes root files; depth filtering runs before the scan cap. |
| `resultView` | `match` | `content` (default), `files`, `countMatches`. |
| `captureText` | `match` | Opt-in captures. |
| `matchContentLength` | `match` | 1–100,000, default 500 characters per hit. |
| `sort`, `reverse` | `match` | `path` (default) or `matchCount`. |
| `pageSize` | all | Default 20 files (`match`), 100 nodes (`syntaxTree`), 500 declarations (`symbols`); ≤1000. |
| `matchPage`, `matchPageSize` | `match` | Default 100 matches per file page (≤1000). |
| `maxFiles` | `match`, `symbols` | Default 2000; ≤100,000 (`match`), ≤50,000 (`symbols`). |
| `scanOffset`, `snapshot` | | Copy from `next`. |
| `namedOnly` | `syntaxTree` | Default `true`: skip anonymous nodes. |
| `kinds` | `symbols` | ≤100 declaration kinds (`function`, `method`, `class`, `constant`, `module`, …). |
| `symbolName` | `symbols` | A name or a list (any matches): an entry equal to a declaration name matches only it, else a substring; a directory scan parses only files whose text contains an entry. |

Scans, limits, and errors:

- At `maxFiles`: a `structural.scan.truncated` warning and, on the last page, `next.expandScan` (doubled `maxFiles`, `scanOffset` = files already evaluated, so only later files are listed; pages plus `expandScan` list every file once). `symbols` resumes the same way and reports `terminalLimit` only at the `maxFiles` maximum or after skipped files.
- Parser or matcher exhaustion keeps completed files and reports staged `diagnostics`; zero matches in an incomplete result prove nothing. `inside` reads each ancestor chain once, so deep nesting does not hit the deadline on `stopBy: end`; a file over the deadline is a `structural.match.deadline` diagnostic, never zero matches.
- A directory scan that skips entries withheld by path policy (`secrets/`, `.aws`) says so in `warnings` and is not `complete`.
- `invalidPattern` (invalid or uncompilable query), `languageUnsupported`, `fileTooLarge` (a typed terminal limit, not `executionFailed`); engine `structural.*` names stay diagnostic codes. YAML `kind` rules are checked against the source grammar first: an unknown kind is a typed compile diagnostic, not zero matches. YAML is the rule format, not a source grammar.

Patterns:

- A declaration pattern without a return type or (Rust) visibility also runs as a rule over those spellings; when that finds more, the result is the union with a `structural.pattern.relaxed` warning naming the variants, and continuations replay that rule. Use an explicit `rule` for exact query equivalence.
- Otherwise modifiers are exact: Rust `fn $N()` does not match `pub fn` (a named child) and adds a debug-only `structural.pattern.visibilityExact` info diagnostic; write `pub fn …` or a rule on the item kind.
- Java call patterns can omit the trailing semicolon: the compiler adds grammar-checked statement context, direct or nested in a rule; complete patterns keep their parse, ranges, and captures.
- Matches inside Rust macro invocations and `macro_rules!` bodies are listed (each body is re-parsed in place).

Results:

- One-based lines and UTF-16 columns (span ends exclusive). A `symbols` declaration's `symbolName` + `line`, or an identifier capture's `text` + `line`, is `lspSearch` `symbolName` + `lineHint` as-is.
- `match` rows: `{line, column, endLine?, value}` (whitespace-normalized); per-file `totalMatchRows`/`returnedMatchRows` only when a page is a subset. `captureText:true` returns `{line, column, value}` rows (`endLine`/`endColumn` for multi-line spans) with `metavarRanges`; `next.expandCaptures` offers it when a row hides something it returns (a match cut to its header, or capture text the value does not show). A match page offers `hints.read`: the top file's hits with 3 lines of context, when they fit 5 ranges.
- `syntaxTree` pages one file's tree; `pagination.totalItems` counts nodes.
- `symbols`: a file returns `symbols`; a directory returns `files: [{path, symbols}]`. `snapshot` only on paged results. A JS/TS declaration is `exported` by its local binding. A single named declaration gets `hints.callers` (function or method) or `hints.references` when a language server serves the file.

Symbols outline (entry strings, containers, `<key>=<value>` fields, `shared`, parse rule: [location rows](TOOL_DATA_CONTRACT.md#search-result-shapes)). The YAML text prints `=== symbols <path> (line[-endLine] kind name; + exported; indented = member; doc = comment above) ===`, then:

```text
105-107 struct NamedPipeServer + doc@43
109-891 impl NamedPipeServer
  130-136 function from_raw_handle + doc@110
```

- `line` is the name line (it feeds `lspSearch` at any depth); `endLine` appears when the declaration spans lines. JSON values: `parent="Outer, Inc"`, `exportedAs=["default"]`.
- Fields appear only when informative: `exported: true` (` +`); `exportedAs` for another export name (`export { foo as bar }`; `export default function foo` gives `default`; ` as a,b`); `docStartLine` for a comment block above (JSDoc, `///`, Python `#`; ` doc` when it ends on the line above, else ` doc@N`); `startLine` when attributes or decorators start earlier (` from@N`); `column` (1-based) only when two declarations of one kind share a name and line (` col N`).
- A trait or interface implementation names its trait (`symbolName:"AsyncRead for NamedPipeServer"`, `kind:"impl"`); a `symbolName` filter on the type still matches it, and its `references` lead anchors on the type. Members declared inside a macro body (`cfg_io_util! { … }`) nest under their real container.
- A member whose container is off the page (page break, `kinds`/`symbolName` filter) stays top level with `parent=` (`parentLine=` when two containers share name and kind); the outline indents members two spaces and ends a detached one with `(in Parent@line)`. `pagination.totalItems` and `pageSize` count declarations at every depth.

Structural extensions: `asm`, `assembly`, `c`, `cc`, `cjs`, `cpp`, `cs`, `cts`, `cxx`, `go`, `h`, `hh`, `hpp`, `hxx`, `java`, `js`, `jsx`, `mjs`, `mts`, `py`, `pyi`, `rs`, `s`, `sbt`, `sc`, `scala`, `ts`, `tsx`. The same 28 back signatures and graph facts in the default release build; query the compiled engine capability API when optional grammar features are off.

### `structureSearch`

Directory outlines (`tree`, the default) and file metadata (`files`), with no parser.

| Field | Operations | Values and meaning |
|---|---|---|
| `path` | both | Directory root. |
| `maxDepth`, `minDepth` | both; `files` | 1–20 (1 = children). `tree` omitted: 1, or 20 with `include`. |
| `include` | both | ORed globs (`["*.ts", "*.tsx"]`); with `/` a glob matches the path below `path` ([shared rules](#shared-local-rules)). |
| `entryType`, `extensions` | both | `f` or `d`; extensions without a dot. |
| `exclude`, `defaultExcludes`, `hidden`, `noIgnore` | both | [Shared local rules](#shared-local-rules); `noIgnore` lists `.gitignore`d entries. |
| `pageSize` | both | ≤1000 rows; omitted: about 24 KB pages. |
| `maxEntries` | both | 1–10,000 entries scanned before paging. |
| `scanOffset`, `snapshot` | both | Copy from `next`. |
| `nameRegex` | `files` | Rust regex over the basename. |
| `time.modifiedWithin`, `time.modifiedBefore`, `time.accessedWithin` | `files` | Ages `<n>m\|h\|d\|w` (`7d`). |
| `size.greater`, `size.less` | `files` | `<n>[b\|k\|m\|g\|t]`, KiB-based (`100k`). |
| `empty`, `permissions`, `access` | `files` | Only empty entries; octal (`"644"`); `executable`, `readable`, `writable` (held by the caller). |
| `detail` | `files` | `basic` (default: name, size), `modified` (adds `modifiedMs`, Unix ms), `full` (also `lineCount`). |
| `sort` | `files` | `path` (default: walk order like `git ls-files`, stops at `maxEntries`), or `modified`, `name`, `size`, `lines` (rank the full walk). |

Rows are `{dir, files}` groups with a workspace-relative `dir` (`dir + "/" + name` is a local path); entry and cut fields (`truncated`, `atLeast`, `totalAvailable`): [search result shapes](TOOL_DATA_CONTRACT.md#search-result-shapes). `tree` marks symlinks with `@` and lists `path`'s own group first, then subdirectories in walk order. A bare `"name/"` entry is left out when that directory has its own group on any page; every other entry appears once. Later pages reuse the first page's walk while the listed entries are unchanged. `summary` counts what was left out: `.gitignore`d entries, policy-withheld entries (the warning counts only entries the filters could match), default-excluded directories (named, such as `build, node_modules`), and, without `hidden`, dot entries (pruned directories such as `.git` excluded).

- A listing cut at `maxEntries` offers `next.expandScan` on its last page (doubled `maxEntries`, `scanOffset` = rows already listed); pages plus `expandScan` list every entry once.
- `files` page 1 offers `hints.read`: the `minify:"symbols"` outline of the first source or doc file.
- An empty or filtered `tree` that skipped dot entries offers `hints.includeHidden`; an empty result that `.gitignore` or the default prune can hide offers `hints.includeIgnored` (`noIgnore:true` and/or `defaultExcludes:false`).
- A missing `path`: `pathNotFound` with `hints.viewTree` (nearest existing parent). A file as `path`: `notADirectory` with `next.read`, its `minify:"symbols"` outline.

### `localFetch`

Reads a known local path; a path-only read returns exact source after redaction. `ranges` (`["a-b"]`, one-based inclusive, 1–10), `matchString`, and `fullContent:true` are mutually exclusive. Reader rules: [`ghGetFileContent`](#ghgetfilecontent).

| Field | Meaning |
| --- | --- |
| `unit`, `offset` | `lines` (default) or UTF-8 `bytes` of the selected view; zero-based offset (outline lines with `minify:"symbols"`; byte offsets on code-point boundaries), default 0. |
| `length` | 1–50,000. Omitted, a line page fills the 16 KiB budget (the continuation carries the line count used); a byte page is 16384 bytes. A first read of a 2,000+ line file with no selector (`matchString`, `ranges`, `block`, `offset`, `unit`, `length`, a `minify` view, or `fullContent`) returns its first 50 lines with a hint; `next.continue` pages on at the default size, and `fullContent: true` reads it whole. |
| `matchString` | Nonempty source text, or a list matching any entry; `regex` and `caseMode` as in `ghGetFileContent`. A multiline match keeps every line it touches, in original byte coordinates. |
| `contextLines` | 0–100, default 10 per side; exclusive with `contextBytes`. |
| `contextBytes` | 0–16384 per side (default 256 for byte pages); requires `matchString`. Full-source redaction precedes byte matching; edges expand to whole code points; disjoint windows are separated by `... [N bytes omitted] ...`. |
| `minify` | `none` (default), `standard` (compact), `symbols` (whole-file outline). Match views keep source text. |
| `block` | Widens a range or match window to its enclosing declaration (≤400 lines). With `matchString`, hits on a declaration head widen to that declaration and other hits keep their window; when no hit is a head, each hit widens. A range, or a match on a declaration's first line, also takes the doc comments and attributes above. JS/TS functions assigned to a member (`res.redirect = function () {}`, `exports.x = () => {}`) are declarations. A window no declaration encloses keeps its lines with a `block:` warning. |
| `fullContent` | Complete unpaged view within resource and security limits; no window controls. |
| `snapshot` | From a continuation; a file changed since is rejected, not mixed. |

- Paging: a source over 10 MiB is read in line windows without loading it whole (several `ranges` in one read with not-requested markers; `unit:"lines"` continues to the end through `next.continue`). Line pages keep whole lines within 16384 bytes. An offset at or past the end returns empty content with an offset-zero `next.restart` (debug: `pagination.outOfRange:true`). Continuations stop at the selected range or matched view.
- Fields: `sourceBytes`, `returnedBytes`, and `returnedLines` are debug-only; a partial page reports `returnedChars` (UTF-16 units). Content and the `symbols` outline are numbered `cat -n` style ([numbered source content](TOOL_DATA_CONTRACT.md#numbered-source-content)).
- A context window that stops inside its declaration (not with `block`, `contextLines:0`, or `contextBytes`) shows the rest inline when that costs no more than a lead (about 200 bytes); otherwise `hints.readBlock` reads the rest of the top hit's declaration (also for a `ranges` read that stops inside one).
- A `matchString` that selects no line: an empty row with one tip and leads `textSearch` (a `localSearch` of the file's directory) and, after a case-sensitive match, `ignoreCase` (`caseMode:"insensitive"`). A missing file: `pathNotFound` (exit 3).
- Redaction: private-key blocks are redacted across the whole file before selection, so a key split by a page or range boundary never leaks. A line page is scanned with 8 KiB of surrounding lines, so a multi-line secret at the edge still matches whole (one page scan per call). A byte page is scanned on its whole lines plus at least 8 KiB of surrounding whole lines (single-line secrets always whole); a redacted line cut by the page end is returned whole, so `next.continue` never splits a secret. Byte offsets stay in unredacted coordinates. `fullContent` is scanned whole; above the scanner's 10,000,000-byte limit, `contentSecurityLimit` offers a smaller source range when possible, else a terminal limit. A full-content view over 50,000 bytes supplies bounded executable recovery. Totals unavailable due to access or resource limits are reported as unavailable.

### `astTopology`

CLI-only beta ([availability](#internal-external-and-hybrid-tools)); without the gate the CLI explains it. One bounded graph gives seven analyses of file topology and candidate reachability. An edge proves that one file syntactically imports or re-exports another, not which binding is used; prove symbol identity with `lspSearch` (`references`, `callers`, `callees`).

| Field | Meaning |
|---|---|
| `operation` | Required: `dependencies` (files the source imports or re-exports), `dependents` (files that import or re-export it), `path` (fewest-edge directed path), `reachability` (from entrypoints), `cycles` (strongly connected components), `deadCode` (unreachable or unretained exports, dead SCCs), `drift` (graph change from `baseline` to `path`). |
| `path` | Absolute scan root. Required for `cycles` and `drift`; else it can be implied by an absolute `source`, `target`, or entrypoint. |
| `baseline` | Absolute baseline root; required for `drift` (`path` is the head). |
| `source`, `target` | Start file, relative to `path` (`dependencies`, `dependents`, `path`); destination (`path`). |
| `depth` | `dependencies`/`dependents` hops, 1–50, default 1. |
| `entrypoints` | ≤100 repository-relative roots for `reachability` and `deadCode`; omitted: `package.json` `main`, `exports`, and `bin`. |
| `includeTests` | Default `true` (tests are roots). `false` drops test roots; imported test modules stay in the graph. |
| `exclude`, `defaultExcludes` | [Shared local rules](#shared-local-rules). |
| `languageGlobs` | Root-relative parser map, `{"cpp":["include/**/*.h"]}`. Not for clangd: use compile commands or `.clangd`. |
| `rustWorkspace` | `syntax` (default) or `cargo`; see Resolution. |
| `maxFiles` | 1–50,000 files; the scan stops and warns past it. |
| `supersedes` | Continuation-only (copy `next.expandScan`). |
| `page`, `pageSize` | Page ≤1000; 1–100 rows, default 50. |
| `diagnosticPage`, `diagnosticPageSize`, `diagnosticSnapshot` | Coverage-diagnostic pages (1–100, default 25), independent of `page`. |

Scope and cuts ([`expandScan` meanings](TOOL_DATA_CONTRACT.md#search-result-shapes)):

- Without an explicit `maxFiles`, a root with more than 5,000 parseable files is refused before parsing (`scopeTooBroad`); the message lists admissible package directories, and the row offers `hints.narrowScope` (the largest) and `next.expandScan` (explicit `maxFiles`).
- A graph cut at `maxFiles` is `isPartial` with `partialReasons:["maxFiles"]`; its last page offers `next.expandScan` (doubled `maxFiles`, `page:1`, `supersedes` = the cut `maxFiles`). The widened scan restarts over a superset of files; its page 1 carries `supersedes: N` and a warning: discard every row of the `maxFiles:N` scan. Later pages never carry `supersedes`, and diagnostic pages offer no `expandScan`. A cut `drift` (two graphs at the same `maxFiles`) does the same.
- A cut that more files cannot lift is `terminalLimit:true` with no `expandScan`: the 2,000,000-edge cap (`partialReasons:["edgeCap"]` and a warning; narrow `path` or add `exclude`), skipped files (`filesSkipped`), or `maxFiles` at its maximum. A cut result is never complete.

Coverage: `coverage` reports language coverage; resolved, external, and non-code (`imports.nonCode`: JSON, styles, assets) import counts; unresolved internal imports; unsupported linking; and parse-recovery diagnostics. Gaps lower `confidence` and set `completeness.graph: "coverage-incomplete"`, never `truncated` or `terminalLimit` (scope cuts only). A result with unresolved internal imports is never complete, so an empty dependency result under a subdirectory root is not absence. By default `coverage` holds counts (`coverage.diagnosticCounts` covers the full scan); `hints.readDiagnostics` (`diagnosticPage:1`) returns the rows without results, grouped by code and message with `files` as `path[:line]`, and `next.nextDiagnosticPage` continues. Changed diagnostics: follow `next.restartDiagnostics`.

Resolution:

- `edgeKinds`: `static-import`, `type-import`, `dynamic-import`, `named-reexport`, `star-reexport`, `type-named-reexport`, `type-star-reexport`, `commonjs-require`, `create-require`, `python-import`, `go-import`, `java-import`, `java-same-package` (no import, so no `importLine`), `rust-module`, `rust-use`, `c-include`. Runtime import candidates: `static-import`, `dynamic-import`, `named-reexport`, `star-reexport`, `commonjs-require`, `create-require`, `python-import`.
- Linked: JS/TS ESM and binding-safe CommonJS (literal loads through an unshadowed, unreassigned `require`, `module.require`, or imported `createRequire(import.meta.url)`), Rust modules, bounded Python absolute and relative imports, quoted relative C/C++ includes. Diagnostics: dynamic and ambiguous loaders, Python wildcard and ambiguous package-attribute imports, C/C++ system and macro includes. Data, style, and asset imports (including `package.json`) count as `imports.nonCode`.
- A root below its package (`packages/app/src`) still reads the nearest `package.json` above it, up to a `.git` boundary, so `#` subpath imports and package exports resolve.
- `dependents` also lists users of the target's items through a re-exporting module (`pub use notify::Notify`, `export { x } from './t'`, `export *`), with `reexportVia`; importers of other items from that module are not listed.
- `rustWorkspace: "syntax"` uses module declarations and literal `#[path]` attributes. `"cargo"` reads target roots and dependency aliases with the host Cargo (offline metadata, no compile, 5 s budget, 32 MiB output bound); include the Cargo manifest in the scan root. Missing tools, excluded targets, conditional dependencies, cfg, and macro expansion stay coverage gaps. Both modes return syntactic edges.

Results: paged; SCC and dead-cluster rows list member `files`. Dependency rows carry `file`, `importLine`, `edgeKinds`, `distance`, and `via` (hoisted to `shared` when all agree). `path` returns `files` and `edges` (each with `edgeKinds`, `importLine`), or `found:false`; edges are unweighted, so this is breadth-first search. Debug adds `immediateDominator`, `topologicalLayer`, `inboundCount`, `transitiveEdge`, condensation counts, `summary.importResolution`, and `coverage.languages`. `call` and `contains` facts are not projected.

| Signal | Meaning | Follow-up |
|---|---|---|
| `cycleEdges` | Deterministic directed witness through one SCC (`from`, `to`, `edgeKinds`). | Read each edge; member order is not a cycle path. |
| `runtimeCycleEdges`, `runtimeCycle` | Witness over runtime import candidates only. | Confirm bindings and initialization before you claim a runtime defect. |
| Topology-only SCC | No cycle among runtime candidates (type-only, Rust module cycles). | Coupling evidence, not a loading cycle. |
| `transitiveEdge: true` (debug) | Condensation-DAG edge that another path also covers. | Check re-exports, side effects, public API, and symbol use before calling it redundant. |
| `immediateDominator` (debug) | The file every route from the root crosses. | A chokepoint; not symbol ownership. |

`deadCode` rows and dead clusters (mutually importing unreachable files) are candidates; confirm with `lspSearch` before removal. In a reachable file an export stays live when an import or re-export chain consumes one of its public names (`import foo from` consumes `default`), or when same-file call and containment edges reach it from a live declaration, a module-level call, or a value escape (a syntax-aware reference other than the declaration, an export clause, or a call target; never comments or strings). JS/TS counts resolved references of the declaration's own symbol; other languages count identifier tokens by name. `viaHeuristic` names the basis: `reexport-chain`, `semantic-references` (JS/TS), `syntax-references`, or `qualified-path-name` (a Rust export kept live only by an unresolved `module::name` call; a qualified call that resolves through the caller's `use`/`mod` binding credits the export exactly). Callers are keyed by declaration identity (a method `run` and a function `run` do not share liveness), and an uncalled private caller keeps nothing live. Exports renamed at the export site carry `exportedAs`; namespace imports retain all target exports. Declaration IDs identify scoped occurrences; unresolved call references do not prove identity, and value-reference counts are conservative. A file whose extraction hit its deadline keeps its gathered facts and carries `graph.traversal.deadlineExceeded`.

`octocode graph ingest|query` ([OCTOCODE_CLI.md](../packages/octocode/docs/OCTOCODE_CLI.md)) uses the same builder with persisted snapshots, symbol-level callers/callees/impact, and issue detectors. Its `deps`/`dependents`/`path`/`cycles` file sets match `astTopology` on the same tree, except `reexportVia` rows and Rust `mod` declarations (containment in `graph`).

### `astRewrite`

CLI-only beta ([availability](#internal-external-and-hybrid-tools)). Preview is the default and applies nothing, but it can recover an interrupted transaction. Applies are serialized, snapshot-bound, and hash-guarded, with journal recovery; cross-file changes are not simultaneously visible. Structural matching skips text in comments and strings.

```bash
octocode astRewrite '{"queries":[{"path":"/ABS/repo/src","pattern":"console.log($A)","rewrite":"logger.info($A)"}]}'
```

| Field | Meaning |
| --- | --- |
| `path` | Source file or directory; relative to `WORKSPACE_ROOT`. |
| `language` | Omitted: the file extension, or the one grammar under a directory that compiles the rule (`cpp` includes `.h`); several return `languageRequired`. `hints.apply` pins the resolved value. |
| `pattern` + `rewrite` | Match and replacement template. |
| `rule` + `fix`, `constraints`, `utils`, `transform` | A YAML string (bare or a rule file), the rule object, or `{rule, constraints?, utils?, transform?}`; every shape previews identically. A rule file's `constraints`/`utils`/`transform` join the fields of those names; one given twice is an error. `fix` is its own field. |
| `include`, `exclude`, `defaultExcludes` | ≤100 globs beneath `path`; `exclude` applies after `include`. |
| `maxFiles`, `maxMatches` | Default 2,000 files (≤50,000), 10,000 matches (≤100,000); a cap reports partial state. |
| `page`, `pageSize`, `snapshot` | Preview pages (`pageSize` default 100, ≤1000); copy continuations with their SHA-256 snapshot. |
| `apply` | Default `false`. Needs the unchanged preview snapshot and non-empty `expectedHashes`. |
| `expectedHashes` | SHA-256 per selected file, keyed by workspace-relative (or absolute) path. A key from an older preview fails closed and says to re-run the preview. With explicit match selection, omit unselected files. A stale or missing selected-file hash aborts the apply. |
| `selectedMatchIds` | 1–100 ids; any unique prefix of at least 12 hex digits. |
| `postconditions` | 1–10 `{kind:"remainingMatches", equals:n}` (0–100,000), checked in the staged rewritten files before commit. |

- Preview pages list only the files their matches touch; a file's `patch` holds only that page's hunks (`patchMatchCount` of `matchCount` when it spans pages), while `beforeHash` and the final page's `hints.apply` cover the whole file. A complete preview states each hash once, in `hints.apply.query.expectedHashes`, and its file rows omit `beforeHash`. Match rows are `{id, path, line}` with a 16-hex id prefix; `debug:true` restores full rows, `afterHash`, and `patchBytes`. Paths, patch headers, and hash keys are workspace-relative (absolute outside it). `range.start`/`range.end` are one-based lines and UTF-16 columns (an emoji counts 2), end exclusive; `range.byteOffset` is the UTF-8 byte span.
- `hints.apply` writes files: its `why` starts with "Writes", it is never a `next` page, and replay tools skip it. After an applied `pattern` edit, `hints.verify` is a read-only `astSearch` `match` of the old pattern (0 matches confirms). Apply returns the full selected-match receipt in one page (`debug` adds only the executable and isolation receipts). A commit can carry cleanup warnings, and an error can report incomplete recovery; read them before a retry. Check `schema astRewrite --view variants` first; a preview does not verify applied behavior.

## LSP tools reference

`lspSearch` is the one semantic tool (operations below). It is a local tool ([shared rules](#shared-local-rules)) and needs a file on disk. For a remote repository, use a `ghCloneRepo` `location.localPath`. Anchor from `localSearch` or `astSearch`, confirmed by `localFetch` ([local research](OCTOCODE_WORKFLOWS.md#local-research)).

### `lspSearch`

<!-- tool: lspSearch -->
```json
{"path": "/workspace/src/run.ts", "operation": "references", "symbolName": "isOctokitDeprecation", "lineHint": 27, "groupByFile": true}
{"operation": "workspaceSymbol", "symbolName": "ToolConfig", "workspaceRoot": "/workspace"}
```

| Field | Values | Meaning |
|---|---|---|
| `path` | file path or URI | Required except for `workspaceSymbol`, which needs `path` or `workspaceRoot`; `path` selects the language server. |
| `operation` | next table | Anchored requests default to `definition`; other requests need it. Keep it in durable examples and continuations. |
| `symbolName`, `lineHint` | string; ≥1 | Exact symbol text on the observed 1-based line (name-anchored operations). `workspaceSymbol` takes `symbolName` as a fuzzy query. |
| `orderHint` | 0–100,000, default 0 | Picks among repeats of the name on that line. |
| `workspaceRoot` | path | Overrides root detection; in mixed-language workspaces also pass `path`. |
| `rustContext` | object | Needs a `.rs` `path`, also for `workspaceSymbol`. See [Rust build context](#rust-build-context). |
| `contextLines`, `groupByFile` | 0–100; boolean | `definition`, `references`, `typeDefinition`, `implementation`: source lines around each location (keep `0` unless you need previews); per-file rollups. |
| `includeDeclaration` | default `true` | `references` only; turn it off before unused analysis. |
| `depth` | 0–20, default 1 | Call and type hierarchy depth; 1 = direct edges. |
| `page`, `pageSize` | ≤1000; 1–100 | Default 40 locations or 100 symbols. Copy `page` only from `next.nextPage`. |
| `snapshot` | ≤128 chars | From `next.nextPage` or `next.nextImporterPage`; omit on page 1, required later. |
| `importerPage` | 1–1000 | TS/JS `references` and `callers` importer window; copy only from `next.nextImporterPage`. |

| `operation` | Use | Output (row shapes: [location rows](TOOL_DATA_CONTRACT.md#search-result-shapes)) |
|---|---|---|
| `definition` | Usage or import to declaration; unresolved provider locations kept unchanged. | `payload.kind="definition"`, `matches[]`. |
| `references` | Uses of a function, type, variable, constant, class. | `files[]` of `{path, matches}`; `value` is the first line, a declaration row has `declaration: true`, a recovered row has `source`. `groupByFile:true` gives `{path, matchCount, lines}`; `groupByFile:false` or `contextLines` gives per-location `matches[]`. Total: `matchCount`. `hints.read` reads the sites. |
| `callers` | Static incoming calls. | `files[]` of the calling declarations (`symbolName` + `line` is the next anchor) with their call `sites` in that file; `detail` is a container name, or a signature only when listed callers share a name. `hints.read` reads the sites. Paged. |
| `callees` | Static outgoing calls. | Rows filed under the caller's file (where the sites are); `path` names the callee's file when it differs. Paged. |
| `hover` | Type, signature, docs. | `payload.hover`; `null` is `empty` with `noHover`. |
| `documentSymbols` | File outline. | `symbols[]` in the [`astSearch`](#astsearch) outline format, `unlistedNested` (function locals, counted only), pagination. |
| `typeDefinition`, `implementation` | Declared type; implementations of an interface or abstract symbol (when supported). | `matches[]`. |
| `workspaceSymbol` | Symbols from one server (not merged across servers). A `workspaceRoot`-only query opens one representative source (tsconfig `include` root, then `src/`). | `payload.matches[]` of `{name, kind, containerName?, path, displayRange}`, paged. |
| `supertypes`, `subtypes` | Type hierarchy (recursive with `depth`) when advertised. | `payload.matches[]` of `{name, kind, detail?, path, displayRange, level, via?}`, or `capabilityUnavailable`. |
| `diagnostic` | Pull diagnostics when the server has a pull provider, else the bounded push cache. | Diagnostics, or `empty` (`noDiagnostics`, `diagnosticsNotPublished`). |

Response fields: `resolvedSymbol` (the anchor; `.name` is an optional discovery hint), `summary` (symbol and call-flow totals), `payload`, `pagination`, `rustContext` (normalized settings and fingerprint, when supplied; also on native document-symbol results), `next` (`nextPage`, `nextImporterPage`, `restart`, `retry`, `continueWalk*`), and `hints` (`read`, `textSearch`, …). Debug only: `operation`, `lsp` (availability, `lsp.source`), `meta.evidence`, and `meta.diagnostics` (partial markers stay on the row).

Empty and partial:

- Empty: `payload.kind="empty"`, row `status: "empty"` (CLI exit `1` when every row is empty), `category` `noLocations`, `noHover`, `noDiagnostics`, `diagnosticsNotPublished`, or `unsupportedOperation`. Read the category, not only the exit code.
- A server still loading the project: partial row, `partialReasons: ["languageServerIndexing"]`, a warning, and `next.retry`; zero results then prove nothing. Other empty results are not retried.
- Reference counts cover the server's returned set; `payload.coverage` states the scope. `source` labels recovered rows: `recoveredAlias` (through an aliasing import; counted in `payload.recoveredAliasReferences`), `recoveredImporter` (re-queried from a verified importer's import anchor), `recoveredFromReferences` (a caller). On `importerScanFailed`, or a TypeScript file with no project configuration (`inferredProject`), the row is partial, `payload.coverage.exhaustive` is `false`, and a name-anchored request carries `hints.textSearch`, a `localSearch` for textual uses.
- `pagination.snapshot` fingerprints the canonical query plus the full result set; it validates a recomputed set across processes and keeps no old rows. A changed set, or a later page without its snapshot, fails as `staleSnapshot` (no items, `next.restart`).

Importer pages (TS/JS `references` and `callers`): candidates are files under the search scope that spell the name and that the server's answer does not list, sorted by path; one request verifies one window of 24 (`importerPage`). A window with more after it is partial (`importerScanCapped`, not terminal), and its last location page carries `next.nextImporterPage` (the candidate-list digest as `snapshot`). Walk `next.nextPage` to the end of a window, then `next.nextImporterPage`.

- Each candidate file is verified in one window, and its recovered rows are listed only there, so every row appears once. A row recovered in a non-candidate file stays with the window that found it.
- Importer page 1 lists the server's answer, then the first window; later pages list only their window's rows. A later window with no verified importer is `empty` and still carries `next.nextImporterPage` when windows remain.
- A changed candidate list gives `staleSnapshot` with `next.restart` (importer page 1, location page 1).
- `callers` with `depth` > 1 windows only level-1 callers; subtrees can repeat across importer pages.

Coordinates (one-based lines, UTF-16 columns = LSP `character + 1`): `resolvedSymbol.foundAtLine`/`foundAtCharacter` (only when the symbol sat off `lineHint`), location and hover `displayRange {startLine, startCharacter, endLine}`, hierarchy and workspace-symbol `displayRange`, and call `sites`. Declaration rows add a `column` only when two symbols share a line. A hierarchy node's `displayRange` starts at the name (usable as `lineHint`) and ends with the declaration.

Hierarchy walks: rows are edges. Type items carry `level` (`1` = direct) and, below level 1, `via {name, path, line, character}`; call rows below level 1 carry `via: {symbolName, line}` (plus `path` when another node shares that name and line), so the flat list rebuilds the tree. `callers` rows put sites in the caller's file, `callees` rows in the `via` (or anchor) file. Repeated (parent, node) pairs merge into one edge. The walk is breadth-first; a node (canonical path + selection range) expands once at its shallowest level, and a node reached again keeps its edge without re-expanding. Caps per walk: depth 20, 200 nodes, 50 results per node.

- Node cap: `payload.truncated`, `isPartial`, `partialReasons: ["hierarchyNodeLimit"]`, and `next.continueWalk` on the first parent whose children were dropped, with the remaining depth; other such parents get `next.continueWalk2`…`N`, and `payload.unexpandedParents` lists all (`name`, `path`, `line`/`character`, `remainingDepth`). Each re-anchors by `symbolName`, `lineHint`, and `orderHint` when the name repeats on the line.
- Fan-out cap: `partialReasons: ["hierarchyFanOutLimit"]`, `terminalLimit: true`; use paged `references`.
- A provider failure after some results: `callHierarchyExpansionFailed` / `typeHierarchyExpansionFailed` with `next.retry`. Items outside the allowed roots are omitted with a warning.

Errors: `outsideAllowedRoots`, `pathNotFound`, other path failures `fileAccessFailed`; `invalidInput`; `timeout` and `serverCrashed` (both `retryable: true`); `capabilityUnavailable` (method not implemented); `requestFailed` (`retryable: true` except rejected parameters); `serverUnavailable` (no configured or startable server); `anchorUnresolved` (`symbolName` not near `lineHint`, exit 3). A name-anchored failure offers `hints.read` of the `symbolName` match windows; empty diagnostics offer no read.

### Rust build context

| `rustContext` field | Default in an explicit context | Meaning |
|---|---|---|
| `features` | `[]` | Cargo features, or `"all"`; deduplicated and sorted for identity. |
| `noDefaultFeatures` | `false` | Disable default features. |
| `target` | unset (the server's Cargo environment) | Target triple, 1–256 chars. |
| `cfgs` | `[]` | ≤100 extra cfg settings (`"custom"`, `"mode=fast"`, `"!custom"`). |
| `buildScripts` | `false` | Run build scripts and load their cfg and generated sources. |
| `procMacros` | `false` | Expand procedural macros; requires `buildScripts: true`. |

- `"rustContext": {}` disables build scripts, proc macros, the implicit test cfg, and check-on-save. Omitting `rustContext` keeps the server defaults, which can enable build scripts or proc macros. Enabled providers run workspace code; the context is not a sandbox.
- rust-analyzer supplies cfg-selected definitions, declarative macro expansion, and enabled build-script or proc-macro results; `astSearch` gets no compiler expansion. A disabled provider can explain an empty answer for a declaration a normal Cargo build generates.
- Different effective settings use different pooled clients. `rustContext.fingerprint` identifies the normalized requested settings and is part of pagination, so a changed context restarts pagination. It does not pin sources, Cargo configuration, toolchain, environment, or generated artifacts, and is not a reproducible-build identifier. See the [engine lifecycle contract](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#rust-context) and [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html).

### Root selection

Without `workspaceRoot`, the root is the nearest ancestor of the file with a project marker (`package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, `pyproject.toml`), even inside `WORKSPACE_ROOT`; with no marker, the process cwd.

### Server fidelity and the no-fallback contract

`documentSymbols` falls back to a native outline (`lsp.source`: `native` via oxc for JS/TS, or `markdown` headings; syntax-only) only when no server is available; a server (`lsp`: type-aware, cross-file) always wins. Every other operation needs a real server. Without one, Octocode does not guess: `status:"error"`, `errorCode:"serverUnavailable"`, and a message that points to `localSearch` or `astSearch` + `localFetch`. There is no same-file-only `references` path. See [LSP server lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md).

### TypeScript backends

1. An `lsp-servers.json` entry for the extension ([custom servers](#custom--bring-your-own-servers)); for `tsgo`: `{"command":"tsgo","args":["--lsp","-stdio"],"languageId":"typescript"}`. `tsgo` on `PATH` alone is not preferred.
2. `typescript-language-server` (the default): an executable on `PATH`; else `node_modules/typescript-language-server/lib/cli.mjs` through the current Node, from Octocode's install tree, then the workspace's `node_modules` only when the workspace is trusted, then the start directory. Trusted means `lsp.trustProjectConfig` (`OCTOCODE_TRUST_PROJECT_LSP_CONFIG=true`) or a workspace at or inside the start directory, so a scanned checkout such as a clone cannot supply the executable.

Definition locations stay server output; import targets are never rewritten. LSP reads minified `.js`, but original source gives far better results.

### Language servers

Built-in routes: JavaScript and TypeScript (`typescript-language-server`), Python (`pylsp`), Rust (`rust-analyzer`), Go (`gopls`), Java (`jdtls`), C, C++, and CUDA (`clangd`), C# (`csharp-ls`), Scala (`metals`). Rust and C/C++ support managed downloads; others use installed executables.

Built-in servers start headless and read-only (user arguments and options win): rust-analyzer runs no build scripts, proc macros, or `cargo check`; clangd runs `--background-index=false --clang-tidy=false --log=error --pch-storage=memory`; jdtls keeps `-data` under the Octocode home with `java.autobuild.enabled:false`; Metals starts with `isHttpEnabled:false`. Each server is capped at `maxMemoryMb` (default 4096, `0` disables); on macOS and Linux an RSS watchdog enforces it on the whole server tree ("language server exceeded memory cap").

Lines end at `\r\n`, `\n`, or a lone `\r` (the LSP rule). A location in an allowed file that cannot be read (too large, not UTF-8) is kept, with `content` stating why.

#### Custom / bring-your-own servers

An `lsp-servers.json` entry adds an extension or replaces a built-in server (another `rust-analyzer`, `tsgo`, a pinned `clangd`). Load order: `lsp.configPath` / `$OCTOCODE_LSP_CONFIG`; `<workspace>/.octocode/lsp-servers.json` (only with `lsp.trustProjectConfig`); `~/.octocode/lsp-servers.json`.

<!-- example: configuration -->
```jsonc
{"languageServers": {".php": {"command": "intelephense", "args": ["--stdio"], "languageId": "php"}}}
```

`command` and `languageId` are required; `args` (default `[]`) and `initializationOptions` (sent verbatim in `initialize`) are optional. The server answers what it advertises; an extension with no entry and no built-in route returns `serverUnavailable`. See [`LSP_SERVER_LIFECYCLE.md`](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#custom-configuration).

## Semantic assessment reference

`clasify` takes 1–5 matrices in `queries[]`; each has optional `mainGoal` and `reasoning`, 1–25 `resources` (supplied `{value}` state or one unread `{tool,query}` read), and 1–25 `questions`, and results are keyed by query, resource, and question IDs. Read `octocode schema clasify --view query` before you write a call, and run `next.clasify` unchanged. Flows: LOCATE a described target in a large file, GATE a large fetch with `sufficient`, SCOUT an unread list, JUDGE a supplied `value` only when the label is the deliverable. Question types, limits, outputs, credentials, and handoffs: [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md).

Not for a known identifier or literal (use `localSearch`, `astSearch`, or `lspSearch`; an identifier target returns `hints.textSearch`), latency-sensitive work, or state you can decide yourself. A low score defers a candidate, never discards it; `sufficient` stops screening only when its read shows the deciding line; scores never prove absence.

## Clone and local tools workflow

`ghCloneRepo` ([fields](#ghclonerepo)) brings remote source to the local, AST, and LSP tools: pass `location.localPath` as `path`, and keep the resolved revision and requested scope. Flow: [external to local](OCTOCODE_WORKFLOWS.md#external-to-local). Clones live under `<octocode-home>/tmp/...`, and the validators add the Octocode home as an allowed root, so `location.localPath` is valid for every local and LSP tool outside your workspace. Example sparse path: `<octocode-home>/tmp/clone/microsoft/typescript/main__sp_a3f8c1__host_4dc11541e2f7a1d9` (`kind: tree`).

### Cache behavior

| Behavior | Details |
|---|---|
| Clone TTL | 24 hours by default (`OCTOCODE_CACHE_TTL_MS`). |
| Clone keys | Ref, sparse scope, and host. Sparse clones use `{branch}__sp_{hash}__host_{hash}/` and coexist with a full clone. |
| Identity | Clones accept a branch, tag, or full SHA and return the actual HEAD as `location.commitSha`. File reads resolve an omitted branch; pass a SHA for reproducible reads. |
| Clone hit | Only with a clean working tree at the recorded HEAD; a modified cache returns `checkoutDirty` and keeps its files. Check `verified` before you trust cached bytes. |
| Clone expiry | Clone activity evicts clean expired entries under their locks; modified checkouts are kept; directory-age sweeps never remove clones. |
| Force refresh | `forceRefresh: true` bypasses the cache and re-clones or re-fetches. |
| GitHub responses | Memory and disk caches, freshness, ETag revalidation, limits, the 24-hour sweep, `octocode cache status`/`clear`: [Cache storage and lifecycle](CONFIGURATION.md#cache-storage-and-lifecycle). |
| Response marker | With `debug: true`, a `ghGetFileContent` row whose files all came from its content cache has `cache: 1` (no other tool sets it; same on CLI and MCP). |
| Live tools | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, and `lspSearch` read the workspace directly and cache no results. |
