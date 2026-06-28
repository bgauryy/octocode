# GitHub Tools Reference

Concise reference for Octocode MCP remote research tools: GitHub code/repo/PR search, GitHub content access, cloning, and npm package lookup.

## Configuration

| Variable | Purpose |
|----------|---------|
| `OCTOCODE_TOKEN` | Highest-priority GitHub token. |
| `GH_TOKEN` | GitHub CLI compatible token. |
| `GITHUB_TOKEN` | GitHub token fallback. |
| `GITHUB_API_URL` | GitHub Enterprise API base URL. |
| `ENABLE_LOCAL` | Local tools default on; set `false` to disable clone-backed local workflows. |
| `ENABLE_CLONE` | Enables `ghCloneRepo` and `ghGetFileContent(type="directory")`. |

Every tool accepts bulk input (`{ "queries": [...] }`), up to 5 queries per call. All tools support `page`, `responseCharOffset`, and `responseCharLength` for pagination. Use `npx octocode tools <toolName> --scheme` for the exact active schema.

## Tool Selection

| Need | Tool |
|------|------|
| Search code across GitHub | `ghSearchCode` |
| Read a file or fetch a directory | `ghGetFileContent` |
| Browse a repository tree | `ghViewRepoStructure` |
| Discover repositories | `ghSearchRepos` |
| Search PR history or inspect a PR | `ghHistoryResearch` |
| Materialize a repo/subtree locally | `ghCloneRepo` |
| Resolve npm package to source repo | `npmSearch` |

## `ghSearchCode`

Search code or paths across GitHub.

Key fields:

| Field | Meaning |
|-------|---------|
| `keywordsToSearch` | Required search terms. Terms are AND-combined; split phrases into separate words only when each word must match. |
| `owner`, `repo` | Scope to an owner or repository. Use both for one repo. |
| `extension` | Extension without a dot, such as `ts`. |
| `filename` | Filename filter. |
| `path` | Parent-directory prefix filter. |
| `match` | `file` for content search, `path` for path/name search. |
| `page` | Result page. |

Examples:

```json
{ "keywordsToSearch": ["useReducer"], "owner": "facebook", "repo": "react" }
{ "keywordsToSearch": ["middleware"], "extension": "ts", "owner": "vercel", "repo": "next.js" }
{ "keywordsToSearch": ["config"], "match": "path", "owner": "facebook" }
```

Rules:

- Scope with `owner`/`repo` as soon as you can.
- Use a few distinctive identifiers.
- Avoid stacking too many filters at once.

## `ghGetFileContent`

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
| `matchStringContextLines` | Context around `matchString`. |
| `matchStringIsRegex`, `matchStringCaseSensitive` | Match behavior. |
| `charOffset`, `charLength` | File-content pagination. |
| `minify` | `standard` (default, strips comments/blank lines), `none` (exact raw text), or `symbols` (structural skeleton only). |

File extraction modes are mutually exclusive: use one of `fullContent`, `startLine`/`endLine`, `matchString`, or `minify: "symbols"`.

Directory mode:

- Requires `ENABLE_CLONE=true` and local tools not explicitly disabled.
- Returns `localPath` (absolute), `location` (kind/source/cached/complete), and `next` pointing directly to `localSearchCode` and `localViewStructure` with ready-to-use `path` values — pass them as-is.
- Rejects file-only extraction fields.

Examples:

```json
{ "owner": "facebook", "repo": "react", "path": "packages/react/index.js", "startLine": 1, "endLine": 80 }
{ "owner": "vercel", "repo": "next.js", "path": "packages/next/src", "type": "directory" }
```

## `ghViewRepoStructure`

Browse a repository tree.

Key fields:

| Field | Meaning |
|-------|---------|
| `owner`, `repo` | Required repository. |
| `branch` | Branch, tag, or commit SHA. |
| `path` | Directory path. Use `""` or `"."` for root. |
| `depth` | Recursion depth. |
| `page` | Result page. |
| `itemsPerPage` | Entries per page, max 200. |

Examples:

```json
{ "owner": "vercel", "repo": "next.js", "path": "", "depth": 1 }
{ "owner": "facebook", "repo": "react", "path": "packages", "depth": 2, "itemsPerPage": 100 }
```

## `ghSearchRepos`

Discover repositories.

Key fields:

| Field | Meaning |
|-------|---------|
| `keywordsToSearch` | Name/description/readme terms. |
| `topicsToSearch` | GitHub topics. Useful but sparse. |
| `owner` | Scope to one owner, or enumerate owner repos when no keywords are supplied. |
| `language` | Primary language filter. |
| `stars`, `size`, `created`, `updated` | GitHub range filters. |
| `archived` | Include archived repos when true. |
| `match` | Search name, description, and/or readme. |
| `sort` | `stars`, `forks`, `help-wanted-issues`, `updated`, or `best-match`. |
| `page` | Result page. |

Examples:

```json
{ "keywordsToSearch": ["auth"], "language": "TypeScript", "stars": ">1000" }
{ "owner": "openai" }
```

## `ghHistoryResearch`

Find PRs or inspect one PR.

Start with metadata, then request targeted content for a specific `prNumber`.

Key search fields:

| Field | Meaning |
|-------|---------|
| `owner`, `repo` | Repository scope. |
| `query` | Free-text PR search. Prefer title keywords first. |
| `matchScope` | `title`, `body`, and/or `comments`. |
| `state` | `open`, `closed`, or `merged`. |
| `author`, `assignee`, `commenter`, `involves`, `mentions` | User filters. |
| `label` / `labels` | Label filter. |
| `created`, `updated`, `closed`, `merged-at` | Date filters. |
| `sort`, `order` | Result ordering. |
| `page` | Search result page. |

Direct PR fields:

| Field | Meaning |
|-------|---------|
| `prNumber` | Direct PR lookup. Cheapest when known. |
| `content` | Explicit selector for body, changed files, patches, comments, reviews, or commits. |
| `reviewMode` | `summary` or `full` convenience mode. |
| `filePage`, `commentPage`, `commitPage`, `itemsPerPage` | Pagination for direct PR content. |
| `includeBots` | Include bot comments. Defaults false. |
| `charOffset`, `charLength` | Body/comment pagination. |

Examples:

```json
{ "owner": "vercel", "repo": "next.js", "query": "middleware", "matchScope": ["title"], "state": "merged" }
{ "owner": "vercel", "repo": "next.js", "prNumber": 12345, "content": { "metadata": true, "changedFiles": true } }
{ "owner": "vercel", "repo": "next.js", "prNumber": 12345, "content": { "patches": { "mode": "selected", "files": ["packages/next/src/server.ts"] } } }
```

Rules:

- Use `prNumber` when known.
- Avoid broad comment searches until title/body search fails.
- Request selected patches instead of full PR content for large PRs.

## `ghCloneRepo`

Clone a repository or sparse subtree into Octocode's local cache.

Requires `ENABLE_CLONE=true` and local tools not explicitly disabled.

Key fields:

| Field | Meaning |
|-------|---------|
| `owner`, `repo` | Required repository. |
| `branch` | Branch to clone. Omit to use default branch. |
| `sparse_path` | Optional subdirectory sparse checkout. |

Returns `localPath` (absolute), `location` (kind/source/cached/complete), and `next` with ready-to-use `localSearch` and `viewStructure` query params — pass `next.localSearch.query` or `next.viewStructure.query` directly to the respective tool.

Examples:

```json
{ "owner": "facebook", "repo": "react", "branch": "main" }
{ "owner": "microsoft", "repo": "TypeScript", "sparse_path": "src/compiler" }
```

Rules:

- Use `sparse_path` for large monorepos.
- Use `ghGetFileContent` when you only need one file.
- Cached clones are reused.
- `next.localSearch.query.path` and `next.viewStructure.query.path` equal `localPath` — use them as-is.

## `npmSearch`

Resolve npm packages to metadata and source repositories.

Key fields:

| Field | Meaning |
|-------|---------|
| `name` | Exact npm package name or npm keyword query. |
| `npmFetchMetadata` | Fetch heavier npm metadata when needed. |
| `page` | Page keyword-search results. |

Examples:

```json
{ "name": "react" }
{ "name": "typescript eslint", "page": 2 }
```

Use `npmSearch` before GitHub repo search when the user gives a package name.

## Workflows

| Task | Flow |
|------|------|
| Understand a package | `npmSearch` -> `ghViewRepoStructure` -> `ghSearchCode` -> `ghGetFileContent` |
| Find examples of a pattern | `ghSearchCode` -> `ghGetFileContent` |
| Explore a repo | `ghViewRepoStructure` -> `ghGetFileContent(README)` -> `ghSearchCode` |
| Explain why code changed | `ghSearchCode` -> `ghHistoryResearch` -> direct `prNumber` content |
| Deep local analysis | `ghCloneRepo` -> local tools |

## Rules

- Use GitHub tools for remote repositories, not files already on disk.
- Use `npmSearch` for known npm package names.
- Use `ghViewRepoStructure` before reading unknown paths.
- Use `matchString`, line ranges, or `minify: "symbols"` instead of `fullContent` for large files.
- Use PR metadata first, then selected content.
- Use `ghCloneRepo` only when local analysis is worth the clone cost.

Related docs:

- [Tool Behavior Guide](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md) — known behaviors, control patterns, and token-cost tradeoffs per tool
- [Local Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md)
- [LSP Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md)
- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md)
