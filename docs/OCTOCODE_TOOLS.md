# Octocode tools reference

Field-level reference for every tool exposed through MCP and the CLI. Schemas and descriptions live in `@octocodeai/octocode-core`; execution and native search/minify/security/LSP primitives live in `@octocodeai/octocode-native`. For MCP tool ratings, quality gaps, and the recommended agent workflow, see [`MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md`](MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md). For the exact active schema, run the compact form first; its `relations` list preserves mode-specific required and mutually exclusive fields:

```bash
npx octocode scheme <toolName> --compact
```

## Tool inventory

| Family | Tools |
|--------|-------|
| GitHub | `ghSearch`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` |
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
- [MCP tool quality and agent workflow](MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md)

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
| `queries` | Required outer field | Array of 1–5 queries for the **same tool**. Queries are independent and response rows retain their zero-based input `index`. Default execution concurrency is 3, so batching reduces round trips but does not create dependencies between rows. |
| `goal` | Optional per query | States the result the query should accomplish. It is agent-facing context, not a ranking instruction or proof of correctness. |
| `reasoning` | Optional per query | States why this query advances the goal. Use a short, decision-relevant sentence; do not put secrets, hidden chain-of-thought, or required runtime data here. |
| `responseCharLength` | Optional outer field | Limits the rendered whole-response text window to 1–50,000 characters. It does not replace a tool's own result pagination. When omitted, responses larger than `output.pagination.defaultCharLength` (default 50,000) are paged automatically; follow `responsePagination.next`. |
| `responseCharOffset` | Optional outer field | Continues a whole-response text window. Copy the returned executable `responsePagination.next` call instead of constructing an offset by hand. |

For ordinary tools, `goal` and `reasoning` are the shared query fields. All other fields belong to a specific tool variant. The schemas are strict: fields from different `operation` branches cannot be mixed, selector pairs such as `startLine`/`endLine` must be complete, and mutually exclusive selectors must not be combined. `clasify` has its own matrix contract and required query-level `reasoning` field.

### Schema discovery, variants, relations, and hints

```bash
# Catalog: canonical names and availability
npx octocode scheme

# Compact agent-facing schema: fields plus branch relations
npx octocode scheme localSearch --compact

# Full JSON Schema: nested selectors, defaults, limits, and descriptions
npx octocode scheme ghGetHistoryItem
```

The compact schema includes `variants` (when a branch applies, required fields, and a minimal example) and `relations` (cross-field rules that a flat field list cannot express). Runtime `hints` appear only on empty/error results: at most two distinct hints per result, each up to 160 characters. They offer recovery guidance and never prove absence or success. Successful results omit optional next-tool suggestions; executable pagination and completeness recovery calls remain available in `next`.

### Results, evidence, partial failures, and continuations

A batched response preserves input order:

```yaml
results:
  - index: 0
    meta:
      evidence:
        kind: lexical
        confidence: medium
    data: { ... }
  - index: 1
    status: error
    data:
      error: "..."
```

Row fields are `index`, optional `status`, optional `cache`, `meta`, and `data`. `status: empty` means no usable result (unsupported capability, unresolved anchor, or no match — interpret with tool diagnostics); `status: error` means that row failed. Neither is inferable from missing output. `meta.evidence` states what the result supports (provider-index, lexical, structural, syntactic-graph, exact-content, or semantic-LSP), each with different failure modes. Pagination is layered: **collection** (`page`/`pageSize`/`matchPage`/cursor/operation selectors), **content** (`chunkType`/`offset`/`chunkSize` for file readers; history uses character windows), and **whole-response** (outer `responseCharLength`/`responseCharOffset`). When partial, run the returned schema-valid `next.*` object — don't stop at a numeric cursor or treat a bounded first page as complete; an impossible continuation emits a typed terminal-limit diagnostic. Keep tokens scoped to their surface (operation-level `snapshot` vs whole-response `responseSnapshot`). The full envelope and continuation contract lives in [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md).

## Internal, external, and hybrid tools

"External" describes the data or provider boundary, not the MCP transport. All fourteen catalog entries use the same MCP and CLI contracts; availability gates can hide or reject an entry on a particular surface.

| Tool | Boundary | How it works |
| --- | --- | --- |
| `ghSearch` | External | Calls GitHub search/tree APIs to discover code, repositories, or a known repository tree. Code search covers the indexed default branch; read exact bytes afterward. |
| `ghGetFileContent` | External | Reads a known GitHub file, ref, range, or match. Full reads return content without creating a local checkout. |
| `ghSearchHistory` | External | Searches GitHub pull-request, issue, or commit metadata. It discovers history identities; it does not replace exact history reads. |
| `ghGetHistoryItem` | External | Reads one known pull request, issue, commit, or comparison, with explicit selectors for bodies, comments, files, reviews, commits, and patches. |
| `artifactSearch` | External | Resolves dependency identities or discovers packages by capability across eight ecosystems. Set `type`; registry metadata and upstream links lead to source research. npm retains registry-scoped authentication. |
| `ghCloneRepo` | Hybrid | Uses provider credentials/network access, then atomically materializes a full or sparse repository under managed local storage. CLI-only; available when persistent local storage is (MCP does not register it). |
| `localSearch` | Internal/local | Runs bounded lexical text/regex search against allowed local paths. |
| `structureSearch` | Internal/local | Outlines directories and finds files by name or metadata under allowed local paths, without parsing. |
| `astSearch` | Internal/local | Finds structural AST matches, declarations, and paginated syntax trees against allowed local paths. |
| `astTopology` | Internal/local | Analyzes syntactic cross-file dependency graphs for dependencies, dependents, paths, cycles, reachability, dead code, and drift. |
| `astRewrite` | CLI only | Opt-in beta feature (`OCTOCODE_BETA=true`); never exposed through MCP. Previews structural ast-grep rewrites and performs serialized, snapshot-bound, hash-guarded applies with journal recovery; inspect the commit or recovery receipt. Cross-file changes are not simultaneously visible. |
| `localFetch` | Internal/local | Reads a known allowed path with full, match, line-range, minified, or symbol-outline views and exact continuations. |
| `lspSearch` | Internal/local with a language-server process | Resolves an anchored symbol and asks a real language server for definitions, references, calls, types, symbols, hierarchy, or diagnostics. It reports unavailable capabilities instead of returning a syntactic approximation as semantic proof. |
| `clasify` | External Jev provider | Executes unread read-tool requests or accepts supplied state, applies Noul, Choice, or Score questions across a resource-question matrix, and returns correlated typed pages without retrieved bodies. |

Remote GitHub tools require provider runtime and credentials. `artifactSearch` uses official registry APIs; `type:"npm"` honors the effective npm registry configuration. Local tools require `ENABLE_LOCAL`; `ghCloneRepo` is CLI-only and requires persistent storage. `astRewrite` and `astTopology` additionally require `OCTOCODE_BETA=true` (the sole gate for both preview and apply). LSP availability also depends on a compatible server for the file language. `clasify` requires a nonblank resolved classification key: `OCTOCODE_CLASSIFICATION_API`, else the selected vendor's key (`OCTOCODE_JEV_KEY` for jev), else `.octocoderc` `classification.api`; without one, MCP omits it and a CLI call returns an actionable missing-key error.

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

Use the returned file and line as an exact-read/LSP anchor. For graph results, preserve `entrypoints`, `includeTests`, exclusions, scan caps, diagnostics, and `rustWorkspace`; changing any of them changes what "reachable" means.

---

## GitHub tools reference

Concise reference for Octocode MCP remote research tools: GitHub code/repo/PR search, GitHub content access, cloning, and package registry lookup/discovery.

### GitHub tool configuration

| Variable | Purpose |
|----------|---------|
| `OCTOCODE_TOKEN` | Highest-priority GitHub token. |
| `GH_TOKEN` | GitHub CLI compatible token. |
| `GITHUB_TOKEN` | GitHub token fallback. |
| `GITHUB_PERSONAL_ACCESS_TOKEN` | Lowest-priority GitHub token env var. |
| `GITHUB_API_URL` | GitHub Enterprise API base URL. |
| `ENABLE_LOCAL` | Turns local tools on or off. Defaults to `true` on both CLI and MCP. |

Every tool accepts bulk input (`{ "queries": [...] }`), up to 5 queries per call. Page-based tools use `page` and `pageSize`; `limit` is a pre-pagination cap where that distinct control exists. When more results remain, run the matching schema-valid `next.*` call: `nextPage`/`nextMatchPage`, `expandLimit`/`expandScan`, or a content continuation. At an unexpandable public or provider cap, metadata reports `terminalLimitReached` and omits unusable continuations. Numeric page, offset, cursor, and raw `nextQuery` fields are not executable by themselves. `matchString` selects all matching slices; file chunks page that selected view without changing the selector. `ghCloneRepo` is atomic and does not paginate its input. Use `npx octocode scheme <toolName> --compact` for the exact active schema and operation scopes.

Search match values and provider text snippets are evidence previews, not collection pagination. A preview may abbreviate visible text only when it retains exact path/line locators and an executable exact-read route; structural capture reduction is explicitly typed with `capturesTruncated` and an executable `next.expandCaptures` replay.

### Choose a GitHub tool

| Need | Tool |
|------|------|
| Search code across GitHub | `ghSearch` with `operation: "code"` |
| Read a known file | `ghGetFileContent` (browse directories with `ghSearch` tree; bring a repo to disk with `ghCloneRepo`) |
| Browse a repository tree | `ghSearch` with `operation: "tree"` |
| Discover repositories | `ghSearch` with `operation: "repositories"` |
| Search PRs, issues, or commits | `ghSearchHistory` with `operation: "pullRequest"`, `"issue"`, or `"commit"` |
| Inspect one PR, issue, commit, or ref comparison | `ghGetHistoryItem` with `operation: "pullRequest"`, `"issue"`, `"commit"`, or `"compare"` |
| Materialize a repo/subtree locally | `ghCloneRepo` |
| Resolve package identity or find packages by capability | `artifactSearch` |

### `ghSearch`

Use the default unified discovery tool with one strict operation per query:

- `operation: "code"` accepts the code-search fields documented below.
- `operation: "repositories"` accepts repository discovery fields.
- `operation: "tree"` requires `owner` and `repo` and accepts tree browsing fields.

Fields from another operation are rejected rather than silently ignored. Mixed bulk
queries are allowed and results retain input order. `ghGetFileContent` stays
separate because it reads and minifies known content rather than discovering it.

<!-- tool: ghSearch -->
```json
{"reasoning": "Use ghSearch for this documented evidence request.", "operation": "code", "keywords": ["useReducer"], "owner": "vercel", "repo": "next.js"}
{"reasoning": "Use ghSearch for this documented evidence request.", "operation": "repositories", "keywords": ["code research"], "language": "TypeScript"}
{"reasoning": "Use ghSearch for this documented evidence request.", "operation": "tree", "owner": "vercel", "repo": "next.js", "path": "packages", "maxDepth": 2}
```

Operation-specific fields:

| Operation | Fields |
|---|---|
| `code` | `keywords`, `owner`, `repo`, `extension`, `filename`, `path`, `language`, `match`, `pageSize`, `page`, `concise` |
| `repositories` | `keywords`, `topics`, `language`, `owner`, repository-range filters, `match`, `sort`, `pageSize`, `page`, `archived`, `visibility`, `license`, `concise` |
| `tree` | required `owner` and `repo`; optional `branch`, `path`, `maxDepth`, `page`, `pageSize`, and `include` |

Use `match:"path"` for path-only code discovery and `match:"file"` when snippets
matter. Repository `match` instead selects searchable metadata fields. For the
exact active branch requirements and field types, inspect the compact schema.

Keywords are literal ANDed terms. Each one is sent as a bare word or as a single
quoted phrase. Interior double quotes and backslashes are dropped, so a keyword
such as `"hello" NOT` becomes the phrase `"hello NOT"` and cannot negate or
replace the `repo:` scope. `owner` and `repo` must be GitHub names. A value with
spaces, quotes, colons, or operators is a validation error.

`repositories` with only `owner` (optionally `sort:"updated"`) reads the REST
owner listing and excludes archived repositories. One call reads up to 5
provider pages to fill a page, so it can return more than `pageSize` rows.
`page` and `nextPage` are provider page cursors that follow GitHub's `Link`
header. The listing reports no `totalMatches` or `totalPages`
(`countScope: "unknown"`).

### `ghGetFileContent`

Read one GitHub file. For directories use `ghSearch` `operation:"tree"`; for local analysis use `ghCloneRepo`.

Key fields:

| Field | Meaning |
|-------|---------|
| `owner`, `repo`, `path` | Required repository and path. |
| `branch` | Branch, tag, or commit SHA. Omit to use default branch. |
| `fullContent` | Read the whole file. Use only for small files. |
| `startLine`, `endLine` | Read a line range. |
| `matchString` | Return matching slices. |
| `contextLines` | Context around `matchString`. |
| `matchStringIsRegex`, `matchStringCaseSensitive` | Match behavior. |
| `chunkType`, `offset`, `chunkSize` | Same line/UTF-8 byte pagination as `localFetch`; follow `next.continue`. |
| `minify` | `standard` (lossy, language-dependent compression), `none` (no minification), or `symbols` (structural outline). Defaults to `none`, exactly as `localFetch`. Security redaction still applies. |

Choose one extraction intent: whole file, line range, matching slices, or symbol outline. Both readers reject symbol outlines combined with match or line selectors. Selection precedes minification, redaction, and pagination. `chunkType` defaults to `lines` with `chunkSize:2000` (the 16384-byte page budget usually ends the page first); `bytes` defaults to 16384 UTF-8 bytes. Offsets are zero-based in the selected view. Line pages have a 16384-byte budget and oversized lines switch to bytes. Byte ends may extend by up to three bytes to finish a code point.

`fullContent:true` requests an unpaged view and rejects chunk controls. A view over 50000 bytes (or a source over 100 KB) returns its first bounded line page inline, `partialReasons:["full-content-limit"]`, and `next.continue` for the rest. A range remains bounded by its original `endLine`; continuing a match preserves its pattern and source-line context. `totalLines` and `sourceBytes` describe the original file; `pagination.totalLines`/`totalBytes` describe the complete selected view. `matchedLines` contains source anchors on the current page and `selectedMatchCount` counts all selected matching lines. `minifyFallback` explains when match evidence or unavailable outlines prevent the requested transform.

File reads return content without creating a checkout. Use `ghSearch operation:"tree"` to browse directories or `ghCloneRepo` with `sparsePath` to create a local subtree.

Examples:

<!-- tool: ghGetFileContent -->
```json
{"reasoning": "Use ghGetFileContent for this documented evidence request.", "owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/config.ts", "matchString": "export", "contextLines": 2, "chunkType": "lines", "chunkSize": 20}
```

Cost by mode:

| Mode | What you get | Approx tokens |
|------|-------------|---------------|
| `matchString` | Every matching slice, plus context | ~50-300 |
| `startLine`/`endLine` (small) | Exact line range | ~100-500 |
| `minify: "symbols"` | Imports and signatures, bodies stripped | 5-20% of the full file |
| `startLine`/`endLine` (large chunk) | Up to `charLength` chars of a range | 1k-10k |
| `fullContent` | Entire file; defaults to no minification | Can exceed 50k |

Behaviors worth knowing:

- `matchString` selects all occurrences with context and source anchors in `matchedLines`/`matchRanges`; both readers disable minification for matches (redaction still applies). Non-adjacent windows are separated by a `... [lines A-B omitted] ...` line (byte windows: `... [N bytes omitted] ...`), so a gap never reads as contiguous source. Follow character continuations when selected content exceeds a window.
- `minify: "symbols"` returns a paginated outline; read its source-line gutter, then follow up with `startLine`/`endLine` and `minify:"none"`.
- Semantic character windows can expand beyond the target — execute `next` unchanged (offsets are exact; `pageCountsKind:"estimated"` marks approximate counters).
- `standard` removes comments and rewrites formatting but does no JS/TS optimization or type-declaration removal; use `none` for source quotes and comment-sensitive evidence. See [minification coverage](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md#minification--file-reads-and-search-fragments).
- Files too large for the `/contents/` API fall back to the Git tree/blob API automatically — no need to switch to `ghCloneRepo` for size alone.

### `ghSearchHistory`

Search GitHub history through one strict discovery operation per query:

- `operation: "pullRequest"` searches PR candidates.
- `operation: "issue"` searches issue candidates.
- `operation: "commit"` walks commit history, optionally scoped to a path or time range.

The search tool returns candidates and stable identities. Fetch detailed content
with `ghGetHistoryItem`; search queries do not accept singular-item identities.

<!-- tool: ghSearchHistory -->
```json
{"reasoning": "Use ghSearchHistory for this documented evidence request.", "operation": "pullRequest", "owner": "vercel", "repo": "next.js", "keywords": ["middleware"], "match": ["title"], "state": "merged"}
{"reasoning": "Use ghSearchHistory for this documented evidence request.", "operation": "issue", "owner": "vercel", "repo": "next.js", "keywords": ["memory leak"], "match": ["title"], "state": "open"}
{"reasoning": "Use ghSearchHistory for this documented evidence request.", "operation": "commit", "owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/", "since": "30d"}
```

Prefer title-first PR and issue searches. For commit archaeology, narrow by path
and time before fetching a commit diff.

Keywords follow the `ghSearch` rule: a bare word or one quoted phrase, never an
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
| `commit` | `owner`, `repo`, `ref` | commit metadata and optional diff |
| `compare` | `owner`, `repo`, `base`, `head` | ahead/behind counts and commits between refs |

Fields from another operation are rejected rather than ignored. In particular,
PR and issue identity is always `number`; commit identity is `ref`; comparison
identity is the `base` + `head` pair.

A merged pull request reports `mergeCommitSha` (from GraphQL `mergeCommit`;
REST API 2026-03-10 drops `merge_commit_sha`) with `next.getMergeCommit`; an
open pull request never reports GitHub's test-merge SHA. A commit offers
`next.findPullRequest`, a `ghSearchHistory` query that finds the pull request
containing that SHA.

<!-- tool: ghGetHistoryItem -->
```json
{"reasoning": "Use ghGetHistoryItem for this documented evidence request.", "operation": "pullRequest", "owner": "vercel", "repo": "next.js", "number": 12345, "content": {"changedFiles": true}}
{"reasoning": "Use ghGetHistoryItem for this documented evidence request.", "operation": "issue", "owner": "vercel", "repo": "next.js", "number": 12345, "content": {"body": true, "comments": {"discussion": true}}}
{"reasoning": "Use ghGetHistoryItem for this documented evidence request.", "operation": "commit", "owner": "vercel", "repo": "next.js", "ref": "abc123", "includeDiff": true}
{"reasoning": "Use ghGetHistoryItem for this documented evidence request.", "operation": "compare", "owner": "vercel", "repo": "next.js", "base": "v14.0.0", "head": "v14.1.0"}
```

Request selected PR patches instead of every patch for large PRs, and leave
commit diffs off until the relevant commit is known.

PR details accept `minify:"none"` or `"standard"`. `none` preserves selected
body, discussion/inline comments, reviews, and all/selected patch text after
security redaction. `standard` compacts Markdown and unchanged diff context
before computing offsets; it preserves changed source lines regardless of
language. Match-filtered reads preserve source anchors.

Issue, commit, and compare details do not accept `minify`; they return exact
selected text after redaction. Issue bodies and comment bodies use
`charOffset`/`charLength`, with automatic 12,000-character windows.
Comment-item continuations reset the text offset. Follow each returned
continuation independently: body, comment body, item page, file, and patch
windows address different parts of the response.

PR collection reads fetch at most one provider batch per requested source:
100 changed files, discussion comments, inline comments, or reviews, and 50
commit summaries. `pageSize` bounds displayed items within each batch.
`reviewPage` pages reviews independently; review bodies retain their text
continuations. Follow the returned `next.*` calls, including when filtering
produces an empty batch. Those calls carry `collectionPages` positions and
reset the relevant item and text windows when advancing. A zero position marks
an exhausted comment source, which is no longer fetched. Counts labeled
`countScope: "providerBatch"` describe that batch, not the complete collection.

Nested `content.commits.includeFiles` reads fetch one file batch for each
displayed commit and return at most `pageSize` files per commit. Each commit's
`next.nextFilePage` and `next.continuePatch` call `ghGetHistoryItem` with its
exact SHA. Exact commit reads carry `fileBatch` across provider batches and
retain independent file and patch windows. Cached batches are isolated by
authentication identity. Provider caps and omitted patches remain explicit
terminal limits; a provider cap does not establish completeness.

A commit read that stops at its file-batch cap reports
`changedFilesCountScope: "partial"`, `countScope: "partial"`, and
`terminalLimit`. It does not report `complete`. PR metadata lists the first 20
labels and sets `labelsTruncated: true` when more exist. PR commit summaries
carry the full commit message, including the body.

### `ghCloneRepo`

Clone a repository or sparse subtree into Octocode's local cache.

Clone is opt-in. Enable it through the supported configuration and inspect the
live catalog; local-access and storage gates also apply.

Key fields:

| Field | Meaning |
|-------|---------|
| `owner`, `repo` | Required repository. |
| `branch` | Branch, tag, or exact commit SHA. Omit to use the default branch. |
| `sparsePath` | Optional file or directory sparse checkout. |

Returns a location with an absolute path, requested-scope completeness, commit
identity, and cache/verification state. Use `location.localPath` with
`structureSearch operation:"tree"` to inspect the checkout.

Examples:

<!-- tool: ghCloneRepo -->
```json
{"reasoning": "Use ghCloneRepo for this documented evidence request.", "owner": "vercel", "repo": "next.js", "branch": "canary"}
{"reasoning": "Use ghCloneRepo for this documented evidence request.", "owner": "microsoft", "repo": "TypeScript", "sparsePath": "src/compiler"}
```

Rules:

- Use `sparsePath` for large monorepos.
- Use `ghGetFileContent` when you only need one file.
- Cached clones are reused.
- Use the returned path as-is. A cache hit requires a clean working tree at the
  recorded HEAD; an edited cache is re-cloned, so `verified` holds for cached
  bytes. Check `verified` separately from scoped `complete`.

### `artifactSearch`

Find packages for a capability, resolve a known dependency to registry metadata, or locate its upstream source. Use local tools to explain installed code and GitHub tools when the repository is already known. A repository link is metadata, not implementation or published-version proof; for an exact lookup whose source is on GitHub, `next.viewRepo` opens that tree.

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
{"reasoning": "Use artifactSearch for this documented evidence request.", "type": "npm", "packageName": "react"}
{"reasoning": "Use artifactSearch for this documented evidence request.", "type": "pypi", "packageName": "requests"}
{"reasoning": "Use artifactSearch for this documented evidence request.", "type": "crates", "keywords": ["async", "runtime"], "pageSize": 10}
{"reasoning": "Use artifactSearch for this documented evidence request.", "type": "maven", "packageName": "org.slf4j:slf4j-api"}
{"reasoning": "Use artifactSearch for this documented evidence request.", "type": "npm", "packageName": "@example/widget", "registry": "https://registry.example.com/"}
```

Each query selects one ecosystem. Compare ecosystems using independent entries in `queries` (maximum five), not `type:"all"`. All providers use official APIs. PyPI keyword discovery returns an unsupported-capability error with exact-lookup guidance; it does not silently fall back to a website or third-party service.

`artifacts[]` contains canonical package identities, registry URLs, and available version, description, license, homepage, and source metadata. Go package paths stay separate from module identities. Optional metadata can be absent; cross-registry popularity scores are not comparable. Read exact source to establish behavior and match the published or installed version before making version-specific claims.

For npm, scoped names honor `@scope:registry` and an explicit `registry` takes precedence; authentication uses registry-scoped npm configuration (with environment interpolation), caches isolate registry/configuration identities, and continuations preserve the selected registry. Private registries must support npm's search endpoint for discovery; other ecosystems use official public services. Follow executable continuations rather than constructing page numbers or interpreting cursors; unknown totals stay unknown, an empty page may still continue, and auth/unsupported/rate-limit/provider failures are errors while a missing exact package is empty.

### Token cost control by goal

| Goal | Cheapest approach |
|------|------------------|
| Find if a function exists in a file | `ghSearch(operation:"code")` with `keywords: ["functionName"]` |
| Read one function body | `ghGetFileContent` with `matchString: "function name"` + small `contextLines` |
| Scan a whole file's structure | `ghGetFileContent` with `minify: "symbols"` |
| Read 2–10 functions from a file | Multiple `startLine`/`endLine` reads in one batched call |
| Read a 3MB+ file | `ghCloneRepo` sparse + local read |
| Understand why a PR was made | `ghSearchHistory(operation:"pullRequest")`, then `ghGetHistoryItem(operation:"pullRequest", number)` with `content.body: true` |
| Review a PR's changes | `content.changedFiles: true` first, then `content.patches.mode: "selected"` for relevant files |
| Get all inline code comments on a PR | `content: { comments: { reviewInline: true, discussion: false } }` |
| Count repositories in an org | `owner: "vercel", archived: false` → search `totalMatches` (an owner-only listing reports no total) |
| Get package version only | `artifactSearch` with `type` and `packageName`; an upstream manifest is not proof of the published version |

### Workflows

| Task | Flow |
|------|------|
| Understand a package | `artifactSearch` -> `ghSearch(operation:"tree")` -> `ghSearch(operation:"code")` -> `ghGetFileContent` |
| Find examples of a pattern | `ghSearch(operation:"code")` -> `ghGetFileContent` |
| Explore a repository | `ghSearch(operation:"tree")` -> `ghGetFileContent(README)` -> `ghSearch(operation:"code")` |
| Explain why code changed | `ghSearch(operation:"code")` -> `ghSearchHistory` -> `ghGetHistoryItem` with the returned identity |
| Deep local analysis | `ghCloneRepo` -> local tools |

### GitHub tool rules

- Use GitHub tools for remote repositories, not files already on disk.
- Use `artifactSearch` for a known dependency or a package capability need; set the ecosystem `type`. Skip it when the source repository is already known or installed behavior needs local evidence.
- Use `ghSearch(operation:"tree")` before reading unknown paths.
- Use `matchString`, line ranges, or `minify: "symbols"` instead of `fullContent` for large files.
- Use PR metadata first, then selected content.
- Use `ghCloneRepo` only when local analysis is worth the clone cost.
- Retry only transport failures, timeouts, and 5xx. HTTP 451 is
  `errorCode: "unavailable"` (blocked for legal reasons). Other unmapped
  statuses are `errorCode: "httpStatus"` with the code in the message. Neither is
  retryable. An error body over the size limit keeps its status classification
  and adds `(error body exceeded limit)` to the message.
- OAuth device login and token refresh never follow redirects. A stored OAuth
  token is refreshed under a per-host lock file in `~/.octocode/tmp/locks`, so
  concurrent processes spend GitHub's single-use refresh token once and reuse
  the result.

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

To hide individual local tools while keeping the rest available, use `DISABLE_TOOLS` or `tools.disabled`.

Useful local-tool environment variables:

| Variable | Description |
|----------|-------------|
| `ENABLE_LOCAL` | Enables local filesystem tools. Defaults to `true` on both CLI and MCP; set `false` to disable them. |
| `WORKSPACE_ROOT` | Root used to resolve relative local paths. Overrides `local.workspaceRoot` in config. |
| `ALLOWED_PATHS` | Optional comma-separated allowlist of extra roots, added on top of the always-allowed home directory. Empty means home directory only (paths outside home are denied). |
| `TOOLS_TO_RUN` | Strict tool allowlist; include every tool that must remain enabled. Removed compatibility names are rejected. |

A path outside the allowed roots (directly or through a symlink) fails with `errorCode: "pathOutsideAllowedRoots"` in `localSearch`, `localFetch`, `lspSearch`, and `astRewrite` (`astSearch` keeps `ast.policy.outsideAllowedRoots`/`ast.policy.symlinkEscape`); the recovery hint is keyed on that code. Run from inside the workspace or extend `ALLOWED_PATHS` / `WORKSPACE_ROOT`.

Config reference: [Configuration Reference](CONFIGURATION.md).

---

### Platform support

`localSearch`, `structureSearch`, and `astSearch` use Octocode's native in-process ripgrep, filesystem-walker, and structural-search engines. There are no external `rg`, `grep`, `find`, or `tree` dependencies.

`localFetch` is pure Node.js and works on macOS, Linux, and Windows.

All local search tools work on macOS, Linux, and Windows. Prefer `localSearch` with `resultView:"files"` when a content query can answer the question.

---

### Pagination

All tools accept up to 5 queries per call.

Local tools expose two pagination layers:

| Layer | Fields | Applies To |
|-------|--------|------------|
| Native result pagination | `page`, `pageSize` | Every `localSearch` query |
| Per-file match pagination | `matchPage`, `maxMatchesPerFile` | `localSearch` when a matched file has more matches |
| Local content pagination | `chunkType`, `offset`, `chunkSize` | `localFetch` selected views |
| Bulk response pagination | `responseCharOffset`, `responseCharLength` | Any local-tool bulk response |

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
| `searchText` | Text or regex pattern. Required. |
| `resultView` | Lexical response shape: `paginated`, `discovery`, `detailed`, `content`, `files`, `filesWithout`, `countLines`, `countMatches`, or `matchOnly`. |
| `matchWindow` | With `resultView:"matchOnly"`, widen each matched span by this many characters of context on each side (… marks trimmed sides). 0 = bare match. |
| `unique` | With `resultView:"matchOnly"`, use `list` for distinct match values per file or `count` for frequencies. |
| `contextLines` | Lines around each match. Default 0 (`detailed`: 3), max 100. |
| `matchContentLength` | Max characters per match snippet, clipped around the hit. Default 200 × (2·contextLines + 1), capped at 4000; explicit max 100000. |
| `pageSize` | Files per lexical result page. Default 20 (100 for path/count views). |
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
| `invertMatch` | Return non-matching lines, or with `resultView:"files"`, files lacking the pattern. |

#### Filters

| Parameter | Description |
|-----------|-------------|
| `langType` | Ripgrep language/type filter such as `ts`, `js`, `py`, `go`. |
| `include` | Glob patterns to include. |
| `exclude` | Glob patterns to exclude. |
| `excludeDir` | Directory names to skip. |
| `hidden` | Include hidden files. |
| `noIgnore` | Ignore `.gitignore` and `.ignore` files. |
| `sort` | Text/structural: `relevance`, `matchCount`, `path`, `modified`, `accessed`, or `created`. Files: `modified`, `name`, `path`, or `size`. Tree: `name`, `size`, `time`, or `extension`. |
| `reverse` | Reverse the selected sort direction where the selected operation supports it. |

#### Output

Normal results include matched files and match snippets with line and column information. For count-only output use `resultView:"countLines"` or `resultView:"countMatches"`.

Coverage and limits:

- `relevance` (default) and `matchCount` keep the 10,000 most-matched files across every searched file. `stats` totals count all matched files; `capReason:"maxCollectedFiles"` marks the trimmed list.
- If no file under `path` can be read, the call fails with `errorCode:"fileAccessFailed"` (exit 5). If some paths are unreadable, the result sets `isPartial` and `terminalLimit`, and `stats.errorCount`/`firstError` report the failures. Zero matches in that result do not prove absence.
- A file with a NUL byte is searched up to that byte, and matches before it are kept. `capped` is true, `capReason` includes `binaryQuit`, a `binaryFileSkipped` warning is added, and the result is `isPartial` (with `terminalLimit` when no other continuation exists): no text tool reads past the NUL, so the kept matches are not the file's full set and an empty result does not prove absence.
- Match values whose secret-shaped text was replaced by `[REDACTED…]` placeholders carry a `redactedMatches` warning; those values are not verbatim source.
- `regex:"pcre2"` searches have a wall-clock limit. At the limit the result keeps the files finished so far and reports `capReason:"pcre2Deadline"`.
- Files are opened without following symlinks. A path replaced by a symlink or special file after the walk counts as a read error.

Row `path` values are relative to the envelope `base`, which is the queried directory (or the parent of a queried file) on every page; `join(base, path)` is the absolute file to pass to `localFetch` or `lspSearch`. The `next` map carries only pagination continuations:

| Next key | Tool | Purpose |
|----------|------|---------|
| `nextPage` / `nextMatchPage` | `localSearch` | Continue file-level or per-file match pagination. A later match page lists only files that still have rows. |
| `restart` | `localSearch` | Rerun from page 1 when the result snapshot is stale. |

#### Examples

```bash
localSearch( path="packages/octocode-mcp/src", searchText="registerTool", langType="ts")
localSearch( path=".", searchText="TODO", resultView="files")
localSearch( path="src", searchText="class\\s+\\w+Service", regex="rust", contextLines=3)
```

#### Structural and AST search

Use `astSearch(operation:"match")` for code-shape queries regex cannot express (find all `await` inside `for` loops, calls with N args, functions missing `try/catch`).

Structural results distinguish file-scan caps from execution limits. A scan cap
sets `truncated` and supplies `next.expandScan` while the bound can grow. Parser
or matcher exhaustion preserves completed files and reports staged
`diagnostics`, `partialReasons: ["structuralLimit"]`, and `terminalLimit` when
no continuation can complete the execution. Zero matches in an incomplete
result do not establish absence. `maxDepth: 0` includes files directly in the
root; depth filtering happens before the file-scan cap.

`astSearch` match, `syntaxTree`, and `symbols` positions use one-based lines and
zero-based UTF-16 code-unit columns. A `symbols` row's `name` + `line`, or an
identifier capture's `text` + `line`, is `lspSearch` `symbolName` + `lineHint`
as-is (with `uri` = the file path).

`operation:"syntaxTree"` pages one file's parsed syntax tree: `nodeOffset`
(default 0), `nodeLimit` (default 100, max 1000), and `namedOnly` (default
`true`).

**Supported structural extensions:** `c`, `cc`, `cjs`, `cpp`, `cs`, `cts`,
`cxx`, `go`, `h`, `hh`, `hpp`, `hxx`, `java`, `js`, `jsx`, `mjs`, `mts`, `py`,
`pyi`, `rs`, `sbt`, `sc`, `scala`, `ts`, and `tsx`. The exact same 25-extension
set backs signatures and graph facts in the default release build. Query the
compiled engine capability API when optional grammar features are disabled.

When a code-shaped pattern returns zero matches, native runtime can retry a
semicolon-normalized form or a relaxed return-type form. CLI and MCP output
expose the retry as a typed `structural.query.rewritten` diagnostic, including
the requested pattern, effective pattern, and an executable continuation that
repeats the effective query explicitly. Use an explicit `rule` query when exact
query equivalence matters.

YAML `kind` rules are checked against the selected source grammar before
execution; YAML is the rule-document format, not a supported source grammar.
An unknown node kind returns a typed compile diagnostic instead of a
high-confidence zero-match result.

`inside` checks read each candidate's ancestor chain once (linear in nesting
depth), so deeply nested files do not hit the deadline on `stopBy: end`. A
file that still exceeds the deadline is reported as a
`structural.match.deadline` diagnostic, never as zero matches.

`operation:"symbols"` rows carry `docStartLine` when a comment block sits directly above the declaration (JSDoc, `///`, `#` in Python). It marks a JS/TS declaration `exported` by its local
binding. When it is exported under another name, `exportedAs` lists the public
names: `export { foo as bar }` gives `foo` with `exportedAs: ["bar"]`, and
`export default function foo` gives `exportedAs: ["default"]`.

Java call patterns may omit their trailing semicolon. The structural compiler
supplies grammar-checked statement context for direct patterns and patterns
nested anywhere in a YAML rule; complete patterns keep their original parse,
match ranges, and captures.

Pattern matching is exact about modifiers: a Rust `fn $N()` pattern does not
match `pub fn` items (the visibility modifier is a named child). Such a pattern
adds a `structural.pattern.visibilityExact` info diagnostic; write `pub fn …` or
use a YAML rule on the item kind. A multi-node `$$$` capture is returned as one
`metavarRanges` span row with `count` (and `capturesTruncated`); set
`captureText:true` (`next.expandCaptures`) for per-node text.

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
| `maxDepth` | Recursion depth; `0` is the root. Use low depth first. |
| `page` | Result page. |
| `pageSize` | Directory entries per page. |
| `limit` | Hard pre-pagination cap. Max 10000. |
| `entryType` | `f` for files only, `d` for directories only; omit for both. |
| `extensions` | Only include files with selected extensions. |
| `excludeDir` | Directory names to prune. |
| `hidden` | Include hidden files and directories. |
| `snapshot` | Copy from `next` when paging. |

#### Output

The response separates structured `files[]` and `folders[]` and includes summary and pagination metadata when applicable.

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
| `time.modifiedBefore` | Files modified before a date/window. |
| `time.accessedWithin` | Files accessed within a window. |
| `size.greater` / `size.less` | Size filters such as `100k` or `1m`. |
| `empty` | Empty files/directories only. |
| `permissions` | Permission string filter. |
| `access` | Permission predicate: `executable`, `readable`, or `writable`. |
| `excludeDir` | Directory names to skip. |
| `detail` | `basic` (default), `modified` (+mtime), or `full` (all metadata). |
| `sort` | Sort by `modified`, `name`, `path`, or `size`. |
| `page` | Result page. |
| `pageSize` | Files per page. Max 50. |
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
| `chunkSize` | Requested lines (default 100) or bytes (default 16384), from 1 through 50000. |
| `matchString` | Nonempty literal source text; enable `matchStringIsRegex` for regex or `matchStringCaseSensitive` for case sensitivity. |
| `contextLines` | Explicit source-line context per side, 0–100; default 5 for line chunks. Exclusive with `contextBytes`. |
| `contextBytes` | UTF-8 context bytes per side, 0–16384; default 256 for byte chunks. Requires `matchString`. Full-source redaction precedes byte matching; edges expand to whole code points, and disjoint windows are separated by a `... [N bytes omitted] ...` marker. |
| `minify` | `none` (default), `standard` compact source, or `symbols` whole-file outline. Symbols cannot accompany range/match selectors. |
| `fullContent` | Complete unpaged view within resource/security limits; cannot accompany chunk controls. |

Selection precedes minification, redaction, and pagination. Line pages preserve complete lines within a 16384-byte budget. An offset at or past the end of the view returns empty content with `pagination.outOfRange:true` and an offset-zero `next.restart`. An oversized line switches to byte paging from the unreturned position. Byte ends extend by at most three bytes to finish a UTF-8 code point. Copy the complete `next.continue` query; do not calculate offsets. Continuations stop at the selected range or matched view.

Every successful text read reports original-file `totalLines` and `sourceBytes`, including empty files and no matches. `pagination.totalLines`/`totalBytes` describe the selected returned view. `returnedBytes`, `returnedLines`, and `returnedChars` describe the current chunk (characters count UTF-16 code units). `none` content has no injected line numbers and preserves whitespace and line endings; the `symbols` outline prefixes each line with `N| `.

`matchRanges` describe all selected source context windows; `matchedLines` contains matching source anchors intersecting the current page, and `selectedMatchCount` counts matching source lines in the selected view. Overlapping context windows are merged. `matchString` forces exact content so minification cannot remove the evidence. `minifyFallback` reports the requested/applied modes and reason when a match forces exact content or an outline is unavailable.

Private-key blocks are redacted across the whole file before any selection, so a key split across a page or range boundary never leaks. A line page is then scanned on its own lines plus 8 KiB of surrounding lines, so a multi-line secret crossing the page edge is still matched whole and paging a large file costs one page scan per call. A byte page is scanned on its whole lines plus at least 8 KiB of surrounding whole lines (single-line secrets are always scanned whole); a redacted line cut by the page end is returned whole and the page extends to that line's end, so `next.continue` never splits a secret. Byte offsets stay in the unredacted view's coordinates. `fullContent` views are scanned as the complete selected view; above the scanner's 10,000,000-byte limit, `contentSecurityLimit` provides a smaller-source-range alternative when possible, otherwise an explicit terminal limit. File totals unavailable due to access or resource limits are identified as unavailable. A full-content view over 50000 bytes supplies executable bounded recovery.

```bash
localFetch(path="/ABS/repo/src/index.ts", startLine=1, endLine=80)
localFetch(path="/ABS/repo/README.md", matchString="Configuration", contextBytes=256, chunkType="bytes", chunkSize=1024)
localFetch(path="/ABS/repo/src/index.ts", minify="symbols")
```

---

### `astTopology`

Scope admission: without an explicit `maxFiles`, a root with more than 5,000
parseable files is refused before parsing (`ast.graph.scopeTooBroad`). The
error lists admissible package directories in `hints`, offers
`next.narrowScope` for the largest one, and `next.expandScan` (explicit
`maxFiles`) to opt in to the full scan.

The `coverage` object separates parser inventory from module-linking support.
It reports language coverage, resolved, external, and non-code (`imports.nonCode`:
JSON, styles, assets) import counts, unresolved internal imports, unsupported
linking, and parse-recovery diagnostics. These gaps lower `confidence` and are
listed in `completeness.coverageGapReasons`; they do not set `truncated` or
`terminalLimit`, which mark only real scope cuts.
Inspect coverage before interpreting an empty dependency or cycle result.
Coverage diagnostics default to 25 rows per page. Aggregate
`coverage.diagnosticCounts` and import counts describe the full scan. Follow
`next.nextDiagnostics` to retrieve the remaining rows; its snapshot token
prevents combining different diagnostic inventories. If diagnostics change,
follow `next.restartDiagnostics`. Use `diagnosticPageSize` to request up to 100
rows per page. Diagnostic pagination and graph-result pagination are independent.

Rust analysis defaults to `rustWorkspace: "syntax"`, which uses explicit module
declarations and supported literal `#[path]` attributes. Set
`rustWorkspace: "cargo"` to inspect Cargo target roots and dependency aliases
with the host Cargo executable. This opt-in mode runs offline metadata discovery
without compiling the project, with a five-second execution budget and a
one-MiB output bound. Include the Cargo manifest within the scan root. Missing
tools, excluded targets, conditional dependencies, cfg, and macro expansion
remain explicit coverage gaps when the analyzer cannot resolve them.

Declaration IDs identify scoped source occurrences; unresolved call references
are not proof of symbol identity. Value-reference counts are conservative
retention evidence and still require LSP confirmation for deletion decisions.

One bounded repository graph provides seven analyses: `dependencies`, `dependents`, `path`, `reachability`, `cycles`, `deadCode`, and `drift`. Import edges come from native syntax facts. Traversal and path results report exact `edgeKinds`: `static-import`, `type-import`, `dynamic-import`, `named-reexport`, `star-reexport`, `type-named-reexport`, `type-star-reexport`, `commonjs-require`, `create-require`, `python-import`, `rust-module`, `rust-use`, `c-include`, and `metadata-import`. Rust module/use edges, C includes, metadata, erased types, and edges without provenance do not establish runtime import cycles.

Cross-file resolution covers JavaScript/TypeScript ESM and binding-safe CommonJS, Rust modules, bounded Python absolute and relative imports, and quoted relative C/C++ includes. Literal CommonJS loads link only when `require`, `module.require`, or an imported `createRequire(import.meta.url)` binding is not shadowed or reassigned. Dynamic and ambiguous loaders remain explicit diagnostics. Python wildcard and ambiguous package-attribute imports remain diagnostics, as do C/C++ system and macro includes. Data, style, and asset imports (including `package.json`) are counted as `imports.nonCode`, not linked or reported as unresolved. Namespace-style imports conservatively retain target exports during dead-code analysis.

Dependency traversal also reports immediate dominators, topological layers, and transitively redundant condensation-DAG edges. Cycle results distinguish runtime import candidates (`runtimeCycle`) from other topology SCCs, expose condensation metadata, and return deterministic directed witnesses in `cycleEdges` and `runtimeCycleEdges`; every witness edge includes `from`, `to`, and `edgeKinds`. Native facts also contain `call` and `contains` relations, but the public operations don't project those symbol-level edges. `deadCode` results are candidates, not deletion proof.

Inside a reachable file, `deadCode` keeps an export live when an import or re-export chain consumes one of its public names (`import foo from` consumes `default`), or when it is reachable over same-file call and containment edges from a live declaration, a module-level call, or a declaration that escapes as a value. A value escape is a syntax-aware reference other than the declaration itself, an export clause, or a call target; comments and string literals never count. JS/TS counts the resolved references of each declaration's own symbol, so a same-named local elsewhere does not keep it live; other languages count identifier tokens by name. `unreferenced-export` rows name the basis in `viaHeuristic`: `reexport-chain`, `semantic-references` (JS/TS), or `syntax-references`. Callers are keyed by declaration identity, so a method `run` and a function `run` do not share liveness, and an uncalled private caller does not keep its callees live. Rows for exports renamed at the export site carry `exportedAs`. When graph extraction for a file hits its deadline, the facts gathered so far are kept and the file carries a `graph.traversal.deadlineExceeded` diagnostic, so a missing edge there is not evidence of absence.

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
| `operation` | Required discriminator: `topology`. |
| `analysis` | Required: `dependencies`, `dependents`, `path`, `reachability`, `cycles`, `deadCode`, or `drift`. |
| `path` | Repository root to analyze. Required. |
| `languageGlobs` | Optional root-relative AST parser map, e.g. `{"cpp":["include/**/*.h"]}`. Also accepted by directory `astSearch` symbols. Does not configure clangd; use compile commands or `.clangd` for C++ header LSP parsing. |
| `file` | Repo-relative source file for `dependencies`, `dependents`, and `path`. |
| `target` | Repo-relative destination file for `path`. |
| `depth` | Traversal depth for `dependencies` and `dependents`. Default 1, max 50. |
| `entrypoints` | Roots for `reachability` and `deadCode`; omit to detect `package.json` `main`, `exports`, and `bin`. |
| `includeTests` | Treat tests as roots for `reachability` and `deadCode`. Default `true`. |
| `excludeDir` | Directory names to prune. Defaults to `node_modules`, `dist`, `build`, `out`, `coverage`, `.git`, `target`, `.next`, and `.cache`. |
| `maxFiles` | Cap on files scanned. Max 50000. The scan stops and warns past this bound. |
| `limit` | Result cap before pagination. Max 5000. |
| `page` | Result page. Max 1000. |
| `pageSize` | Results per page. Max 50. |

Results never dump the complete graph: the result list is paginated, SCC/dead-cluster rows list their member `files`, and complete shortest paths cap at 100 files. Longer paths return `complete:false`, empty `files`/`edges`, bounded `prefix` and `suffix`, the target, total file count, and omitted-middle count so a prefix cannot be mistaken for a complete source-to-target path. A five-query large-repository batch must remain at or below 32 KiB in compact structured output; use pagination instead of expanding nested collections.

#### Graph result interpretation

| Signal | Interpretation | Required follow-up |
|--------|----------------|--------------------|
| `cycleEdges` | A deterministic directed witness through one reported SCC. Each edge names `from`, `to`, and its syntactic `edgeKinds`. | Read every reported edge exactly; SCC member order alone is not a valid cycle path. |
| `runtimeCycleEdges` | A directed witness using supported runtime import candidates; Rust module/use, C include, metadata, erased-type, and unknown-provenance edges are excluded. | Confirm the imported bindings and initialization behavior before claiming a runtime defect. |
| Topology-only SCC | Files are mutually connected in the full graph, but no cycle remains among runtime import candidates. This includes type-only and Rust module cycles. | Report it as topology or coupling evidence, not as a module-loading cycle. |
| `transitiveCandidates` | Condensation-DAG edges for which another directed path already connects the same components. They can indicate redundant architectural wiring. | Check re-export contracts, side effects, public API intent, and symbol usage before calling an import duplicate. |
| `immediateDominators` | Components that every directed route from the selected root must cross. | Use them to prioritize chokepoints; do not infer symbol ownership from file topology. |

The graph assigns no weights to edges. `path` therefore uses breadth-first search to return the fewest-edge directed import path, not Dijkstra's weighted shortest-path algorithm. A syntactically redundant edge can still be semantically necessary because it imports a value for side effects, preserves a public barrel contract, or selects a different binding.

#### Examples

```bash
astTopology(operation="topology", analysis="dependencies", path="/ABS/repo", file="src/index.ts", depth=2)
astTopology(operation="topology", analysis="cycles", path="/ABS/repo", pageSize=20, limit=100)
astTopology(operation="topology", analysis="deadCode", path="/ABS/repo", entrypoints=["src/index.ts"], includeTests=false)
```

For a cycle, read the exact imports named by `cycleEdges`; use `runtimeCycleEdges` when investigating loading behavior. Verify a dead-code or transitive-edge candidate with `lspSearch` before removing it.

---

### Local workflows

#### Explore a new repository

```text
structureSearch(operation="tree", path=root, maxDepth=1)
structureSearch(operation="tree", path=root+"/src", maxDepth=2)
structureSearch(operation="files", path=root, names=["package.json", "tsconfig.json", "README.md"])
localSearch( path=root, searchText="export", resultView="files")
localFetch(path="README.md", minify="symbols")
```

#### Search, then read

```text
localSearch( path="src", searchText="validateInput", contextLines=2)
localFetch(path="src/validation.ts", matchString="validateInput", contextLines=20)
```

#### Find tests for a feature

```text
structureSearch(operation="files", path=".", names=["*.test.ts", "*.spec.ts"])
localSearch( path="tests", searchText="featureName", resultView="files")
localFetch(path="tests/feature.test.ts", matchString="featureName")
```

#### Inspect recent changes

```text
structureSearch(operation="files", path=".", time={"modifiedWithin":"24h"}, entryType="f", detail="full")
localSearch( path=".", searchText="TODO|FIXME", regex="rust")
```

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
| `path`, `langType` | Required scope and language. |
| `ruleKind` | Required selector: `pattern`, `rule`, or `experimental`; inspect the live schema for the selected form. |
| `pattern`, `rewrite` | Match and replacement for the `pattern` form. |
| `include`, `exclude` | Optional file filters. |
| `maxFiles`, `maxMatches` | Scan bounds; defaults are 2,000 files and 10,000 matches. |
| `page`, `pageSize`, `snapshot` | Preview pagination; copy executable continuations and their snapshot. A page lists only the files its matches touch; a file's whole-file `patch` is sent once, on the first page touching it, and later pages carry `patchOnPage` instead. |
| `apply` | Defaults to `false`. Applying requires the unchanged preview snapshot and non-empty `expectedHashes`; a complete preview returns `next.apply` with both filled in. |
| `expectedHashes`, `selectedMatchIds` | Preview SHA-256 hashes for exactly the selected files. With explicit match selection, omit unselected-file hashes. A stale or missing selected-file hash aborts the apply. |
| `postconditions` | Check a required `remainingMatches` count in the staged selected files before commit. |

```bash
node packages/octocode/out/octocode.js astRewrite '{"reasoning":"<why>","path":"/ABS/repo/src","langType":"typescript","ruleKind":"pattern","pattern":"console.log($A)","rewrite":"logger.info($A)"}'
```

Match `range.start`/`range.end` lines are one-based; columns are zero-based UTF-16 code units (an emoji counts 2). `range.byteOffset` is the UTF-8 byte span.

Use the preview's identities and diff to review the change. Inspect `scheme astRewrite --compact` for the current operation constraints before applying. A successful preview alone does not verify applied behavior.

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
whole-program coverage. `payload.coverage` states that scope. When a renamed import
resolves to the same definition but its binding is absent from that set,
`next.searchAliasReferences*` provides an exact-position reference query. Execute
the relevant follow-ups and retain both sets, deduplicating by URI and exact range.
Alias discovery uses native syntax facts; identity requires matching LSP definitions.
Inspection or verification gaps remain explicit in coverage rather than becoming
invented references.

Import token ranges assign alias verification to individual reference pages.
When a grouped page contains more references than its page-size budget,
`next.nextAliasReferences` starts ungrouped inspection with ordinary pagination.
Execute that query unchanged; grouped file counts do not bound alias work.
References recovered through an aliasing import (not reported by the server)
carry `source: "recoveredAlias"`, and `payload.recoveredAliasReferences` counts
them.

Exact `position` anchors pass directly to the language server, including positions
in quoted property names or module paths. `resolvedSymbol.name` is an optional identifier hint for discovery. If a
name cannot be inferred, name-based consumer warmup reports `anchorName` rather
than searching an empty pattern or rejecting the semantic request.

### `lspSearch`

Required fields:

| Field | Required | Notes |
|-------|----------|-------|
| `uri` | Required for anchored, document, and diagnostic operations; one of `uri` or `workspaceRoot` for `workspaceSymbol` | Absolute local file path. For `workspaceSymbol`, `uri` selects one language server. |
| `operation` | Required for an operation-specific request; omission uses the schema default | One of the documented semantic, document, diagnostic, or workspace-symbol operations. Include it in durable examples and continuations. |
| `symbolName` | Required for name-anchored operations and `workspaceSymbol` | Exact symbol text at the target line; omitted with `position`. |
| `lineHint` | Required for name-anchored operations | 1-based line number from search results. Use `position` instead for an exact zero-based UTF-16 position. |

Optional fields:

| Field | Notes |
|-------|-------|
| `orderHint` | Disambiguates repeated symbol text on the same line. |
| `position` | Alternative anchor for semantic symbol operations: zero-based UTF-16 `{line, character}`. Use either `position` or `symbolName` + `lineHint`, never both. |
| `workspaceRoot` | Overrides automatic project-root detection. |
| `rustContext` | Explicit rust-analyzer build context. Requires a `.rs` URI, including for `workspaceSymbol`. See [Rust build context](#rust-build-context). |
| `contextLines` | Adds source previews to call-flow results. Keep `0` unless previews are needed. |
| `page` | Result page copied from an executable semantic continuation. |
| `pageSize` | Semantic items per page. Defaults to `40`. Max `100`. |
| `snapshot` | Content-addressed result-set token copied from `next.nextPage`. Omit on page 1; required on later pages. |
| `depth` | Call- and type-hierarchy depth; `1` (default) returns direct edges. Deeper walks are breadth-first and capped (see the call-flow rules below). |
| `includeDeclaration` | For `references`; defaults to `true`. |
| `groupByFile` | For `references`; adds per-file rollups. |

Semantic types:

| `operation` | Best for | Output |
|--------|----------|--------|
| `definition` | Jumping from usage/import to declaration. TypeScript uses the full semantic server from its first request, so imports and path aliases resolve without a synthetic location. Unresolved provider locations are preserved unchanged. | `payload.kind="definition"`, `locations[]`. |
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

All semantic responses use this envelope:

| Field | Meaning |
|-------|---------|
| `operation` | Requested semantic type. |
| `uri` | Resolved local file path. |
| `resolvedSymbol` | Symbol anchor for symbol-based requests. |
| `lsp` | Server availability and provider/source metadata. |
| `meta.evidence` | Confidence for the bulk result. |
| `meta.diagnostics` | Typed partial-state and terminal-limit information. |
| `summary` | Agent-readable totals for symbol and call-flow requests. |
| `payload` | Typed semantic payload. |
| `pagination` | Native semantic pagination for symbol and call-flow requests. |
| `rustContext` | Normalized requested Rust settings and their fingerprint, when supplied. This field also remains visible on native document-symbol results. |
| `next` | Executable reads, searches, completeness checks, or pagination requests. |

Empty semantic payloads use `payload.kind="empty"` with a machine-readable
`category`: `noLocations` (no locations, calls, symbols, or types), `noHover`,
`noDiagnostics`, `diagnosticsNotPublished`, or `unsupportedOperation`. A successfully executed semantic miss exits with code `0`.
Scripts must inspect the typed payload instead of using the process exit code to
distinguish an empty result.

If the server did not confirm readiness, an empty semantic result includes
`partialReasons: ["readinessUnconfirmed"]` and a warning. This state means the
answer cannot establish absence; inspect the supplied search continuation or
query again after the server finishes indexing. Octocode does not automatically
retry every empty result.

Reference results preserve typed warmup, definition-only, empty, partial, and
continuation metadata in structured and compact presentations. An incomplete
warmup supplies an executable lexical verification query when a name is available;
otherwise it reports a terminal warmup limitation. Zero references do
not establish absence while that partial state is present.

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
`displayRange {startLine, startCharacter, endLine}`, call/type-hierarchy and
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
`next.retry`. Use `contextLines>0` only when source previews are useful.

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
octocode lspSearch '{"reasoning":"<why>","uri":"/ABS/repo/src/lib.rs","operation":"definition","symbolName":"selected","lineHint":5,"rustContext":{"features":["selected"]}}'
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
[engine lifecycle contract](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md#rust-context-and-server-identity)
and [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html).

### Root selection

If `workspaceRoot` is omitted:

1. Files inside `WORKSPACE_ROOT` use that configured root.
2. Files outside `WORKSPACE_ROOT` walk upward to the nearest project marker, such as `package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, or `pyproject.toml`.
3. If no marker exists, the file's directory is used.

### Native compared with server fidelity, and the no-fallback contract

`documentSymbols` has a **native fast path** (oxc for JS/TS, Markdown heading outline) that runs with no language server and is preferred even when a server is present:

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

Octocode starts `typescript-language-server` with
`tsserver.useSyntaxServer:"never"`. This can add startup latency, but it avoids
first-request definition results from the partial syntax server. Definition
locations remain language-server output; Octocode no longer rewrites import
targets with regular expressions.

### Language servers

TypeScript and JavaScript use `typescript-language-server`; JS/TS also has the
server-free document-symbol path above. Built-in routes cover JavaScript,
TypeScript, Python, Rust, Go, Java, C, C++, C#, and Scala. Rust and C/C++ support
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

Use `octocode clasify --input request.json`; inspect `octocode scheme clasify --compact` before hand-authoring a call. Pass one complete `SemanticQuery` directly or batch one to five complete matrices in `queries[]`. Within a query, every question sees every resource, each resource is captured once, and every result is keyed by `queryId`, `resourceId`, and `questionId`. Keep a matrix at 25 cells or fewer and the batch at 50 cells or fewer.

Context is supplied non-empty `{value}` state or one unread `{tool,query}` request. `instructions` is always a non-null, non-empty string, object, or array. Noul criteria are optional; when present, both `true` and `false` are required and their descriptions may be null. Choice requires 2–255 labels whose descriptions may be null. Score requires 2–10 ordered, non-null, non-empty string/object/array levels.

The runtime executes supported read requests under normal policy and returns resource-major results: `resources[].pages[].answers[questionId]`, with the resolved `model` and summed `usage` once per query; it never silently averages or reduces pages. Follow an executable `next.clasify` unchanged, retain error pages, and never use partial coverage to establish global absence. See the [complete contract, research workflow, and examples](OCTOCODE_CLASIFY.md).

---

## Clone and local tools workflow

Use `ghCloneRepo` to bring remote source into local AST, search, and LSP tools. Clone is CLI-only and requires local access plus persistent storage (see [availability](#internal-external-and-hybrid-tools)); inspect the live catalog.

| Need | Tool |
|---|---|
| Read one remote file | `ghGetFileContent` |
| Browse remote paths | `ghSearch operation:"tree"` |
| Inspect a subtree locally | `ghCloneRepo` with `sparsePath` |
| Analyze cross-file semantics | `ghCloneRepo`, then `lspSearch` |

Use the returned `location.localPath` for local queries. Preserve the resolved revision and requested scope; a sparse checkout can omit dependencies required by LSP.

### Two clone modes

- **Full clone** — general exploration; LSP works best with full repositories. Omit `branch` to auto-detect the default.
- **Sparse fetch** — one package/directory of a large monorepo via `sparsePath`; much faster, but LSP cross-file resolution may be limited since not all files are present.

Both return `location.localPath` (absolute), `location.kind` (`repo`/`tree`), `source`, `cached`, `complete`, `commitSha`, and `requestedPath` for sparse. Use `location.localPath` as the absolute `path`/`uri` for local queries. Sparse checkouts use a separate cache key (`{branch}__sp_{hash}/`) and can coexist with a full clone.

```
ghCloneRepo(owner="microsoft", repo="TypeScript", sparsePath="src/compiler")
→ location.localPath = <octocode-home>/tmp/clone/microsoft/TypeScript/main__sp_a3f8c1  (kind: tree, complete: false)
```

### Step-by-step workflows

- **Browse a cloned tree:** `ghCloneRepo` → `structureSearch(operation="tree", path=localPath, maxDepth=2)`, drilling into subdirectories.
- **Deep analysis with LSP:** `ghCloneRepo` → `localSearch` for the symbol + `lineHint` → `lspSearch(operation="definition"|"callers", uri=localPath+"/file", symbolName, lineHint)`.
- **GitHub browse → local:** `ghSearch(operation="tree")` to scout → `ghCloneRepo` → `localSearch` (full regex/type filters) → `lspSearch(operation="references")`.
- **Sparse monorepo package:** scout with `ghSearch(operation="tree")` → `ghCloneRepo(sparsePath=...)` → `localSearch`/`structureSearch(operation="files")` within the subtree.

---

### Cache behavior

| Behavior | Details |
|----------|---------|
| **Materialization TTL** | Clone entries use 24 hours by default (configurable through `OCTOCODE_CACHE_TTL_MS`) |
| **Shared response cache** | `ghSearch`, `ghSearchHistory`, `ghGetHistoryItem`, and `artifactSearch` use per-response freshness periods from 5 minutes to 24 hours |
| **Conditional cache** | `ghGetFileContent` and the `ghSearch` tree operation retain response bodies and ETags for conditional refresh; stale bodies can remain available for up to 24 hours |
| **Response marker** | A result whose primary response payload was served from cache includes `cache: 1`. Fresh results and helper-only cache hits omit `cache`; no other marker value is valid. The contract is identical in CLI and MCP output. |
| **Clone cache** | `ghCloneRepo` uses the clone/materialization cache |
| **Live tools** | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, and `lspSearch` read the workspace directly and don't cache tool results |
| **Location** | Use returned paths. Clone cache keys include ref, sparse scope, and host. Remote response L2 uses `<octocode-home>/tmp/response/` |
| **Identity** | File reads resolve an omitted branch; pass a commit SHA for reproducible reads. Clones accept branch, tag, or full commit SHA and return the actual HEAD as `location.commitSha` |
| **Sparse clones** | Separate cache: `{branch}__sp_{hash}/` |
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

Clones live under `<octocode-home>/tmp/...`, and both the path and execution-context validators automatically add the Octocode home as an allowed root, so a returned `location.localPath` is valid for all local and LSP tools even outside your shell workspace. LSP picks project context from the target file: inside `WORKSPACE_ROOT` it keeps that root; otherwise it walks up to the nearest marker (`package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, `pyproject.toml`). Clone through the CLI with persistent storage; MCP does not expose `ghCloneRepo`. For TS/JS LSP, Octocode uses its bundled `typescript-language-server`; if unavailable, install it (plus `typescript`) on `PATH` or set `OCTOCODE_TS_SERVER_PATH`. LSP can read minified `.js`, but quality is far better on original source.

### Quick reference

| Action | Tool | Key parameter |
|--------|------|---------------|
| Clone repository / branch / one folder | `ghCloneRepo` | `owner`, `repo`, optional `branch` or `sparsePath` |
| Force re-clone | `ghCloneRepo` | `forceRefresh: true` |
| Browse / search / read / find in a clone | `localSearch`, `localFetch` | `path` = `localPath` |
| Definition / references / callers / callees | `lspSearch` | `uri` = file in `localPath` |
