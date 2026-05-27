# GitHub Tools Reference

> Complete reference for Octocode MCP code host tools — external research, code search, repository exploration, and package discovery on **GitHub**.

---

## Configuration

### GitHub

| Variable         | Description                                                                                       |
| ---------------- | ------------------------------------------------------------------------------------------------- |
| `GITHUB_TOKEN`   | GitHub personal access token                                                                      |
| `OCTOCODE_TOKEN` | Octocode-specific token (highest priority)                                                        |
| `GH_TOKEN`       | GitHub CLI compatible token                                                                       |
| `GITHUB_API_URL` | Custom API URL for GitHub Enterprise                                                              |
| `ENABLE_CLONE`   | Enable `githubCloneRepo` and `githubGetFileContent` directory mode (requires `ENABLE_LOCAL=true`) |

---

## Overview

Octocode MCP provides **7 tools** for external code research on GitHub:

| Category              | Tools                                                                      | Purpose                                       |
| --------------------- | -------------------------------------------------------------------------- | --------------------------------------------- |
| **Search Tools** (3)  | `githubSearchCode`, `githubSearchRepositories`, `githubSearchPullRequests` | Find code, repos, and PRs                     |
| **Content Tools** (3) | `githubGetFileContent`, `githubViewRepoStructure`, `githubCloneRepo`       | Read files, browse trees, clone repos locally |
| **Package Tools** (1) | `packageSearch`                                                            | Lookup NPM/PyPI packages → get repo URLs      |

### Research Context (All Tools)

Every tool query **requires** three research context fields:

| Field              | Description                                  |
| ------------------ | -------------------------------------------- |
| `mainResearchGoal` | High-level objective of the research session |
| `researchGoal`     | Specific goal for this particular query      |
| `reasoning`        | Why this tool/query was chosen               |

These fields are required on **every query** for all GitHub and package tools. They help track research intent and improve result quality.

### Universal Output Pagination

All external-research tools support an output-size continuation contract in addition to any tool-specific paging such as `page`, `limit`, `entriesPerPage`, or `prNumber`.

- Query-level pagination: use `charOffset` and `charLength` on a query to continue oversized single-query results. For content tools, these fields page file content. For search/list tools, they page the returned payload after provider pagination is applied.
- Bulk-response pagination: use top-level `responseCharOffset` and `responseCharLength` on the tool call to page the outer `results[]` array when a multi-query response becomes too large.
- Response fields:
  - `pagination`: provider/domain pagination or file-content pagination
  - `outputPagination`: query-level output-size pagination metadata
  - `responsePagination`: top-level bulk response pagination metadata
- Default budget: if you do not pass overrides, Octocode auto-pages oversized responses using `output.pagination.defaultCharLength` from config, which defaults to `8000`.

---

## Tools at a Glance

### Search Tools

| Tool                           | Description                                                                                                                                                |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`githubSearchCode`**         | Search for code patterns across repositories by keywords. Filter by file extension, filename, path, or match type (content vs path).                       |
| **`githubSearchRepositories`** | Discover repositories by keywords or topics. Filter by stars, size, dates, and sort results.                                                               |
| **`githubSearchPullRequests`** | Search pull requests with extensive filters. Retrieve metadata, changed-file info, comments, and diff details.                                             |

### Content Tools

| Tool                          | Description                                                                                                                                                                                                                                |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **`githubGetFileContent`**    | Read file content from repositories, or fetch an entire directory to disk (`type: "directory"`). Supports line ranges, string matching with context, and pagination for large files. Directory mode requires `ENABLE_LOCAL=true` and `ENABLE_CLONE=true`. |
| **`githubViewRepoStructure`** | Display directory tree structure of a repository. Configurable depth and pagination.                                                                                                                                                       |
| **`githubCloneRepo`**         | Clone a repository (or subdirectory) locally for deep analysis with local + LSP tools. Requires `ENABLE_LOCAL=true` and `ENABLE_CLONE=true`.                                                                                                |

### Package Tools

| Tool                | Description                                                                                                       |
| ------------------- | ----------------------------------------------------------------------------------------------------------------- |
| **`packageSearch`** | Lookup NPM or Python packages to find repository URLs, version info, and metadata including deprecation warnings. |

### Quick Decision Guide

| Question                             | Tool                       |
| ------------------------------------ | -------------------------- |
| "Find code pattern across repos"     | `githubSearchCode`         |
| "Find repositories about X"          | `githubSearchRepositories` |
| "Find PRs that changed X"            | `githubSearchPullRequests` |
| "Read file from repo"                | `githubGetFileContent`     |
| "Browse repository structure"        | `githubViewRepoStructure`  |
| "Clone repo for deep local analysis" | `githubCloneRepo`          |
| "Get repo URL for npm package"       | `packageSearch`            |

---

## Search Tools (Detailed)

### `githubSearchCode`

**What it does:** Search for code patterns using keywords across repositories.

**Key parameters:**

- `keywordsToSearch` (required): Array of 1-5 search keywords
- `owner`: User or organization
- `repo`: Specific repository
- `extension`: Filter by file extension (e.g., `ts`, `py`)
- `filename`: Filter by filename
- `path`: Filter by path prefix
- `match`: `file` (search content) or `path` (search file paths)
- `limit`: Results per page (default: 10, max: 100)
- `page`: Page number (default: 1, max: 10)

**Example queries:**

```
# Find useState hook implementations
keywordsToSearch=["useState", "hook"], extension="ts"

# Find config files in an org
owner="facebook", keywordsToSearch=["config"], match="path"

# Find code in a specific repo
owner="vercel", repo="next.js", keywordsToSearch=["middleware"]
```

**Response format:** Each file in the result contains `path`, `text_matches`, and `lastModifiedAt`. When all results come from the same repo, a `repositoryContext` with just the `branch` name is included (for follow-up calls to `githubGetFileContent`).

**⚠️ Gotchas:**

- Use 1-2 filters max. **Never combine** extension + filename + path together
- `path` is a strict prefix: `pkg` finds `pkg/file`, NOT `parent/pkg/file`

---

### `githubSearchRepositories`

**What it does:** Discover repositories by keywords or topics.

**Key parameters:**

- `keywordsToSearch`: Keywords to search in repos
- `topicsToSearch`: Topics to filter by
- `owner`: Filter by user or organization
- `stars`: Star count filter (e.g., `>1000`, `100..500`)
- `size`: Repository size in KB
- `created`: Creation date filters
- `updated`: Last update date
- `sort`: Sorting field (`forks`, `stars`, `updated`, `best-match`)
- `match`: Match scope — array of: `name`, `description`, `readme`
- `limit`: Results per page (default: 10)
- `page`: Page number (default: 1, max: 10)

**Example queries:**

```
# Find popular TypeScript CLI tools
topicsToSearch=["typescript", "cli"], stars=">1000"

# Find auth services in an org
owner="wix-private", keywordsToSearch=["auth-service"]
```

**⚠️ Gotchas:**

- Check `pushedAt` (code change) > `updatedAt` (meta change) for activity
- `stars >1000` filters noise but may hide new projects
- Try synonyms: `auth` ↔ `authentication`, `plugin` ↔ `extension`
- Archived repos are auto-excluded

---

### `githubSearchPullRequests`

**What it does:** Search GitHub pull requests with extensive filtering.

**Key parameters:**

- `prNumber`: Direct lookup (ignores all other filters)
- `owner`: User or organization
- `repo`: Repository name
- `query`: Free-text search
- `state`: `open`, `closed`
- `author`: User filter
- `assignee`: Assignee filter
- `label`: Label filter (string or array)
- `created`: Date filters
- `updated`: Update date
- `merged`: Boolean — only merged PRs
- `draft`: Boolean — draft status
- `sort`: `created`, `updated`, `best-match`
- `order`: `asc` or `desc` (default: desc)
- `type`: `metadata`, `fullContent`, `partialContent`
- `withComments`: Include comments
- `withCommits`: Include commit list
- `partialContentMetadata`: Specific files to fetch diff for (`[{ file, additions?, deletions? }]`)
- `limit`: Max results (default: 5, max: 10)
- `page`: Page number (default: 1, max: 10)

**Additional filters:**

- `head`/`base`: Branch filters (source/target branch)
- `commenter`/`involves`/`mentions`: User involvement filters
- `review-requested`/`reviewed-by`: Review filters
- `no-label`/`no-milestone`/`no-project`/`no-assignee`: Negative filters
- `closed`/`merged-at`: Date-based close/merge filters
- `comments`/`reactions`/`interactions`: Numeric range filters
- `match`: Search scope (`title`, `body`, `comments`)

**PR body auto-truncation:** PR body text is automatically truncated based on the `limit` (batch size) to save tokens:

| Batch Size  | Body Limit    | Rationale            |
| ----------- | ------------- | -------------------- |
| `limit=1`   | No truncation | Deep-dive intent     |
| `limit=2-3` | 2000 chars    | Moderate exploration |
| `limit=4+`  | 800 chars     | Batch scan           |

Truncated bodies include a hint with the PR number for requesting the full body.

**Example queries:**

```
# Get specific PR metadata
prNumber=123, type="metadata", owner="org", repo="app"

# Find merged PRs that changed auth
owner="org", repo="app", state="closed", merged=true, query="authentication"

# Get PR with comments (understand WHY)
prNumber=123, type="metadata", withComments=true, owner="org", repo="app"
```

**Output pagination:**

- `charOffset`/`charLength`: Character-based pagination for large responses
- PRs with >30 file changes emit hints guiding you to use `type='partialContent'` with `partialContentMetadata` for targeted file diffs

**⚠️ Gotchas:**

- `prNumber` **ignores ALL other filters** when set
- Use `type=metadata` first (fast), then `partialContent` for details
- Avoid `fullContent` on large PRs (token expensive)
- Set `withComments=true` to understand the reasoning behind changes

---

## Content Tools (Detailed)

### `githubGetFileContent`

**What it does:** Read file content from repositories with flexible extraction options, or fetch an entire directory to disk for local tool analysis.

**Key parameters:**

- `owner` (required): User or organization
- `repo` (required): Repository name
- `path` (required): File path or directory path in repository
- `branch`: Branch name (optional — auto-detects default)
- `type`: `"file"` (default) or `"directory"` — controls fetch mode
- `fullContent`: Read entire file (use sparingly, file mode only)
- `startLine`/`endLine`: Line range (1-indexed, file mode only)
- `matchString`: Find specific content with context (file mode only)
- `matchStringContextLines`: Lines around match (default: 5, max: 50, file mode only)
- `charOffset`/`charLength`: Character-based pagination (file mode only)

**Fetch modes (choose ONE via `type`):**

| Mode               | `type`        | What it does                               | Returns                     |
| ------------------ | ------------- | ------------------------------------------ | --------------------------- |
| **File** (default) | `"file"`      | Read a single file with extraction options | Inline content              |
| **Directory**      | `"directory"` | Fetch all files in a directory to disk     | `localPath` for local tools |

**File mode extraction (choose ONE):**

1. `matchString` with context lines
2. `startLine` + `endLine`
3. `fullContent=true` (small configs only)

**Directory mode details:**

- **Requires `ENABLE_LOCAL=true` and `ENABLE_CLONE=true`** (same as `githubCloneRepo`)
- Fetches all files via GitHub Contents API + `download_url` (no git required)
- Saves to `~/.octocode/repos/{owner}/{repo}/{branch}/{path}/` (same cache as clone tool)
- **Branch resolution**: If `branch` is omitted, auto-detects the default branch via GitHub API (same as `githubCloneRepo`)
- The resolved branch name is always included in the result (`branch` field) and the cache path
- 24-hour cache TTL (same as `githubCloneRepo`)
- Returns `localPath` — use with `localSearchCode`, `localGetFileContent`, `localViewStructure`, etc.
- Limits: max 50 files, max 5MB total, max 300KB per file, skips binary files
- Shares cache with clone tool — fetching multiple directories builds up the same repo cache

**Example queries:**

```
# Read specific function (file mode, default)
owner="vercel", repo="next.js", path="packages/next/src/server/app-render.tsx",
matchString="export function handleAuth", matchStringContextLines=20

# Read file header
owner="facebook", repo="react", path="packages/react/index.js",
startLine=1, endLine=50

# Fetch entire directory to disk (directory mode)
# Requires ENABLE_LOCAL=true and ENABLE_CLONE=true
owner="facebook", repo="react", path="packages/react/src",
type="directory", branch="main"
# Returns localPath — use with localSearchCode, localGetFileContent, etc.

# Read entire config (small files only)
path="package.json", fullContent=true, owner="org", repo="repo"
```

**Response format (file mode):** Returns `owner`, `repo`, `path`, `branch`, `content`, and optional `pagination`, `matchLocations`, `isPartial`, `startLine`/`endLine`. Fields like `contentLength`, minification metadata, and `cached`/`expiresAt` are not included.

**Response format (directory mode):** Returns `localPath`, `owner`, `repo`, `branch`, `directoryPath`, `fileCount`, `totalSize`, `files`. Internal cache fields (`cached`, `expiresAt`) are not included. Clone-cache is never overwritten by directory fetch — if a git clone already exists, it is used as-is.

**⚠️ Gotchas:**

- Choose ONE mode: `matchString` OR `startLine/endLine` OR `fullContent` (file mode only)
- When `type="directory"`: `startLine`, `endLine`, `matchString`, `charOffset`, `charLength` are rejected
- **Directory mode requires `ENABLE_LOCAL=true` and `ENABLE_CLONE=true`**
- Max file size: 300KB (FILE_TOO_LARGE error)
- Directory mode: max 50 files, max 5MB total, skips binary files
- For `branch`: Use NAME (e.g., `main`), not SHA
- Prefer `matchString` for large files (token efficient)
- Directory mode shares cache with `githubCloneRepo` — if a clone exists, it's reused (clone content is never downgraded)

---

### `githubViewRepoStructure`

**What it does:** Display the directory tree structure of a repository.

**Key parameters:**

- `owner` (required): User or organization
- `repo` (required): Repository name
- `branch`: Branch name (optional — auto-detects default)
- `path`: Starting path (default: root `""`)
- `depth`: Traversal depth (1-2, default: 1)
- `entriesPerPage`: Entries per page (default: 50, max: 200)
- `entryPageNumber`: Page number (default: 1)

**Exploration workflow:**

1. Start at root: `path=""`, `depth=1`
2. Drill into source: `path="src"`, `depth=2`
3. Explore specific area: `path="packages/core"`, `depth=1`

**Example queries:**

```
# See root structure
owner="vercel", repo="next.js", branch="canary", path="", depth=1

# Drill into source directory
owner="facebook", repo="react", branch="main", path="packages", depth=2
```

**⚠️ Gotchas:**

- Start at root (`path=""`, `depth=1`) first
- `depth=2` is slow on large directories — use on subdirs only
- For monorepos: Check `packages/`, `apps/`, `libs/`
- Max 200 entries per page — check `summary.truncated`
- Noisy directories auto-filtered: `.git`, `node_modules`, `dist`

---

### `githubCloneRepo`

**What it does:** Clone or partially fetch a GitHub repository to the local filesystem for deep analysis with local and LSP tools.

> Requires `ENABLE_LOCAL=true` and `ENABLE_CLONE=true`.

| Feature           | Description                                                     |
| ----------------- | --------------------------------------------------------------- |
| **Full clone**    | Shallow `git clone --depth 1` of the entire repo                |
| **Partial fetch** | Sparse checkout of a single subdirectory (fast for monorepos)   |
| **Caching**       | Cloned repos cached for **24 hours** under `~/.octocode/repos/` |
| **After clone**   | Returns `localPath` — pass it to local + LSP tools              |

**Key parameters:**

- `owner` (required): User or organization (max 200 chars)
- `repo` (required): Repository name (max 150 chars)
- `branch`: Branch to clone (max 255 chars). Omit to auto-detect the repo's default branch via the GitHub API (falls back to `main`)
- `sparse_path`: Fetch only this subdirectory via sparse checkout (max 500 chars). Dramatically faster for large monorepos. Examples: `src/compiler`, `packages/core/src`

**Two modes:**

| Mode              | When to use                                         | Parameter                        |
| ----------------- | --------------------------------------------------- | -------------------------------- |
| **Full clone**    | General exploration, LSP needs full project context | _(default — omit `sparse_path`)_ |
| **Partial fetch** | Large monorepo, only need a specific package/dir    | `sparse_path="packages/core"`    |

**Branch resolution:**

- If `branch` is provided, that specific branch is cloned
- If `branch` is **omitted**, the default branch is **auto-detected** via the GitHub API (falls back to `main`)
- The resolved branch name is **always included** in the result (`branch` field) and the cache path

**Cache path format:** `~/.octocode/repos/{owner}/{repo}/{branch}/` (branch is always present)

**Example queries:**

```
# Clone entire repo (branch auto-detected)
owner="vercel", repo="next.js"
# → localPath: ~/.octocode/repos/vercel/next.js/canary (auto-detected default branch)

# Clone specific branch
owner="facebook", repo="react", branch="main"
# → localPath: ~/.octocode/repos/facebook/react/main

# Sparse checkout — only fetch one directory (fast!)
owner="microsoft", repo="TypeScript", sparse_path="src/compiler"
# → localPath: ~/.octocode/repos/microsoft/TypeScript/main__sp_a3f8c1
```

**After cloning, use these tools on the returned `localPath`:**

- `localSearchCode` — search code with ripgrep
- `localGetFileContent` — read file content
- `localViewStructure` — browse the directory tree
- `localFindFiles` — find files by name/metadata
- `lspGotoDefinition` — jump to symbol definitions (semantic)
- `lspFindReferences` — find all usages of a symbol
- `lspCallHierarchy` — trace call chains (incoming/outgoing)

**Response format:** Returns `owner`, `repo`, `branch`, `localPath`, and optionally `sparse_path`. Internal fields like `cached` and `expiresAt` are not included in the response.

**⚠️ Gotchas:**

- Requires `ENABLE_LOCAL=true` and `ENABLE_CLONE=true` (both must be enabled)
- Clone timeout: **2 minutes** for full clone, **30 seconds** for sparse checkout
- Use `sparse_path` for large monorepos to avoid downloading unnecessary files
- Cached clones are reused within 24 hours (idempotent)
- Branch is **auto-detected** when omitted — the resolved branch is always in the result
- Cache path includes branch: `{owner}/{repo}/{branch}/` — different branches get separate caches
- Auth token is passed via `http.extraHeader` — never persisted in the clone URL or on-disk git config

---

## Package Tools (Detailed)

### `packageSearch`

**What it does:** Lookup NPM or Python packages to find their source repositories.

| Feature            | Description                              |
| ------------------ | ---------------------------------------- |
| **Ecosystems**     | NPM (npm) and Python (PyPI)              |
| **Repository URL** | Get owner/repo for further exploration   |
| **Metadata**       | Version, description, deprecation status |
| **Alternatives**   | Search for similar packages              |

**Key parameters:**

- `name` (required): Package name
- `ecosystem` (required): `npm` or `python`
- `searchLimit`: Number of results (default: 1, max: 10)
- `npmFetchMetadata`: Fetch extended NPM metadata
- `pythonFetchMetadata`: Fetch extended PyPI metadata

**Example queries:**

```
# Quick lookup - get repo URL
ecosystem="npm", name="express"

# Python package
ecosystem="python", name="requests"

# Find alternatives
ecosystem="npm", name="lodash", searchLimit=5
```

**⚠️ Gotchas:**

- Use `searchLimit=1` for known package names
- Python always returns 1 result (PyPI limitation)
- NPM uses dashes (`my-package`), Python uses underscores (`my_package`)
- Check DEPRECATED warnings first before using

**vs GitHub Search:**

- `packageSearch`: Fast lookup by exact name → get repo URL
- `githubSearchRepositories`: Broad discovery by keywords

**Use `packageSearch` first** for known package names, then `github*` tools for source exploration.

---

## Research Flows

### Flow 1: "How does package X work?"

```
packageSearch → githubViewRepoStructure → githubSearchCode → githubGetFileContent
```

**Steps:**

1. `packageSearch(name="express", ecosystem="npm")` → Get repo URL
2. `githubViewRepoStructure(owner="expressjs", repo="express", depth=1)` → See structure
3. `githubSearchCode(owner="expressjs", repo="express", keywordsToSearch=["middleware"])` → Find code
4. `githubGetFileContent(matchString="function middleware")` → Read implementation

---

### Flow 2: "Find examples of pattern X"

```
githubSearchCode → githubViewRepoStructure → githubGetFileContent
```

**Steps:**

1. `githubSearchCode(keywordsToSearch=["useReducer", "context"], extension="tsx")` → Find files
2. `githubViewRepoStructure` on interesting repos → Understand layout
3. `githubGetFileContent(matchString="useReducer")` → Read full implementation

---

### Flow 3: "Why was code changed this way?"

```
githubSearchCode → githubSearchPullRequests → githubGetFileContent
```

**Steps:**

1. `githubSearchCode(owner="org", repo="app", keywordsToSearch=["deprecatedFunc"])` → Find code
2. `githubSearchPullRequests(owner="org", repo="app", query="deprecatedFunc", merged=true)` → Find PRs
3. `githubSearchPullRequests(prNumber=123, type="partialContent", withComments=true)` → Get details

---

### Flow 4: "Explore a new codebase"

```
githubViewRepoStructure → githubGetFileContent → githubSearchCode
```

**Steps:**

1. `githubViewRepoStructure(path="", depth=1)` → Root overview
2. `githubGetFileContent(path="README.md", fullContent=true)` → Read docs
3. `githubGetFileContent(path="package.json", fullContent=true)` → Check deps
4. `githubViewRepoStructure(path="src", depth=2)` → Explore source
5. `githubSearchCode(keywordsToSearch=["export"])` → Find entry points

---

### Flow 5: "Find how others implement X"

```
githubSearchRepositories → githubViewRepoStructure → githubSearchCode → githubGetFileContent
```

**Steps:**

1. `githubSearchRepositories(topicsToSearch=["authentication"], stars=">500")` → Find projects
2. `githubViewRepoStructure` on top results → Browse structure
3. `githubSearchCode(keywordsToSearch=["oauth", "token"])` → Find implementations
4. `githubGetFileContent(matchString="async function authenticate")` → Read code

---

### Flow 6: "Deep analysis of external repo with LSP"

```
githubCloneRepo → localSearchCode → lspGotoDefinition → lspCallHierarchy
```

**Steps:**

1. `githubCloneRepo(owner="vercel", repo="next.js", sparse_path="packages/next/src")` → Get `localPath`
2. `localSearchCode(pattern="handleRequest", path=localPath)` → Find code + get `lineHint`
3. `lspGotoDefinition(uri="file.ts", symbolName="handleRequest", lineHint=42)` → Jump to definition
4. `lspCallHierarchy(direction="incoming")` → Trace callers

**When to use:** You need semantic analysis (LSP) on an external codebase — definitions, references, call hierarchy. Clone it first, then use local + LSP tools.

---

## Quick Reference

### Tool Selection Guide

| Question                              | Tool                       |
| ------------------------------------- | -------------------------- |
| "Search code patterns"                | `githubSearchCode`         |
| "Find repositories about X"           | `githubSearchRepositories` |
| "Find PRs that changed X"             | `githubSearchPullRequests` |
| "Read file from repo"                 | `githubGetFileContent`     |
| "Browse repo directory tree"          | `githubViewRepoStructure`  |
| "Clone repo for local + LSP analysis" | `githubCloneRepo`          |
| "Get repo URL for package X"          | `packageSearch`            |

### GitHub vs Local Tools

| Scenario                           | Use                                       |
| ---------------------------------- | ----------------------------------------- |
| Your codebase (files on disk)      | **Local tools** + LSP                     |
| External repos — quick browse/read | **GitHub tools**                          |
| External repos — deep LSP analysis | `githubCloneRepo` → **Local tools** + LSP |
| Found import, need source?         | `packageSearch` → GitHub tools            |

**⚠️ Local code questions → NEVER use `github*` tools. Use `localSearchCode` → LSP.**

---

## Critical Rules

### ⚠️ Rule 1: Know Your Scope

```
❌ WRONG: githubSearchCode for your own project files
✅ RIGHT: localSearchCode → LSP tools for local files
✅ RIGHT: githubSearchCode for external repositories
```

### ⚠️ Rule 2: Package First for External Deps

```
❌ WRONG: githubSearchRepositories(keywordsToSearch=["express"])
✅ RIGHT: packageSearch(name="express") → githubViewRepoStructure
```

`packageSearch` gives you exact repo URL; search gives broad results.

### ⚠️ Rule 3: Start Lean with Filters

```
❌ WRONG: extension="ts" + filename="config" + path="src"
✅ RIGHT: keywordsToSearch=["config"] + extension="ts"
```

Search APIs fail with too many combined filters.

### ⚠️ Rule 4: Metadata First for PRs

```
❌ WRONG: prNumber=123, type="fullContent"
✅ RIGHT: prNumber=123, type="metadata" → then partialContent if needed
```

Avoid token-expensive operations until you know what you need.

### ⚠️ Rule 5: Prefer `matchString` for Large Files

```
❌ WRONG: githubGetFileContent(fullContent=true) on 10,000 line file
✅ RIGHT: githubGetFileContent(matchString="function authenticate")
```

---

## Response Optimization

All GitHub tool responses are optimized for LLM token efficiency:

- **Per-result hints**: Hints are included inline within each query result (not aggregated at the top level)
- **Compact instructions**: Bulk response instructions are a single short line (e.g., `"3 results: 2 data, 1 empty."`)
- **No redundant metadata**: Fields like `contentLength`, `cached`, `expiresAt`, minification metadata, `searchEngine` are not included in responses
- **PR body auto-truncation**: PR bodies are truncated based on batch size (`limit`) — unlimited for single PR, 2000 chars for 2-3, 800 chars for 4+
- **Slimmed PR info**: Assignees are returned as `string[]` (login names only); `milestone`, `review_comments`, `id`, `html_url` are removed
- **Conditional fields**: `head_sha` and `base_sha` are only included when present (not empty strings)
- **Code search context**: `repositoryContext` contains only `branch` (not redundant `owner`/`repo`)

---

## Anti-Patterns to Avoid

| Anti-Pattern                                      | Why It's Wrong                  | Correct Approach                                          |
| ------------------------------------------------- | ------------------------------- | --------------------------------------------------------- |
| Using GitHub tools for local code                 | Slower, less semantic           | Use local + LSP tools                                     |
| Searching GitHub for known packages               | Broad results                   | `packageSearch` first                                     |
| Too many filters in code search                   | API fails                       | Start with 1-2 filters                                    |
| `fullContent=true` on large files                 | Token waste                     | Use `matchString`                                         |
| `type=fullContent` for PRs                        | Token expensive                 | `metadata` → `partialContent`                             |
| Ignoring `packageSearch`                          | Miss exact repo URL             | Always check for packages                                 |
| Cloning repo just to read one file                | Slow, wastes disk               | Use `githubGetFileContent` instead                        |
| Cloning without `sparse_path` on monorepo         | Downloads everything            | Set `sparse_path` to the dir you need                     |

---

## Parallel Execution

Tools with no dependencies can run in parallel:

```
✅ Parallel OK:
- githubSearchCode(owner="A") + githubSearchCode(owner="B")
- githubViewRepoStructure(repo="A") + githubViewRepoStructure(repo="B")
- packageSearch(name="express") + packageSearch(name="lodash")

❌ Must be Sequential:
- packageSearch → githubViewRepoStructure (needs owner/repo)
- githubViewRepoStructure → githubGetFileContent (needs path discovery)
- githubSearchPullRequests(metadata) → githubSearchPullRequests(partialContent)
- githubCloneRepo → localSearchCode / lspGotoDefinition (needs localPath)
```

**Batch limits:**

- All GitHub tools: Up to **3 queries** per call
- Package search: Up to **3 queries** per call
