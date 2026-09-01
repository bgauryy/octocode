# Octocode tools reference

One reference for every Octocode research tool exposed through MCP and the CLI. Schemas, descriptions, and execution live in `@octocodeai/octocode-tools-core`; native search, minify, security, and LSP primitives live in `@octocodeai/octocode-engine`.

Use this page when you need field-level guidance, cross-tool workflows, known behavior, or release verification checks. For MCP tool ratings, quality gaps, per-tool improvement backlogs, and the recommended agent workflow, see [`MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md`](https://github.com/bgauryy/octocode/blob/main/docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md). For the exact active schema in a local checkout, run the compact form first; its `relations` list preserves mode-specific required and mutually exclusive fields:

```bash
npx octocode tools <toolName> --scheme --json --compact
```

## Tool inventory

| Family | Tools |
|--------|-------|
| GitHub | `ghSearch`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` |
| Packages | `npmSearch` |
| Local | `localSearch`, `localGetFileContent`, `localAnalyzeGraph` |
| LSP | `lspGetSemantics` |

## Contents

- [GitHub tools reference](#github-tools-reference)
- [Local code tools reference](#local-code-tools-reference)
- [LSP tools reference](#lsp-tools-reference)
- [Clone and local tools workflow](#clone-and-local-tools-workflow)
- [Tool verification playbook](#tool-verification-playbook)
- [MCP tool quality and agent workflow](https://github.com/bgauryy/octocode/blob/main/docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md)


---

## GitHub tools reference

Concise reference for Octocode MCP remote research tools: GitHub code/repo/PR search, GitHub content access, cloning, and npm package lookup.

### GitHub tool configuration

| Variable | Purpose |
|----------|---------|
| `OCTOCODE_TOKEN` | Highest-priority GitHub token. |
| `GH_TOKEN` | GitHub CLI compatible token. |
| `GITHUB_TOKEN` | GitHub token fallback. |
| `GITHUB_PERSONAL_ACCESS_TOKEN` | Lowest-priority GitHub token env var. |
| `GITHUB_API_URL` | GitHub Enterprise API base URL. |
| `ENABLE_LOCAL` | Turns local tools on or off. Defaults to `true` on both CLI and MCP. |
| `ENABLE_CLONE` | Controls `ghCloneRepo` and `ghGetFileContent(type="directory")`. Defaults to `true`; set `false` to disable them. |

Every tool accepts bulk input (`{ "queries": [...] }`), up to 5 queries per call. Page-based tools use `page` and `pageSize`; `limit` is a pre-pagination cap where that distinct control exists. When more results remain, run the matching schema-valid `next.*` call: `nextPage`/`nextMatchPage`, `expandLimit`/`expandScan`, or a line/character continuation. At an unexpandable public or provider cap, metadata reports `terminalLimitReached` and omits unusable continuations. Numeric page, offset, cursor, and raw `nextQuery` fields are not executable by themselves. `matchString` is a complete all-matches selector, while explicit line/character windows continue independently. `ghCloneRepo` is atomic and does not paginate its input. Use `npx octocode tools <toolName> --scheme --json --compact` for the exact active schema and operation scopes.

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
| Resolve npm package to source repository | `npmSearch` |

### `ghSearch`

Use the default unified discovery tool with one strict operation per query:

- `operation: "code"` accepts the code-search fields documented below.
- `operation: "repositories"` accepts repository discovery fields.
- `operation: "tree"` requires `owner` and `repo` and accepts tree browsing fields.

Fields from another operation are rejected rather than silently ignored. Mixed bulk
queries are allowed and results retain input order. `ghGetFileContent` stays
separate because it reads and minifies known content rather than discovering it.

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
| `type` | `file` by default; `directory` materializes a subtree locally. |
| `fullContent` | Read the whole file. Use only for small files. |
| `startLine`, `endLine` | Read a line range. |
| `matchString` | Return matching slices. |
| `contextLines` | Context around `matchString`. |
| `matchStringIsRegex`, `matchStringCaseSensitive` | Match behavior. |
| `charOffset`, `charLength` | File-content pagination. |
| `minify` | `standard` (default, strips comments/blank lines), `none` (exact raw text), or `symbols` (structural skeleton only). |

File extraction modes are mutually exclusive: use one of `fullContent`, `startLine`/`endLine`, `matchString`, or `minify: "symbols"`.

Directory mode:

- Enabled by default. Requires clone and local tools not to be explicitly disabled.
- Returns `localPath` (absolute), `location` (kind/source/cached/complete), and `next` entries for `localSearch` text and tree operations with ready-to-use paths.
- Rejects file-only extraction fields.

Examples:

```json
{ "owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/config.ts", "startLine": 1, "endLine": 80 }
{ "owner": "vercel", "repo": "next.js", "path": "packages/next/src", "type": "directory" }
```

Cost by mode:

| Mode | What you get | Approx tokens |
|------|-------------|---------------|
| `matchString` | Every matching slice, plus context | ~50-300 |
| `startLine`/`endLine` (small) | Exact line range | ~100-500 |
| `minify: "symbols"` | Imports and signatures, bodies stripped | 5-20% of the full file |
| `startLine`/`endLine` (large chunk) | Up to `charLength` chars of a range | 1k-10k |
| `fullContent` | Entire file, minified | Can exceed 50k |

Behaviors worth knowing:

- `matchString` returns **all** occurrences as separate slices. The response carries
  `matchCount`, `matchedLines`, and `matchRanges`, each slice with its own
  `contextLines`. You do not need to re-read line by line.
- `matchString` searches the raw file, not the minified view. Minification runs on
  the extracted slice afterwards, so comments and whitespace in your search string
  still match.
- `minify: "symbols"` ignores `matchString` and returns the whole skeleton. Read the
  line numbers from the gutter, then follow up with `startLine`/`endLine`.
- Files too large for the `/contents/` API fall back to the Git tree and blob API
  automatically. You do not need to switch to `ghCloneRepo` for size alone.

### `ghSearchHistory`

Search GitHub history through one strict discovery operation per query:

- `operation: "pullRequests"` searches PR candidates.
- `operation: "issues"` searches issue candidates.
- `operation: "commits"` walks commit history, optionally scoped to a path or time range.

The search tool returns candidates and stable identities. Fetch detailed content
with `ghGetHistoryItem`; search queries do not accept singular-item identities.

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

```json
{ "operation": "pullRequest", "owner": "vercel", "repo": "next.js", "number": 12345, "content": { "changedFiles": true } }
{ "operation": "issue", "owner": "vercel", "repo": "next.js", "number": 12345, "content": { "body": true, "comments": {} } }
{ "operation": "commit", "owner": "vercel", "repo": "next.js", "ref": "abc123", "includeDiff": true }
{ "operation": "compare", "owner": "vercel", "repo": "next.js", "base": "v14.0.0", "head": "v14.1.0" }
```

Request selected PR patches instead of every patch for large PRs, and leave
commit diffs off until the relevant commit is known.

### `ghCloneRepo`

Clone a repository or sparse subtree into Octocode's local cache.

Enabled by default. Set `ENABLE_CLONE=false` to disable clone-backed workflows.

Key fields:

| Field | Meaning |
|-------|---------|
| `owner`, `repo` | Required repository. |
| `branch` | Branch to clone. Omit to use default branch. |
| `sparsePath` | Optional subdirectory sparse checkout. |

Returns `localPath` (absolute), `location` (kind/source/cached/complete), and `next` with ready-to-use `localSearch` and `viewStructure` query params — pass `next.localSearch.query` or `next.viewStructure.query` directly to the respective tool.

Examples:

```json
{ "owner": "vercel", "repo": "next.js", "branch": "canary" }
{ "owner": "microsoft", "repo": "TypeScript", "sparsePath": "src/compiler" }
```

Rules:

- Use `sparsePath` for large monorepos.
- Use `ghGetFileContent` when you only need one file.
- Cached clones are reused.
- `next.localSearch.query.path` and `next.viewStructure.query.path` equal `localPath` — use them as-is.

### `npmSearch`

Resolve npm packages to metadata and source repositories.

Key fields:

| Field | Meaning |
|-------|---------|
| `name` | Exact npm package name or npm keyword query. |
| `npmFetchMetadata` | Fetch heavier npm metadata when needed. |
| `page` | Page keyword-search results. |
| `pageSize` | Return 1–100 keyword-search results per page; partial pages include executable `next.nextPage`. |

Examples:

```json
{ "name": "react" }
{ "name": "typescript eslint", "page": 2, "pageSize": 20 }
```

Use `npmSearch` before GitHub repository search when you already have a package name.

Field reliability:

| Data point | Reliability |
|-----------|------------|
| `version` | High, from `npm view` or the registry directly |
| `repository.url` | High, from `package.json` |
| `homepage` | High |
| `description` | High |
| `weeklyDownloads` | Low: a separate endpoint with an 8-second timeout and no retry |
| `entrypoints` (main, module, types) | Medium, parsed from the `package.json` exports map |

Behaviors worth knowing:

- Every network call has an 8-second timeout, and the weekly-downloads fetch runs
  with `maxRetries: 0`. On a VPN, corporate proxy, or CI runner, `weeklyDownloads`
  is absent, with no warning and no fallback hint. Read it from
  `https://api.npmjs.org/downloads/point/last-week/<name>` when you need it.
- When the registry is unreachable the tool tries a web-search fallback, then returns
  an error hinting at `ghSearch(operation:"repositories")`. Acting on that hint is your call, not automatic.
- Scoped packages need the full `@scope/name`.

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
| Get package version only | `npmSearch` — if it times out, read `package.json` from GitHub |

### Workflows

| Task | Flow |
|------|------|
| Understand a package | `npmSearch` -> `ghSearch(operation:"tree")` -> `ghSearch(operation:"code")` -> `ghGetFileContent` |
| Find examples of a pattern | `ghSearch(operation:"code")` -> `ghGetFileContent` |
| Explore a repository | `ghSearch(operation:"tree")` -> `ghGetFileContent(README)` -> `ghSearch(operation:"code")` |
| Explain why code changed | `ghSearch(operation:"code")` -> `ghSearchHistory` -> `ghGetHistoryItem` with the returned identity |
| Deep local analysis | `ghCloneRepo` -> local tools |

### GitHub tool rules

- Use GitHub tools for remote repositories, not files already on disk.
- Use `npmSearch` for known npm package names.
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
| `localSearch` | Unified local discovery. Set `operation` to `text`, `structural`, `files`, or `tree`; text and structural matches provide anchors for `lspGetSemantics`. |
| `localGetFileContent` | Read targeted file content by line range, match, signature skeleton, or char page. |
| `localAnalyzeGraph` | Run one bounded repository-graph operation: dependencies, dependents, path, reachability, cycles, or dead-code candidates. |

---

### Local tool configuration

Local tools are on by default. To turn the whole local surface off:

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
| `ENABLE_CLONE` | Enables clone-backed workflows and GitHub directory fetches that materialize local files. Defaults to `true`; set `false` to disable them. |
| `TOOLS_TO_RUN` | Strict tool allowlist; include every tool that must remain enabled. Removed compatibility names are rejected. |

Config reference: [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

---

### Platform support

`localSearch` uses Octocode's native in-process ripgrep, structural-search, and filesystem-walker engines. There are no external `rg`, `grep`, `find`, or `tree` dependencies.

`localGetFileContent` is pure Node.js and works on macOS, Linux, and Windows.

All four `localSearch` operations work on macOS, Linux, and Windows. Prefer `operation:"text", resultView:"files"` when a content query can answer the question.

---

### Pagination

All tools accept up to 5 queries per call.

Local tools expose two pagination layers:

| Layer | Fields | Applies To |
|-------|--------|------------|
| Native result pagination | `page`, `pageSize` | Every `localSearch` operation |
| Per-file match pagination | `matchPage`, `maxMatchesPerFile` | `localSearch(operation:"text")` when a matched file has more matches |
| Content pagination | `charOffset`, `charLength` | `localGetFileContent` and oversized per-query payloads |
| Bulk response pagination | `responseCharOffset`, `responseCharLength` | Any local-tool bulk response |

Use native pagination first for result lists, then char pagination only when a single result payload is still too large.

---

### Choose a local tool

| Need | Use |
|------|-----|
| "Which directories/files exist here?" | `localSearch(operation:"tree")` |
| "Find files named `*.test.ts` or changed recently." | `localSearch(operation:"files")` |
| "Search for text, regex, imports, TODOs, or identifiers." | `localSearch(operation:"text")` |
| "Read this exact file section." | `localGetFileContent` |
| "Find files containing a pattern without match bodies." | `localSearch(operation:"text", resultView:"files", ...)` |
| "Find files that do not contain a pattern." | `localSearch(operation:"text", resultView:"filesWithout", ...)` |

Recommended order for code research:

```text
DISCOVER -> SEARCH -> READ
```

Start broad with structure or metadata, narrow with content search, then read the smallest exact file slice needed.

`localSearch` is the only local discovery entry point. Its four strict operation
branches reject fields from other operations. Removed compatibility names cannot
be restored with `TOOLS_TO_RUN` or `.octocoderc`.

---

### `localSearch`

One discriminated local-discovery surface. Always set `operation`; fields from different operations cannot be mixed.

#### Best for

- Finding identifiers, imports, route names, constants, TODOs, errors, config keys, and string literals.
- Listing files that contain or do not contain a pattern.
- Getting compact match context before deciding which file section to read.

#### Key parameters

| Parameter | Description |
|-----------|-------------|
| `path` | File or directory to search. Relative paths resolve from the workspace root. For remote repos: pass `localPath` from a `ghCloneRepo` or `ghGetFileContent(type:"directory")` result — it is already absolute and immediately valid. |
| `operation` | Required. `text`, `structural`, `files`, or `tree`. |
| `searchText` | Text or regex pattern. Required with `operation:"text"`. |
| `resultView` | Text/structural response shape. Text supports `paginated`, `discovery`, `detailed`, `content`, `files`, `filesWithout`, `countLines`, `countMatches`, and `matchOnly`; structural supports content/file/count/match views. |
| `pattern` | Octocode code-shaped AST pattern. `$X` = one node, `$$$ARGS` = a list. Use only with `operation:"structural"`; provide exactly one of `pattern` or `rule`. |
| `rule` | YAML relational rule (`not`/`inside`/`has`/`all`/`any`). Add `stopBy: end` for ancestor/descendant relations. Use only with `operation:"structural"`. |
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
| `regex` | `fixed` for literal text, `smart` for normal regex, `perl` for PCRE2 features. |
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
| `fetch` | `localGetFileContent` | Read the first hit; adjust `minify` on the follow-up when needed. |
| `lspDefinition` / `lspReferences` | `lspGetSemantics` | Follow the first match semantically when a safe symbol name can be inferred. |
| `nextPage` / `nextMatchPage` | `localSearch` | Continue file-level or per-file match pagination. |

#### Examples

```bash
localSearch(operation="text", path="packages/octocode-mcp/src", searchText="registerTool", langType="ts")
localSearch(operation="text", path=".", searchText="TODO", resultView="files")
localSearch(operation="text", path="src", searchText="class\\s+\\w+Service", regex="perl", contextLines=3)
```

#### Structural and AST search

Use `operation:"structural"` for code-shape queries regex cannot express (find all `await` inside `for` loops, calls with N args, functions missing `try/catch`).

**Supported languages:** ts, tsx, js, jsx, mjs, cjs, py, go, rs, java, c/h, cpp/cc/cxx, cs, sh/bash/zsh.

```bash
localSearch(operation="structural", path="src", pattern="track($$$ARGS)")
# `rule` is a YAML string: \n below are real newline escapes in the JSON tool
# arg (not literal backslash-n). On the CLI, use $'...' or a real multiline string.
localSearch(operation="structural", path="src", rule="rule:\n  pattern: await $C\n  inside:\n    kind: for_statement\n    stopBy: end")
localSearch(operation="structural", path=".", pattern="eval($X)")
```

---

### `localSearch(operation:"tree")`

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
localSearch(operation="tree", path=".", maxDepth=1)
localSearch(operation="tree", path="packages/octocode-mcp/src", maxDepth=2, entryType="d")
localSearch(operation="tree", path="docs", extensions=["md"], detail="full")
```

---

### `localSearch(operation:"files")`

Metadata search for files and directories.

#### Best for

- Finding files by name, extension, regex, path slice, size, permission, or modified time.
- Locating tests, configs, generated files, or recently changed files.
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
localSearch(operation="files", path=".", names=["*.test.ts"])
localSearch(operation="files", path="packages", pathRegex="^readme\\.md$")
localSearch(operation="files", path=".", time={"modifiedWithin":"24h"}, entryType="f", detail="full")
```

---

### `localGetFileContent`

Targeted file reading. Use it after structure/search has narrowed the file and section.

#### Best for

- Reading a known line range.
- Extracting context around a known string or regex.
- Viewing a small whole file.
- Getting a structural skeleton without full bodies.

#### Extraction modes

Choose one main extraction mode:

| Mode | Fields |
|------|--------|
| Match extraction | `matchString`, optional `contextLines`, `matchStringIsRegex`, `matchStringCaseSensitive` |
| Line range | `startLine` and `endLine` |
| Whole file | `fullContent=true` |
| Structural skeleton | `minify:"symbols"` |

Do not combine `fullContent` with match or line-range extraction. Do not combine `matchString` with `startLine`/`endLine`.

#### Key parameters

| Parameter | Description |
|-----------|-------------|
| `path` | File path to read. Use `localSearch` for directories. |
| `startLine` / `endLine` | 1-based inclusive line range. Use together. |
| `matchString` | Anchor text or regex. |
| `contextLines` | Lines around each match. Default 5, max 100. |
| `matchStringIsRegex` | Treat `matchString` as regex. |
| `matchStringCaseSensitive` | Case-sensitive match search. |
| `charOffset` / `charLength` | Character pagination for large content. |
| `minify` | `symbols` for skeleton, `standard` for compact readable content, `none` for exact bytes. |
| `charOffset` | Continue character pagination when the response advertises more content. |

#### Examples

```bash
localGetFileContent(path="packages/octocode-mcp/src/public.ts", startLine=1, endLine=80, minify="none")
localGetFileContent(path="README.md", matchString="Configuration", contextLines=4)
localGetFileContent(path="src/index.ts", minify="symbols")
```

---

### `localAnalyzeGraph`

One bounded repository graph with six explicit operations: `dependencies`, `dependents`, `path`, `reachability`, `cycles`, and `deadCode`. Import edges come from native syntax facts. Traversal and path results report exact `edgeKinds` (`static-import`, `type-import`, `dynamic-import`, `named-reexport`, or `star-reexport`) with syntactic confidence. Dependency traversal also reports immediate dominators, topological layers, and transitively redundant condensation-DAG edges. Cycle results distinguish `runtimeCycle` from type-only SCCs, expose condensation metadata, and return deterministic directed witnesses in `cycleEdges` and `runtimeCycleEdges`; every witness edge includes `from`, `to`, and `edgeKinds`. Native facts also contain `call` and `contains` relations, but the current public operations don't project those symbol-level edges. `deadCode` results are candidates, not deletion proof.

#### Best for

- Tracing forward dependencies or reverse dependents to a bounded depth.
- Finding the shortest directed import path between two files.
- Finding mandatory dependency chokepoints, topological layers, and redundant edges.
- Classifying entrypoint reachability and finding strongly connected import cycles.
- Finding repository-wide dead-export candidates and dead clusters in one pass. A dead cluster is a strongly connected set of mutually importing, unreachable files; the files don't necessarily call one another.

Use `localAnalyzeGraph` to discover repository-scale file topology and candidate reachability. Use `lspGetSemantics` with `references`, `callers`, or `callees` to prove the identity and semantic connections of one known symbol. A graph edge proves that one file syntactically imports or re-exports another; it doesn't prove which binding is used.

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
| `runtimeCycleEdges` | A directed witness that remains after the analysis removes `type-import` edges. This is the relevant witness for module-loading risk. | Confirm the imported bindings and initialization behavior before claiming a runtime defect. |
| Type-only SCC | Files are mutually connected in the full import graph, but no runtime cycle remains after the analysis erases type-only edges. | Report it as topology or coupling evidence, not as a module-loading cycle. |
| `transitiveCandidates` | Condensation-DAG edges for which another directed path already connects the same components. They can indicate redundant architectural wiring. | Check re-export contracts, side effects, public API intent, and symbol usage before calling an import duplicate. |
| `immediateDominators` | Components that every directed route from the selected root must cross. | Use them to prioritize chokepoints; do not infer symbol ownership from file topology. |

The graph assigns no weights to edges. `path` therefore uses breadth-first search to return the fewest-edge directed import path, not Dijkstra's weighted shortest-path algorithm. A syntactically redundant edge can still be semantically necessary because it imports a value for side effects, preserves a public barrel contract, or selects a different binding.

#### Examples

```bash
localAnalyzeGraph(operation="dependencies", path="/ABS/repo", file="src/index.ts", depth=2)
localAnalyzeGraph(operation="cycles", path="/ABS/repo", pageSize=20, limit=100)
localAnalyzeGraph(operation="deadCode", path="/ABS/repo", entrypoints=["src/index.ts"], includeTests=false)
```

For a cycle, read the exact imports named by `cycleEdges`; use `runtimeCycleEdges` when investigating loading behavior. Verify a dead-code or transitive-edge candidate with `lspGetSemantics` before removing it.

---

### Local workflows

#### Explore a new repository

```text
localSearch(operation="tree", path=root, maxDepth=1)
localSearch(operation="tree", path=root+"/src", maxDepth=2)
localSearch(operation="files", path=root, names=["package.json", "tsconfig.json", "README.md"])
localSearch(operation="text", path=root, searchText="export", resultView="files")
localGetFileContent(path="README.md", minify="symbols")
```

#### Search, then read

```text
localSearch(operation="text", path="src", searchText="validateInput", contextLines=2)
localGetFileContent(path="src/validation.ts", matchString="validateInput", contextLines=20)
```

#### Find tests for a feature

```text
localSearch(operation="files", path=".", names=["*.test.ts", "*.spec.ts"])
localSearch(operation="text", path="tests", searchText="featureName", resultView="files")
localGetFileContent(path="tests/feature.test.ts", matchString="featureName")
```

#### Inspect recent changes

```text
localSearch(operation="files", path=".", time={"modifiedWithin":"24h"}, entryType="f", detail="full")
localSearch(operation="text", path=".", searchText="TODO|FIXME", regex="perl")
```

---

### Local tool rules

1. Use `localSearch(operation:"tree")` or `localSearch(operation:"files")` before reading when the file is unknown.
2. Use `localSearch(operation:"text", resultView:"files")` for fast discovery when match bodies are not needed.
3. Use `localSearch(operation:"text")` with `contextLines` before opening a large file.
4. Use `localGetFileContent` with `matchString`, `startLine`/`endLine`, or `minify:"symbols"` instead of `fullContent` for large files.
5. Use pagination fields when a response advertises `hasMore=true`.

---

### Response shape

- Bulk envelope: `results[]` with `data`, `hints`, `pagination`, `outputPagination`.
- `localSearch` returns an operation-specific payload: text/structural matches, file rows, or tree entries.
- `localGetFileContent` returns file slices only — not directory listings.

### Anti-patterns

| Anti-Pattern | Better Approach |
|--------------|-----------------|
| `fullContent=true` on large files | Use `matchString`, line range, or `minify:"symbols"` |
| Search without scoping dirs | Use `excludeDir` to skip generated/vendor folders |
| Regex for exact literals | Use `regex:"fixed"` |
| Combining mutually exclusive flags | Pick one extraction mode |

**Parallelism:** independent queries run in parallel (batch limit: 5 per call). Sequential dependencies (`structure → search → read`) stay sequential.

---

### Local tools: related documentation

- [Clone and local tools workflow](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#clone-and-local-tools-workflow) - cloning repositories before local analysis.
- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#github-tools-reference) - remote GitHub search, fetch, clone, and PR tools.
- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) - environment variables and config file behavior.

---

## LSP tools reference

This is the canonical reference for Octocode's semantic code-intelligence operations. LSP is the protocol layer behind these operations; structural AST search remains part of `localSearch`.

Octocode exposes **one** public semantic tool:

| Tool | Use it for |
|------|------------|
| `lspGetSemantics` | Definitions, references, callers, callees, bidirectional call hierarchy, hover, document symbols, type definitions, and implementations. |

Semantic operations are local-only. Local tools default on for both CLI and MCP; set `ENABLE_LOCAL=false` to disable them. LSP needs a file that exists on disk. Use `localSearch` first when you need a symbol `lineHint`; `operation:"structural"` matches can provide AST-derived anchors before LSP proves symbol identity.

For external repos: clone first with `ghCloneRepo` (or fetch a subtree with `ghGetFileContent(type:"directory")`), then use the returned `localPath` as the `uri` prefix for `lspGetSemantics`. The path is always absolute and immediately valid.

### Workflow

1. Search with `localSearch(operation:"text")` or `localSearch(operation:"structural")` and capture the exact `lineHint`.
2. Query `lspGetSemantics` with `uri`, `type`, `symbolName`, and `lineHint`.
3. Page large symbol or call-flow results with `page` and `pageSize`.
4. Run project lint, typecheck, and tests before claiming risky changes are fully verified.

### `lspGetSemantics`

Required fields:

| Field | Required | Notes |
|-------|----------|-------|
| `uri` | Yes, except `workspaceSymbol` | Absolute local file path. |
| `type` | No | Defaults to `definition`. |
| `symbolName` | For anchored operations and `workspaceSymbol` | Exact symbol text at the target line. |
| `lineHint` | For anchored operations | 1-based line number from search results. |

Optional fields:

| Field | Notes |
|-------|-------|
| `orderHint` | Disambiguates repeated symbol text on the same line. |
| `workspaceRoot` | Overrides automatic project-root detection. |
| `contextLines` | Adds source previews to call-flow results. Keep `0` unless previews are needed. |
| `page` | Result page for `documentSymbols` and call-flow results. |
| `pageSize` | Semantic items per page. Defaults to `40` for `documentSymbols`, `10` for call-flow. Max `100`. |
| `depth` | Call-flow recursion depth. Keep `1` unless you need nested calls. |
| `includeDeclaration` | For `references`; defaults to `true`. |
| `groupByFile` | For `references`; adds per-file rollups. |

Semantic types:

| `type` | Best for | Output |
|--------|----------|--------|
| `definition` | Jumping from usage/import to declaration. For local TypeScript/JavaScript import aliases, definitions follow the import to the exported declaration when the language server first returns the import binding. | `payload.kind="definition"`, `locations[]`. |
| `references` | Blast radius for functions, types, variables, constants, classes. | `locations[]`, `totalReferences`, `totalFiles`, optional `byFile`. |
| `callers` | Static incoming calls to a callable symbol. | Compact `calls[]`, `summary.incomingCalls`, pagination. |
| `callees` | Static outgoing calls made by a callable symbol. | Compact `calls[]`, `summary.outgoingCalls`, pagination. |
| `callHierarchy` | Bidirectional call-flow snapshot. | Incoming and outgoing calls in one compact page. |
| `hover` | Quick type/signature/docs from the language server. | `markdown` or `text`. |
| `documentSymbols` | File outline and symbol inventory. | Compact `symbols[]`, `summary.kinds`, pagination. |
| `typeDefinition` | Declared type behind a symbol. | `locations[]`. |
| `implementation` | Concrete implementation behind an interface/abstract symbol when the server supports it. | `locations[]`. |

All semantic responses use this envelope:

| Field | Meaning |
|-------|---------|
| `type` | Requested semantic type. |
| `uri` | Resolved local file path. |
| `resolvedSymbol` | Symbol anchor for symbol-based requests. |
| `lsp` | Server availability and provider/source metadata. |
| `evidence` | Confidence, completeness, and reason when incomplete. |
| `summary` | Agent-readable totals for symbol and call-flow requests. |
| `payload` | Typed semantic payload. |
| `pagination` | Native semantic pagination for symbol and call-flow requests. |
| `warnings` | Incomplete or unavailable evidence reasons. |
| `hints` | Suggested next steps. |

Empty semantic payloads use `payload.kind="empty"` with a machine-readable `category`, such as `symbolNotFound`, `noLocations`, `noReferences`, `noHover`, or `noCalls`. The CLI maps these semantic misses to exit code `3` (`not found`) so scripts can fail without parsing the JSON envelope.

Call-flow payloads are compact by default. Each call includes the target item, sampled call ranges, `rangeCount`, and `rangeSampleCount`. Use `contextLines>0` only when source previews are useful.

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

Every **other** semantic operation — `references`, `definition`, `hover`, `callers`/`callees`/`callHierarchy`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`/`subtypes`, `diagnostic` — requires a real server. When no server is available octocode **does not fall back to a syntactic guess**: it returns `status:"error"` with `errorCode:"lspServerUnavailable"` and a message directing you to `localSearch` (text/structural search) + `localGetFileContent`. (There is no longer a same-file-only `references` native path — a partial answer that silently omits cross-file usages is a trap, so it now errors instead.) See [LSP server lifecycle](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md).

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

### Language servers

TypeScript and JavaScript are bundled through `typescript-language-server` and `typescript`; JS/TS also has the server-free native path above. Other languages require their language server to be installed or configured.

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
| `OCTOCODE_PHP_SERVER_PATH` | PHP |
| `OCTOCODE_SQL_SERVER_PATH` | SQL |
| `OCTOCODE_SWIFT_SERVER_PATH` | Swift |
| `OCTOCODE_JSON_SERVER_PATH` | JSON |
| `OCTOCODE_YAML_SERVER_PATH` | YAML |
| `OCTOCODE_HTML_SERVER_PATH` | HTML |
| `OCTOCODE_CSS_SERVER_PATH` | CSS/SCSS/LESS |

#### Custom / bring-your-own servers

To add a language with **no built-in server** (for example, Scala, Kotlin, or Ruby) — or to replace a
built-in one — register it in a JSON config. Loaded in precedence order:

1. `$OCTOCODE_LSP_CONFIG` (explicit file path)
2. `<workspace>/.octocode/lsp-servers.json` (per-project)
3. `~/.octocode/lsp-servers.json` (per-user)

The file maps a file **extension** to a launch spec; a custom entry overrides the built-in spec
for that extension:

```jsonc
{
  "languageServers": {
    ".scala": { "command": "metals", "args": ["stdio"], "languageId": "scala" }
  }
}
```

`command` and `languageId` are required; `args` (default `[]`) and `initializationOptions`
(passed verbatim in `initialize`) are optional. With the config present, every semantic op works
for that language; without it the extension is unsupported and semantic ops throw
`lspServerUnavailable` (→ fall back to `localSearch`). See
[`LSP_SERVER_LIFECYCLE.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/LSP_SERVER_LIFECYCLE.md#custom--bring-your-own-lsp-any-language).

### Examples

Definition:

```json
{
  "uri": "/workspace/src/run.ts",
  "type": "definition",
  "symbolName": "printSchema",
  "lineHint": 133
}
```

References grouped by file:

```json
{
  "uri": "/workspace/src/run.ts",
  "type": "references",
  "symbolName": "isOctokitDeprecation",
  "lineHint": 27,
  "includeDeclaration": true,
  "groupByFile": true
}
```

Paginated call flow:

```json
{
  "uri": "/workspace/src/run.ts",
  "type": "callHierarchy",
  "symbolName": "printSchema",
  "lineHint": 133,
  "limit": 5,
  "page": 1
}
```

Diagnostics:

```json
{
  "uri": "/workspace/src/run.ts",
  "severity": "all"
}
```

### LSP: related documentation

- [Local code tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#local-code-tools-reference)
- [Clone and local tools workflow](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#clone-and-local-tools-workflow)

---

## Clone and local tools workflow

> How to use `ghCloneRepo` and `ghGetFileContent` (directory mode) to bridge GitHub repositories with local + LSP tools for deep code analysis.

> **Prerequisites:** Clone is enabled by default. Clone and local tools must not be explicitly disabled.

---

### The bridge: GitHub to clone or fetch to local and LSP

Octocode MCP has two worlds of tools:

| World | Tools | Strengths | Limitations |
|-------|-------|-----------|-------------|
| **GitHub** | `ghSearch`, `ghGetFileContent` | Fast, no disk usage, works on any repository | No LSP, no semantic analysis, API rate limits |
| **Local + LSP** | `localSearch`, `localGetFileContent`, `localAnalyzeGraph`, `lspGetSemantics` | Unified discovery, exact reads, topology, and semantic navigation | Only works on files on disk |

**Two tools bridge these worlds** — they download content to `<octocode-home>/tmp/` so local and LSP tools can analyze it:

| Bridge Tool | When to Use | How it Works |
|-------------|-------------|--------------|
| **`ghCloneRepo`** | Full repository or sparse subtree | Uses `git clone` into `tmp/clone` (requires git) |
| **`ghGetFileContent`** (type: `"directory"`) | Single directory of files | Uses GitHub API + `download_url` into `tmp/tree` (no git needed) |

Clones and API-fetched trees use separate tmp buckets with the same 24-hour TTL policy: `<octocode-home>/tmp/clone/{owner}/{repo}/{branch}/` for git clones, and `<octocode-home>/tmp/tree/{owner}/{repo}/{commitSha}/` for file/tree materialization.

**Ref resolution:** Both tools auto-detect the repository's default branch through the GitHub API when no `branch` is specified. Clones use the resolved branch in their path. File and directory materialization resolve the ref to a commit, return both `resolvedBranch` and `commitSha`, and use the immutable commit SHA in the path. A short-lived, auth-scoped ref pointer avoids resolving the same branch on every request.

```
┌─────────────────────┐       ┌────────────────────────────┐       ┌──────────────────────────┐
│  GitHub (remote)     │       │  Bridge Tools              │       │  Local + LSP (on disk)   │
│                      │       │                            │       │                          │
│  ghSearch        │──────▶│  ghCloneRepo           │──────▶│  localSearch         │
│  (code/tree/repos)│       │  (full/sparse clone)       │       │  localSearch      │
│  ghGetFileContent│       │                            │       │  localGetFileContent     │
│                      │       │  ghGetFileContent      │       │  localSearch          │
│                      │       │  (type: "directory")       │       │  lspGetSemantics   │
│                      │       │  (lightweight, no git)     │       │                          │
│                      │       │  Both return localPath     │       │                          │
│                      │       │  + next (localSearch,      │       │                          │
│                      │       │    viewStructure) + location│       │                          │
└─────────────────────┘       └────────────────────────────┘       └──────────────────────────┘
```

---

### When to clone versus directory fetch

| Scenario | Use GitHub Tools | Use Directory Fetch | Use Clone |
|----------|-----------------|--------------------|----|
| Quick file read | ✅ `ghGetFileContent` | Overkill | Overkill |
| Browse repository tree | ✅ `ghSearch(operation:"tree")` | Overkill | Overkill |
| Find code pattern across repositories | ✅ `ghSearch(operation:"code")` | Overkill | Overkill |
| **Read all files in a directory** | ❌ One-by-one | ✅ `type: "directory"` | Overkill |
| **Search within a directory** | Limited | ✅ Directory fetch → `localSearch` | Also works |
| **Trace function call chains** | ❌ Not possible | ❌ Partial context | ✅ Clone → `lspGetSemantics(type="callers")` / `type="callees"` |
| **Jump to symbol definitions** | ❌ Not possible | ❌ Partial context | ✅ Clone → `lspGetSemantics(type="definition")` |
| **Find all usages of a type** | ❌ Not possible | ❌ Partial context | ✅ Clone → `lspGetSemantics(type="references")` |
| **Deep code search with regex** | Limited | ✅ If scope is small | ✅ Clone → `localSearch` |
| **Explore monorepo subtree** | Slow (many API calls) | ✅ For small dirs | ✅ Sparse clone for large dirs |

**Rule of thumb:**
- Need a **single directory**? → `ghGetFileContent` with `type: "directory"` (no git required)
- Need **semantic analysis** (definitions, references, call hierarchy)? → `ghCloneRepo` first
- Need a **large subtree or full project context**? → `ghCloneRepo` with `sparsePath`

---

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
next:
  localSearch:
    tool: localSearch
    query:
      path: <octocode-home>/tmp/clone/vercel/next.js/main
      mode: discovery
  viewStructure:
    tool: localSearch
    query:
      path: <octocode-home>/tmp/clone/vercel/next.js/main
```

Pass `next.localSearch.query` or `next.viewStructure.query` directly to the respective tool — the `path` is always absolute.

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
next:
  localSearch:
    tool: localSearch
    query:
      path: <octocode-home>/tmp/clone/microsoft/TypeScript/main__sp_a3f8c1
      mode: discovery
  viewStructure:
    tool: localSearch
    query:
      path: <octocode-home>/tmp/clone/microsoft/TypeScript/main__sp_a3f8c1
```

> **Note:** LSP may have limited cross-file resolution in sparse checkouts since not all source files are present. If you need full project context, clone without `sparsePath`.

#### Mode 3: Directory fetch with `ghGetFileContent type:"directory"`

Lightweight alternative — no git required. Downloads individual files through the GitHub Contents API into `tmp/tree`.

```
ghGetFileContent:
  owner: "vercel"
  repo: "next.js"
  path: "packages/next/src/server"
  type: "directory"
```

**Result:**
```yaml
localPath: <octocode-home>/tmp/tree/vercel/next.js/0123456789abcdef0123456789abcdef01234567/packages/next/src/server
repoRoot: <octocode-home>/tmp/tree/vercel/next.js/0123456789abcdef0123456789abcdef01234567
commitSha: 0123456789abcdef0123456789abcdef01234567
fileCount: 12
complete: true
location:
  kind: directory
  localPath: <octocode-home>/tmp/tree/vercel/next.js/0123456789abcdef0123456789abcdef01234567/packages/next/src/server
  repoRoot: <octocode-home>/tmp/tree/vercel/next.js/0123456789abcdef0123456789abcdef01234567
  source: treeFetch
  commitSha: 0123456789abcdef0123456789abcdef01234567
  cached: false
  complete: true
next:
  localSearch:
    tool: localSearch
    query:
      path: <octocode-home>/tmp/tree/vercel/next.js/0123456789abcdef0123456789abcdef01234567/packages/next/src/server
      mode: discovery
  viewStructure:
    tool: localSearch
    query:
      path: <octocode-home>/tmp/tree/vercel/next.js/0123456789abcdef0123456789abcdef01234567/packages/next/src/server
```

> **Note:** `complete: false` means some files were skipped (binary, oversized, or file-limit). Use `ghCloneRepo` when completeness matters.

---

### Step-by-step workflows

#### Workflow 1: Browse a cloned repository tree

**Goal:** Understand the structure of an external repository using local tools.

```
Step 1: Clone the repo
  ghCloneRepo(owner="vercel", repo="next.js")
  → localPath = "<octocode-home>/tmp/clone/vercel/next.js/canary"

Step 2: Browse the tree
  localSearch(operation="tree", path=localPath, maxDepth=2)
  → See the full directory structure with file sizes and dates

Step 3: Drill into a directory
  localSearch(operation="tree", path=localPath + "/packages/next/src", maxDepth=2)
  → See the subdirectory contents
```

#### Workflow 2: Deep code analysis with LSP

**Goal:** Trace who calls a function in an external repository.

```
Step 1: Clone the repo
  ghCloneRepo(owner="vercel", repo="next.js", sparsePath="packages/next/src")
  → localPath

Step 2: Search for the function
  localSearch(operation="text", path=localPath, searchText="handleRequest")
  → Get file paths and lineHint values

Step 3: Jump to definition
  lspGetSemantics(type="definition", uri=localPath+"/server/router.ts", symbolName="handleRequest", lineHint=42)
  → See the function definition

Step 4: Trace callers
  lspGetSemantics(type="callers", uri=..., symbolName="handleRequest", lineHint=42)
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
  localSearch(operation="text", path=localPath, searchText="def route\\(", langType="py")
  → Full regex search, file type filtering, match context

Step 4: Use LSP
  lspGetSemantics(type="references", uri=localPath+"/src/flask/app.py", symbolName="route", lineHint=...)
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
  localSearch(operation="text", path=localPath, searchText="transformTypeScript")
  → Search only within the compiler code

Step 4: Find files by metadata
  localSearch(operation="files", path=localPath, names=["*.ts"], time={"modifiedWithin":"30d"})
  → Recently modified TypeScript files in the compiler
```

---

### Cache behavior

| Behavior | Details |
|----------|---------|
| **Materialization TTL** | Clone and tree entries use 24 hours by default (configurable through `OCTOCODE_CACHE_TTL_MS`) |
| **Shared response cache** | `ghSearch`, `ghSearchHistory`, `ghGetHistoryItem`, and `npmSearch` use per-response freshness periods from 5 minutes to 24 hours |
| **Conditional cache** | `ghGetFileContent` and the `ghSearch` tree operation retain response bodies and ETags for conditional refresh; stale bodies can remain available for up to 24 hours |
| **Response marker** | A result whose primary response payload was served from cache includes `cache: 1`. Fresh results and helper-only cache hits omit `cache`; no other marker value is valid. The contract is identical in CLI and MCP output. |
| **Clone cache** | `ghCloneRepo` uses the clone/materialization cache |
| **Live tools** | `localSearch`, `localGetFileContent`, `localAnalyzeGraph`, and `lspGetSemantics` read the workspace directly and don't cache tool results |
| **Location** | Clones: `<octocode-home>/tmp/clone/{owner}/{repo}/{branch}/`; file/tree fetches: `<octocode-home>/tmp/tree/{owner}/{repo}/{commitSha}/`; remote response L2: `<octocode-home>/tmp/response/` |
| **Identity** | The API resolves an omitted branch, then pins file/tree bytes and paths to the resolved commit SHA |
| **Ref pointer** | Auth-scoped branch/tag-to-commit results are cached for 60 seconds; `forceRefresh` bypasses the pointer |
| **Sparse clones** | Separate cache: `{branch}__sp_{hash}/` |
| **Coexistence** | Full clone and sparse clones of the same repository can coexist |
| **Cache hit** | Reuses immutable tree bytes; ref resolution can make one request after the 60-second pointer expires |
| **Clone vs directory** | Clone-cache and directory/file materialization are separate; directory fetch never overwrites a git clone |
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
4. **Result**: Any `localPath` returned by `ghCloneRepo` or `ghGetFileContent` (directory mode) is automatically valid for all local + LSP tools, even when the cloned repository lives outside your current shell workspace

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
| Read cloned file | `localGetFileContent` | `path` = `localPath + "/file.ts"` |
| Find files in clone | `localSearch` | `path` = `localPath` |
| Jump to definition | `lspGetSemantics` with `type="definition"` | `uri` = file in `localPath` |
| Find all references | `lspGetSemantics` with `type="references"` | `uri` = file in `localPath` |
| Trace callers/callees | `lspGetSemantics` with `type="callers"` or `type="callees"` | `uri` = file in `localPath` |

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

The active MCP tool catalog is defined in [packages/octocode-tools-core/src/tools/toolConfig.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/tools/toolConfig.ts). Local schema helpers live in [packages/octocode-tools-core/src/scheme/fields.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/scheme/fields.ts); each GitHub/package/LSP tool owns its independent `scheme.ts` beside the tool implementation, for example [packages/octocode-tools-core/src/tools/github_search_pull_requests/scheme.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/tools/github_search_pull_requests/scheme.ts) and [packages/octocode-tools-core/src/tools/lsp/semantic_content/scheme.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/tools/lsp/semantic_content/scheme.ts).

Response behavior is shared through [packages/octocode-tools-core/src/utils/response/bulk.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/utils/response/bulk.ts), [packages/octocode-tools-core/src/utils/pagination/core.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/utils/pagination/core.ts), [packages/octocode-tools-core/src/utils/pagination/hints.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/utils/pagination/hints.ts), and [packages/octocode-tools-core/src/scheme/responseEnvelope.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/scheme/responseEnvelope.ts).

This playbook extends the existing contract tests: [the all-tools pagination contract test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/tests/tools/all-tools.pagination-contract.test.ts), [the all-tools schema contract test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/tests/scheme/all-tools.schema-contract.test.ts), [the execution-boundaries flow test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/tests/tools/executionBoundaries.flows.test.ts), and [the response contract test](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/tests/utils/response/responses.contract.test.ts).

### Verification goals

Every tool must pass the same top-level contract:

| Area | Required checks |
| --- | --- |
| Registration | Tool is present in `ALL_TOOLS`, has a direct execution definition, has an MCP input schema, publishes no output schema, and registers with the expected security wrapper. |
| Bulk envelope | `queries` accepts 1 to 5 items without caller IDs. Response rows use matching zero-based `index` values, preserve input order, isolate per-query errors, and do not let one failed query block siblings. |
| Output shape | Responses expose machine data in `structuredContent.results[]` and preserve the complete sanitized YAML or JSON representation in `content[0].text` for every MCP client. Lean hoists apply: `base` relativizes absolute `path`/`uri`, `shared` collapses constants identical across leaves (identity keys `owner`/`repo`/`name`/`id` are never hoisted). |
| Pagination | Native page fields, query-level `charOffset`/`charLength`, and top-level `responseCharOffset`/`responseCharLength` work independently and together. Pagination hints appear only when `hasMore=true`. |
| Hints | Tool `hints.ts` files expose only `empty` and `error`. Empty hints are conditional and filter-aware. Error hints classify the failure and stay short. Success path hints are limited to data-bearing signals such as pagination or warnings. |
| Empty results | Successful no-match responses are not errors. They must include a clear empty signal, preserve the query's ordered index, and provide recovery hints only when the query context makes a concrete next step possible. |
| Errors | Provider, validation, path, auth, rate-limit, timeout, LSP-unavailable, and command failures return structured errors with recovery context and without leaking secrets. |
| Evidence | Tools that can report evidence must set `evidence.kind`, `answerReady`, `confidence`, and `complete` consistently. Aggregated evidence should downgrade confidence and completeness when any query is partial or fallback-based. |
| Security | Local tools respect path validation and command allow-lists. Remote tools sanitize errors and redact secrets. Clone and directory fetch do not write outside the intended cache or checkout root. |

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
| Params | Verify `owner`, `repo`, `path`, optional `branch`, file versus directory mode, `fullContent`, `matchString`, `startLine`/`endLine`, `contextLines`, `charOffset`, and `charLength`. |
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

#### Verify `npmSearch`

Primary code: [packages/octocode-tools-core/src/tools/package_search/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/package_search). Schema: `NpmSearchQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Set exactly one of `packageName` for exact lookup or `keywords` for discovery; `page` paginates discovery. |
| Implementation | npm registry metadata is normalized, repository URLs are parsed into owner/repo when possible, deprecated packages add warning context, and package-not-found is empty. |
| Pagination | `page` continues discovery. Top-level `responseCharLength` still pages large metadata responses. |
| Empty | Empty search returns package-specific recovery without pretending the package exists. |
| Research quality | Results must include package identity, version, description, repository URL or owner/repo, homepage, weekly downloads if fetched, license, keywords, and freshness metadata when available. |

#### Verify `ghCloneRepo`

Primary code: [packages/octocode-tools-core/src/tools/github_clone_repo/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/github_clone_repo). Schema: `CloneRepoQueryLocalSchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `owner`, `repo`, `branch`, `sparsePath`, and `forceRefresh`. Clone is side-effecting and has no verbosity field. |
| Implementation | Requires clone/local enablement, resolves branch fallback, reuses cache when valid, refreshes expired cache, and returns a safe `localPath`. |
| Pagination | No native pagination is expected, but bulk response pagination must still work. |
| Empty | Not applicable. A missing repository, branch, or path is an error with recovery context. |
| Data management | Verify cache TTL, cache invalidation, concurrent clone locking, cleanup on failed clone, and no writes outside the tmp materialization roots. |
| Research quality | Returned `localPath` must be immediately usable by `localSearch`, `localGetFileContent`, and LSP tools. |

#### Verify `localSearch`

Public router: [packages/octocode-tools-core/src/tools/local_search/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/local_search). Schema: `LocalSearchQuerySchema`. The router delegates to the internal text/structural, file-discovery, and tree engines.

| Surface | Checks |
| --- | --- |
| Params | Verify `operation`, `path`, `searchText` or `pattern`, `regex`, `caseMode`, `wholeWord`, type/include/exclude/excludeDir, hidden/noIgnore, `resultView`, `contextLines`, `matchContentLength`, `maxMatchesPerFile`, `matchPage`, `pageSize`, and `page`. |
| Hidden fields | MCP schema must not expose implementation-only knobs such as threads, binary, encoding, debug, passthru, or symlink following. |
| Mutex | Structural `pattern` conflicts with `rule`; non-structural search requires `searchText`. Invalid enum combinations become per-query errors. |
| Implementation | Runs ripgrep in-process through the native engine. No external `rg` binary and no grep fallback. Invalid regex, path errors, and no-permission paths are structured errors. |
| Pagination | File and match pagination work independently. `line` values are stable 1-indexed `lineHint` inputs for LSP tools. |
| Empty | Empty hints name active filters such as type, include, exclude, excludeDir, or path. No-filter empty stays silent. |
| Research quality | Results must include file path, match count, line, column, snippet value, and enough context to drive precise `lspGetSemantics` queries such as `type="definition"`, `type="references"`, `type="callers"`, or `type="callees"`. |

#### Verify `localSearch(operation:"tree")`

Primary code: [packages/octocode-tools-core/src/tools/local_view_structure/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/local_view_structure). Schema: `ViewStructureQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `operation:"tree"`, `path`, `namePattern`, `extensions`, exclude filters, `maxDepth`, total `limit`, `pageSize`, and `page`. |
| Hidden fields | `extension` singular and `recursive` are not exposed in unified `localSearch`. Use `extensions`; set a bounded `maxDepth` to recurse. |
| Implementation | Directory walk respects path validation, depth cap, ignored directories, sorting, and entry typing. Symlink and permission cases are explicit. |
| Pagination | Entry pagination uses stable ordering so page 2 continues page 1 without duplicates or missed entries. |
| Empty | Empty directories are empty, not errors. Missing paths are errors. Filtered empties name the active filter. |
| Research quality | Entries must identify name, path, type, size, modified time, and depth so the next search or content call can be scoped. |

#### Verify `localSearch`

Primary code: [packages/octocode-tools-core/src/tools/local_find_files/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/local_find_files). Schema: `FindFilesQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `operation:"files"`, `path`, name/path/regex filters, entry type, size filters, modified/accessed filters, permissions if exposed, total `limit`, `pageSize`, and `page`. |
| Implementation | Uses the safe file-discovery path, respects allowed paths, handles large trees without unbounded output, and returns stable metadata. |
| Pagination | File pagination and char pagination both work. Cap notices must not replace next-page cursors. |
| Empty | Empty hints quote active filters such as `names`, `time.modifiedWithin`, or `size.greater`. No-filter empty stays silent. |
| Research quality | Results must support targeted follow-ups by path, type, size, permissions, and timestamps. |

#### Verify `localGetFileContent`

Primary code: [packages/octocode-tools-core/src/tools/local_fetch_content/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/local_fetch_content). Schema: `FetchContentQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `path`, `fullContent`, `matchString`, `startLine`, `endLine`, `contextLines`, `charOffset`, `charLength`. |
| Mutex | `fullContent`, `matchString`, and line ranges are mutually exclusive, with per-query errors inside bulk calls. |
| Implementation | Handles UTF-8 files, large files, minified content, binary/unreadable files, no trailing newline, and out-of-range line requests. |
| Pagination | Character pagination continues exact content without overlap. Match extraction plus pagination must preserve `matchRanges`. |
| Empty | A missing `matchString` result returns empty with no fake content. Missing file and invalid path are errors. |
| Research quality | Returned content must include path, line range, total lines, `isPartial`, and enough source text to cite or reason from. Partial line-range reads emit a `startLine=N` continuation hint. |

#### Verify `lspGetSemantics`

Primary code: [packages/octocode-tools-core/src/tools/lsp/semantic_content/](https://github.com/bgauryy/octocode/tree/main/packages/octocode-tools-core/src/tools/lsp/semantic_content). Schema: `LspGetSemanticsQuerySchema`.

| Surface | Checks |
| --- | --- |
| Params | Verify `uri`, `type`, `symbolName`, `lineHint`, `orderHint`, `includeDeclaration`, `groupByFile`, `depth`, `page`, `contextLines`, and output pagination. |
| Implementation | Requires a `lineHint` for symbol-anchored types, resolves exact code occurrences while ignoring string/comment hits, uses pooled LSP clients, and reports capability gaps explicitly. |
| Pagination | Large semantic payloads page without losing target identity. |
| Empty | Symbol-not-found, unsupported capability, and LSP-unavailable paths are explicit. |
| Semantic quality | Definition/reference/call/hover/symbol outputs identify URI, range, symbol identity, completeness, and static-vs-dynamic limits where applicable. |

### Cross-tool research quality suites

These suites verify that tools compose into reliable research workflows.

| Suite | Steps | Pass criteria |
| --- | --- | --- |
| Local semantic navigation | `localSearch` for a symbol, then `lspGetSemantics` with `type="definition"`, `type="references"`, and `type="callers"`/`type="callees"` using returned line hints. | LSP tools resolve the same symbol, references include the definition when requested, call direction is correct, and fallback mode is explicit if used. |
| Remote to local deep dive | `ghSearch(operation:"code"|"repositories")`, then `ghCloneRepo`, then local search and LSP tools on `localPath`. | Remote identity, branch, clone path, and local path all line up. No result requires guessing a path or branch. |
| Structure to content | `ghSearch(operation:"tree")` or `localSearch(operation:"tree")`, then content fetch on selected entries. | Paths emitted by structure tools are directly accepted by content tools. Empty directories and missing files are differentiated. |
| Package provenance | `npmSearch`, then `ghSearch(operation:"tree"|"code")` on parsed repository owner/name. | Package repository metadata is normalized enough to drive GitHub tools, and missing/ambiguous repository URLs are represented as missing evidence. |
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
yarn test tests/tools/executionBoundaries.flows.test.ts
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

1. All 10 default-enabled tools still register with input schemas and without output schemas.
2. All public schema defaults, caps, hidden fields, and mutex rules have tests.
3. Every tool has success, empty, error, mixed-bulk, pagination, lean-output (`base`/`shared`), and verbosity coverage.
4. Remote tools cover auth, rate limit, provider error, no results, and provider-mapper edge cases.
5. Local tools cover path validation, large output, hidden/ignored files, empty results, and command allow-list behavior.
6. LSP tools cover semantic success, fallback mode, symbol-not-found, wrong line hint, `orderHint`, pagination, and context snippets.
7. Hints are short, contextual, deduped, and absent on final pages.
8. Evidence metadata correctly represents whether the result is answer-ready, complete, and high-confidence.
9. No response leaks secrets, raw tokens, internal stack traces, or unrelated query metadata.
10. `yarn lint`, `yarn typecheck`, and `yarn test` pass in the package environment.
