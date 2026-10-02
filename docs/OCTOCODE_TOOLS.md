# Octocode tools reference

Field-level reference for every tool exposed through MCP and the CLI. Schemas and descriptions are authored in `@octocodeai/octocode-core` and published in-repo through `@octocodeai/config/schema`; execution and native search/minify/security/LSP primitives live in `@octocodeai/octocode-native`. The cross-tool envelope, evidence, and continuation contract lives in [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md); tool selection strategy lives in the [research manifest](OCTOCODE_RESEARCH_MANIFEST.md); MCP setup lives in [OCTOCODE_MCP.md](OCTOCODE_MCP.md) and CLI usage in [OCTOCODE_CLI.md](../packages/octocode/docs/OCTOCODE_CLI.md). For the exact active schema, inspect the live contract; its `variants` and `defaults` preserve mode-specific required fields and defaults:

```bash
npx octocode scheme <toolName> --view query --compact
```

## Tool inventory

| Family | Tools |
|--------|-------|
| GitHub | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` |
| Packages | `artifactSearch` |
| Local | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, `astTopology`, `astRewrite` |
| LSP | `lspSearch` |
| Semantic assessment | `clasify` |

## Contents

- [How every tool call works](#how-every-tool-call-works)
- [Internal, external, and hybrid tools](#internal-external-and-hybrid-tools)
- [Text, AST, graph, and LSP: choose the evidence you need](#text-ast-graph-and-lsp-choose-the-evidence-you-need)
- [GitHub tools reference](#github-tools-reference)
- [Local code tools reference](#local-code-tools-reference)
- [LSP tools reference](#lsp-tools-reference)
- [Semantic assessment reference](#semantic-assessment-reference)
- [Clone and local tools workflow](#clone-and-local-tools-workflow)
- [MCP tool quality and agent workflow](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md)

## How every tool call works

The CLI and MCP server expose the same canonical contracts from `@octocodeai/octocode-core` and execute through `@octocodeai/octocode-native`. Ordinary tools validate a strict `queries[]` envelope, run independent queries with bounded concurrency, and return one row for every input position. `clasify` instead accepts one complete `SemanticQuery` directly or a batch of complete queries; see [Semantic assessment reference](#semantic-assessment-reference). A failure in one row or assessment page does not erase successful siblings.

### Base call envelope

<!-- example: schematic -->
```json
{
  "queries": [
    {
      "goal": "Find the public parser entrypoint",
      "reasoning": "A definition anchor is needed before requesting references"
    }
  ],
  "responseCharLength": 20000,
  "responseCharOffset": 0
}
```

| Field | Scope | Meaning |
| --- | --- | --- |
| `queries` | Required outer field | Array of 1–5 queries for the **same tool**. Queries are independent and response rows retain their zero-based input `index`. Rows of one call run concurrently (`ghCloneRepo` and `astRewrite` rows run in order; `clasify` schedules its own provider work), so batching reduces round trips but does not create dependencies between rows. |
| `goal` | Required per query, at most 500 characters | States what this row must find or decide. Each row states its own; a top-level `goal` is not inherited. `next.*` continuations already carry the producing query's goal. It is not a ranking instruction or proof of correctness. |
| `reasoning` | Required per query, at most 500 characters | States why this row advances its goal; `next.*` continuations carry it too. Do not put secrets, hidden chain-of-thought, or required runtime data here. |
| `debug` | Optional per query | `true` adds metadata (scan stats, provider receipts, snapshots, info diagnostics). Default output is minimal. |
| `responseCharLength` | Optional outer field | Limits the rendered whole-response text window to 1–50,000 characters. It does not replace a tool's own result pagination. When omitted, responses larger than `output.pagination.defaultCharLength` (default 50,000) are paged automatically; follow `responsePagination.next`. |
| `responseCharOffset` | Optional outer field | Continues a whole-response text window. Copy the returned executable `responsePagination.next` call instead of constructing an offset by hand. |
| `responseSnapshot`, `responseScope` | Optional outer fields | `responseSnapshot` is copied from `responsePagination.next`. `responseScope` selects what an explicit window pages: `text` (default), `structured` (the serialized envelope), or `rows` (whole result rows). |

Every new query requires its own `goal` and `reasoning`; a missing brief rejects only that row. All other fields belong to a specific tool variant. Fields from different `operation` branches cannot be mixed, selector pairs such as `startLine`/`endLine` must be complete, and mutually exclusive selectors must not be combined. `clasify` sends `reasoning` and `goal` in each page's evidence state, and sends `goal` again with every question.

### Schema discovery, variants, defaults, and hints

```bash
# Catalog: canonical names, availability, and agent instructions
npx octocode scheme

# Branch names, required fields, and minimal examples
npx octocode scheme localSearch --view variants

# Self-contained query schema, optionally isolated to one variant
npx octocode scheme ghGetHistoryItem --view query --select variant=pullRequest

# Full contract: variants, examples, defaults, prepare rules, and input schema
npx octocode scheme ghGetHistoryItem --view full
```

The contract lists `variants` (when a branch applies, its required fields, and a minimal example), `defaults` (values the runtime adds for each branch), and `rules` (preparation and validation steps). Runtime `hints` appear only on empty/error results: one concise recovery hint per row, at most 120 characters. It offers recovery guidance and never proves absence or success. Successful results omit advisory next-tool suggestions; executable pagination and completeness recovery calls remain available in `next`.

### Results, evidence, partial failures, and continuations

A batched response preserves input order:

```yaml
results:
  - index: 0
    data: { ... }
  - index: 1
    status: error
    data:
      error: "..."
```

Row fields are `index`, optional `status`, `data`, and, with `debug: true`, `meta` and `cache`. `status: empty` means no usable result (unsupported capability, unresolved anchor, or no match — interpret with tool diagnostics); `status: error` means that row failed. Neither is inferable from missing output. When partial, run the returned schema-valid `next.*` object — don't stop at a numeric cursor or treat a bounded first page as complete. Minimal-output rules, text rendering, evidence kinds, pagination layers, and continuation rules are owned by [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md).

CLI exit codes: `0` success, `1` every row empty, `2` invalid input (including any rejected batch row), `3` not found, `4` authentication or permission, `5` execution error, `6` results with a re-runnable `next.*` continuation or partial source read, `7` rate limited, `130` interrupted.

## Internal, external, and hybrid tools

"External" describes the data or provider boundary, not the MCP transport. All sixteen catalog entries use the same MCP and CLI contracts; availability gates can hide or reject an entry on a particular surface.

| Tool | Boundary | How it works |
| --- | --- | --- |
| `ghSearchRepo` | External | Calls GitHub repository search (or the owner listing) to discover repositories. |
| `ghSearchCode` | External | Calls GitHub code search to discover files and snippets. Covers the indexed default branch; read exact bytes afterward. |
| `ghStructure` | External | Calls the GitHub tree API to browse a known repository tree, with optional repository metadata. |
| `ghGetFileContent` | External | Reads a known GitHub file, ref, range, or match. Full reads return content without creating a local checkout. |
| `ghSearchHistory` | External | Searches GitHub pull-request, issue, or commit metadata. It discovers history identities; it does not replace exact history reads. |
| `ghGetHistoryItem` | External | Reads one known pull request, issue, commit, or comparison, with explicit selectors for bodies, comments, files, reviews, commits, and patches. |
| `artifactSearch` | External | Resolves dependency identities or discovers packages by capability across eight ecosystems. Set `type`; registry metadata and upstream links lead to source research. npm retains registry-scoped authentication. |
| `ghCloneRepo` | Hybrid | Uses provider credentials/network access, then atomically materializes a full or sparse repository under managed local storage. CLI-only (MCP does not register it); requires persistent storage. |
| `localSearch` | Internal/local | Runs bounded lexical text/regex search against allowed local paths. |
| `structureSearch` | Internal/local | Outlines directories and finds files by name or metadata under allowed local paths, without parsing. |
| `astSearch` | Internal/local | Finds structural AST matches, declarations, and paginated syntax trees against allowed local paths. |
| `astTopology` | Internal/local | Analyzes syntactic cross-file dependency graphs for dependencies, dependents, paths, cycles, reachability, dead code, and drift. |
| `astRewrite` | CLI only | Opt-in beta feature (`OCTOCODE_BETA=true`); never exposed through MCP. Previews structural ast-grep rewrites and performs serialized, snapshot-bound, hash-guarded applies with journal recovery; inspect the commit or recovery receipt. Cross-file changes are not simultaneously visible. |
| `localFetch` | Internal/local | Reads a known allowed path with full, match, line-range, minified, or symbol-outline views and exact continuations. |
| `lspSearch` | Internal/local with a language-server process | Resolves an anchored symbol and asks a real language server for definitions, references, calls, types, symbols, hierarchy, or diagnostics. It reports unavailable capabilities instead of returning a syntactic approximation as semantic proof. |
| `clasify` | External Jev provider | Executes unread read-tool requests or accepts supplied state, applies Noul, Choice, or Score questions across a resource-question matrix, and returns correlated typed pages without retrieved bodies. |

Remote GitHub tools require provider runtime and credentials ([authentication](AUTHENTICATION.md)). `artifactSearch` uses official registry APIs; `type:"npm"` honors the effective npm registry configuration. Local tools are on unless `ENABLE_LOCAL`/`local.enabled` is `false`; `ghCloneRepo` is CLI-only and requires persistent storage. `astRewrite` and `astTopology` additionally require `OCTOCODE_BETA=true` (or `local.beta: true`), their sole gate (for `astRewrite` it covers preview and apply); `scheme` reports them as `availability.enabled:false` with `envVar:"OCTOCODE_BETA"` until then. LSP availability also depends on a compatible server for the file language. `clasify` requires a resolved classification key (see [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md)); without one, MCP omits it and a CLI call returns an actionable missing-key error. Configuration keys are owned by [CONFIGURATION.md](CONFIGURATION.md).

## Text, AST, graph, and LSP: choose the evidence you need

These surfaces complement one another; they are not interchangeable.

| Surface | Octocode operation | Establishes | Does not establish |
| --- | --- | --- | --- |
| Text/regex | `localSearch` | Exact lexical occurrences, paths, and source-line anchors within the scanned scope. | Symbol identity, reachability, or all runtime uses. |
| Structural AST | `astSearch(operation:"match")` with exactly one of `pattern` or `rule` | Syntax-shaped matches that ignore formatting differences and can expose captures. | That two same-shaped nodes refer to the same symbol or execute at runtime. |
| File graph | `astTopology` | Syntactic import topology, candidate paths/cycles, and reachability under stated roots and exclusions. | Symbol-level identity, dynamic imports that were not resolved, or safe deletion by itself. |
| LSP semantics | `lspSearch` | Language-server identity and relations such as definitions, references, callers, callees, implementations, types, symbols, and diagnostics. | Runtime behavior outside the server's configured project/build context. |

Recommended proof ladder:

1. Orient with `structureSearch(operation:"tree"|"files")`.
2. Find a lexical anchor with `localSearch`.
3. Use structural search when syntax shape matters or text is noisy.
4. Use `astTopology` to map file-level blast radius or candidate reachability.
5. Read exact source with `localFetch`.
6. Use `lspSearch` from a real file/line/symbol anchor to prove identity and usages.
7. Run the relevant test, build, or runtime path before claiming behavior.

Example:

<!-- tool: astSearch -->
```json
{
  "queries": [
    {
      "operation": "match",
      "path": ".",
      "langType": "typescript",
      "pattern": "defineTool({ $$$FIELDS })",
      "goal": "Find tool contract declarations",
      "reasoning": "AST shape avoids unrelated prose matches"
    }
  ]
}
```

Use the returned file and line as an exact-read/LSP anchor. For graph results, preserve `entrypoints`, `includeTests`, exclusions, scan caps, diagnostics, and `rustWorkspace`; changing any of them changes what "reachable" means. Choosing among these surfaces for a task is covered by the [research manifest](OCTOCODE_RESEARCH_MANIFEST.md).

---

## GitHub tools reference

Concise reference for Octocode MCP remote research tools: GitHub code/repo/PR search, GitHub content access, cloning, and package registry lookup/discovery.

### GitHub tool configuration

Token variables, their precedence, `GITHUB_API_URL` (GitHub Enterprise), OAuth login, and token refresh are owned by [AUTHENTICATION.md](AUTHENTICATION.md).

Every tool accepts bulk input (`{ "queries": [...] }`), up to 5 queries per call. Page-based tools use `page` and `pageSize`. When more results remain, run the matching schema-valid `next.*` call: `nextPage` or a content continuation (`continue`, `continuePatch`, `nextCharOffset`, and similar). At an unexpandable public or provider cap, metadata reports `terminalLimitReached` and omits unusable continuations. Numeric page, offset, cursor, and raw `nextQuery` fields are not executable by themselves. `matchString` selects all matching slices; file chunks page that selected view without changing the selector. `ghCloneRepo` is atomic and does not paginate its input. Use `npx octocode scheme <toolName> --view query --compact` for the exact active schema and operation scopes.

Search match values and provider text snippets are evidence previews, not collection pagination; read exact bytes before quoting them.

### Choose a GitHub tool

| Need | Tool |
|------|------|
| Search code across GitHub | `ghSearchCode` |
| Read a known file | `ghGetFileContent` (browse directories with `ghStructure`; bring a repo to disk with `ghCloneRepo`) |
| Browse a repository tree | `ghStructure` |
| Discover repositories | `ghSearchRepo` |
| Search PRs, issues, or commits | `ghSearchHistory` with `operation: "pullRequest"`, `"issue"`, or `"commit"` |
| Inspect one PR, issue, commit, or ref comparison | `ghGetHistoryItem` with `operation: "pullRequest"`, `"issue"`, `"commit"`, or `"compare"` |
| Materialize a repo/subtree locally | `ghCloneRepo` |
| Resolve package identity or find packages by capability | `artifactSearch` |

GitHub discovery is split into three tools with no `operation` field.
`ghGetFileContent` stays separate because it reads and minifies known content
rather than discovering it.

### `ghSearchRepo`

Discover repositories by keywords, topics, owner, and metadata filters.

<!-- tool: ghSearchRepo -->
```json
{"goal": "Show a documented ghSearchRepo result.", "reasoning": "Use ghSearchRepo for this documented evidence request.", "keywords": ["code research"], "language": "TypeScript"}
```

Fields: `keywords`, `topics`, `language`, `owner`, `stars`, `license`,
`archived`, `match` (array of `name`, `description`, `readme`), `qualifiers`,
`sort`, `page`, `pageSize` (1-100), and `concise`. `qualifiers` carries the
rarer GitHub filters as one space-separated string (`forks:>50 size:<5000
created:>2023-01 pushed:>2025-01 good-first-issues:>2 is:public`; keys are
allowlisted, no `repo:`/`org:`/`user:`). The older `forks`, `goodFirstIssues`,
`updated`, `created`, `size`, and `visibility` fields stay valid but are not
advertised to MCP hosts.

Rows are compact: `repo` (`owner/name`), `stars`, `language`, `license`,
`pushedAt`, `description` (≤160 chars), and at most 5 `topics` (query matches
first) plus `topicCount` when more exist. `debug:true` adds `forks`,
`createdAt`, and `updatedAt`. `pagination` holds only `totalMatches`/`hasMore`;
the page cursor is `next.nextPage`.

`ghSearchRepo` with only `owner` (optionally `sort:"updated"`) reads the REST
owner listing ordered by latest push (`order: "pushed"`) and excludes archived
repositories. One call reads up to 5 provider pages to fill a page, so it can
return more than `pageSize` rows. `next.nextPage.query.page` is a provider page
cursor that follows GitHub's `Link` header. The listing reports no
`totalMatches`.

### `ghSearchCode`

Search indexed default-branch code within one owner (required) and optional repo.

<!-- tool: ghSearchCode -->
```json
{"goal": "Show a documented ghSearchCode result.", "reasoning": "Use ghSearchCode for this documented evidence request.", "keywords": ["useReducer"], "owner": "vercel", "repo": "next.js"}
```

Fields: `keywords`, required `owner`, `repo`, `path` (prefix), `extension`,
`filename`, `language`, `match` (`"file"` or `"path"`), `page`, `pageSize`
(1-100, default 30), and `concise`. `match` defaults to `"file"`; use
`match:"path"` for path-only discovery. `next.readTopMatch` routes to
`ghGetFileContent` pinned to the commit the hit was verified at; a file absent
at a requested `branch` offers no read. A page GitHub marks incomplete is
`isPartial` with `partialReasons:["providerIncompleteResults"]`, empty or not. A repository that GitHub reports as renamed returns
`next.retryRenamed` against the new name.

Keywords (also for `ghSearchRepo`) are literal ANDed terms. Each one is sent as
a bare word or as a single quoted phrase. Interior double quotes and backslashes
are dropped, so a keyword such as `"hello" NOT` becomes the phrase `"hello NOT"`
and cannot negate or replace the `repo:` scope. `owner` and `repo` must be
GitHub names. A value with spaces, quotes, colons, or operators is a validation
error.

### `ghStructure`

Browse a known repository tree. `owner` and `repo` are required; `path` is a
directory (`""` or `"."` for the root).

<!-- tool: ghStructure -->
```json
{"goal": "Show a documented ghStructure result.", "reasoning": "Use ghStructure for this documented evidence request.", "owner": "vercel", "repo": "next.js", "path": "packages", "maxDepth": 2}
```

Fields: `owner`, `repo`, `path`, `branch`, `maxDepth` (1-20), `pattern`,
`page`, `pageSize` (1-500, default 300), `include` (`sizes`, `languages`,
`contributors`, `branches`, `tags`), `metadataPage`, `materialize`, and
`materializeOffset`.
`pattern` finds paths by name at any ref in one call: a case-insensitive glob
over repo-relative paths (`**/_exception_handler.py`, `src/**/*.ts`); without
a `/` it matches entry names, and a bare word matches names containing it.
With `pattern` and no `maxDepth`, every level is searched. Rows keep the
`dir`/`files`/`folders` shape and `summary.pattern` echoes the filter.
For cross-file grep at a ref over MCP (where `ghCloneRepo` is unavailable),
combine `pattern` with `materialize:true` (≤50 files, ≤300 KiB each), then run
`localSearch` at the returned `location.localPath`.
A `branch` that does not exist is an error; it never falls back to the default
branch. A missing `path` is a not-found error whose `next.viewTree` lists the
nearest existing directory (case-corrected).
For exact field types and branch rules, inspect `scheme ghStructure --view query`.

### `ghGetFileContent`

Read one GitHub file. For directories use `ghStructure`; for local analysis use `ghCloneRepo`.

Key fields:

| Field | Meaning |
|-------|---------|
| `owner`, `repo`, `path` | Required repository and path. |
| `branch` | Branch, tag, or commit SHA. Omit to use default branch. |
| `fullContent` | Read the whole file. Use only for small files. |
| `startLine`, `endLine` | Read a line range. |
| `matchString` | Return matching slices. |
| `contextLines` / `contextBytes` | Line or byte context around `matchString` (mutually exclusive; `contextBytes` requires `matchString`). |
| `matchStringIsRegex`, `matchStringCaseSensitive` | Match behavior. |
| `chunkType`, `offset`, `chunkSize` | Same line/UTF-8 byte pagination as `localFetch`; follow `next.continue`. |
| `minify` | `standard` (lossy, language-dependent compression), `none` (no minification), or `symbols` (structural outline). Defaults to `none`, exactly as `localFetch`. Security redaction still applies. |
| `forceRefresh` | Bypass the content cache. |

Choose one extraction intent: whole file, line range, matching slices, or symbol outline. Both readers reject symbol outlines combined with match or line selectors. Selection precedes minification, redaction, and pagination. `chunkType` defaults to `lines` with `chunkSize:2000` (the 16384-byte page budget usually ends the page first); `bytes` defaults to 16384 UTF-8 bytes. Offsets are zero-based in the selected view. Line pages have a 16384-byte budget and oversized lines switch to bytes. Byte ends may extend by up to three bytes to finish a code point.

`fullContent:true` requests an unpaged view and rejects chunk controls. A view over 50000 bytes (or a source over 100 KB) returns its first bounded line page inline, `partialReasons:["full-content-size-limit"]`, and `next.continue` (pinned to the resolved commit SHA) for the rest. Each read returns `data.files[]` with `commitSha`. A range remains bounded by its original `endLine`; continuing a match preserves its pattern and source-line context. `totalLines` (and, with `debug: true`, `sourceBytes`) describe the original file; `pagination.totalLines`/`totalBytes` describe the complete selected view. `matchedLines` contains source anchors on the current page; with `debug: true`, `selectedMatchCount` counts all selected matching lines and a first-page read adds `lastModified`/`lastModifiedBy` from the last commit touching the path (one more request, sent only after the content read succeeds). `minifyFallback` explains when match evidence or unavailable outlines prevent the requested transform.

File reads return content without creating a checkout. Use `ghStructure` to browse directories or `ghCloneRepo` with `sparsePath` to create a local subtree.

Examples:

<!-- tool: ghGetFileContent -->
```json
{"goal": "Show a documented ghGetFileContent result.", "reasoning": "Use ghGetFileContent for this documented evidence request.", "owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/config.ts", "matchString": "export", "contextLines": 2, "chunkType": "lines", "chunkSize": 20}
```

Cost by mode:

| Mode | What you get | Approx tokens |
|------|-------------|---------------|
| `matchString` | Every matching slice, plus context | ~50-300 |
| `startLine`/`endLine` (small) | Exact line range | ~100-500 |
| `minify: "symbols"` | Imports and signatures, bodies stripped | 5-20% of the full file |
| `startLine`/`endLine` (large range) | One bounded page of the range, plus `next.continue` | 1k-10k |
| `fullContent` | Entire file; defaults to no minification | Can exceed 50k |

Behaviors worth knowing:

- `matchString` selects all occurrences with context and source anchors in `matchedLines`/`matchRanges`; both readers disable minification for matches (redaction still applies). Non-adjacent windows are separated by a `... [lines A-B omitted] ...` line (byte windows: `... [N bytes omitted] ...`), so a gap never reads as contiguous source. Follow character continuations when selected content exceeds a window.
- `minify: "symbols"` returns a paginated outline; read its source-line gutter, then follow up with `startLine`/`endLine` and `minify:"none"`.
- Continuation offsets are exact — execute `next` unchanged rather than recomputing them.
- `standard` removes comments and rewrites formatting but does no JS/TS optimization or type-declaration removal; use `none` for source quotes and comment-sensitive evidence. See [minification coverage](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md#minification--file-reads-and-search-fragments).
- Files too large for the `/contents/` API fall back to the Git tree/blob API automatically — no need to switch to `ghCloneRepo` for size alone.

### `ghSearchHistory`

Search GitHub history through one strict discovery operation per query:

- `operation: "pullRequest"` searches PR candidates.
- `operation: "issue"` searches issue candidates.
- `operation: "commit"` walks commit history, optionally scoped to a path or time range.

The search tool returns candidates and stable identities. Fetch detailed content
with `ghGetHistoryItem`; search queries do not accept singular-item identities.
Pull-request search may omit `owner` and `repo` to search all of GitHub; issue
and commit searches require both. Commit `keywords` search commit messages on
the default branch and cannot be combined with `path` or `branch`.

<!-- tool: ghSearchHistory -->
```json
{"goal": "Show a documented ghSearchHistory result.", "reasoning": "Use ghSearchHistory for this documented evidence request.", "operation": "pullRequest", "owner": "vercel", "repo": "next.js", "keywords": ["middleware"], "match": ["title"], "state": "merged"}
{"goal": "Show a documented ghSearchHistory result.", "reasoning": "Use ghSearchHistory for this documented evidence request.", "operation": "issue", "owner": "vercel", "repo": "next.js", "keywords": ["memory leak"], "match": ["title"], "state": "open"}
{"goal": "Show a documented ghSearchHistory result.", "reasoning": "Use ghSearchHistory for this documented evidence request.", "operation": "commit", "owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/", "since": "30d"}
```

Prefer title-first PR and issue searches. For commit archaeology, narrow by path
and time before fetching a commit diff. Rarer PR/issue filters go in one
`qualifiers` string (`"reviewed-by:x review:approved label:bug comments:>5"`);
it maps onto the typed fields (which stay valid), and rejects `repo:`/`org:`/
`user:` (scope comes from `owner`/`repo`), unknown keys (with a suggestion),
negation other than a PR's `-is:draft`, and a filter set twice. `archived`
always routes a PR query through search, which enforces it. `next.readPr` targets the first merged row (else the first)
and lists up to three `candidates`; a bare issue number in `keywords` adds
`next.readIssueLinks`, the issue read whose `closedBy` names its fix PRs.
Commit rows carry the author's login (else git name), never an email; read the
full message with `next.readCommit`.

Keywords follow the `ghSearchCode` rule: a bare word or one quoted phrase, never an
operator. `owner`, `repo`, and person fields (`author`, `committer`, `assignee`,
`mentions`, `commenter`, `reviewed-by`, `review-requested`) must be GitHub logins,
names, or commit emails. Labels cannot contain quotes or backslashes. Range and
state qualifiers must be one term; whitespace inside a range such as `> 5` is
removed. Any other value is a validation error, never a changed scope. Commit
search does not accept `includeDiff`; read diffs with `ghGetHistoryItem`.

### `ghGetHistoryItem`

Read one known history item or compare two refs through one strict operation:

| Operation | Required identity | Typical detail |
|---|---|---|
| `pullRequest` | `owner`, `repo`, `number` | body, changed files, selected patches, comments, reviews, commits |
| `issue` | `owner`, `repo`, `number` | body and comments |
| `commit` | `owner`, `repo`, `ref` (+ `base` to compare `base...ref`) | commit metadata and optional diff |
| `compare` | `owner`, `repo`, `base`, `head` | ahead/behind counts and commits between refs |

Sections are selected with `include` (PR: `body`, `files`, `patches`,
`comments`, `reviews`, `commits`; issue: `body`, `comments`; commit:
`patches`) and narrowed with `files` (paths, `dir/`, or globs) and `status`.
The nested spellings (`content.*`, `fileFilter`, `includeDiff`, `path`,
`operation:"compare"`) stay valid aliases and return identical rows. Every PR
row carries its merge state (`mergedAt`, `closedAt`, `targetBranch`; labels on
the first page). An issue read lists up to 25 pull requests that closed it
(`closedBy`, merged first) and offers `next.readFixPr`; past 25 the row is
`isPartial` with `partialReasons:["closingReferenceLimit"]` and the fix is a
medium-confidence candidate.

Fields from another operation are rejected rather than ignored. In particular,
PR and issue identity is always `number`; commit identity is `ref`; comparison
identity is the `base` + `head` pair.

A merged pull request reports `mergeCommitSha` (from GraphQL `mergeCommit`;
REST API 2026-03-10 drops `merge_commit_sha`) with `next.getMergeCommit`; an
open pull request never reports GitHub's test-merge SHA. A commit offers
`next.findPullRequest`, a `ghSearchHistory` query that finds the pull request
containing that SHA. A comparison resolves both `base` and `head` to SHAs, and
each commit or file page carries only its own data.

<!-- tool: ghGetHistoryItem -->
```json
{"goal": "Show a documented ghGetHistoryItem result.", "reasoning": "Use ghGetHistoryItem for this documented evidence request.", "operation": "pullRequest", "owner": "vercel", "repo": "next.js", "number": 12345, "include": ["body", "files"]}
{"goal": "Find where a large PR touches esbuild.", "reasoning": "Search patches for the literal before paging the file inventory.", "operation": "pullRequest", "owner": "microsoft", "repo": "TypeScript", "number": 51387, "matchString": "esbuild", "files": ["*.json", "*.mjs"]}
{"goal": "Show a documented ghGetHistoryItem result.", "reasoning": "Use ghGetHistoryItem for this documented evidence request.", "operation": "issue", "owner": "vercel", "repo": "next.js", "number": 12345, "content": {"body": true, "comments": {"discussion": true}}}
{"goal": "Show a documented ghGetHistoryItem result.", "reasoning": "Use ghGetHistoryItem for this documented evidence request.", "operation": "commit", "owner": "vercel", "repo": "next.js", "ref": "abc123", "includeDiff": true}
{"goal": "Show a documented ghGetHistoryItem result.", "reasoning": "Use ghGetHistoryItem for this documented evidence request.", "operation": "compare", "owner": "vercel", "repo": "next.js", "base": "v14.0.0", "head": "v14.1.0"}
```

Request selected PR patches instead of every patch for large PRs, and leave
commit diffs off until the relevant commit is known. A PR summary offers at
most four reads: `getChangedFiles` (the body rides along when the preview cuts
it), `reviewPatches` (every patch on a small PR), `getDiscussion` (comments and
reviews), and `getMergeCommit` (with the diff when the PR is small). A merged
PR whose patches were read also offers `readAtMerge`: the first changed source
file at the merge commit, as a `block` read anchored on its first added line.
An inventory read offers `reviewPatches`: up to 30 source files (no tests,
docs, lockfiles, generated or binary files; tests by each language's layout
and naming, such as `_test.go`, `test_*.py`, `FooTest.java`, `_spec.rb`, or a
`test/` directory), the ones that fit one patch budget first, the rest through
`continuePatch`.
Rows of one call share one patch budget, so a multi-row patch read fits one
response page.

`content.changedFiles` returns compact patch-free rows,
`"M +3 -1 [!flag ]path[ <- old/path]"`: a git status letter (`T` changed,
`U` unchanged), line counts, and the path, with files sharing a directory
grouped as `{"dir/": [rows]}` (path = dir + name). `!flag` marks a file GitHub
returned without a patch: `!tooLarge` (counts still known), `!binary`, or
`!omitted` (no patch and no counts, for example past the diff budget; 0/0 may
still have changed). `<- old/path` marks a rename. Patch rows (PR, commit, and
comparison) are `{path, stat: "M +3 -1", patch}` objects that keep
`patchUnavailable`, `previousPath`, and `patchPagination` when cut; files not
reached yet ride the continuation, not empty rows; a pure rename's patch is
`""`.
GitHub lists at most 3000 files. Past that limit the file page reports
`countScope: "partial"` and `providerLimit.reason: "providerFileListLimit"`.

PR reads narrow with three optional fields:

- `fileFilter` keeps files matching all given filters: `paths` (any-of, 1–100
  file or directory paths or globs; `*` one segment, `**` any depth, no `/`
  matches the file name, e.g. `"*.ts"`), `status` (`added`, `removed`,
  `modified`, `renamed`, `copied`, `changed`, `unchanged`), and `minChanges`
  (additions + deletions ≥ n; files without counts pass).
- `matchString` keeps only the patch lines that match, implies patches, and
  returns every hit file of the PR on one page. Files with no patch to search
  (`!tooLarge`, `!binary`, `!omitted`) are listed as unsearched, so a miss
  there is not absence.
- `matchContext` (0–10) sets the lines kept around each hit. Omitted, it is 0
  (hit lines only); narrowed files offer `next.readFullPatches` (up to five
  files) or `next.widenContext` (`matchContext: 3`).
- A `files`/`status` scope that matches no changed file returns
  `noSelectedFilesMatched` with an inventory hint.

A file inventory, a patch read, or a later page (a file, comment, commit, or
review page after the first, or a nonzero body offset) returns a slim identity header (`number`, `state`, `sourceSha`,
and the merge state `mergedAt`/`closedAt`/`targetBranch`; first pages also keep
labels, inventories title, author, and counts), not the body preview or the
full follow-up menu, unless `debug: true`.

PR details accept `minify:"none"` or `"standard"`. `none` preserves selected
body, discussion/inline comments, reviews, and all/selected patch text after
security redaction. `standard` (the default) compacts Markdown and unchanged
diff context before computing offsets; it preserves changed source lines
regardless of language. In a patch over 30 lines it keeps the hunk headers and
2 context lines around each change, and replaces each run of other context
lines with one `...` line, so patch lengths differ from GitHub's and line
numbers cannot be counted from a hunk header across a `...`. Pass
`minify:"none"` for the verbatim patch. Match-filtered reads preserve source
anchors.

Issue, commit, and compare details do not accept `minify`; they return exact
selected text after redaction. Issue bodies and comment bodies use
`charOffset`/`charLength`, with automatic 12,000-character windows.
Comment-item continuations reset the text offset. Follow each returned
continuation independently: body, comment body, item page, file, and patch
windows address different parts of the response.

PR collection reads fetch at most one provider batch per requested source:
100 discussion comments, inline comments, or reviews, and 50 commit summaries.
`pageSize` bounds displayed items within each batch (default 30, max 100). A
patch-free `content.changedFiles` inventory pages by `filePage`; with
`pageSize` omitted it fills the response page (100–1000 rows), and an explicit
`pageSize` may reach 1000.
`reviewPage` pages reviews independently; review bodies retain their text
continuations. Follow the returned `next.*` calls, including when filtering
produces an empty batch. Those calls carry `collectionPages` positions and
reset the relevant item and text windows when advancing. A zero position marks
an exhausted comment source, which is no longer fetched. Counts labeled
`countScope: "providerBatch"` describe that batch, not the complete collection.

Nested `content.commits.includeFiles` reads fetch one file batch for each
displayed commit and return at most `pageSize` files per commit. Each commit's
`next.nextFilePage` and `next.continuePatch` call `ghGetHistoryItem` with its
exact SHA. Exact commit reads page files with `filePage` and retain independent
file and patch (`charOffset`/`charLength`) windows. An explicit `charLength`
sizes the patch window up to the schema maximum (larger rows split across
response pages); omitted, the window fits one automatic response page. Each
file's `patchPagination` uses that file's own offsets; a commit or compare
page's stream cursor is `filesPagination.nextPatchCharOffset`, which
`next.continuePatch` carries. History reads are never cached. Provider caps and omitted patches remain explicit
terminal limits; a provider cap does not establish completeness.

A commit read that stops at its file-batch cap reports
`changedFilesCountScope: "partial"`, `countScope: "partial"`, and
`terminalLimit`. It does not report `complete`. PR metadata lists the first 20
labels and sets `labelsTruncated: true` when more exist. PR commit summaries
carry the full commit message, including the body.

### `ghCloneRepo`

Clone a repository or sparse subtree into Octocode's local cache.

Clone is CLI-only (MCP never registers it) and needs local tools plus
persistent storage (`storage.mode`, the default).

Key fields:

| Field | Meaning |
|-------|---------|
| `owner`, `repo` | Required repository. |
| `branch` | Branch, tag, or exact commit SHA. Omit to use the default branch. |
| `sparsePath` | Optional file or directory, or an array of up to 10, for a sparse checkout. A missing path is a not-found error (exit 3). |
| `depth` | Commits of history, 1–50 (default 1). |
| `forceRefresh` | Bypass the clone cache and re-clone. |

Returns a location with an absolute path, commit identity, resolved branch,
`cached`, and `location.clonedAt`/`expiresAt` (fresh clones and cache hits
alike; a hit's commit may lag the branch until then, so pass `forceRefresh` for
the current head). `verified` and `complete` appear only when false.
`next.exploreClone` lists the checkout (the sparse subtree for one path, the
root for several) with `structureSearch operation:"tree"`; a single sparse
file is read with `localFetch` instead. No GitHub API call
precedes git: an unbranched cache hit finds its entry through a recorded
default-branch alias, and a fresh clone lets git resolve the remote HEAD. The
API is asked only after git fails, to name a missing or inaccessible
repository.

Examples:

<!-- tool: ghCloneRepo -->
```json
{"goal": "Show a documented ghCloneRepo result.", "reasoning": "Use ghCloneRepo for this documented evidence request.", "owner": "vercel", "repo": "next.js", "branch": "canary"}
{"goal": "Show a documented ghCloneRepo result.", "reasoning": "Use ghCloneRepo for this documented evidence request.", "owner": "microsoft", "repo": "TypeScript", "sparsePath": "src/compiler"}
```

Rules:

- Use `sparsePath` for large monorepos.
- Use `ghGetFileContent` when you only need one file.
- Cached clones are reused.
- Use the returned path as-is. A cache hit requires a clean working tree at the
  recorded HEAD; an edited cache is re-cloned, so `verified` holds for cached
  bytes. Check `verified` separately from scoped `complete`.

### `artifactSearch`

Find packages for a capability, resolve a known dependency to registry metadata, or locate its upstream source. Use local tools to explain installed code and GitHub tools when the repository is already known. A repository link is metadata, not implementation or published-version proof. `version` pins an exact lookup to an exact version, a range (`^3`, `>=2.31,<3`), or a tag (`latest`, `next`) on npm, PyPI, and crates.io (the `name@version` and PyPI `name==version` coordinates stay valid); a range resolves like the registry's installer, and a missing version is `versionNotFound` with the nearest published versions. Exact rows add release facts when the registry has them: `publishedAt`, `deprecated`, `yanked`, `dependencies`/`peerDependencies` counts, and `engines` (npm), `requiresPython` (PyPI), or `rustVersion` (crates). npm discovery rows carry `downloadsMonthly`. Exact GitHub-backed lookups offer `next.viewRepo` for default-branch code and, when the registry names a commit or tag, `next.viewReleaseSource` for that release. Each labels `source.scope` (`defaultBranch` or `release`); `viewRepo` is always `source.verification:"unverified"`, and `viewReleaseSource.source.verification` is `provenance` when an npm SLSA provenance attestation binds this exact tarball (subject digest equals `dist.integrity`) to the manifest's repository and names the commit (the registry verifies the attestation at publish; octocode does not re-verify signatures), else `unverified` (npm `gitHead`, Go/Composer refs). Execute the selected lead and check its resolved revision before treating it as evidence. A missing release ref may be unpublished or stale; the default branch is a recovery lead, not evidence of that release.

| Field | Meaning |
|-------|---------|
| `type` | Required ecosystem: `npm` (JavaScript/TypeScript), `pypi` (Python/pip/uv), `crates` (Rust/Cargo), `maven` (Java/Kotlin), `nuget` (.NET), `go` (Go modules), `packagist` (PHP/Composer), or `rubygems` (Ruby/Bundler). |
| `packageName` | Exact ecosystem coordinate, such as `@scope/name`, `group:artifact`, or a Go module path. Exclusive with `keywords`. |
| `keywords` | One or more discovery terms as an array. PyPI supports exact lookup only. |
| `cursor` | Opaque discovery continuation; copy the complete returned `next.nextPage` query. Exact lookup has no pagination controls. |
| `pageSize` | Discovery result count, default 10, range 1–100. |
| `registry` | npm-only HTTP or HTTPS registry override. Omit for npm environment and `.npmrc` routing. Credentials are not tool inputs. |

<!-- tool: artifactSearch -->
```json
{"goal": "Show a documented artifactSearch result.", "reasoning": "Use artifactSearch for this documented evidence request.", "type": "npm", "packageName": "react"}
{"goal": "Show a documented artifactSearch result.", "reasoning": "Use artifactSearch for this documented evidence request.", "type": "pypi", "packageName": "requests"}
{"goal": "Show a documented artifactSearch result.", "reasoning": "Use artifactSearch for this documented evidence request.", "type": "crates", "keywords": ["async", "runtime"], "pageSize": 10}
{"goal": "Show a documented artifactSearch result.", "reasoning": "Use artifactSearch for this documented evidence request.", "type": "maven", "packageName": "org.slf4j:slf4j-api"}
{"goal": "Show a documented artifactSearch result.", "reasoning": "Use artifactSearch for this documented evidence request.", "type": "npm", "packageName": "@example/widget", "registry": "https://registry.example.com/"}
```

Each query selects one ecosystem. Compare ecosystems using independent entries in `queries` (maximum five), not `type:"all"`. All providers use official APIs. PyPI has no `keywords` field, so a PyPI keyword query is rejected as `invalidInput`; it never falls back to a website or third-party service.

`artifacts[]` contains canonical package identities and available version, description, license, homepage (omitted when it is the repository page), and source metadata; `debug:true` adds registry URLs. Go package paths stay separate from module identities. Optional metadata can be absent; cross-registry popularity scores are not comparable. Read exact source to establish behavior and match the published or installed version before making version-specific claims.

For npm, scoped names honor `@scope:registry` and an explicit `registry` takes precedence; authentication uses registry-scoped npm configuration (with environment interpolation), caches isolate registry/configuration identities, and continuations preserve the selected registry. Private registries must support npm's search endpoint for discovery; other ecosystems use official public services. Follow executable continuations rather than constructing page numbers or interpreting cursors; unknown totals stay unknown, an empty page may still continue, and auth/unsupported/rate-limit/provider failures are errors while a missing exact package is `notFound`.

### Token cost control by goal

| Goal | Cheapest approach |
|------|------------------|
| Find if a function exists in a file | `ghSearchCode` with `keywords: ["functionName"]` |
| Read one function body | `ghGetFileContent` with `matchString: "function name"` + small `contextLines` |
| Scan a whole file's structure | `ghGetFileContent` with `minify: "symbols"` |
| Read 2–10 functions from a file | Multiple `startLine`/`endLine` reads in one batched call |
| Read a 3MB+ file | `ghCloneRepo` sparse + local read |
| Understand why a PR was made | `ghSearchHistory(operation:"pullRequest")`, then `ghGetHistoryItem(operation:"pullRequest", number)` with `include: ["body"]` |
| Review a PR's changes | Small PR: `include: ["patches"]`. Large PR: `matchString` (+ `files`) over all patches, or `include: ["files"]` then `next.reviewPatches` |
| Get all inline code comments on a PR | `content: { comments: { reviewInline: true, discussion: false } }` |
| Count repositories in an org | `owner: "vercel", archived: false` → search `totalMatches` (an owner-only listing reports no total) |
| Get package version only | `artifactSearch` with `type` and `packageName`; an upstream manifest is not proof of the published version |

### Workflows

End-to-end GitHub, history, and clone flows are owned by the [research manifest](OCTOCODE_RESEARCH_MANIFEST.md#external-workflow).

### GitHub tool rules

- Use GitHub tools for remote repositories, not files already on disk.
- Use `artifactSearch` for a known dependency or a package capability need; set the ecosystem `type`. Skip it when the source repository is already known or installed behavior needs local evidence.
- Use `ghStructure` before reading unknown paths.
- Use `matchString`, line ranges, or `minify: "symbols"` instead of `fullContent` for large files.
- Use PR metadata first, then selected content.
- Use `ghCloneRepo` only when local analysis is worth the clone cost.
- Retry only transport failures, timeouts, and 5xx. HTTP 451 is
  `errorCode: "unavailable"` (blocked for legal reasons). Other unmapped
  statuses are `errorCode: "httpStatus"` with the code in the message. Neither is
  retryable. An error body over the size limit keeps its status classification
  and adds `(error body exceeded limit)` to the message.

---

## Local code tools reference

> Complete reference for Octocode MCP local code tools: file system exploration, metadata search, text/regex search, structural AST search, semantic follow-up anchors, and targeted file reading.

---

### Scope

| Tool | Purpose |
|------|---------|
| `localSearch` | Lexical local discovery; matches provide anchors for `lspSearch`. |
| `structureSearch` | Directory outlines and file discovery by name or metadata. |
| `astSearch` | Structural AST matches, syntax trees, and symbols. |
| `astTopology` | Cross-file dependency graph analysis and coverage diagnostics. |
| `localFetch` | Read targeted file content by line range, match, signature skeleton, or line/byte chunk. |

---

### Local tool configuration

Local tools are on by default. To turn the whole local surface off:

<!-- example: configuration -->
```json
{
  "local": {
    "enabled": false
  }
}
```

`ENABLE_LOCAL=false` does the same. To hide individual local tools while keeping the rest available, use `DISABLE_TOOLS` or `tools.disabled`; `TOOLS_TO_RUN` is a strict allowlist.

Relative local paths resolve against the workspace root (`WORKSPACE_ROOT`, default the process cwd). The allowed roots are the workspace root, `ALLOWED_PATHS`/`local.allowedPaths` entries, and the Octocode home; the user home directory is not allowed unless listed.

A path outside the allowed roots (directly or through a symlink) fails with `errorCode: "pathOutsideAllowedRoots"` in `localSearch`, `localFetch`, `lspSearch`, and `astRewrite` (`astSearch` keeps `ast.policy.outsideAllowedRoots`/`ast.policy.symlinkEscape`; `structureSearch` returns `structure.policy.outsideAllowedRoots`); the recovery hint is keyed on that code. Run from inside the workspace or extend `ALLOWED_PATHS` / `WORKSPACE_ROOT`.

Config reference: [Configuration Reference](CONFIGURATION.md).

---

### Platform support

`localSearch`, `structureSearch`, `astSearch`, and `localFetch` use Octocode's native in-process ripgrep, filesystem-walker, and structural-search engines. There are no external `rg`, `grep`, `find`, or `tree` dependencies.

All local search tools work on macOS, Linux, and Windows. Prefer `localSearch` with `resultView:"files"` when a content query can answer the question.

---

### Pagination

All tools accept up to 5 queries per call.

Local tools expose two pagination layers:

| Layer | Fields | Applies To |
|-------|--------|------------|
| Native result pagination | `page`, `pageSize` | `localSearch`, `structureSearch`, `astSearch`, `astTopology` |
| Per-file match pagination | `matchPage`, `maxMatchesPerFile` | `localSearch` when a matched file has more matches |
| Local content pagination | `chunkType`, `offset`, `chunkSize` | `localFetch` selected views |
| Bulk response pagination | `responseCharOffset`, `responseCharLength` | Any bulk response |

Use native pagination first for result lists, then char pagination only when a single result payload is still too large.

---

### Choose a local tool

| Need | Use |
|------|-----|
| "Which directories/files exist here?" | `structureSearch(operation:"tree")` |
| "Find files named `*.test.ts` or modified within a time window." | `structureSearch(operation:"files")` |
| "Search for text, regex, imports, TODOs, or identifiers." | `localSearch` |
| "Read this exact file section." | `localFetch` |
| "Find files containing a pattern without match bodies." | `localSearch( resultView:"files", ...)` |
| "Find files that do not contain a pattern." | `localSearch( resultView:"filesWithout", ...)` |

Recommended order for code research:

```text
DISCOVER -> SEARCH -> READ
```

Start broad with structure or metadata, narrow with content search, then read the smallest exact file slice needed.

`localSearch` handles lexical search, `structureSearch` handles directory
outlines and file metadata, `astSearch` handles structural matches, syntax
trees, and symbols, and `astTopology` handles file graphs. Each public tool rejects fields
from other operations. Removed compatibility names cannot be restored with
`TOOLS_TO_RUN` or `.octocoderc`.

---

### `localSearch`

Lexical local search. The query is selected by `searchText` and `regex`; use
`structureSearch` for directory outlines or file metadata, `astSearch` for syntax
matches, syntax trees, or symbols, and `astTopology` for file-graph queries.

#### Best for

- Finding identifiers, imports, route names, constants, TODOs, errors, config keys, and string literals.
- Listing files that contain or do not contain a pattern.
- Getting compact match context before deciding which file section to read.

#### Key parameters

| Parameter | Description |
|-----------|-------------|
| `path` | File or directory to search. Relative paths resolve from the workspace root. For remote repos: pass `localPath` from a `ghCloneRepo` result — it is already absolute and immediately valid. |
| `searchText` | Text or regex pattern. Required, with `path`. |
| `resultView` | Lexical response shape: `paginated`, `detailed`, `content`, `files`, `filesWithout`, `countLines`, `countMatches`, or `matchOnly`. |
| `matchWindow` | With `resultView:"matchOnly"`, widen each matched span by this many characters of context on each side (… marks trimmed sides). 0 = bare match. |
| `unique` | With `resultView:"matchOnly"`, use `list` for distinct match values per file or `count` for frequencies. |
| `contextLines` | Lines around each match. Default 0 (`detailed`: 3), max 100. |
| `matchContentLength` | Max characters per match snippet, clipped around the hit. Default 200 × (2·contextLines + 1), capped at 4000; explicit max 100000. |
| `pageSize` | Files per lexical result page, 1–1000. Default 20 (100 for path/count views). |
| `maxMatchesPerFile` | Per-file match page size. Default 10. Pair with `matchPage` to continue. |
| `page` | Result page across matched files. |
| `matchPage` | Per-file match page when a file has more matches. |

#### Match options

| Parameter | Description |
|-----------|-------------|
| `regex` | `literal` for literal text, `rust` for Rust regex, or `pcre2` for PCRE2 features. |
| `caseMode` | `smart`, `sensitive`, or `insensitive`. |
| `wholeWord` | Match whole words only. |
| `multiline` | `off`, `on`, or `dotall` for cross-line matching. |
| `invertMatch` | Return non-matching lines. With `resultView:"files"` it lists files that contain any non-matching line, not files without the pattern; use `resultView:"filesWithout"` for that. |

#### Filters

| Parameter | Description |
|-----------|-------------|
| `langType` | Ripgrep language/type filter such as `ts`, `js`, `py`, `go`. |
| `maxDepth` | Directory depth limit, 0–20. |
| `include` | Glob patterns to include. |
| `exclude` | Glob patterns to exclude. |
| `excludeDir` | Directory names to skip, added to the default prune (dependency, build, cache, credential, and editor/CI config directories such as `node_modules`, `target`, `.git`, `secrets`, `.github`). `defaultExcludes: false` turns that prune off, but `.gitignore` still hides ignored directories until `noIgnore: true`, and sensitive directories such as `secrets/` are never walked. |
| `hidden` | Include hidden files. |
| `noIgnore` | Ignore `.gitignore` and `.ignore` files. |
| `sort` | `relevance` (default), `traversal`, `matchCount`, `path`, `modified`, `accessed`, or `created`. |
| `reverse` | Reverse the selected sort before pagination. |

#### Output

Normal results include matched files and match snippets with line and column information. For count-only output use `resultView:"countLines"` or `resultView:"countMatches"`.

Coverage and limits:

- `relevance` (default): for one bare identifier, source files that declare it rank first; then match count, source paths before test/generated/vendored paths, then declaration hits before code and comment/string hits, then path; `files`/`filesWithout` have no count and order by source paths first, then path. `relevance` and `matchCount` keep the 10,000 highest-ranked files across every searched file. Debug `stats` totals count all matched files; `capReason:"maxCollectedFiles"` marks the trimmed list.
- If no file under `path` can be read, the call fails with `errorCode:"fileAccessFailed"` (exit 5). If some paths are unreadable, the result sets `isPartial` and `terminalLimit`, and `stats.errorCount`/`firstError` report the failures. Zero matches in that result do not prove absence.
- A file with a NUL byte is searched up to that byte, and matches before it are kept. A `binaryFileSkipped` warning is added and the result is `isPartial` (debug stats also show `capped` and `capReason: binaryQuit`) (with `terminalLimit` when no other continuation exists): no text tool reads past the NUL, so the kept matches are not the file's full set and an empty result does not prove absence.
- Match values whose secret-shaped text was replaced by `[REDACTED…]` placeholders carry a `redactedMatches` warning; those values are not verbatim source.
- `regex:"pcre2"` searches have a wall-clock limit. At the limit the result keeps the files finished so far and reports `capReason:"pcre2Deadline"`.
- Files are opened without following symlinks. A path replaced by a symlink or special file after the walk counts as a read error.

Row `path` values are relative to the envelope `base`, which is the queried directory (or the parent of a queried file) on every page; `join(base, path)` is the absolute file to pass to `localFetch` or `lspSearch`. The `next` map carries:

| Next key | Tool | Purpose |
|----------|------|---------|
| `nextPage` / `nextMatchPage` | `localSearch` | Continue file-level or per-file match pagination. A later match page lists only files that still have rows. |
| `restart` | `localSearch` | Rerun from page 1 when the result snapshot is stale. |
| `clasify` | `clasify` | Wide pages only (see [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md)); present only while `clasify` is available. |

#### Examples

```bash
localSearch( path="packages/octocode-mcp/src", searchText="registerTool", langType="ts")
localSearch( path=".", searchText="TODO", resultView="files")
localSearch( path="src", searchText="class\\s+\\w+Service", regex="rust", contextLines=3)
```

#### Structural and AST search

Use `astSearch(operation:"match")` for code-shape queries regex cannot express (find all `await` inside `for` loops, calls with N args, functions missing `try/catch`).

Structural results distinguish file-scan caps from execution limits. Reaching
`maxFiles` (default 2000) adds a `structural.scan.truncated` warning
diagnostic; narrow the scope or raise `maxFiles` and rerun. Parser or matcher
exhaustion preserves completed files and reports staged `diagnostics`. Zero
matches in an incomplete result do not establish absence. `maxDepth: 0`
includes files directly in the root; depth filtering happens before the
file-scan cap. `operation:"symbols"` scans up to `maxFiles` (default 2000,
max 50000) and reports `terminalLimit` when the scan was truncated or files were skipped.

`astSearch` match, `syntaxTree`, and `symbols` positions use one-based lines and
zero-based UTF-16 code-unit columns. A `symbols` row's `name` + `line`, or an
identifier capture's `text` + `line`, is `lspSearch` `symbolName` + `lineHint`
as-is (with `uri` = the file path).

`operation:"syntaxTree"` pages one file's parsed syntax tree: `nodeOffset`
(default 0), `nodeLimit` (default 100, max 1000), and `namedOnly` (default
`true`).

**Supported structural extensions:** `asm`, `assembly`, `c`, `cc`, `cjs`, `cpp`, `cs`, `cts`,
`cxx`, `go`, `h`, `hh`, `hpp`, `hxx`, `java`, `js`, `jsx`, `mjs`, `mts`, `py`,
`pyi`, `rs`, `s`, `sbt`, `sc`, `scala`, `ts`, and `tsx`. The exact same 28-extension
set backs signatures and graph facts in the default release build. Query the
compiled engine capability API when optional grammar features are disabled.

A code-shaped pattern that returns zero matches is not retried in another form;
use an explicit `rule` query when exact query equivalence matters.

YAML `kind` rules are checked against the selected source grammar before
execution; YAML is the rule-document format, not a supported source grammar.
An unknown node kind returns a typed compile diagnostic instead of a
high-confidence zero-match result.

`inside` checks read each candidate's ancestor chain once (linear in nesting
depth), so deeply nested files do not hit the deadline on `stopBy: end`. A
file that still exceeds the deadline is reported as a
`structural.match.deadline` diagnostic, never as zero matches.

`operation:"symbols"` takes `name` as one substring or a list (`name:["complete","try_read_output"]` returns either); pages hold 500 declarations by default. Rows are `{name, kind, line}` plus only what adds information:
- `endLine` when the declaration spans lines, and `startLine` when attributes or decorators start before the name line.
- `docStartLine` when a comment block sits directly above (JSDoc, `///`, `#` in Python).
- `parent`: the containing declaration's name; nested declarations (a function inside a function, in TypeScript as in Python and Rust) are listed with their container as `parent`. It stays meaningful when a `kinds`/`name` filter drops the parent row. `parentLine` is added only when two containers share that name and kind (two `impl A` blocks).
- `character` only when two declarations of the same kind share a name and line.

`line` feeds `lspSearch` as `symbolName` + `lineHint`. The YAML text channel renders the rows as an indented outline after the metadata (`=== symbols <path> (line[-endLine] kind name; + exported; indented = member) ===`, with `as`, `doc` (doc block on the line above; `doc@N` when it starts elsewhere), `from@N`, `col N` and `(in Parent)` suffixes); structured content keeps the rows. A single-file outline returns top-level `declarations`. A directory outline returns `files: [{path, declarations}]`, the same grouping as `match`, so each path is written once. `snapshot` appears only on paginated results. It marks a JS/TS declaration `exported` by its local
binding. When it is exported under another name, `exportedAs` lists the public
names: `export { foo as bar }` gives `foo` with `exportedAs: ["bar"]`, and
`export default function foo` gives `exportedAs: ["default"]`.

Java call patterns may omit their trailing semicolon. The structural compiler
supplies grammar-checked statement context for direct patterns and patterns
nested anywhere in a YAML rule; complete patterns keep their original parse,
match ranges, and captures.

Pattern matching is exact about modifiers: a Rust `fn $N()` pattern does not
match `pub fn` items (the visibility modifier is a named child). Such a pattern
adds a `structural.pattern.visibilityExact` info diagnostic, visible only with
`debug: true`; write `pub fn …` or
use a YAML rule on the item kind. Match rows are `{line, column, value}`
(`endLine`/`endColumn` only for multi-line spans); per-file
`totalMatchRows`/`returnedMatchRows` appear only when a match page is a subset.
Captures are opt-in: `captureText:true` (offered as `next.expandCaptures` when
the query has metavariables or a match was cut to its header) adds
`metavarRanges` with per-node text and positions.

`rule` is a YAML string or the equivalent ast-grep rule object (the same
`AstRule` shape astRewrite takes). A directory `match` without `langType`
uses the one grammar whose files occur under `path` and that parses the query
(`inferredLangType` names it and continuations pin it); several candidates
return `ast.language.required` naming them.

Structural failures retain native public codes such as
`structural.query.invalid`, `structural.query.compileFailed`,
`structural.language.unsupported`, and `structural.content.tooLarge`. Content
size exhaustion is a typed terminal limit rather than a generic execution
failure.

```bash
astSearch(operation="match", path="src", pattern="track($$$ARGS)")
# `rule` is a YAML string: \n below are real newline escapes in the JSON tool
# arg (not literal backslash-n). On the CLI, use $'...' or a real multiline string.
astSearch(operation="match", path="src", rule="rule:\n  pattern: await $C\n  inside:\n    kind: for_statement\n    stopBy: end")
astSearch(operation="match", path=".", pattern="eval($X)")
astSearch(operation="syntaxTree", path="src/index.ts", nodeLimit=100)
```

---

### `structureSearch(operation:"tree")`

Bounded directory outline (no parser) for understanding shape, ownership, and file distribution.

#### Best for

- Orienting in a new repository.
- Inspecting package/source/test boundaries.
- Finding likely entry points before content search.

#### Key parameters

| Parameter | Description |
|-----------|-------------|
| `path` | Directory to browse. Relative paths resolve from the workspace root. |
| `maxDepth` | Levels below `path`, 0–20, default 1; `0` lists immediate children. Use low depth first. |
| `page` | Result page. |
| `pageSize` | Directory entries per page, max 100. |
| `limit` | Hard pre-pagination cap. Max 10000. |
| `entryType` | `f` for files only, `d` for directories only; omit for both. |
| `extensions` | Only include files with selected extensions. |
| `excludeDir` | Directory names to prune, added to the default prune (dependency, build, cache, and credential directories such as `node_modules`, `target`, `.git`, `secrets`; `.github`-style config stays visible). `defaultExcludes: false` turns that prune off; sensitive directories such as `secrets/` stay hidden. |
| `hidden` | Include hidden files and directories. |
| `snapshot` | Copy from `next` when paging. |

#### Output

`operation` defaults to `"tree"`. The response lists `entries` as compact strings (`name (size)`, directories with a trailing `/`) plus pagination metadata when more remain.

#### Examples

```bash
structureSearch(operation="tree", path=".", maxDepth=1)
structureSearch(operation="tree", path="packages/octocode-mcp/src", maxDepth=2, entryType="d")
structureSearch(operation="tree", path="docs", extensions=["md"])
```

---

### `structureSearch(operation:"files")`

Metadata search for files and directories.

#### Best for

- Finding files by name, extension, regex, path slice, size, permission, or modified time.
- Locating tests, configs, generated files, or files modified within a time window.
- Metadata search when content search is not needed.

#### Key parameters

| Parameter | Description |
|-----------|-------------|
| `path` | Directory root for metadata search. |
| `names` | Filename globs OR-combined, such as `["*.ts", "*.tsx"]`. |
| `pathPattern` | Glob matched against the full path. |
| `pathRegex` | Rust regex over the basename only. |
| `entryType` | `f` for files, `d` for directories. |
| `minDepth` / `maxDepth` | Depth bounds. |
| `time.modifiedWithin` | Files modified within a window, such as `7d` or `2h`. |
| `time.modifiedBefore` | Files older than a relative window, such as `7d`. |
| `time.accessedWithin` | Files accessed within a window. |
| `size.greater` / `size.less` | Size filters such as `100k` or `1m`. |
| `empty` | Empty files/directories only. |
| `permissions` | Octal permission filter, such as `"644"`. |
| `access` | Permission predicate: `executable`, `readable`, or `writable`. |
| `excludeDir` | Directory names to prune, added to the default prune (dependency, build, cache, and credential directories such as `node_modules`, `target`, `.git`, `secrets`; `.github`-style config stays visible). `defaultExcludes: false` turns that prune off; sensitive directories such as `secrets/` stay hidden. |
| `detail` | `basic` (default), `modified` (adds `modifiedMs`, Unix milliseconds), or `full` (also adds exact size and line count). |
| `sort` | Sort by `modified` (default), `name`, `path`, `size`, or `lines`. |
| `page` | Result page. |
| `pageSize` | Files per page. Max 100. |
| `limit` | Hard pre-pagination cap. Max 10000. |

#### Examples

```bash
structureSearch(operation="files", path=".", names=["*.test.ts"])
structureSearch(operation="files", path="packages", pathRegex="^readme\\.md$")
structureSearch(operation="files", path=".", time={"modifiedWithin":"24h"}, entryType="f", detail="full")
```

---

### `localFetch`

Read a known local path. Path-only reads are valid and return exact source subject to redaction. Optionally select a source range (`startLine` + `endLine`, one-based inclusive), `matchString`, or `fullContent:true`; these selectors are mutually exclusive.

| Field | Meaning |
| --- | --- |
| `chunkType` | `lines` (default) or UTF-8 `bytes` in the selected returned view. |
| `offset` | Zero-based view offset; byte offsets must start on code-point boundaries. Default 0. |
| `chunkSize` | Requested lines or bytes, 1–50000. Omitted, a line page fills the 16 KiB page budget (the continuation carries the line count it used); a byte page is 16384 bytes. A first read of a file of at least 2,000 lines with no selector (no `matchString`, range, `block`, `offset`, `chunkType`, `chunkSize`, `minify` view, or `fullContent`) returns only its first 50 lines with a hint; `next.continue` pages on at the default size and `fullContent: true` reads it whole. |
| `matchString` | Nonempty literal source text; enable `matchStringIsRegex` for regex or `matchStringCaseSensitive` for case sensitivity. |
| `contextLines` | Explicit source-line context per side; default 5 for line chunks. Values above 100 clamp to 100 with a warning; the schema rejects values above 10000. Exclusive with `contextBytes`. |
| `contextBytes` | UTF-8 context bytes per side, 0–16384; default 256 for byte chunks. Requires `matchString`. Full-source redaction precedes byte matching; edges expand to whole code points, and disjoint windows are separated by a `... [N bytes omitted] ...` marker. |
| `minify` | `none` (default), `standard` compact source, or `symbols` whole-file outline. Match views preserve source text; symbols cannot accompany range/match selectors. |
| `block` | Widen each range or match window to its enclosing declaration (at most 400 lines). JS/TS functions assigned to a member (`res.redirect = function () {}`, `exports.x = () => {}`) are declarations. A window that no declaration encloses keeps its lines and says so in a `block:` warning. |
| `fullContent` | Complete unpaged view within resource/security limits; cannot accompany chunk controls. |
| `snapshot` | Copied from a continuation; a file that changed since is rejected rather than mixed. |

Selection precedes minification, redaction, and pagination. Line pages preserve complete lines within a 16384-byte budget. An offset at or past the end of the view returns empty content with an offset-zero `next.restart` (`pagination.outOfRange:true` with `debug: true`). An oversized line switches to byte paging from the unreturned position. Byte ends extend by at most three bytes to finish a UTF-8 code point. Copy the complete `next.continue` query; do not calculate offsets. Continuations stop at the selected range or matched view.

Every successful text read reports original-file `totalLines`, including empty files and no matches. `sourceBytes`, `returnedBytes`, and `returnedLines` are debug-only; a partial page also reports `returnedChars` (UTF-16 code units). `pagination.totalLines`/`totalBytes` describe the selected returned view. Content whose lines map onto original source lines is numbered in the structured result itself, `cat -n` style (`279<TAB>fn a() {`), with omission markers unnumbered and `sourceLineRanges` then omitted; strip the prefix up to the first TAB before copying text (see [numbered source content](TOOL_DATA_CONTRACT.md#numbered-source-content)). Whitespace and line endings after the prefix are preserved. YAML text prints the same lines under `content (source lines):`, or `content (copy-safe):` unnumbered when lines cannot be mapped. The `symbols` outline prefixes each line with `N| `.

`matchRanges` describe all selected source context windows; `matchedLines` contains matching source anchors intersecting the current page, and `selectedMatchCount` counts matching source lines in the selected view. Overlapping context windows are merged. `matchString` forces exact content so minification cannot remove the evidence. `minifyFallback` reports the requested/applied modes and reason when a match forces exact content or an outline is unavailable.

Private-key blocks are redacted across the whole file before any selection, so a key split across a page or range boundary never leaks. A line page is then scanned on its own lines plus 8 KiB of surrounding lines, so a multi-line secret crossing the page edge is still matched whole and paging a large file costs one page scan per call. A byte page is scanned on its whole lines plus at least 8 KiB of surrounding whole lines (single-line secrets are always scanned whole); a redacted line cut by the page end is returned whole and the page extends to that line's end, so `next.continue` never splits a secret. Byte offsets stay in the unredacted view's coordinates. `fullContent` views are scanned as the complete selected view; above the scanner's 10,000,000-byte limit, `contentSecurityLimit` provides a smaller-source-range alternative when possible, otherwise an explicit terminal limit. File totals unavailable due to access or resource limits are identified as unavailable. A full-content view over 50000 bytes supplies executable bounded recovery.

```bash
localFetch(path="/ABS/repo/src/index.ts", startLine=1, endLine=80)
localFetch(path="/ABS/repo/README.md", matchString="Configuration", contextBytes=256, chunkType="bytes", chunkSize=1024)
localFetch(path="/ABS/repo/src/index.ts", minify="symbols")
```

---

### `astTopology`

`astTopology` is a beta tool: it needs `OCTOCODE_BETA=true` (or `local.beta: true`); without it MCP omits the tool and the CLI explains the gate.

Scope admission: without an explicit `maxFiles`, a root with more than 5,000
parseable files is refused before parsing (`ast.graph.scopeTooBroad`). The
error message lists admissible package directories, and the row offers
`next.narrowScope` for the largest one and `next.expandScan` (explicit
`maxFiles`) to opt in to the full scan.

The `coverage` object separates parser inventory from module-linking support.
It reports language coverage, resolved, external, and non-code (`imports.nonCode`:
JSON, styles, assets) import counts, unresolved internal imports, unsupported
linking, and parse-recovery diagnostics. These gaps lower `confidence` and are
listed in `completeness.coverageGapReasons`; they do not set `truncated` or
`terminalLimit`, which mark only real scope cuts.
A result with unresolved internal imports is never reported as complete, so an empty dependency result under a subdirectory root that cannot resolve its imports is not absence.
Inspect coverage before interpreting an empty dependency or cycle result.
By default coverage carries counts only: `coverage.diagnosticCounts` and import
counts describe the full scan, and `completeness.diagnostics` stays `pageable`
while rows exist. Follow `next.nextDiagnostics` (`diagnosticPage:1`) to
retrieve the rows, 25 per page by default (`diagnosticPageSize` up to 100).
Rows with the same code and message are grouped into one row whose `files`
lists each `path[:line]`. The snapshot token prevents combining different
diagnostic inventories; if diagnostics change, follow `next.restartDiagnostics`.
Diagnostic pagination and graph-result pagination are independent.

A scan rooted below its package (`packages/app/src`) still reads the nearest
`package.json` above the root, up to a `.git` boundary, so `#` subpath imports
and package exports resolve. `dependents` also lists files that use the
target's items through a module re-exporting them (`pub use notify::Notify`,
`export { x } from './t'`, `export *`); those rows carry `reexportVia` naming
that module, and importers of other items from the module are not listed.

`astTopology` is the contract surface (MCP and CLI, validated input, `next.*`
continuations); `octocode graph ingest|query` is the CLI power surface over the
same graph builder (persisted snapshots, symbol-level callers/callees/impact,
issue detectors). Their `deps`/`dependents`/`path`/`cycles` file sets agree on
the same tree, apart from the `reexportVia` rows above and Rust `mod`
declarations, which `graph` models as containment rather than imports.

Rust analysis defaults to `rustWorkspace: "syntax"`, which uses explicit module
declarations and supported literal `#[path]` attributes. Set
`rustWorkspace: "cargo"` to inspect Cargo target roots and dependency aliases
with the host Cargo executable. This opt-in mode runs offline metadata discovery
without compiling the project, with a five-second execution budget and a
32-MiB output bound. Include the Cargo manifest within the scan root. Missing
tools, excluded targets, conditional dependencies, cfg, and macro expansion
remain explicit coverage gaps when the analyzer cannot resolve them.

Declaration IDs identify scoped source occurrences; unresolved call references
are not proof of symbol identity. Value-reference counts are conservative
retention evidence and still require LSP confirmation for deletion decisions.

One bounded repository graph provides seven analyses: `dependencies`, `dependents`, `path`, `reachability`, `cycles`, `deadCode`, and `drift` (compares `path`, the head, against an absolute `baseline` root). Import edges come from native syntax facts. Traversal and path results report exact `edgeKinds`: `static-import`, `type-import`, `dynamic-import`, `named-reexport`, `star-reexport`, `type-named-reexport`, `type-star-reexport`, `commonjs-require`, `create-require`, `python-import`, `go-import`, `java-import`, `java-same-package` (a same-package class use; no import, so no `importLine`), `rust-module`, `rust-use`, and `c-include`. Only `static-import`, `dynamic-import`, `named-reexport`, `star-reexport`, `commonjs-require`, `create-require`, and `python-import` edges are runtime import candidates; type-only, Go, Java, Rust module/use, and C include edges do not establish runtime import cycles.

Cross-file resolution covers JavaScript/TypeScript ESM and binding-safe CommonJS, Rust modules, bounded Python absolute and relative imports, and quoted relative C/C++ includes. Literal CommonJS loads link only when `require`, `module.require`, or an imported `createRequire(import.meta.url)` binding is not shadowed or reassigned. Dynamic and ambiguous loaders remain explicit diagnostics. Python wildcard and ambiguous package-attribute imports remain diagnostics, as do C/C++ system and macro includes. Data, style, and asset imports (including `package.json`) are counted as `imports.nonCode`, not linked or reported as unresolved. Namespace-style imports conservatively retain target exports during dead-code analysis.

Dependency traversal items also carry `immediateDominator`, `topologicalLayer`, `inboundCount`, `importLine`, and `transitiveEdge` (a condensation-DAG edge that another directed path already covers). Cycle results distinguish runtime import candidates (`runtimeCycle`) from other topology SCCs, expose condensation metadata, and return deterministic directed witnesses in `cycleEdges` and `runtimeCycleEdges`; every witness edge includes `from`, `to`, and `edgeKinds`. Native facts also contain `call` and `contains` relations, but the public operations don't project those symbol-level edges. `deadCode` results are candidates, not deletion proof.

Inside a reachable file, `deadCode` keeps an export live when an import or re-export chain consumes one of its public names (`import foo from` consumes `default`), or when it is reachable over same-file call and containment edges from a live declaration, a module-level call, or a declaration that escapes as a value. A value escape is a syntax-aware reference other than the declaration itself, an export clause, or a call target; comments and string literals never count. JS/TS counts the resolved references of each declaration's own symbol, so a same-named local elsewhere does not keep it live; other languages count identifier tokens by name. `unreferenced-export` rows name the basis in `viaHeuristic`: `reexport-chain`, `semantic-references` (JS/TS), `syntax-references`, or `qualified-path-name`. `qualified-path-name` marks a Rust export kept live only because a `module::name` call names it without resolving to its file. A qualified call from a live caller that resolves through the calling file's `use`/`mod` binding credits the export exactly. Callers are keyed by declaration identity, so a method `run` and a function `run` do not share liveness, and an uncalled private caller does not keep its callees live. Rows for exports renamed at the export site carry `exportedAs`. When graph extraction for a file hits its deadline, the facts gathered so far are kept and the file carries a `graph.traversal.deadlineExceeded` diagnostic, so a missing edge there is not evidence of absence.

#### Best for

- Tracing forward dependencies or reverse dependents to a bounded depth.
- Finding the shortest directed import path between two files.
- Finding mandatory dependency chokepoints, topological layers, and redundant edges.
- Classifying entrypoint reachability and finding strongly connected import cycles.
- Finding repository-wide dead-export candidates and dead clusters in one pass. A dead cluster is a strongly connected set of mutually importing, unreachable files; the files don't necessarily call one another.

Use `astTopology` to discover repository-scale file topology and candidate reachability. Use `lspSearch` with `references`, `callers`, or `callees` to prove the identity and semantic connections of one known symbol. A graph edge proves that one file syntactically imports or re-exports another; it doesn't prove which binding is used.

#### Key parameters

| Parameter | Description |
|-----------|-------------|
| `analysis` | Required: `dependencies`, `dependents`, `path`, `reachability`, `cycles`, `deadCode`, or `drift`. |
| `path` | Absolute scan root. Required for `cycles` and `drift`; otherwise it may be omitted when an absolute `file`, `target`, or entrypoint implies it. |
| `baseline` | Absolute baseline root for `drift` (required there). |
| `languageGlobs` | Optional root-relative AST parser map, e.g. `{"cpp":["include/**/*.h"]}`. Also accepted by directory `astSearch` symbols. Does not configure clangd; use compile commands or `.clangd` for C++ header LSP parsing. |
| `file` | Traversal start file, relative to `path`; required for `dependencies`, `dependents`, and `path`. |
| `target` | Destination file for `path` (required there). |
| `depth` | Traversal depth for `dependencies` and `dependents`. Default 1, max 50. |
| `entrypoints` | Roots for `reachability` and `deadCode`; omit to detect `package.json` `main`, `exports`, and `bin`. |
| `includeTests` | Treat tests as roots for `reachability` and `deadCode`. Default `true`. |
| `excludeDir` | Directory names to prune, added to the default prune shared with structureSearch and astSearch (dependency, build, cache, and credential directories such as `node_modules`, `target`, `.git`, `secrets`; `.github`-style config stays visible). |
| `maxFiles` | Cap on files scanned. Max 50000. The scan stops and warns past this bound. |
| `limit` | Result cap before pagination. Max 5000. |
| `page` | Result page. Max 1000. |
| `pageSize` | Results per page. Max 100. |
| `diagnosticPage`, `diagnosticPageSize`, `diagnosticSnapshot` | Coverage-diagnostic pagination, independent of `page` (see above). |

Results never dump the complete graph: the result list is paginated, and SCC/dead-cluster rows list their member `files`. A `path` result returns the full fewest-edge path as `files` and `edges` (each edge with `edgeKinds` and `importLine`), or `found:false`.

#### Graph result interpretation

| Signal | Interpretation | Required follow-up |
|--------|----------------|--------------------|
| `cycleEdges` | A deterministic directed witness through one reported SCC. Each edge names `from`, `to`, and its syntactic `edgeKinds`. | Read every reported edge exactly; SCC member order alone is not a valid cycle path. |
| `runtimeCycleEdges` | A directed witness using only runtime import candidates (see the edge kinds above). | Confirm the imported bindings and initialization behavior before claiming a runtime defect. |
| Topology-only SCC | Files are mutually connected in the full graph, but no cycle remains among runtime import candidates. This includes type-only and Rust module cycles. | Report it as topology or coupling evidence, not as a module-loading cycle. |
| `transitiveEdge: true` | A condensation-DAG edge for which another directed path already connects the same components. It can indicate redundant architectural wiring. | Check re-export contracts, side effects, public API intent, and symbol usage before calling an import duplicate. |
| `immediateDominator` | The file every directed route from the selected root must cross to reach this item. | Use it to prioritize chokepoints; do not infer symbol ownership from file topology. |

The graph assigns no weights to edges. `path` therefore uses breadth-first search to return the fewest-edge directed import path, not Dijkstra's weighted shortest-path algorithm. A syntactically redundant edge can still be semantically necessary because it imports a value for side effects, preserves a public barrel contract, or selects a different binding.

#### Examples

```bash
astTopology(analysis="dependencies", path="/ABS/repo", file="src/index.ts", depth=2)
astTopology(analysis="cycles", path="/ABS/repo", pageSize=20, limit=100)
astTopology(analysis="deadCode", path="/ABS/repo", entrypoints=["src/index.ts"], includeTests=false)
```

For a cycle, read the exact imports named by `cycleEdges`; use `runtimeCycleEdges` when investigating loading behavior. Verify a dead-code or transitive-edge candidate with `lspSearch` before removing it.

---

### Local workflows

The discover → search → read → prove flow is owned by the [research manifest](OCTOCODE_RESEARCH_MANIFEST.md#local-workflow).

---

### Local tool rules

1. Use `structureSearch(operation:"tree")` or `structureSearch(operation:"files")` before reading when the file is unknown.
2. Use `localSearch( resultView:"files")` for fast discovery when match bodies are not needed.
3. Use `localSearch` with `contextLines` before opening a large file.
4. Use `localFetch` with `matchString`, `startLine`/`endLine`, or `minify:"symbols"` instead of `fullContent` for large files.
5. Use pagination fields when a response advertises `hasMore=true`.

---

### Response shape

- Tool results use the shared `results[]` row envelope; each row may contain `index`, `status`, `meta`, and `data`. Tool-specific payloads own their pagination and hints; see [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md) for the common rules.
- `localSearch` returns lexical matches; `structureSearch` returns directory-outline or file-metadata payloads; `astSearch` returns structural, syntax-tree, or symbol payloads; `astTopology` returns graph-analysis payloads.
- `localFetch` returns file slices only — not directory listings.

### Anti-patterns

| Anti-Pattern | Better Approach |
|--------------|-----------------|
| `fullContent=true` on large files | Use `matchString`, line range, or `minify:"symbols"` |
| Search without scoping dirs | Use `excludeDir` to skip generated/vendor folders |
| Regex for exact literals | Use `regex:"literal"` |
| Combining mutually exclusive flags | Pick one extraction mode |

**Parallelism:** independent queries run in parallel (batch limit: 5 per call). Sequential dependencies (`structure → search → read`) stay sequential.

---

### `astRewrite`

Preview structural ast-grep rewrites before applying them. Preview is the default and does not apply proposed edits, but it may recover an interrupted transaction. Structural matching distinguishes executable syntax from matching text in comments and strings.

This is a beta feature, disabled by default. Set `OCTOCODE_BETA=true` (or
`local.beta: true`) to enable both preview and apply.

| Field | Meaning |
| --- | --- |
| `path` | Source file or directory; relative paths resolve against `WORKSPACE_ROOT`. |
| `langType` | Optional parser. Omitted: a file's extension, or the one grammar under a directory that compiles the rule (several return `ast.rewrite.language_required` naming them). `next.apply` pins the resolved value. |
| `ruleKind` | Optional; inferred from the fields (`pattern`+`rewrite` or `rule`+`fix`). |
| `pattern`, `rewrite` | Match and replacement for the `pattern` form. |
| `rule`, `fix`, `constraints`, `utils`, `transform` | ast-grep rule (YAML string, bare or a rule file's `rule:`, or the rule object; both preview identically) and fix for the `rule` form; inspect the live schema. |
| `include`, `exclude`, `defaultExcludes` | Optional file filters. |
| `maxFiles`, `maxMatches` | Scan bounds; defaults are 2,000 files (max 50,000) and 10,000 matches (max 100,000). |
| `page`, `pageSize`, `snapshot` | Preview pagination (`pageSize` default 100, max 1000); copy executable continuations and their snapshot. A page lists only the files its matches touch, and each file's `patch` holds only the hunks of that page's matches (`patchMatchCount` of `matchCount` when the file spans pages); `beforeHash` and the final page's `next.apply` still cover the whole file. Match rows are `{id, path, line}` with a 16-hex id prefix (the patch shows text and replacement); `debug:true` restores full rows, `afterHash`, `patchBytes` and `absolutePath`. |
| `apply` | Defaults to `false`. Applying requires the unchanged preview snapshot and non-empty `expectedHashes`; a complete preview returns `next.apply` with both filled in. |
| `expectedHashes`, `selectedMatchIds` | Preview SHA-256 hashes for exactly the selected files. Match ids may be any unique prefix of at least 12 hex digits. With explicit match selection, omit unselected-file hashes. A stale or missing selected-file hash aborts the apply. |
| `postconditions` | 1–10 checks, `{kind:"remainingMatches", equals:n}`, evaluated in the staged rewritten files before commit. |

```bash
node packages/octocode/out/octocode.js astRewrite '{"goal":"<what to find>","reasoning":"<why>","path":"/ABS/repo/src","pattern":"console.log($A)","rewrite":"logger.info($A)"}'
```

Match `range.start`/`range.end` lines are one-based; columns are zero-based UTF-16 code units (an emoji counts 2). `range.byteOffset` is the UTF-8 byte span.

Use the preview's identities and diff to review the change. Inspect `scheme astRewrite --view variants` for the current operation constraints before applying. A successful preview alone does not verify applied behavior.

Apply returns the complete selected-match receipt in one page. A committed transaction
may include cleanup warnings; an error may report incomplete recovery. Inspect those
outcomes before retrying instead of assuming every error restored the original files.

## LSP tools reference

This is the canonical reference for Octocode's semantic code-intelligence operations. LSP is the protocol layer behind these operations; structural AST search is exposed by `astSearch`.

Octocode exposes **one** public semantic tool:

| Tool | Use it for |
|------|------------|
| `lspSearch` | Definitions, references, callers, callees, bidirectional call hierarchy, hover, document, and workspace symbols, type definitions, implementations, type hierarchy, and diagnostics. |

Semantic operations are local-only. Local tools default on for both CLI and MCP; set `ENABLE_LOCAL=false` to disable them. LSP needs a file that exists on disk. Use `localSearch` first when you need a symbol `lineHint`; `astSearch(operation:"match")` can provide AST-derived anchors before LSP proves symbol identity.

For external repos: clone first with `ghCloneRepo` (set `sparsePath` for a subtree), then use the returned `localPath` as the `uri` prefix for `lspSearch`. The path is always absolute and immediately valid.

### Workflow

1. Search with `localSearch` or `astSearch(operation:"match"|"symbols")`, then read the observed source with `localFetch` to verify the exact symbol spelling and line.
2. Query `lspSearch` with `uri`, `operation`, and either `symbolName` plus a 1-based `lineHint` or a zero-based UTF-16 `position`.
3. Page large symbol or call-flow results by executing `next.nextPage` unchanged;
   pages after the first require its snapshot token.
4. Run project lint, typecheck, and tests before claiming risky changes are fully verified.

Reference counts describe the language server's returned set, not guaranteed
whole-program coverage. `payload.coverage` states that scope. References
recovered through an aliasing import (not reported by the server) carry
`source: "recoveredAlias"`, and `payload.recoveredAliasReferences` counts them.
References found by re-querying the server from a verified importer's import
anchor carry `source: "recoveredImporter"`. When the importer scan is capped
(`importerScanCapped`) or fails (`importerScanFailed`), or a TypeScript file has no project configuration
(`inferredProject`), the row is partial, `payload.coverage.exhaustive` is
`false`, and a name-anchored request carries `next.textSearch`, a `localSearch`
for the textual uses the server cannot see. Inspection or verification gaps
remain explicit in coverage rather than becoming invented references.

Exact `position` anchors pass directly to the language server, including positions
in quoted property names or module paths. `resolvedSymbol.name` is an optional identifier hint for discovery.

### `lspSearch`

Required fields:

| Field | Required | Notes |
|-------|----------|-------|
| `uri` | Required for anchored, document, and diagnostic operations; one of `uri` or `workspaceRoot` for `workspaceSymbol` | Absolute local file path. For `workspaceSymbol`, `uri` selects one language server. |
| `operation` | Required for document, diagnostic, and workspace-symbol requests; anchored requests default to `definition` | One of the documented semantic, document, diagnostic, or workspace-symbol operations. Include it in durable examples and continuations. |
| `symbolName` | Required for name-anchored operations and `workspaceSymbol` | Exact symbol text at the target line; omitted with `position`. |
| `lineHint` | Required for name-anchored operations | 1-based line number from search results. Use `position` instead for an exact zero-based UTF-16 position. |

Optional fields:

| Field | Notes |
|-------|-------|
| `orderHint` | Disambiguates repeated symbol text on the same line. |
| `position` | Alternative anchor for semantic symbol operations: zero-based UTF-16 `{line, character}`. Use either `position` or `symbolName` + `lineHint`, never both. |
| `workspaceRoot` | Overrides automatic project-root detection. |
| `rustContext` | Explicit rust-analyzer build context. Requires a `.rs` URI, including for `workspaceSymbol`. See [Rust build context](#rust-build-context). |
| `contextLines` | Source lines around each returned location, 0–100. Keep `0` unless previews are needed. |
| `page` | Result page copied from an executable semantic continuation. |
| `pageSize` | Semantic items per page. Defaults to `40`. Max `100`. |
| `snapshot` | Content-addressed result-set token copied from `next.nextPage`. Omit on page 1; required on later pages. |
| `depth` | Call- and type-hierarchy depth, max 20; `1` (default) returns direct edges. Deeper walks are breadth-first and capped (see the call-flow rules below). |
| `includeDeclaration` | For `references`; defaults to `true`. |
| `groupByFile` | For `references`; adds per-file rollups. |

Semantic types:

| `operation` | Best for | Output |
|--------|----------|--------|
| `definition` | Jumping from usage/import to declaration. Unresolved provider locations are preserved unchanged. | `payload.kind="definition"`, `locations[]`. |
| `references` | Affected references for functions, types, variables, constants, and classes. | `locations[]` (or, with `groupByFile`, `byFile[]` of `{path, references, lines}` instead), `totalReferences`, `totalFiles`. |
| `callers` | Static incoming calls to a callable symbol. | `payload.items[]` of `{from, fromRanges, level, via?}`, pagination. |
| `callees` | Static outgoing calls made by a callable symbol. | `payload.items[]` of `{to, fromRanges, level, via?}`, pagination. |
| `callHierarchy` | Bidirectional call-flow snapshot. | Incoming (`from`) then outgoing (`to`) items in one paginated list. |
| `hover` | Quick type/signature/docs from the language server. | `payload.hover`; a `null` hover is `empty` with category `noHover`. |
| `documentSymbols` | File outline and symbol inventory. | Compact `symbols[]`, `summary.kinds`, pagination. |
| `typeDefinition` | Declared type behind a symbol. | `locations[]`. |
| `implementation` | Concrete implementation behind an interface/abstract symbol when the server supports it. | `locations[]`. |
| `workspaceSymbol` | Symbols reported by one language server for a workspace. Provide `uri` to select the language and project; a `workspaceRoot`-only query opens one representative source (tsconfig `include` root, then `src/`). This operation does not merge results from every language server. | `payload.items[]` of `{name, kind, containerName?, uri, displayRange}`, pagination. |
| `supertypes` | Supertypes (recursive with `depth`) when the server advertises type hierarchy. | `payload.items[]` of `{name, kind, detail?, uri, displayRange, level, via?}`, or `lsp.capabilityUnavailable`. |
| `subtypes` | Subtypes (recursive with `depth`) when the server advertises type hierarchy. | Same item shape as `supertypes`. |
| `diagnostic` | Pull diagnostics when the server advertises a pull-diagnostic provider; otherwise the bounded push-diagnostic cache. | Diagnostics or typed `empty` (`noDiagnostics`, `diagnosticsNotPublished`). |

Semantic responses use this envelope (minimal output drops `operation`, `uri`, `lsp`, and `meta`; add `debug: true` to see them, for example to check `lsp.source`):

| Field | Meaning |
|-------|---------|
| `operation` | Requested semantic type. |
| `uri` | Resolved local file path. |
| `resolvedSymbol` | Symbol anchor for symbol-based requests. |
| `lsp` | Server availability and provider/source metadata (debug-only). |
| `meta.evidence` | Confidence for the bulk result (debug-only). |
| `meta.diagnostics` | Typed partial-state and terminal-limit information (debug-only; partial markers stay on the row). |
| `summary` | Agent-readable totals for symbol and call-flow requests. |
| `payload` | Typed semantic payload. |
| `pagination` | Native semantic pagination for symbol and call-flow requests. |
| `rustContext` | Normalized requested Rust settings and their fingerprint, when supplied. This field also remains visible on native document-symbol results. |
| `next` | Executable reads, searches, completeness checks, or pagination requests. |

Empty semantic payloads use `payload.kind="empty"` with a machine-readable
`category`: `noLocations` (no locations, calls, symbols, or types), `noHover`,
`noDiagnostics`, `diagnosticsNotPublished`, or `unsupportedOperation`, and the
row has `status: "empty"` (CLI exit `1` when every row is empty). Inspect the
typed category, not only the exit code, to tell these apart.

If the language server is still loading the project when the request runs, the
row is partial with `partialReasons: ["languageServerIndexing"]`, a warning,
and `next.retry`. Zero results in that state do not establish absence; run the
retry after the server finishes indexing. Octocode does not automatically retry
every empty result.

Paginated semantic results fingerprint the canonical query plus the complete
result set into `pagination.snapshot`. Follow `next.nextPage` unchanged; it
carries the snapshot. If the result set changed between requests, or a later
page omits its snapshot, the row is an error with `errorCode:
"lsp.snapshot.changed"`, no page items, and `next.restart` (page 1, no
snapshot): discard previously collected pages and run it. Tokens validate a
recomputed result set across processes; they do not retain historical rows.

Coordinates: `position` input is zero-based UTF-16 (LSP). Every emitted
coordinate is one-based — lines and UTF-16 code-unit columns (LSP
`character + 1`): `resolvedSymbol.foundAtLine`/`foundAtCharacter`, location
and `payload.hover` `displayRange {startLine, startCharacter, endLine}`, call/type-hierarchy and
workspace-symbol `displayRange`, call `fromRanges[]` (same shape),
`via.line`/`via.character`, and document-symbol `line`/`character`/`endLine`.
To reuse an emitted point as `position`, subtract 1 from line and character.
Hierarchy-node `displayRange` starts at the symbol name (usable as `lineHint`)
and ends with the declaration.

Call-flow and type-hierarchy items are edges: each carries `level` (`1` =
direct edge of the anchor), and items with `level > 1` also carry
`via {name, uri, line, character}`, the parent node they connect to, so the
tree is reconstructable from the flat list. Incoming calls put call-site
`fromRanges` in the caller's file; outgoing calls put them in the `via` (or
anchor) file. Repeated (parent, node) pairs merge into one edge with all call
sites. The walk is breadth-first: nodes are identified by canonical path and
selection range, each expands once at the shallowest level it is reached, and
a node reached again (cycle or diamond) keeps its edge without re-expanding.
Caps are enforced per walk: depth 20, 200 nodes, 50 results per node. At the
node cap the row has `payload.truncated: true`, `isPartial: true`,
`partialReasons: ["hierarchyNodeLimit"]`, and an executable
`next.continueWalk` that re-anchors on the first parent whose children were
dropped with the remaining depth. Every other such parent gets its own
`next.continueWalk2`…`next.continueWalkN`, and `payload.unexpandedParents`
lists them all (`name`, `uri`, one-based `line`/`character`,
`remainingDepth`). `callHierarchy` walks both directions and keeps one combined
list: each unexpanded parent also carries `direction` (`incoming` or
`outgoing`), and its continuation runs `callers` or `callees` so it resumes only
that direction. At the fan-out cap it has
`partialReasons: ["hierarchyFanOutLimit"]` and `terminalLimit: true` (use
`references` with pagination for the full list). Items outside the allowed
read roots are omitted with a warning. A provider failure after some results
is marked `callHierarchyExpansionFailed` / `typeHierarchyExpansionFailed` with
`next.retry`; items recovered from a references query carry
`source: "recoveredFromReferences"`. Use `contextLines>0` only when source previews are useful.

Request failures are typed: a path outside the allowed roots is
`pathOutsideAllowedRoots` (other path failures stay `fileAccessFailed`); invalid fields are
`lsp.invalidQuery`; a request timeout is `lsp.timeout` (`retryable: true`); a
server that exited mid-request is `lsp.serverCrashed` (`retryable: true`); a
method the server does not implement is `lsp.capabilityUnavailable`; any other
failed server request is `lsp.requestFailed` (`retryable: true` except for
rejected parameters); no configured or startable server is
`lsp.serverUnavailable`. An explicit `position` past the end of the document
or line is `lsp.anchorUnresolved`. Recovery `next.readFile` reads the
`symbolName` match windows when a name anchored the request; empty diagnostic
results carry no read recovery.

### Rust build context

Supply `rustContext` to select the Rust configuration used for a semantic query:

| Field | Default in an explicit context | Meaning |
|-------|-------------------------------|---------|
| `features` | `[]` | Cargo feature names, or `"all"`. Names are deduplicated and sorted for identity. |
| `noDefaultFeatures` | `false` | Disable the package's default Cargo features. |
| `target` | Unset | Cargo target triple; an unset value uses the server's Cargo environment. |
| `cfgs` | `[]` | Additional rust-analyzer cfg settings, such as `"custom"`, `"mode=fast"`, or `"!custom"`. |
| `buildScripts` | `false` | Allow rust-analyzer to run build scripts and load their cfg and generated-source results. |
| `procMacros` | `false` | Allow procedural macro expansion. Requires `buildScripts: true`. |

For a Rust call found at line 5, query the definition with the `selected` feature:

```bash
octocode lspSearch '{"goal":"<what to find>","reasoning":"<why>","uri":"/ABS/repo/src/lib.rs","operation":"definition","symbolName":"selected","lineHint":5,"rustContext":{"features":["selected"]}}'
```

Replace the path, symbol, and line with an anchor from `localSearch`. An explicit
empty context (`"rustContext": {}`) disables build scripts and procedural macros;
it also disables rust-analyzer's implicit test cfg and check-on-save. Omitting
`rustContext` preserves the configured server defaults, which can enable build
scripts or procedural macros. Enabling these providers permits workspace code
execution; the context is not a sandbox.

The tool uses rust-analyzer for cfg-selected definitions, declarative macro
expansion, and enabled build-script or procedural-macro results. The syntax graph
from `astSearch` remains a separate source analysis and does not acquire
compiler expansion through this option. A disabled provider can explain an empty
answer even when the declaration is generated during a normal Cargo build.

Different effective server settings use different pooled clients. The returned
`rustContext.fingerprint` identifies the normalized requested settings, and semantic
pagination includes that identity. Follow continuations unchanged; changing the
context requires restarting pagination. This fingerprint does not pin source
files, Cargo configuration, the toolchain, environment changes, or generated
artifacts. It is not a reproducible-build identifier. See the
[engine lifecycle contract](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#rust-context)
and [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html).

### Root selection

If `workspaceRoot` is omitted:

1. The file's directory and its ancestors are walked upward to the nearest project marker, such as `package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, or `pyproject.toml`; that directory is the root even inside `WORKSPACE_ROOT`.
2. If no marker exists, the process cwd is used.

### Native compared with server fidelity, and the no-fallback contract

`documentSymbols` has a **native fallback** (oxc for JS/TS, Markdown heading outline) that runs only when no language server is available; a server always wins when one exists:

| Source (`lsp.source`) | When | Fidelity |
|-----------------------|------|----------|
| `lsp` | A language server is available | Type-aware, cross-file. |
| `native` / `markdown` | `documentSymbols` only | Syntax-only outline; no type inference. |

Every **other** semantic operation — `references`, `definition`, `hover`, `callers`/`callees`/`callHierarchy`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`/`subtypes`, `diagnostic` — requires a real server. When no server is available octocode **does not fall back to a syntactic guess**: it returns `status:"error"` with `errorCode:"lsp.serverUnavailable"` and a message directing you to lexical `localSearch` or structural `astSearch` + `localFetch`. There is no same-file-only `references` path: a partial answer that silently omits cross-file usages would be a trap. See [LSP server lifecycle](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md).

### TypeScript backends

The TS/JS server resolves in this order:

1. `OCTOCODE_TS_SERVER_PATH` — explicit override (args auto-selected: `--lsp -stdio` if the path is `tsgo`, else `--stdio`).
2. **`typescript-language-server`** — the stable zero-config default.

Octocode does not automatically prefer `tsgo` merely because it is on `PATH`;
select it explicitly until the held-out operation matrix establishes parity.

For the bundled default, Octocode first honors an executable
`typescript-language-server` already available on `PATH`. If the command is not
available, the resolver runs `node_modules/typescript-language-server/lib/cli.mjs`
through the current Node executable, looking in Octocode's own install tree
first, then in the workspace's `node_modules` **only when the workspace is
trusted**, then in the directory Octocode was started in. The workspace is
trusted when `OCTOCODE_TRUST_PROJECT_LSP_CONFIG=true` or when it is the start
directory (or inside it), so a scanned checkout, such as a clone, cannot supply
the executable. Cloned and external workspaces still work through Octocode's
own install or the start directory.

Definition locations remain language-server output; Octocode does not rewrite
import targets with regular expressions.

### Language servers

TypeScript and JavaScript use `typescript-language-server`; JS/TS also has the
server-free document-symbol path above. Built-in routes cover JavaScript,
TypeScript, Python (`pylsp`), Rust (`rust-analyzer`), Go (`gopls`), Java
(`jdtls`), C, C++, and CUDA (`clangd`), C# (`csharp-ls`), and Scala (`metals`). Rust and C/C++ support
managed downloads; other routes resolve installed host or user-provided executables.

Built-in servers start headless and read-only by default (user arguments and
options still win): rust-analyzer runs no build scripts, proc-macros, or
`cargo check`; clangd runs with `--background-index=false --clang-tidy=false
--log=error --pch-storage=memory`; jdtls keeps its `-data` directory under the
Octocode home (never in the repository) with `java.autobuild.enabled:false`;
Metals starts with `isHttpEnabled:false`. Each server is capped at
`maxMemoryMb` (default 4096, `0` disables); on macOS an RSS watchdog enforces
the cap and a server over it fails with "language server exceeded memory cap".

Explicit `position` lines follow the LSP line-break rule: `\r\n`, `\n`, and a
lone `\r` each end a line. A location in an allowed file whose content cannot
be read (too large, not UTF-8) is kept, with `content` stating why it is
unavailable.

Common environment overrides:

| Variable | Language |
|----------|----------|
| `OCTOCODE_TS_SERVER_PATH` | TypeScript/JavaScript (bundled — override only if needed) |
| `OCTOCODE_PYTHON_SERVER_PATH` | Python |
| `OCTOCODE_GO_SERVER_PATH` | Go |
| `OCTOCODE_RUST_SERVER_PATH` | Rust |
| `OCTOCODE_JAVA_SERVER_PATH` | Java |
| `OCTOCODE_CLANGD_SERVER_PATH` | C/C++ |
| `OCTOCODE_CSHARP_SERVER_PATH` | C# |
| `OCTOCODE_SCALA_SERVER_PATH` | Scala |

#### Custom / bring-your-own servers

To support another extension, or to replace a built-in server, register it in a
JSON config. Octocode loads the
configuration in this precedence order:

1. `$OCTOCODE_LSP_CONFIG` (explicit file path)
2. `<workspace>/.octocode/lsp-servers.json` (per-project)
3. `~/.octocode/lsp-servers.json` (per-user)

The file maps a file **extension** to a launch spec; a custom entry overrides the built-in spec
for that extension:

<!-- example: configuration -->
```jsonc
{
  "languageServers": {
    ".php": { "command": "intelephense", "args": ["--stdio"], "languageId": "php" }
  }
}
```

`command` and `languageId` are required; `args` (default `[]`) and `initializationOptions`
(passed verbatim in `initialize`) are optional. With the config present, the server can answer the
semantic operations it advertises; without it the extension is unsupported and semantic ops return
`lsp.serverUnavailable` (→ fall back to `localSearch`). See
[`LSP_SERVER_LIFECYCLE.md`](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#custom-configuration).

### Examples

Definition:

<!-- tool: lspSearch -->
```json
{
  "uri": "/workspace/src/run.ts",
  "operation": "definition",
  "symbolName": "printSchema",
  "lineHint": 133
}
```

References grouped by file:

<!-- tool: lspSearch -->
```json
{
  "uri": "/workspace/src/run.ts",
  "operation": "references",
  "symbolName": "isOctokitDeprecation",
  "lineHint": 27,
  "includeDeclaration": true,
  "groupByFile": true
}
```

Paginated call flow:

<!-- tool: lspSearch -->
```json
{
  "uri": "/workspace/src/run.ts",
  "operation": "callHierarchy",
  "symbolName": "printSchema",
  "lineHint": 133,
  "pageSize": 5,
  "page": 1
}
```

Diagnostics:

<!-- tool: lspSearch -->
```json
{
  "uri": "/workspace/src/run.ts",
  "operation": "diagnostic"
}
```

Workspace-symbol search:

<!-- tool: lspSearch -->
```json
{
  "operation": "workspaceSymbol",
  "symbolName": "ToolConfig",
  "workspaceRoot": "/workspace"
}
```

## Semantic assessment reference

`clasify` accepts one complete matrix directly or one to five matrices in `queries[]`. Each matrix carries its own required `goal` and `reasoning`, 1–25 `resources` (supplied `{value}` state or one unread `{tool,query}` read request), and 1–25 `questions`, and results are keyed by query, resource, and question IDs. Inspect `octocode scheme clasify --view query` before hand-authoring a call, and follow an executable `next.clasify` unchanged. Question types, cell limits, result fields, credentials, and the research workflow are owned by [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md).

---

## Clone and local tools workflow

Use `ghCloneRepo` to bring remote source into local AST, search, and LSP tools. Clone is CLI-only and requires local access plus persistent storage (see [availability](#internal-external-and-hybrid-tools)); inspect the live catalog.

| Need | Tool |
|---|---|
| Read one remote file | `ghGetFileContent` |
| Browse remote paths | `ghStructure` |
| Inspect a subtree locally | `ghCloneRepo` with `sparsePath` |
| Analyze cross-file semantics | `ghCloneRepo`, then `lspSearch` |

Use the returned `location.localPath` for local queries. Preserve the resolved revision and requested scope; a sparse checkout can omit dependencies required by LSP.

### Two clone modes

- **Full clone** — general exploration; LSP works best with full repositories. Omit `branch` to auto-detect the default.
- **Sparse fetch** — one package/directory of a large monorepo via `sparsePath`; much faster, but LSP cross-file resolution may be limited since not all files are present.

Both return `location.localPath` (absolute), `location.kind` (`repo`/`tree`), `source`, `cached`, `verified`, `complete`, `commitSha`, `resolvedBranch`, and `requestedPath` for sparse. Use `location.localPath` as the absolute `path`/`uri` for local queries. Clone directories use the lowercased owner and repository names. Sparse checkouts use a separate cache key (`{branch}__sp_{hash}__host_{hash}/`) and can coexist with a full clone. `complete` is relative to the requested scope, so a finished sparse clone reports `complete: true`.

```
ghCloneRepo(owner="microsoft", repo="TypeScript", sparsePath="src/compiler")
→ location.localPath = <octocode-home>/tmp/clone/microsoft/typescript/main__sp_a3f8c1__host_4dc11541e2f7a1d9  (kind: tree, complete: true)
```

### Step-by-step workflows

- **Browse a cloned tree:** `ghCloneRepo` → `structureSearch(operation="tree", path=localPath, maxDepth=2)`, drilling into subdirectories.
- **Deep analysis with LSP:** `ghCloneRepo` → `localSearch` for the symbol + `lineHint` → `lspSearch(operation="definition"|"callers", uri=localPath+"/file", symbolName, lineHint)`.
- **GitHub browse → local:** `ghStructure` to scout → `ghCloneRepo` → `localSearch` (full regex/type filters) → `lspSearch(operation="references")`.
- **Sparse monorepo package:** scout with `ghStructure` → `ghCloneRepo(sparsePath=...)` → `localSearch`/`structureSearch(operation="files")` within the subtree.

---

### Cache behavior

| Behavior | Details |
|----------|---------|
| **Materialization TTL** | Clone entries use 24 hours by default (configurable through `OCTOCODE_CACHE_TTL_MS`) |
| **No response cache** | `ghSearchHistory` and `ghGetHistoryItem` bypass caching on purpose (history is mutable). `ghSearchRepo` has none; `ghSearchCode` and `artifactSearch` keep only per-process memos that no later process sees |
| **Conditional cache** | `ghGetFileContent` and `ghStructure` retain response bodies and ETags for conditional refresh; stale bodies can remain available for up to 24 hours |
| **Response marker** | With `debug: true`, a `ghGetFileContent` row whose files all came from its content cache includes `cache: 1`; no other tool sets it. Minimal output omits `cache`. The contract is identical in CLI and MCP output. |
| **Clone cache** | `ghCloneRepo` uses the clone/materialization cache |
| **Live tools** | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, and `lspSearch` read the workspace directly and don't cache tool results |
| **Location** | Use returned paths. Clone cache keys include ref, sparse scope, and host. Remote response L2 uses `<octocode-home>/tmp/response/` |
| **Identity** | File reads resolve an omitted branch; pass a commit SHA for reproducible reads. Clones accept branch, tag, or full commit SHA and return the actual HEAD as `location.commitSha` |
| **Sparse clones** | Separate cache: `{branch}__sp_{hash}__host_{hash}/` |
| **Coexistence** | Full clone and sparse clones of the same repository can coexist |
| **Cache hit** | Reuses a clone checkout only when its working tree is clean at the recorded HEAD; a modified cache is re-cloned. Inspect `verified` separately from scoped `complete`. |
| **Expired** | Owned entries are evicted when requested and by the shared 24-hour lifecycle |
| **Force refresh** | Set `forceRefresh: true` in the query to bypass cache and re-clone/re-fetch |
| **Periodic GC** | CLI tool-runtime bootstrap performs a persisted due-check once per process and exits without a timer. MCP performs the same bootstrap check, then uses an unreferenced deadline timer. Both use one persisted 24-hour marker. A cross-process lock prevents duplicate sweeps; a cleanup failure doesn't block startup. |
| **Cleanup scope** | Automatic maintenance removes expired entries only from owned clone, tree, response, and managed artifact roots. It preserves unrelated files under `tmp`. |
| **Response limits** | Response entries also obey configurable per-entry and total-disk limits. See [Cache storage and lifecycle](CONFIGURATION.md#cache-storage-and-lifecycle). |
| **Manual clear** | `octocode cache clear` deletes all cached responses; `octocode cache status` prints the cache home directory. There are no selective clear flags. |

---

### Path validation: why it works

Clones live under `<octocode-home>/tmp/...`, and both the path and execution-context validators automatically add the Octocode home as an allowed root, so a returned `location.localPath` is valid for all local and LSP tools even outside your shell workspace. LSP picks project context from the target file by walking up to the nearest marker (`package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, `pyproject.toml`); see [Root selection](#root-selection). Clone through the CLI with persistent storage; MCP does not expose `ghCloneRepo`. For TS/JS LSP, Octocode uses its bundled `typescript-language-server`; if unavailable, install it (plus `typescript`) on `PATH` or set `OCTOCODE_TS_SERVER_PATH`. LSP can read minified `.js`, but quality is far better on original source.

### Quick reference

| Action | Tool | Key parameter |
|--------|------|---------------|
| Clone repository / branch / one folder | `ghCloneRepo` | `owner`, `repo`, optional `branch` or `sparsePath` |
| Force re-clone | `ghCloneRepo` | `forceRefresh: true` |
| Browse / search / read / find in a clone | `localSearch`, `localFetch` | `path` = `localPath` |
| Definition / references / callers / callees | `lspSearch` | `uri` = file in `localPath` |
