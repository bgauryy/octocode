# Octocode tools reference

This is the field-level reference for the research surface of the Octocode agentic toolkit. It covers every tool exposed through MCP and the CLI. Schemas and descriptions live in `@octocodeai/octocode-core`; execution lives in `@octocodeai/octocode-tools-core`; native search, minify, security, and LSP primitives live in `@octocodeai/octocode-engine`.

Use this page when you need field-level guidance, cross-tool workflows, known behavior, or release verification checks. For MCP tool ratings, quality gaps, per-tool improvement backlogs, and the recommended agent workflow, see [`MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md`](https://github.com/bgauryy/octocode/blob/main/docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md). For the exact active schema in a local checkout, run the compact form first; its `relations` list preserves mode-specific required and mutually exclusive fields:

```bash
npx octocode tools <toolName> --scheme --json --compact
```

## Tool inventory

| Family | Tools |
|--------|-------|
| GitHub | `ghSearch`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` |
| Packages | `artifactSearch` |
| Local | `localSearch`, `localFetch`, `astSearch` |
| LSP | `lspSearch` |

## Contents

- [How every tool call works](#how-every-tool-call-works)
- [Internal, external, and hybrid tools](#internal-external-and-hybrid-tools)
- [Text, AST, graph, and LSP: choose the evidence you need](#text-ast-graph-and-lsp-choose-the-evidence-you-need)
- [GitHub tools reference](#github-tools-reference)
- [Local code tools reference](#local-code-tools-reference)
- [LSP tools reference](#lsp-tools-reference)
- [Clone and local tools workflow](#clone-and-local-tools-workflow)
- [Tool verification playbook](#tool-verification-playbook)
- [MCP tool quality and agent workflow](https://github.com/bgauryy/octocode/blob/main/docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md)

## How every tool call works

The CLI and MCP server expose the same canonical contracts from `@octocodeai/octocode-tools-core`. The interface validates one strict outer object, validates each query against the selected tool and operation, runs independent queries with bounded concurrency, and returns one row for every input position. A failure in one row does not erase successful sibling rows.

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
| `responseCharLength` | Optional outer field | Limits the rendered whole-response text window to 1–50,000 characters. It does not replace a tool's own result pagination. |
| `responseCharOffset` | Optional outer field | Continues a whole-response text window. Copy the returned executable `responsePagination.next` call instead of constructing an offset by hand. |

`goal` and `reasoning` are the only fields shared by every individual query. All other fields belong to a specific tool variant. The schemas are strict: fields from different `operation` branches cannot be mixed, selector pairs such as `startLine`/`endLine` must be complete, and mutually exclusive selectors must not be combined.

### Schema discovery, variants, relations, and hints

```bash
# Catalog: canonical names and availability
npx octocode tools --json

# Compact agent-facing schema: fields plus branch relations
npx octocode tools localSearch --scheme --json --compact

# Full JSON Schema: nested selectors, defaults, limits, and descriptions
npx octocode tools ghGetHistoryItem --scheme --json
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

Common row fields are `index`, optional `status`, optional `cache`, `meta`, and `data`. `status: empty` means the tool produced no usable result for that row; interpret it with tool-specific diagnostics because unsupported capabilities, unresolved anchors, and no matches can all produce empty-like outcomes. `status: error` means that row failed. Neither status should be inferred from missing output. Evidence metadata describes what the result can support: provider-index, lexical, structural, syntactic graph, exact content, or semantic LSP evidence have different failure modes. The shared envelope and continuation contract is maintained in [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md); tool-specific payloads may add their own completeness or partial markers.

Pagination is layered:

1. **Collection pagination** uses tool fields such as `page`, `pageSize`, `matchPage`, a cursor, or operation-specific selectors.
2. **Content pagination** uses `chunkType`/`offset`/`limit` for both file readers; history text uses its own character windows.
3. **Whole-response pagination** uses outer `responseCharLength` and `responseCharOffset` only for the rendered aggregate response.

When output is partial, run the returned schema-valid `next.*` object. Do not stop at a numeric cursor, silently drop later pages, or treat a bounded first page as complete. When continuation is impossible, the tool emits a typed terminal-limit diagnostic instead of a fake next call.

Keep continuation tokens scoped to their surface: operation-level `snapshot` values continue one result set, while the outer response contract uses `responseSnapshot` for whole-response pagination. Do not substitute one for the other; copy the executable `next.*` query returned by the same surface.

## Internal, external, and hybrid tools

"External" describes the data or provider boundary, not the MCP transport. All ten tools can be called through MCP or the CLI.

| Tool | Boundary | How it works |
| --- | --- | --- |
| `ghSearch` | External | Calls GitHub search/tree APIs to discover code, repositories, or a known repository tree. Code search covers the indexed default branch; read exact bytes afterward. |
| `ghGetFileContent` | External | Reads a known GitHub file, ref, range, or match. Full reads return content without creating a local checkout. |
| `ghSearchHistory` | External | Searches GitHub pull-request, issue, or commit metadata. It discovers history identities; it does not replace exact history reads. |
| `ghGetHistoryItem` | External | Reads one known pull request, issue, commit, or comparison, with explicit selectors for bodies, comments, files, reviews, commits, and patches. |
| `artifactSearch` | External | Resolves dependency identities or discovers packages by capability across eight ecosystems. Set `type`; registry metadata and upstream links lead to source research. npm retains registry-scoped authentication. |
| `ghCloneRepo` | Hybrid | Uses provider credentials/network access, then atomically materializes a full or sparse repository under managed local storage. Disabled unless cloning and local storage are enabled. |
| `localSearch` | Internal/local | Runs bounded lexical text/regex search against allowed local paths. |
| `astSearch` | Internal/local | Runs structural AST, filesystem, tree, symbol, and topology queries against allowed local paths. |
| `localFetch` | Internal/local | Reads a known allowed path with full, match, line-range, minified, or symbol-outline views and exact continuations. |
| `lspSearch` | Internal/local with a language-server process | Resolves an anchored symbol and asks a real language server for definitions, references, calls, types, symbols, hierarchy, or diagnostics. It reports unavailable capabilities instead of returning a syntactic approximation as semantic proof. |

Remote GitHub tools require provider runtime and credentials. `artifactSearch` uses official registry APIs; `type:"npm"` honors the effective npm registry configuration. Local tools require `ENABLE_LOCAL`; clone/materialization additionally requires `ENABLE_CLONE` and persistent storage. LSP availability also depends on a compatible server for the file language.

## Text, AST, graph, and LSP: choose the evidence you need

These surfaces complement one another; they are not interchangeable.

| Surface | Octocode operation | Establishes | Does not establish |
| --- | --- | --- | --- |
| Text/regex | `localSearch` | Exact lexical occurrences, paths, and source-line anchors within the scanned scope. | Symbol identity, reachability, or all runtime uses. |
| Structural AST | `astSearch(operation:"match")` with exactly one of `pattern` or `rule` | Syntax-shaped matches that ignore formatting differences and can expose captures. | That two same-shaped nodes refer to the same symbol or execute at runtime. |
| File graph | `astSearch` | Syntactic import topology, candidate paths/cycles, and reachability under stated roots and exclusions. | Symbol-level identity, dynamic imports that were not resolved, or safe deletion by itself. |
| LSP semantics | `lspSearch` | Language-server identity and relations such as definitions, references, callers, callees, implementations, types, symbols, and diagnostics. | Runtime behavior outside the server's configured project/build context. |

Recommended proof ladder:

1. Orient with `astSearch(operation:"tree"|"files")`.
2. Find a lexical anchor with `localSearch`.
3. Use structural search when syntax shape matters or text is noisy.
4. Use `astSearch` to map file-level blast radius or candidate reachability.
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
| `ENABLE_CLONE` | Controls `ghCloneRepo`. Defaults to `false`; set `true` to enable it. Persistent storage and local access are also required. |

Every tool accepts bulk input (`{ "queries": [...] }`), up to 5 queries per call. Page-based tools use `page` and `pageSize`; `limit` is a pre-pagination cap where that distinct control exists. When more results remain, run the matching schema-valid `next.*` call: `nextPage`/`nextMatchPage`, `expandLimit`/`expandScan`, or a content continuation. At an unexpandable public or provider cap, metadata reports `terminalLimitReached` and omits unusable continuations. Numeric page, offset, cursor, and raw `nextQuery` fields are not executable by themselves. `matchString` selects all matching slices; file chunks page that selected view without changing the selector. `ghCloneRepo` is atomic and does not paginate its input. Use `npx octocode tools <toolName> --scheme --json --compact` for the exact active schema and operation scopes.

Search match values and provider text snippets are evidence previews, not collection pagination. A preview may abbreviate visible text only when it retains exact path/line locators and an executable exact-read route; structural capture reduction is explicitly typed with `capturesTruncated` and an executable `next.expandCaptures` replay.

### Choose a GitHub tool

| Need | Tool |
|------|------|
| Search code across GitHub | `ghSearch` with `operation: "code"` |
| Read a file or fetch a directory | `ghGetFileContent` |
| Browse a repository tree | `ghSearch` with `operation: "tree"` |
| Discover repositories | `ghSearch` with `operation: "repositories"` |
| Search PRs, issues, or commits | `ghSearchHistory` with `operation: "pullRequests"`, `"issues"`, or `"commits"` |
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
{ "operation": "code", "keywords": ["useReducer"], "owner": "vercel", "repo": "next.js" }
{ "operation": "repositories", "keywords": ["code research"], "language": "TypeScript" }
{ "operation": "tree", "owner": "vercel", "repo": "next.js", "path": "packages", "maxDepth": 2 }
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

### `ghGetFileContent`

Read one GitHub file or fetch a directory to disk.

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
| `chunkType`, `offset`, `limit` | Same line/UTF-8 byte pagination as `localFetch`; follow `next.continue`. |
| `minify` | `standard` (lossy, language-dependent compression), `none` (no minification), or `symbols` (structural outline). Defaults to `none`, exactly as `localFetch`. Security redaction still applies. |

Choose one extraction intent: whole file, line range, matching slices, or symbol outline. Both readers reject symbol outlines combined with match or line selectors. Selection precedes minification, redaction, and pagination. `chunkType` defaults to `lines` with `limit:100`; `bytes` defaults to 16384 UTF-8 bytes. Offsets are zero-based in the selected view. Line pages have a 16384-byte budget and oversized lines switch to bytes. Byte ends may extend by up to three bytes to finish a code point.

`fullContent:true` requests an unpaged view and rejects chunk controls. Views over 50000 bytes return a typed `fullContentLimit` with a bounded `next.continue` call. A range remains bounded by its original `endLine`; continuing a match preserves its pattern and source-line context. `totalLines` and `sourceBytes` describe the original file; `pagination.totalLines`/`totalBytes` describe the complete selected view. `matchedLines` contains source anchors on the current page and `selectedMatchCount` counts all selected matching lines. `minifyFallback` explains when match evidence or unavailable outlines prevent the requested transform.

File reads return content without creating a checkout. Use `ghSearch operation:"tree"` to browse directories or `ghCloneRepo` with `sparsePath` to create a local subtree.

Examples:

<!-- tool: ghGetFileContent -->
```json
{ "owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/config.ts", "matchString": "export", "contextLines": 2, "chunkType": "lines", "limit": 20 }
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

- `matchString` selects all occurrences with context and source anchors in
  `matchedLines` and `matchRanges`. Follow character continuations if the selected
  content exceeds a window. Both readers disable minification for matches;
  security redaction still applies.
- `minify: "symbols"` returns a paginated outline. Read its source-line gutter,
  then follow up with `startLine`/`endLine` and `minify:"none"`.
- Semantic character windows can expand beyond the requested target. Execute
  `next` unchanged; offsets are exact and `pageCountsKind:"estimated"` identifies
  approximate page counters. Continuations retain the requested character target.
- `standard` compacts source without JS/TS optimization or type-declaration
  removal. It still removes comments and rewrites formatting. Use `none` for
  source quotes and comment-sensitive evidence. See
  [minification coverage](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/SUPPORTED_LANGUAGES_AND_FEATURES.md#minification--file-reads-and-search-fragments).
- Files too large for the `/contents/` API fall back to the Git tree and blob API
  automatically. You do not need to switch to `ghCloneRepo` for size alone.

### `ghSearchHistory`

Search GitHub history through one strict discovery operation per query:

- `operation: "pullRequests"` searches PR candidates.
- `operation: "issues"` searches issue candidates.
- `operation: "commits"` walks commit history, optionally scoped to a path or time range.

The search tool returns candidates and stable identities. Fetch detailed content
with `ghGetHistoryItem`; search queries do not accept singular-item identities.

<!-- tool: ghSearchHistory -->
```json
{ "operation": "pullRequests", "owner": "vercel", "repo": "next.js", "keywords": ["middleware"], "match": ["title"], "state": "merged" }
{ "operation": "issues", "owner": "vercel", "repo": "next.js", "keywords": ["memory leak"], "match": ["title"], "state": "open" }
{ "operation": "commits", "owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/", "since": "30d" }
```

Prefer title-first PR and issue searches. For commit archaeology, narrow by path
and time before fetching a commit diff.

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

<!-- tool: ghGetHistoryItem -->
```json
{ "operation": "pullRequest", "owner": "vercel", "repo": "next.js", "number": 12345, "content": { "changedFiles": true } }
{ "operation": "issue", "owner": "vercel", "repo": "next.js", "number": 12345, "content": { "body": true, "comments": {} } }
{ "operation": "commit", "owner": "vercel", "repo": "next.js", "ref": "abc123", "includeDiff": true }
{ "operation": "compare", "owner": "vercel", "repo": "next.js", "base": "v14.0.0", "head": "v14.1.0" }
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
`astSearch operation:"tree"` to inspect the checkout.

Examples:

<!-- tool: ghCloneRepo -->
```json
{ "owner": "vercel", "repo": "next.js", "branch": "canary" }
{ "owner": "microsoft", "repo": "TypeScript", "sparsePath": "src/compiler" }
```

Rules:

- Use `sparsePath` for large monorepos.
- Use `ghGetFileContent` when you only need one file.
- Cached clones are reused.
- Use the returned path as-is. Cached HEAD identity does not verify uncommitted
  working-tree bytes; check `verified` separately from `complete`.

### `artifactSearch`

Find packages for a capability, resolve a known dependency to registry metadata, or locate its upstream source. Use local tools to explain installed code and GitHub tools when the repository is already known. A repository link is metadata, not implementation or published-version proof.

| Field | Meaning |
|-------|---------|
| `type` | Required ecosystem: `npm` (JavaScript/TypeScript), `pypi` (Python/pip/uv), `crates` (Rust/Cargo), `maven` (Java/Kotlin), `nuget` (.NET), `go` (Go modules), `packagist` (PHP/Composer), or `rubygems` (Ruby/Bundler). |
| `packageName` | Exact ecosystem coordinate, such as `@scope/name`, `group:artifact`, or a Go module path. Exclusive with `keywords`. |
| `keywords` | One or more discovery terms as an array. PyPI supports exact lookup only. |
| `cursor` | Opaque discovery continuation; copy the complete returned `next.nextPage` query. Exact lookup has no pagination controls. |
| `pageSize` | Discovery result count, default 10, range 1–100. |
| `registry` | npm-only HTTP(S) registry override. Omit for npm environment and `.npmrc` routing. Credentials are not tool inputs. |

<!-- tool: artifactSearch -->
```json
{ "type": "npm", "packageName": "react" }
{ "type": "pypi", "packageName": "requests" }
{ "type": "crates", "keywords": ["async", "runtime"], "pageSize": 10 }
{ "type": "maven", "packageName": "org.slf4j:slf4j-api" }
{ "type": "npm", "packageName": "@example/widget", "registry": "https://registry.example.com/" }
```

Each query selects one ecosystem. Compare ecosystems using independent entries in `queries` (maximum five), not `type:"all"`. All providers use official APIs. PyPI keyword discovery returns an unsupported-capability error with exact-lookup guidance; it does not silently fall back to a website or third-party service.

`artifacts[]` contains canonical package identities, registry URLs, and available version, description, license, homepage, and source metadata. Go package paths stay separate from module identities. Optional metadata can be absent; cross-registry popularity scores are not comparable. Read exact source to establish behavior and match the published or installed version before making version-specific claims.

For npm, scoped names honor `@scope:registry`; explicit `registry` takes precedence. Authentication uses registry-scoped npm configuration, including environment interpolation. Configuration reloads per query, caches isolate registry/configuration identities, and continuations preserve the selected registry. Private registries must support npm's search endpoint for discovery. Other ecosystems use official public services.

Discovery continuations preserve type, selectors, page size, registry, and provider state. Follow executable continuations instead of constructing provider page numbers or interpreting cursors. Unknown totals remain unknown; incomplete results and terminal provider limits remain explicit. An empty page may still have a continuation. Authentication, unsupported operations, rate limits, and provider failures remain errors; a missing exact package is empty. Hints appear only for empty/error results; successful pages retain required continuations.

### Token cost control by goal

| Goal | Cheapest approach |
|------|------------------|
| Find if a function exists in a file | `ghSearch(operation:"code")` with `keywords: ["functionName"]` |
| Read one function body | `ghGetFileContent` with `matchString: "function name"` + small `contextLines` |
| Scan a whole file's structure | `ghGetFileContent` with `minify: "symbols"` |
| Read 2–10 functions from a file | Multiple `startLine`/`endLine` reads in one batched call |
| Read a 3MB+ file | `ghCloneRepo` sparse + local read |
| Understand why a PR was made | `ghSearchHistory(operation:"pullRequests")`, then `ghGetHistoryItem(operation:"pullRequest", number)` with `content.body: true` |
| Review a PR's changes | `content.changedFiles: true` first, then `content.patches.mode: "selected"` for relevant files |
| Get all inline code comments on a PR | `content: { comments: { reviewInline: true, discussion: false } }` |
| Count repositories in an org | `owner: "vercel"` with no keywords → `totalMatches` from pagination |
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

Related docs:

- [Local code tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#local-code-tools-reference)
- [LSP tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#lsp-tools-reference)
- [Clone and local tools workflow](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#clone-and-local-tools-workflow)

---

## Local code tools reference

> Complete reference for Octocode MCP local code tools: file system exploration, metadata search, text/regex search, structural AST search, semantic follow-up anchors, and targeted file reading.

---

### Scope

| Tool | Purpose |
|------|---------|
| `localSearch` | Lexical local discovery; matches provide anchors for `lspSearch`. |
| `astSearch` | Structural AST, filesystem, tree, symbol, and topology discovery. |
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
| `ENABLE_CLONE` | Enables `ghCloneRepo` workflows. Defaults to `false`; set `true` to enable cloning. Persistent storage and local access are also required. |
| `TOOLS_TO_RUN` | Strict tool allowlist; include every tool that must remain enabled. Removed compatibility names are rejected. |

Config reference: [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

---

### Platform support

`localSearch` and `astSearch` use Octocode's native in-process ripgrep, structural-search, and filesystem-walker engines. There are no external `rg`, `grep`, `find`, or `tree` dependencies.

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
| Local content pagination | `chunkType`, `offset`, `limit` | `localFetch` selected views |
| Bulk response pagination | `responseCharOffset`, `responseCharLength` | Any local-tool bulk response |

Use native pagination first for result lists, then char pagination only when a single result payload is still too large.

---

### Choose a local tool

| Need | Use |
|------|-----|
| "Which directories/files exist here?" | `astSearch(operation:"tree")` |
| "Find files named `*.test.ts` or modified within a time window." | `astSearch(operation:"files")` |
| "Search for text, regex, imports, TODOs, or identifiers." | `localSearch` |
| "Read this exact file section." | `localFetch` |
| "Find files containing a pattern without match bodies." | `localSearch( resultView:"files", ...)` |
| "Find files that do not contain a pattern." | `localSearch( resultView:"filesWithout", ...)` |

Recommended order for code research:

```text
DISCOVER -> SEARCH -> READ
```

Start broad with structure or metadata, narrow with content search, then read the smallest exact file slice needed.

`localSearch` handles lexical search, while `astSearch` handles structural,
filesystem, tree, symbol, and topology queries. Each public tool rejects fields
from other operations. Removed compatibility names cannot be restored with
`TOOLS_TO_RUN` or `.octocoderc`.

---

### `localSearch`

Lexical local search. The query is selected by `searchText` and `regex`; use
`astSearch` for syntax, filesystem, tree, symbol, or topology queries.

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
| `pattern` | Use `astSearch(operation:"match")` for Octocode code-shaped AST patterns. |
| `rule` | Use `astSearch(operation:"match")` for YAML relational rules. |
| `matchWindow` | With `resultView:"matchOnly"`, widen each matched span by this many characters of context on each side (… marks trimmed sides). 0 = bare match. |
| `unique` | With `resultView:"matchOnly"`, use `list` for distinct match values per file or `count` for frequencies. |
| `contextLines` | Lines around each match. Max 100. |
| `matchContentLength` | Max characters per individual match snippet. Default 500, max 100000. |
| `maxFiles` | Text: per-page ceiling. Structural: scan cap; stats report truncation. |
| `maxMatchesPerFile` | Per-file match page size. Pair with `matchPage` to continue. |
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

When matches are returned, `localSearch` also emits a machine-readable
`next` map for common agent follow-ups:

| Next key | Tool | Purpose |
|----------|------|---------|
| `fetch` | `localFetch` | Read the first hit; adjust `minify` on the follow-up when needed. |
| `lspDefinition` / `lspReferences` | `lspSearch` | Follow the first match semantically when a safe symbol name can be inferred. |
| `nextPage` / `nextMatchPage` | `localSearch` | Continue file-level or per-file match pagination. |

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

**Supported structural extensions:** `c`, `cc`, `cjs`, `cpp`, `cs`, `css`,
`cts`, `cxx`, `gemspec`, `go`, `h`, `hh`, `hpp`, `htm`, `html`, `hxx`,
`java`, `js`, `json`, `jsonc`, `jsx`, `kt`, `kts`, `lua`, `mjs`, `mts`,
`php`, `py`, `pyi`, `rake`, `rb`, `rs`, `ru`, `sbt`, `sc`, `scala`, `scss`,
`sql`, `swift`, `toml`, `ts`, `tsx`, `yaml`, `yml`, and `zig`. Query the
compiled engine capability API when optional grammar features are disabled.

When a code-shaped pattern returns zero matches, tools-core can retry a
semicolon-normalized form or a relaxed return-type form. CLI and MCP output do
expose the retry as a typed `structural.query.rewritten` diagnostic, including
the requested pattern, effective pattern, and an executable continuation that
repeats the effective query explicitly. Use an explicit `rule` query when exact
query equivalence matters.

HTML pattern `<$TAG>` matches parsed start tags, including `script`, `style`,
and self-closing tags. Tag-shaped text inside raw-text elements is not treated
as markup. YAML `kind` rules are checked against the selected grammar before
execution; an unknown node kind returns a typed compile diagnostic instead of a
high-confidence zero-match result.

Java call patterns and CSS/SCSS declaration patterns may omit their trailing
semicolon. The structural compiler supplies grammar-checked statement context
for direct patterns and patterns nested anywhere in a YAML rule; already
complete patterns keep their original parse, match ranges, and captures.
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
```

---

### `astSearch(operation:"tree")`

Directory browsing for understanding shape, ownership, and file distribution.

#### Best for

- Orienting in a new repository.
- Inspecting package/source/test boundaries.
- Finding likely entry points before content search.

#### Key parameters

| Parameter | Description |
|-----------|-------------|
| `path` | Directory to browse. Relative paths resolve from the workspace root. |
| `maxDepth` | Recursion depth; setting it enables traversal. Max 20. Use low depth first. |
| `page` | Result page. |
| `pageSize` | Directory entries per page. Max 50. |
| `limit` | Hard pre-pagination cap. Max 10000. |
| `entryType` | `f` for files only, `d` for directories only; omit for both. |
| `extensions` | Only include files with selected extensions. |
| `namePattern` | Filter entries by glob or substring. |
| `hidden` | Include hidden files and directories. |
| `detail` | `basic` (default), `modified` (+mtime), or `full` (size/permissions/mtime). |
| `sort` | Sort field. |
| `reverse` | Reverse sort order. |

#### Output

The response separates structured `files[]` and `folders[]` and includes summary and pagination metadata when applicable.

#### Examples

```bash
astSearch(operation="tree", path=".", maxDepth=1)
astSearch(operation="tree", path="packages/octocode-mcp/src", maxDepth=2, entryType="d")
astSearch(operation="tree", path="docs", extensions=["md"], detail="full")
```

---

### `astSearch(operation:"files")`

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
astSearch(operation="files", path=".", names=["*.test.ts"])
astSearch(operation="files", path="packages", pathRegex="^readme\\.md$")
astSearch(operation="files", path=".", time={"modifiedWithin":"24h"}, entryType="f", detail="full")
```

---

### `localFetch`

Read a known local path. Path-only reads are valid and return exact source subject to redaction. Optionally select a source range (`startLine` + `endLine`, one-based inclusive), `matchString`, or `fullContent:true`; these selectors are mutually exclusive.

| Field | Meaning |
| --- | --- |
| `chunkType` | `lines` (default) or UTF-8 `bytes` in the selected returned view. |
| `offset` | Zero-based view offset; byte offsets must start on code-point boundaries. Default 0. |
| `limit` | Requested lines (default 100) or bytes (default 16384), from 1 through 50000. |
| `matchString` | Nonempty literal source text; enable `matchStringIsRegex` for regex or `matchStringCaseSensitive` for case sensitivity. |
| `contextLines` | Explicit source-line context per side, 0–100; default 5 for line chunks. Exclusive with `contextBytes`. |
| `contextBytes` | UTF-8 context bytes per side, 0–16384; default 256 for byte chunks. Requires `matchString`. Full-source redaction precedes byte matching; edges expand to whole code points, and disjoint windows are joined by a newline. |
| `minify` | `none` (default), `standard` compact source, or `symbols` whole-file outline. Symbols cannot accompany range/match selectors. |
| `fullContent` | Complete unpaged view within resource/security limits; cannot accompany chunk controls. |

Selection precedes minification, redaction, and pagination. Line pages preserve complete lines within a 16384-byte budget. An oversized line switches to byte paging from the unreturned position. Byte ends extend by at most three bytes to finish a UTF-8 code point. Copy the complete `next.continue` query; do not calculate offsets. Continuations stop at the selected range or matched view.

Every successful text read reports original-file `totalLines` and `sourceBytes`, including empty files and no matches. `pagination.totalLines`/`totalBytes` describe the selected returned view. `returnedBytes`, `returnedLines`, and `returnedChars` describe the current chunk (characters count UTF-16 code units). Content has no injected line numbers; exact views preserve whitespace and line endings.

`matchRanges` describe all selected source context windows; `matchedLines` contains matching source anchors intersecting the current page, and `selectedMatchCount` counts matching source lines in the selected view. Overlapping context windows are merged. `matchString` forces exact content so minification cannot remove the evidence. `minifyFallback` reports the requested/applied modes and reason when a match forces exact content or an outline is unavailable.

Security scanning applies to the complete selected view before paging. Above the scanner's 10,000,000-byte limit, `contentSecurityLimit` provides a smaller-source-range alternative when possible, otherwise an explicit terminal limit. File totals unavailable due to access or resource limits are identified as unavailable. A full-content view over 50000 bytes supplies executable bounded recovery.

```bash
localFetch(path="/ABS/repo/src/index.ts", startLine=1, endLine=80)
localFetch(path="/ABS/repo/README.md", matchString="Configuration", contextBytes=256, chunkType="bytes", limit=1024)
localFetch(path="/ABS/repo/src/index.ts", minify="symbols")
```

---

### `astSearch`

The `coverage` object separates parser inventory from module-linking support.
It reports language coverage, resolved and external import counts, unresolved
internal imports, unsupported linking, and parse-recovery diagnostics. These
gaps make the result partial even when every result page has been returned.
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
are not proof of symbol identity. Lexical occurrence counts are conservative
retention evidence and still require LSP confirmation for deletion decisions.

One bounded repository graph provides six operations: `dependencies`, `dependents`, `path`, `reachability`, `cycles`, and `deadCode`. Import edges come from native syntax facts. Traversal and path results report exact `edgeKinds`: `static-import`, `type-import`, `dynamic-import`, `named-reexport`, `star-reexport`, `type-named-reexport`, `type-star-reexport`, `commonjs-require`, `create-require`, `python-import`, `rust-module`, `rust-use`, `c-include`, and `metadata-import`. Rust module/use edges, C includes, metadata, erased types, and edges without provenance do not establish runtime import cycles.

Cross-file resolution covers JavaScript/TypeScript ESM and binding-safe CommonJS, Rust modules, bounded Python absolute and relative imports, and quoted relative C/C++ includes. Literal CommonJS loads link only when `require`, `module.require`, or an imported `createRequire(import.meta.url)` binding is not shadowed or reassigned. Dynamic and ambiguous loaders remain explicit diagnostics. Python wildcard and ambiguous package-attribute imports remain diagnostics, as do C/C++ system and macro includes. Explicit relative `package.json` imports can link to a manifest inside the root or the nearest ancestor boundary; these manifests are validated, limited to 64 KiB, count against `maxFiles`, and remain metadata leaves. Namespace-style imports conservatively retain target exports during dead-code analysis.

Dependency traversal also reports immediate dominators, topological layers, and transitively redundant condensation-DAG edges. Cycle results distinguish runtime import candidates (`runtimeCycle`) from other topology SCCs, expose condensation metadata, and return deterministic directed witnesses in `cycleEdges` and `runtimeCycleEdges`; every witness edge includes `from`, `to`, and `edgeKinds`. Native facts also contain `call` and `contains` relations, but the public operations don't project those symbol-level edges. `deadCode` results are candidates, not deletion proof.

#### Best for

- Tracing forward dependencies or reverse dependents to a bounded depth.
- Finding the shortest directed import path between two files.
- Finding mandatory dependency chokepoints, topological layers, and redundant edges.
- Classifying entrypoint reachability and finding strongly connected import cycles.
- Finding repository-wide dead-export candidates and dead clusters in one pass. A dead cluster is a strongly connected set of mutually importing, unreachable files; the files don't necessarily call one another.

Use `astSearch` to discover repository-scale file topology and candidate reachability. Use `lspSearch` with `references`, `callers`, or `callees` to prove the identity and semantic connections of one known symbol. A graph edge proves that one file syntactically imports or re-exports another; it doesn't prove which binding is used.

#### Key parameters

| Parameter | Description |
|-----------|-------------|
| `operation` | Required: `dependencies`, `dependents`, `path`, `reachability`, `cycles`, or `deadCode`. |
| `path` | Repository root to analyze. Required. |
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

Results never dump the complete graph: the result list is paginated, SCC/dead-cluster members cap at 50 files with `size` and `truncated`, and complete shortest paths cap at 100 files. Longer paths return `complete:false`, empty `files`/`edges`, bounded `prefix` and `suffix`, the target, total file count, and omitted-middle count so a prefix cannot be mistaken for a complete source-to-target path. A five-query large-repository batch must remain at or below 32 KiB in compact structured output; use pagination instead of expanding nested collections.

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
astSearch(operation="topology", analysis="dependencies", path="/ABS/repo", file="src/index.ts", depth=2)
astSearch(operation="topology", analysis="cycles", path="/ABS/repo", pageSize=20, limit=100)
astSearch(operation="topology", analysis="deadCode", path="/ABS/repo", entrypoints=["src/index.ts"], includeTests=false)
```

For a cycle, read the exact imports named by `cycleEdges`; use `runtimeCycleEdges` when investigating loading behavior. Verify a dead-code or transitive-edge candidate with `lspSearch` before removing it.

---

### Local workflows

#### Explore a new repository

```text
astSearch(operation="tree", path=root, maxDepth=1)
astSearch(operation="tree", path=root+"/src", maxDepth=2)
astSearch(operation="files", path=root, names=["package.json", "tsconfig.json", "README.md"])
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
astSearch(operation="files", path=".", names=["*.test.ts", "*.spec.ts"])
localSearch( path="tests", searchText="featureName", resultView="files")
localFetch(path="tests/feature.test.ts", matchString="featureName")
```

#### Inspect recent changes

```text
astSearch(operation="files", path=".", time={"modifiedWithin":"24h"}, entryType="f", detail="full")
localSearch( path=".", searchText="TODO|FIXME", regex="rust")
```

---

### Local tool rules

1. Use `astSearch(operation:"tree")` or `astSearch(operation:"files")` before reading when the file is unknown.
2. Use `localSearch( resultView:"files")` for fast discovery when match bodies are not needed.
3. Use `localSearch` with `contextLines` before opening a large file.
4. Use `localFetch` with `matchString`, `startLine`/`endLine`, or `minify:"symbols"` instead of `fullContent` for large files.
5. Use pagination fields when a response advertises `hasMore=true`.

---

### Response shape

- Tool results use the shared `results[]` row envelope; each row may contain `index`, `status`, `meta`, and `data`. Tool-specific payloads own their pagination and hints; see [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md) for the common rules.
- `localSearch` returns lexical matches; `astSearch` returns operation-specific structural, file, tree, symbol, or topology payloads.
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

### Local tools: related documentation

- [Clone and local tools workflow](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#clone-and-local-tools-workflow) - cloning repositories before local analysis.
- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#github-tools-reference) - remote GitHub search, fetch, clone, and PR tools.
- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) - environment variables and config file behavior.

---

## LSP tools reference

This is the canonical reference for Octocode's semantic code-intelligence operations. LSP is the protocol layer behind these operations; structural AST search is exposed by `astSearch`.

Octocode exposes **one** public semantic tool:

| Tool | Use it for |
|------|------------|
| `lspSearch` | Definitions, references, callers, callees, bidirectional call hierarchy, hover, document, and workspace symbols, type definitions, implementations, type hierarchy, and diagnostics. |

Semantic operations are local-only. Local tools default on for both CLI and MCP; set `ENABLE_LOCAL=false` to disable them. LSP needs a file that exists on disk. Use `localSearch` first when you need a symbol `lineHint`; `astSearch(operation:"match")` can provide AST-derived anchors before LSP proves symbol identity.

For external repos: clone first with `ghCloneRepo` (set `sparsePath` for a subtree), then use the returned `localPath` as the `uri` prefix for `lspSearch`. The path is always absolute and immediately valid.

### Workflow

1. Search with `localSearch` or `astSearch(operation:"match")`, then read the observed source with `localFetch` to verify the exact symbol spelling and line.
2. Query `lspSearch` with `uri`, `operation`, and either `symbolName` plus a 1-based `lineHint` or a zero-based UTF-16 `position`.
3. Page large symbol or call-flow results by executing `next.nextPage` unchanged;
   pages after the first require its snapshot token.
4. Run project lint, typecheck, and tests before claiming risky changes are fully verified.

### `lspSearch`

Required fields:

| Field | Required | Notes |
|-------|----------|-------|
| `uri` | Required for anchored, document, and diagnostic operations; one of `uri` or `workspaceRoot` for `workspaceSymbol` | Absolute local file path. For `workspaceSymbol`, `uri` selects one language server. |
| `operation` | Required for an operation-specific request; omission uses the schema default | One of the documented semantic, document, diagnostic, or workspace-symbol operations. Include it in durable examples and continuations. |
| `symbolName` | Required for anchored operations and `workspaceSymbol` | Exact symbol text at the target line for anchored operations. |
| `lineHint` | Required for name-anchored operations | 1-based line number from search results. Use `position` instead for an exact zero-based UTF-16 position. |

Optional fields:

| Field | Notes |
|-------|-------|
| `orderHint` | Disambiguates repeated symbol text on the same line. |
| `position` | Alternative anchor for semantic symbol operations: zero-based UTF-16 `{line, character}`. Use either `position` or `symbolName` + `lineHint`, never both. |
| `workspaceRoot` | Overrides automatic project-root detection. |
| `rustContext` | Explicit rust-analyzer build context. Requires a `.rs` URI, including for `workspaceSymbol`. See [Rust build context](#rust-build-context). |
| `contextLines` | Adds source previews to call-flow results. Keep `0` unless previews are needed. |
| `page` | Result page for `documentSymbols` and call-flow results. |
| `pageSize` | Semantic items per page. Defaults to `40` for `documentSymbols`, `10` for call-flow. Max `100`. |
| `snapshot` | Content-addressed result-set token copied from `next.nextPage`. Omit on page 1; required on later pages. |
| `depth` | Call-flow recursion depth. Keep `1` unless you need nested calls. |
| `includeDeclaration` | For `references`; defaults to `true`. |
| `groupByFile` | For `references`; adds per-file rollups. |

Semantic types:

| `operation` | Best for | Output |
|--------|----------|--------|
| `definition` | Jumping from usage/import to declaration. TypeScript uses the full semantic server from its first request, so imports and path aliases resolve without a synthetic location. Unresolved provider locations are preserved unchanged. | `payload.kind="definition"`, `locations[]`. |
| `references` | Affected references for functions, types, variables, constants, and classes. | `locations[]`, `totalReferences`, `totalFiles`, optional `byFile`. |
| `callers` | Static incoming calls to a callable symbol. | Compact `calls[]`, `summary.incomingCalls`, pagination. |
| `callees` | Static outgoing calls made by a callable symbol. | Compact `calls[]`, `summary.outgoingCalls`, pagination. |
| `callHierarchy` | Bidirectional call-flow snapshot. | Incoming and outgoing calls in one compact page. |
| `hover` | Quick type/signature/docs from the language server. | `markdown` or `text`. |
| `documentSymbols` | File outline and symbol inventory. | Compact `symbols[]`, `summary.kinds`, pagination. |
| `typeDefinition` | Declared type behind a symbol. | `locations[]`. |
| `implementation` | Concrete implementation behind an interface/abstract symbol when the server supports it. | `locations[]`. |
| `workspaceSymbol` | Symbols reported by one language server for a workspace. Provide `uri` to select the language. This operation does not merge results from every language server. | `symbols[]`, `totalSymbols`. |
| `supertypes` | Direct or recursive supertypes when the server advertises type hierarchy. | Type-hierarchy items or typed `unsupportedOperation`. |
| `subtypes` | Direct or recursive subtypes when the server advertises type hierarchy. | Type-hierarchy items or typed `unsupportedOperation`. |
| `diagnostic` | Pull diagnostics when the server advertises a pull-diagnostic provider. Servers that publish diagnostics only through push notifications return `unsupportedOperation`. | Diagnostics or typed `empty`. |

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
`category`, such as `symbolNotFound`, `noLocations`, `noReferences`, `noHover`,
or `noCalls`. A successfully executed semantic miss exits with code `0`.
Scripts must inspect the typed payload instead of using the process exit code to
distinguish an empty result.

If the server did not confirm readiness, an empty semantic result includes
`partialReasons: ["readinessUnconfirmed"]` and a warning. This state means the
answer cannot establish absence; inspect the supplied search continuation or
query again after the server finishes indexing. Octocode does not automatically
retry every empty result.

Reference results preserve typed warmup, definition-only, empty, partial, and
continuation metadata in structured and compact presentations. An incomplete
warmup supplies an executable lexical verification query; zero references do
not establish absence while that partial state is present.

Paginated semantic results are sorted deterministically and fingerprint the
canonical query plus the complete result set. Follow `next.nextPage` unchanged.
If the server's results change between requests, Octocode returns no page rows,
the typed `paginationChanged` diagnostic, and `next.restartPagination`; discard
previously collected pages and restart at page 1. A later page without a token
returns `paginationSnapshotRequired`. Tokens validate a recomputed result set
across processes; they do not retain historical rows.

Call-flow payloads include the target item, every provider call range, and
`rangeCount`. Ranges preserve both endpoints (`line`, `character`, `endLine`,
`endCharacter`): lines are 1-based and characters are 0-based UTF-16 offsets.
Compact text renders each range as `line:character-endLine:endCharacter`.
Call-item pagination and whole-response text pagination preserve these ranges;
presentation does not sample them. Use `contextLines>0` only when source previews
are useful.
An `expandDepth` continuation appears only when unvisited project calls remain;
filtered standard-library calls do not make an otherwise complete result look
depth-truncated.

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
octocode tools lspSearch --queries '{"uri":"/ABS/repo/src/lib.rs","operation":"definition","symbolName":"selected","lineHint":5,"rustContext":{"features":["selected"]}}' --json
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
[engine lifecycle contract](../packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md#rust-context-and-server-identity)
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

Every **other** semantic operation — `references`, `definition`, `hover`, `callers`/`callees`/`callHierarchy`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`/`subtypes`, `diagnostic` — requires a real server. When no server is available octocode **does not fall back to a syntactic guess**: it returns `status:"error"` with `errorCode:"lspServerUnavailable"` and a message directing you to lexical `localSearch` or structural `astSearch` + `localFetch`. (There is no longer a same-file-only `references` native path — a partial answer that silently omits cross-file usages is a trap, so it now errors instead.) See [LSP server lifecycle](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md).

### TypeScript backends

The TS/JS server resolves in this order:

1. `OCTOCODE_TS_SERVER_PATH` — explicit override (args auto-selected: `--lsp -stdio` if the path is `tsgo`, else `--stdio`).
2. **`tsgo` on `PATH`** — Microsoft's Go-native server (`tsgo --lsp -stdio`, Node-free, ~10× faster). Opt-in: present-on-PATH only, no flag. References/rename are still maturing upstream.
3. **`typescript-language-server`** — the bundled zero-config default.

For the bundled default, Octocode first honors an executable
`typescript-language-server` already available on `PATH`. If the command is not
available, the resolver looks for `node_modules/typescript-language-server/lib/cli.mjs`
from the detected `workspaceRoot` and then from Octocode's package root. That
fallback keeps cloned or external workspaces working without installing a
language server inside every analyzed repository; the CLI path is run through the
current Node executable.

Octocode starts `typescript-language-server` with
`tsserver.useSyntaxServer:"never"`. This can add startup latency, but it avoids
first-request definition results from the partial syntax server. Definition
locations remain language-server output; Octocode no longer rewrites import
targets with regular expressions.

### Language servers

TypeScript and JavaScript are bundled through `typescript-language-server` and
`typescript`; JS/TS also has the server-free document-symbol path above.
Python, YAML, JSON, HTML, CSS, SCSS, and Less also have packaged server
resolvers. Rust and C/C++ support managed downloads. The remaining built-in
routes resolve host or user-provided executables.

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
| `OCTOCODE_BASH_SERVER_PATH` | Shell |
| `OCTOCODE_PHP_SERVER_PATH` | PHP |
| `OCTOCODE_SQL_SERVER_PATH` | SQL |
| `OCTOCODE_SWIFT_SERVER_PATH` | Swift |
| `OCTOCODE_JSON_SERVER_PATH` | JSON |
| `OCTOCODE_YAML_SERVER_PATH` | YAML |
| `OCTOCODE_HTML_SERVER_PATH` | HTML |
| `OCTOCODE_CSS_SERVER_PATH` | CSS/SCSS/Less |
| `OCTOCODE_RUBY_SERVER_PATH` | Ruby |
| `OCTOCODE_KOTLIN_SERVER_PATH` | Kotlin |
| `OCTOCODE_ELIXIR_SERVER_PATH` | Elixir |

#### Custom / bring-your-own servers

Scala has no built-in server. To support it, or to
replace a built-in server, register it in a JSON config. Octocode loads the
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
    ".scala": { "command": "metals", "args": ["stdio"], "languageId": "scala" }
  }
}
```

`command` and `languageId` are required; `args` (default `[]`) and `initializationOptions`
(passed verbatim in `initialize`) are optional. With the config present, the server can answer the
semantic operations it advertises; without it the extension is unsupported and semantic ops throw
`lspServerUnavailable` (→ fall back to `localSearch`). See
[`LSP_SERVER_LIFECYCLE.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md#custom--bring-your-own-lsp-any-language).

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

### LSP: related documentation

- [Local code tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#local-code-tools-reference)
- [Clone and local tools workflow](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#clone-and-local-tools-workflow)

---

## Clone and local tools workflow

Use `ghCloneRepo` to bring remote source into local AST, search, and LSP tools. Clone is opt-in and requires local access plus persistent storage; inspect the live catalog.

| Need | Tool |
|---|---|
| Read one remote file | `ghGetFileContent` |
| Browse remote paths | `ghSearch operation:"tree"` |
| Inspect a subtree locally | `ghCloneRepo` with `sparsePath` |
| Analyze cross-file semantics | `ghCloneRepo`, then `lspSearch` |

Use the returned `location.localPath` for local queries. Preserve the resolved revision and requested scope; a sparse checkout can omit dependencies required by LSP.

### Two clone modes

#### Mode 1: Full clone

Best for general exploration where you need full project context (LSP works best with full repositories).

```
ghCloneRepo:
  owner: "vercel"
  repo: "next.js"
  # branch omitted → auto-detects default branch
```

**Result:**
```yaml
owner: vercel
repo: next.js
branch: main
localPath: <octocode-home>/tmp/clone/vercel/next.js/main
location:
  kind: repo
  localPath: <octocode-home>/tmp/clone/vercel/next.js/main
  repoRoot: <octocode-home>/tmp/clone/vercel/next.js/main
  source: clone
  cached: false
  complete: true
```

Use `location.localPath` as the absolute `path` for local search, tree, or fetch queries.

#### Mode 2: Sparse folder fetch

Best for large monorepos where you only need one package/directory. Dramatically faster.

```
ghCloneRepo:
  owner: "microsoft"
  repo: "TypeScript"
  sparsePath: "src/compiler"
```

**Result:**
```yaml
owner: microsoft
repo: TypeScript
branch: main
localPath: <octocode-home>/tmp/clone/microsoft/TypeScript/main__sp_a3f8c1
sparsePath: "src/compiler"
location:
  kind: tree
  localPath: <octocode-home>/tmp/clone/microsoft/TypeScript/main__sp_a3f8c1
  repoRoot: <octocode-home>/tmp/clone/microsoft/TypeScript/main__sp_a3f8c1
  source: clone
  cached: false
  complete: false
  requestedPath: "src/compiler"
```

> **Note:** LSP may have limited cross-file resolution in sparse checkouts since not all source files are present. If you need full project context, clone without `sparsePath`.

### Step-by-step workflows

#### Workflow 1: Browse a cloned repository tree

**Goal:** Understand the structure of an external repository using local tools.

```
Step 1: Clone the repo
  ghCloneRepo(owner="vercel", repo="next.js")
  → localPath = "<octocode-home>/tmp/clone/vercel/next.js/canary"

Step 2: Browse the tree
  astSearch(operation="tree", path=localPath, maxDepth=2)
  → See the full directory structure with file sizes and dates

Step 3: Drill into a directory
  astSearch(operation="tree", path=localPath + "/packages/next/src", maxDepth=2)
  → See the subdirectory contents
```

#### Workflow 2: Deep code analysis with LSP

**Goal:** Trace who calls a function in an external repository.

```
Step 1: Clone the repo
  ghCloneRepo(owner="vercel", repo="next.js", sparsePath="packages/next/src")
  → localPath

Step 2: Search for the function
  localSearch( path=localPath, searchText="handleRequest")
  → Get file paths and lineHint values

Step 3: Jump to definition
  lspSearch(operation="definition", uri=localPath+"/server/router.ts", symbolName="handleRequest", lineHint=42)
  → See the function definition

Step 4: Trace callers
  lspSearch(operation="callers", uri=..., symbolName="handleRequest", lineHint=42)
  → See all functions that call handleRequest
```

#### Workflow 3: From GitHub browsing to deep local analysis

**Goal:** You're browsing a repository on GitHub and want to go deeper.

```
Step 1: Browse on GitHub first (quick)
  ghSearch(operation="tree", owner="pallets", repo="flask", maxDepth=2)
  → See the tree, find interesting directory "src/flask"

Step 2: Clone for deep analysis
  ghCloneRepo(owner="pallets", repo="flask")
  → localPath

Step 3: Use full ripgrep power
  localSearch( path=localPath, searchText="def route\\(", langType="py")
  → Full regex search, file type filtering, match context

Step 4: Use LSP
  lspSearch(operation="references", uri=localPath+"/src/flask/app.py", symbolName="route", lineHint=...)
  → Find every file that uses the @route decorator
```

#### Workflow 4: Sparse fetch of a monorepo package

**Goal:** Analyze one package in a large monorepo without cloning the entire thing.

```
Step 1: Browse the monorepo structure on GitHub (quick discovery)
  ghSearch(operation="tree", owner="microsoft", repo="TypeScript", path="src", maxDepth=1)
  → See packages: compiler, services, harness, ...

Step 2: Clone only the compiler
  ghCloneRepo(owner="microsoft", repo="TypeScript", sparsePath="src/compiler")
  → localPath (only downloads src/compiler, much faster)

Step 3: Search within the fetched subtree
  localSearch( path=localPath, searchText="transformTypeScript")
  → Search only within the compiler code

Step 4: Find files by metadata
  astSearch(operation="files", path=localPath, names=["*.ts"], time={"modifiedWithin":"30d"})
  → TypeScript files in the compiler modified within 30 days
```

---

### Cache behavior

| Behavior | Details |
|----------|---------|
| **Materialization TTL** | Clone entries use 24 hours by default (configurable through `OCTOCODE_CACHE_TTL_MS`) |
| **Shared response cache** | `ghSearch`, `ghSearchHistory`, `ghGetHistoryItem`, and `artifactSearch` use per-response freshness periods from 5 minutes to 24 hours |
| **Conditional cache** | `ghGetFileContent` and the `ghSearch` tree operation retain response bodies and ETags for conditional refresh; stale bodies can remain available for up to 24 hours |
| **Response marker** | A result whose primary response payload was served from cache includes `cache: 1`. Fresh results and helper-only cache hits omit `cache`; no other marker value is valid. The contract is identical in CLI and MCP output. |
| **Clone cache** | `ghCloneRepo` uses the clone/materialization cache |
| **Live tools** | `localSearch`, `localFetch`, `astSearch`, and `lspSearch` read the workspace directly and don't cache tool results |
| **Location** | Use returned paths. Clone cache keys include ref, sparse scope, and host. Remote response L2 uses `<octocode-home>/tmp/response/` |
| **Identity** | File reads resolve an omitted branch; pass a commit SHA for reproducible reads. Clones accept branch, tag, or full commit SHA and return the actual HEAD as `location.commitSha` |
| **Sparse clones** | Separate cache: `{branch}__sp_{hash}/` |
| **Coexistence** | Full clone and sparse clones of the same repository can coexist |
| **Cache hit** | Reuses a clone checkout. Cached working files are not reverified; inspect `verified` separately from scoped `complete`. |
| **Expired** | Owned entries are evicted when requested and by the shared 24-hour lifecycle |
| **Force refresh** | Set `forceRefresh: true` in the query to bypass cache and re-clone/re-fetch |
| **Periodic GC** | CLI tool-runtime bootstrap performs a persisted due-check once per process and exits without a timer. MCP performs the same bootstrap check, then uses an unreferenced deadline timer. Both use one persisted 24-hour marker. A cross-process lock prevents duplicate sweeps; a cleanup failure doesn't block startup. |
| **Cleanup scope** | Automatic maintenance removes expired entries only from owned clone, tree, response, and managed artifact roots. It preserves unrelated files under `tmp`. |
| **Response limits** | Response entries also obey configurable per-entry and total-disk limits. See [Response cache](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#response-cache). |
| **Manual clear** | `octocode cache clear --clone` and `--tree` are selective. `--all` removes the entire Octocode `tmp` directory. This deletes response entries and maintenance metadata. There is no response-only clear flag. |

---

### Path validation: why it works

Local tools validate all paths against allowed roots. Cloned repositories are accessible because:

1. **Tmp destination**: `<octocode-home>/tmp/...` is under the Octocode home directory
2. **PathValidator and ExecutionContextValidator**: Both automatically add Octocode home as an allowed root alongside the workspace directory
3. **Workspace root resolution**: Local tools validate paths against allowed roots, and LSP tools automatically choose project context from the target file path. If a cloned file is inside `WORKSPACE_ROOT`, Octocode keeps that root; otherwise it walks up from the file to the nearest project marker (`package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, `pyproject.toml`, etc.)
4. **Result**: The `location.localPath` returned by `ghCloneRepo` is automatically valid for all local + LSP tools, even when the cloned repository lives outside your current shell workspace

For MCP, set `ENABLE_CLONE=true` and leave local tools enabled. The CLI defaults both local and clone support on unless explicitly disabled.

For TypeScript/JavaScript LSP:

- Octocode tries its bundled `typescript-language-server` first.
- If that bundled server is not available in your environment, install `typescript-language-server` + `typescript` on `PATH`, or set `OCTOCODE_TS_SERVER_PATH`.
- LSP can analyze bundled/minified `.js` files, but semantic quality is usually much better on original source trees than on large generated artifacts.

---

### Quick reference

| Action | Tool | Key Parameter |
|--------|------|---------------|
| Clone entire repository | `ghCloneRepo` | `owner`, `repo` (branch auto-detected) |
| Clone specific branch | `ghCloneRepo` | `owner`, `repo`, `branch` |
| Clone one folder | `ghCloneRepo` | `owner`, `repo`, `sparsePath` (branch auto-detected) |
| Force re-clone | `ghCloneRepo` | `forceRefresh: true` (bypasses valid cache) |
| Browse cloned tree | `localSearch` | `path` = `localPath` |
| Search cloned code | `localSearch` | `path` = `localPath` |
| Read cloned file | `localFetch` | `path` = `localPath + "/file.ts"` |
| Find files in clone | `localSearch` | `path` = `localPath` |
| Jump to definition | `lspSearch` with `operation="definition"` | `uri` = file in `localPath` |
| Find all references | `lspSearch` with `operation="references"` | `uri` = file in `localPath` |
| Trace callers/callees | `lspSearch` with `operation="callers"` or `operation="callees"` | `uri` = file in `localPath` |

---

### Clone workflow: related documentation

- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#github-tools-reference) — Full `ghCloneRepo` parameter reference
- [Local code tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#local-code-tools-reference) — Local filesystem search, structure, metadata, and content tools
- [LSP tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#lsp-tools-reference) — Semantic content tool
- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) — `ENABLE_LOCAL`, `ENABLE_CLONE`, and other settings

---

## Tool verification playbook

This playbook verifies that every Octocode MCP tool works as a research tool, not merely as a callable function. Use it before releases, after schema changes, after response-shape changes, and after changes to pagination, hints, security, LSP behavior, provider mapping.

### Source of truth

Public tool names, descriptions, executable input schemas, relations, and examples come from `@octocodeai/octocode-core/schema` in the sibling `octocode-mcp-host` repository. Input preparation and pure schema/presentation helpers use the same core entrypoint. Shared server instructions and CLI context come from `@octocodeai/octocode-core/mcp`; transports supply runtime availability. The [tools-core registry](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/tools/toolConfig.ts) attaches execution, availability, and security to those contracts; interface packages register or render them.

Response behavior is shared through [packages/octocode-tools-core/src/utils/response/bulk/response.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/utils/response/bulk/response.ts), [packages/octocode-tools-core/src/utils/pagination/core.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/utils/pagination/core.ts), [packages/octocode-tools-core/src/utils/pagination/hints.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/utils/pagination/hints.ts), and [packages/octocode-tools-core/src/types/toolOutput.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/types/toolOutput.ts).

This playbook extends the existing contract tests: [the all-tools pagination contract test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/tests/tools/all-tools.pagination-contract.test.ts), [the all-tools schema contract test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/tests/scheme/all-tools.schema-contract.test.ts), [the private-registry flow test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/tests/tools/package_search/privateRegistry.test.ts), and [the response contract test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/tests/utils/response/responses.contract.test.ts).

### Verification goals

Every tool must pass the same top-level contract:

| Area | Required checks |
| --- | --- |
| Registration | Tool is present in `ALL_TOOLS`, has a direct execution definition, has an MCP input schema, publishes no output schema, and registers with the expected security wrapper. |
| Bulk envelope | `queries` accepts 1 to 5 items without caller IDs. Response rows use matching zero-based `index` values, preserve input order, isolate per-query errors, and do not let one failed query block siblings. |
| Output shape | Responses expose machine data in `structuredContent.results[]` and preserve the complete sanitized YAML or JSON representation in `content[0].text` for every MCP client. Lean hoists apply: `base` relativizes absolute `path`/`uri`, `shared` collapses constants identical across leaves (identity keys `owner`/`repo`/`name`/`id` are never hoisted). |
| Pagination | Native page fields, query-level `charOffset`/`charLength`, and top-level `responseCharOffset`/`responseCharLength` work independently and together. Pagination hints appear only when `hasMore=true`. |
| Hints | Tool `hints.ts` files expose only `empty` and `error`. Empty hints are conditional and filter-aware. Error hints classify the failure and stay short. Successful rows omit prose hints and optional next-tool suggestions; typed diagnostics and executable continuations remain. |
| Empty results | Successful no-match responses are not errors. They must include a clear empty signal, preserve the query's ordered index, and provide recovery hints only when the query context makes a concrete next step possible. |
| Errors | Provider, validation, path, auth, rate-limit, timeout, LSP-unavailable, and command failures return structured errors with recovery context and without leaking secrets. |
| Evidence | Preserve the fields each tool returns, including `evidence.kind`/`confidence`, row status, and tool-specific partial or completeness markers. Do not infer universal `answerReady` or `complete` fields; partial, fallback, and unsupported states must remain explicit. |
| Security | Local tools respect path validation and command allow-lists. Remote tools sanitize errors and redact secrets. Cloning does not write outside the intended checkout root; file fetches do not create checkouts. |

### Global scenario matrix

Run these scenarios for every tool before adding tool-specific edge cases:

| Scenario | What to verify |
| --- | --- |
| Minimal valid query | Tool accepts the smallest useful input and returns non-error `CallToolResult`. |
| Full valid query | Every public schema field is accepted, survives mapping, and affects execution as documented. |
| Unknown fields | MCP schema strips or rejects extra fields according to the overlay. Hidden fields must not reach execution from MCP calls. |
| Invalid field type | The call fails validation cleanly, with the invalid path named. |
| Invalid enum | The response names the allowed values or a usable correction. |
| Empty array | Bulk `queries:[]` is rejected by schema or returns an error before execution. |
| Six queries | Bulk schema rejects more than five queries. |
| Mixed success, empty, error | One response contains all three states, preserves input order, dedupes top-level hints, and sets `isError=false` unless all entries failed. |
| All errors | Tool sets `isError=true`, preserves one error per query, and includes no misleading success evidence. |
| Lean output | `base` relativizes absolute `path`/`uri` against a common root; `shared` hoists scalar fields identical across all leaves; both are lossless and reconstructable, and identity keys (`owner`/`repo`/`name`/`id`) stay per-item. |
| Query pagination | `charLength` creates query-level pagination metadata and a next cursor; re-calling with the cursor continues without duplicating content. |
| Response pagination | `responseCharLength` pages the outer multi-query response and leaves native per-query pagination intact. |
| Final page | No pagination hint appears when `hasMore=false`. |
| Auth unavailable | Remote tools return actionable auth errors without exposing token names beyond approved env var guidance. |
| Rate limit | Remote tools preserve rate-limit reset/retry metadata when provider data includes it. |
| Secret redaction | Responses and errors redact tokens, keys, and credentials from content, paths, and provider errors. |

### Tool checklist

#### Verify `ghSearch`

Primary code: [packages/octocode-tools-core/src/tools/github_search/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/github_search). Schema: `GitHubSearchQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify the strict `code`, `repositories`, and `tree` operation branches and rejection of fields from another branch. |
| Implementation | Each operation delegates to the matching provider flow, preserves input order in mixed bulk calls, and reports `data.operation`. |
| Pagination | Code/repository pages and tree entry pages preserve their native parameters; top-level response pagination remains independent. |
| Continuations | Every discovery continuation names `ghSearch`, preserves the operation, and produces a query valid for that operation. |
| Research quality | Code hits are candidate evidence, repository rows support triage, and tree entries support a bounded exact-read follow-up. |

#### Verify `ghGetFileContent`

Primary code: [packages/octocode-tools-core/src/tools/github_fetch_content/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/github_fetch_content). Schema: `FileContentQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `owner`, `repo`, `path`, optional `branch`, `fullContent`, `matchString`, `startLine`/`endLine`, `contextLines`/`contextBytes`, `chunkType`, `offset`, and `limit`. |
| Mutex | `fullContent`, `matchString`, and line ranges are mutually exclusive. Invalid combinations produce per-query errors in bulk calls. |
| File mode | Line ranges are accurate, `totalLines` is correct, branch fallback/resolution is reported, large files page by character cursor, and partial content sets `isPartial=true`. |
| Directory mode | Requires local and clone support. Returns `localPath`, file count, total size, cached state, and resolved branch. Follow-up local tools must work against `localPath`. |
| Empty | `matchString` with no matches returns empty, not provider error, and does not fabricate content. |
| Warnings | `content-truncated` includes group id, path, full content length, truncation point, and recovery. |
| Research quality | File content must be answer-ready when the query requested a line range or match. Directory mode must be treated as setup evidence for local and LSP follow-ups. |

#### Verify `ghSearchHistory`

Primary code: [packages/octocode-tools-core/src/tools/github_search_pull_requests/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/github_search_pull_requests).

| Surface | Checks |
| --- | --- |
| Operations | Verify the exact plural operations `pullRequests`, `issues`, and `commits`; reject fields belonging to another operation. |
| Identity | Search results expose the `number` or commit `ref` needed by `ghGetHistoryItem`. Singular identity fields are rejected by the search contract. |
| Pagination | Each operation pages without dropping or duplicating candidates and preserves its operation in continuations. |
| Empty | Empty responses name operation, repository scope, and active filters without inventing evidence. |

#### Verify `ghGetHistoryItem`

Primary code: [packages/octocode-tools-core/src/tools/github_search_pull_requests/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/github_search_pull_requests).

| Surface | Checks |
| --- | --- |
| Operations | Verify the exact singular operations `pullRequest`, `issue`, `commit`, and `compare`; reject fields belonging to another operation. |
| Identity | PR and issue require `number`; commit requires `ref`; compare requires both `base` and `head`. |
| Detail | Requested PR body/patches/comments/reviews/commits, issue body/comments, commit diff, and compare counts are returned only by their matching operation. |
| Research quality | Every result preserves repository plus stable item identity, and large detail payloads provide a targeted follow-up rather than an unusable dump. |

#### Verify `artifactSearch`

Primary code: [packages/octocode-tools-core/src/tools/package_search/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/package_search). Schema: `ArtifactSearchQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Require `type` and exactly one of `packageName` or `keywords`; discovery uses `cursor` and `pageSize`. PyPI discovery reports unsupported. |
| Implementation | Verify authenticated registry requests, scoped routing, explicit overrides, cache isolation after credential changes, and exact package-name identity. Verify missing exact packages versus unsupported operations and provider failures across all ecosystems. |
| Pagination | Execute `next.nextPage` until the fixture union is complete; the continuation must preserve ecosystem, selectors, registry, page size, and provider state. |
| Empty | Empty search returns package-specific recovery without pretending the package exists. |
| Research quality | Preserve package identity and available version, description, license, and repository/subdirectory details. Do not claim unrequested download statistics or enrichment. |

#### Verify `ghCloneRepo`

Primary code: [packages/octocode-tools-core/src/tools/github_clone_repo/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/github_clone_repo). Schema: `CloneRepoQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `owner`, `repo`, `branch`, `sparsePath`, and `forceRefresh`. Clone is side-effecting and has no verbosity field. |
| Implementation | Requires clone/local enablement, resolves branch fallback, reuses cache when valid, refreshes expired cache, and returns a safe `localPath`. |
| Pagination | No native pagination is expected, but bulk response pagination must still work. |
| Empty | Not applicable. A missing repository, branch, or path is an error with recovery context. |
| Data management | Verify cache TTL, cache invalidation, concurrent clone locking, cleanup on failed clone, and no writes outside the tmp materialization roots. |
| Research quality | Returned `localPath` must be immediately usable by `localSearch`, `localFetch`, and LSP tools. |

#### Verify `localSearch`

Primary code: [packages/octocode-tools-core/src/tools/local_search/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/local_search). Schema: `LocalSearchQuerySchema`. This tool performs lexical text and regex search; structural, file-discovery, and tree queries use `astSearch`.

| Surface | Checks |
| --- | --- |
| Params | Verify `path`, `searchText`, `regex`, `caseMode`, `wholeWord`, include/exclude/excludeDir, hidden/noIgnore, `resultView`, `contextLines`, `matchContentLength`, `maxMatchesPerFile`, `matchPage`, `pageSize`, `page`, and the current schema's remaining lexical filters. |
| Hidden fields | MCP schema must not expose implementation-only knobs such as threads, binary, encoding, debug, passthru, or symlink following. |
| Mutex | Lexical search requires `searchText`; invalid enum combinations become per-query errors. |
| Implementation | Runs ripgrep in-process through the native engine. No external `rg` binary and no grep fallback. Invalid regex, path errors, and no-permission paths are structured errors. |
| Pagination | File and match pagination work independently. `line` values are stable 1-indexed `lineHint` inputs for LSP tools. |
| Empty | Empty hints name active filters such as type, include, exclude, excludeDir, or path. No-filter empty stays silent. |
| Research quality | Results must include file path, match count, line, column, snippet value, and enough context to drive precise `lspSearch` queries such as `operation="definition"`, `operation="references"`, `operation="callers"`, or `operation="callees"`. |

#### Verify `astSearch` operations

Primary code: [packages/octocode-tools-core/src/tools/ast_search/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/ast_search). Schema: `AstSearchQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify every operation branch (`match`, `files`, `tree`, `symbols`, and `topology`) against the live schema. For filesystem/tree queries cover `path`, name and extension filters, exclude filters, depth, limits, pages, and entry details; for `match`, verify exactly one `pattern` or `rule`; for `topology`, verify its analysis-specific selectors and diagnostics. |
| Hidden fields | `extension` singular and `recursive` are not exposed in `astSearch`. Use `extensions`; set a bounded `maxDepth` to recurse. |
| Implementation | Directory walk respects path validation, depth cap, ignored directories, sorting, and entry typing. Symlink and permission cases are explicit. |
| Pagination | Entry pagination uses stable ordering so page 2 continues page 1 without duplicates or missed entries. |
| Empty | Empty directories are empty, not errors. Missing paths are errors. Filtered empties name the active filter. |
| Research quality | Entries must identify name, path, type, size, modified time, and depth so the next search or content call can be scoped. |

#### Verify `astSearch(operation:"files")`

Primary code: [packages/octocode-tools-core/src/tools/ast_search/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/ast_search). Schema: `AstSearchQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `operation:"files"`, `path`, name/path/regex filters, entry type, size filters, modified/accessed filters, permissions if exposed, total `limit`, `pageSize`, and `page`. |
| Implementation | Uses the safe file-discovery path, respects allowed paths, handles large trees without unbounded output, and returns stable metadata. |
| Pagination | File pagination and char pagination both work. Cap notices must not replace next-page cursors. |
| Empty | Empty hints quote active filters such as `names`, `time.modifiedWithin`, or `size.greater`. No-filter empty stays silent. |
| Research quality | Results must support targeted follow-ups by path, type, size, permissions, and timestamps. |

#### Verify `astSearch`

Primary code: [packages/octocode-tools-core/src/tools/ast_search/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/ast_search), with topology analysis under `ast_search/topology/`. Schema: `AstSearchQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `operation:"topology"` with each `analysis` (`dependencies`, `dependents`, `path`, `cycles`, `reachability`, and `deadCode`), required `file`/`target` selectors, `depth`, roots, `includeTests`, exclusions, caps, `rustWorkspace`, result pages, and diagnostic pages. |
| Implementation | Build one bounded graph per distinct batch configuration; preserve unresolved imports, inferred roots, skipped files, and partial scan diagnostics. |
| Pagination | Execute `nextPage`, `nextDiagnostics`, and safe scan-expansion continuations; reject stale diagnostic snapshots. |
| Empty | Distinguish a valid empty relation from unsupported syntax, unresolved imports, uncertain entrypoints, or an incomplete scan. |
| Research quality | Treat imports and dead-code output as candidates. Confirm exact edges with source reads and prove symbol identity/reachability with LSP or runtime checks before change or deletion claims. |

#### Verify `localFetch`

Primary code: [packages/octocode-tools-core/src/tools/local_fetch_content/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/local_fetch_content). Schema: `FetchContentQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `path`, `fullContent`, `matchString`, `startLine`, `endLine`, `contextLines`, `charOffset`, `charLength`, and `minify`. |
| Mutex | `fullContent`, `matchString`, and line ranges are mutually exclusive, with per-query errors inside bulk calls. |
| Implementation | Handles UTF-8 files, large files, minified content, binary/unreadable files, no trailing newline, and out-of-range line requests. |
| Pagination | Character pagination continues exact content without overlap. Match extraction plus pagination must preserve `matchRanges`. |
| Empty | A missing `matchString` result returns empty with no fake content. Missing file and invalid path are errors. |
| Research quality | Returned content must include path, line range, total lines, `isPartial`, and enough source text to cite or reason from. Partial line-range reads emit a `startLine=N` continuation hint. |

#### Verify `lspSearch`

Primary code: [packages/octocode-tools-core/src/tools/lsp/semantic_content/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/lsp/semantic_content). Schema: `LspGetSemanticsQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `uri`, `operation`, `symbolName`, `lineHint` or zero-based UTF-16 `position`, `workspaceRoot`, `orderHint`, `includeDeclaration`, `groupByFile`, `depth`, `page`, `pageSize`, `contextLines`, and operation-level pagination. |
| Implementation | Requires either a name plus `lineHint` or an exact `position` for symbol-anchored operations, resolves exact code occurrences while ignoring string/comment hits, uses pooled LSP clients, and reports capability gaps explicitly. |
| Pagination | Large semantic payloads page without losing target identity. |
| Empty | Symbol-not-found, unsupported capability, and LSP-unavailable paths are explicit. |
| Semantic quality | Definition/reference/call/hover/symbol outputs identify URI, range, symbol identity, completeness, and static-vs-dynamic limits where applicable. |

### Cross-tool research quality suites

These suites verify that tools compose into reliable research workflows.

| Suite | Steps | Pass criteria |
| --- | --- | --- |
| Local semantic navigation | `localSearch` for a symbol, then `lspSearch` with `operation="definition"`, `operation="references"`, and `operation="callers"`/`operation="callees"` using returned line hints. | LSP tools resolve the same symbol, references include the definition when requested, call direction is correct, and fallback mode is explicit if used. |
| Remote to local deep dive | `ghSearch(operation:"code"|"repositories")`, then `ghCloneRepo`, then local search and LSP tools on `localPath`. | Remote identity, branch, clone path, and local path all line up. No result requires guessing a path or branch. |
| Structure to content | `ghSearch(operation:"tree")` or `astSearch(operation:"tree")`, then content fetch on selected entries. | Paths emitted by structure tools are directly accepted by content tools. Empty directories and missing files are differentiated. |
| Package provenance | `artifactSearch`, then `ghSearch(operation:"tree"|"code")` on parsed repository owner/name. | Package repository metadata is normalized enough to drive GitHub tools, and missing/ambiguous repository URLs are represented as missing evidence. |
| PR archaeology | `ghSearchHistory(operation:"pullRequests")` with title search, then `ghGetHistoryItem(operation:"pullRequest", number)` and file-content or code search follow-up. | Approximate search finds candidates; number lookup returns the requested body/diff data; large diffs guide targeted follow-up. |
| Empty-result recovery | Run over-constrained queries across GitHub, local, and LSP tools. | Each tool either stays silent when no concrete advice exists or names exactly which filter to relax. |
| Pagination chain | Force small `pageSize`, plus total `limit` where supported, and exercise `maxMatchesPerFile`, `matchPage`, `charLength`, and `responseCharLength`. | Every next cursor continues the same result set without duplicates, missing entries, or final-page chatter. |
| Output-efficiency chain | Use `concise:true` where supported, `--compact` for lean CLI envelopes, and targeted exact reads with `minify:"none"` for citation-grade evidence. | Each narrower output preserves the identity and cursor needed for the next step. |

### Data management and reliability

Verify these behaviors whenever touching cache, clone, local files, pagination, or response shaping:

| Area | Checks |
| --- | --- |
| Clone cache | TTL is honored, branch/ref changes do not return stale checkout content, concurrent clone requests do not corrupt cache, and failed clones clean up partial directories. |
| Local reads | Large files, binary files, permission-denied files, hidden files, symlinks, and paths outside allowed roots are handled explicitly. |
| Provider cache | Cached HTTP responses do not hide auth/rate-limit failures and do not merge responses from different owners, repositories, branches, pages, or query filters. |
| Response sizes | Raw source chars, transformed chars, output chars, and char-savings stats remain best-effort and never break responses. |
| Sanitization | File content, provider errors, clone paths, and local command output pass through secret redaction. |
| Concurrency | Bulk concurrency preserves input order, isolates timeouts, and sets `isError` only when every query failed. |

### Test-coverage gaps to close

Use this list to turn verification failures into focused improvements:

| Topic | Improvement target |
| --- | --- |
| Schema visibility | Add tests that snapshot MCP JSON schemas for hidden fields, defaults, enum values, and max bounds. |
| Evidence quality | Enforce evidence metadata for every tool family and downgrade confidence for fallback, partial, or paginated results. |
| Empty states | Add per-tool tests for over-constrained filters so empty hints stay specific and do not become generic workflow prose. |
| Pagination | Add end-to-end cursor replay tests for every pagination dimension, not only generator unit tests. |
| LSP semantics | Add fixtures covering same-symbol multiple occurrences, import/export chains, dynamic imports, generated files, and non-TypeScript language fallbacks. |
| Remote semantics | Add provider-mapper tests for branch fallback, merged PR state, repository language filtering, topic/query merging, and package repository URL normalization. |
| Lean-output contract | Add tests for `base`/`shared` hoisting across tools with warnings, empty queries, and mixed results — including identity-key exclusion and exact reconstruction. |
| Direct CLI parity | Verify direct CLI schema help, auto-filled metadata, JSON/YAML output, and direct execution match MCP behavior. |

### Suggested command sets

From `packages/octocode-mcp/`, run the focused suites first:

```bash
yarn test tests/tools/all-tools.pagination-contract.test.ts
yarn test tests/scheme/all-tools.schema-contract.test.ts
yarn test tests/tools/pagination-hints-fixes.test.ts
yarn test tests/tools/stats_emission_contract.test.ts
```

Then run broader tool coverage:

```bash
yarn test tests/tools
yarn lint
yarn typecheck
yarn test
```

For dead-code and dependency drift, run from the repository root:

```bash
npx knip
```

### Release gate

Do not mark a tool-surface change complete until these are true:

1. The catalog contains 10 tools; 9 are enabled by default and `ghCloneRepo` remains opt-in. Every registered tool has its input schema and the MCP registration publishes no output schema.
2. All public schema defaults, caps, hidden fields, and mutex rules have tests.
3. Every tool has success, empty, error, mixed-bulk, pagination, lean-output (`base`/`shared`), and verbosity coverage.
4. Remote tools cover auth, rate limit, provider error, no results, and provider-mapper edge cases.
5. Local tools cover path validation, large output, hidden/ignored files, empty results, and command allow-list behavior.
6. LSP tools cover semantic success, fallback mode, symbol-not-found, wrong line hint, `orderHint`, pagination, and context snippets.
7. Hints are short, contextual, deduped, and absent on final pages.
8. Each tool's evidence metadata and tool-specific partial/completeness markers match the result it returns; the gate does not assume universal `answerReady` or `complete` fields.
9. No response leaks secrets, raw tokens, internal stack traces, or unrelated query metadata.
10. `yarn lint`, `yarn typecheck`, and `yarn test` pass in the package environment.
