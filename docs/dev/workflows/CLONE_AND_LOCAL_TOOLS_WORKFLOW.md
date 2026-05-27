# Clone & Local Tools Workflow

> How to use `githubCloneRepo` and `githubGetFileContent` (directory mode) to bridge GitHub repositories with local + LSP tools for deep code analysis.

> **Prerequisites:** Requires `ENABLE_LOCAL=true` and `ENABLE_CLONE=true`.

---

## The Bridge: GitHub → Clone/Fetch → Local + LSP

Octocode MCP has two worlds of tools:

| World | Tools | Strengths | Limitations |
|-------|-------|-----------|-------------|
| **GitHub** | `githubSearchCode`, `githubGetFileContent`, `githubViewRepoStructure` | Fast, no disk usage, works on any repo | No LSP, no semantic analysis, API rate limits |
| **Local + LSP** | `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent`, `lspGotoDefinition`, `lspFindReferences`, `lspCallHierarchy` | Semantic navigation, call tracing, full ripgrep power | Only works on files on disk |

**Two tools bridge these worlds** — they download content to `~/.octocode/repos/` so local and LSP tools can analyze it:

| Bridge Tool | When to Use | How it Works |
|-------------|-------------|--------------|
| **`githubCloneRepo`** | Full repo or sparse subtree | Uses `git clone` (requires git) |
| **`githubGetFileContent`** (type: `"directory"`) | Single directory of files | Uses GitHub API + `download_url` (no git needed) |

Both share the **same cache** (`~/.octocode/repos/{owner}/{repo}/{branch}/`) with 24-hour TTL. Fetching a directory and then cloning the same repo reuses the cache location.

**Branch resolution:** Both tools auto-detect the repository's default branch via the GitHub API when no `branch` is specified (falls back to `main`). The resolved branch name is always included in the result and the cache path.

```
┌─────────────────────┐       ┌────────────────────────────┐       ┌──────────────────────────┐
│  GitHub (remote)     │       │  Bridge Tools              │       │  Local + LSP (on disk)   │
│                      │       │                            │       │                          │
│  githubSearchCode    │──────▶│  githubCloneRepo           │──────▶│  localSearchCode         │
│  githubViewStructure │       │  (full/sparse clone)       │       │  localViewStructure      │
│  githubGetFileContent│       │                            │       │  localGetFileContent     │
│                      │       │  githubGetFileContent      │       │  localFindFiles          │
│                      │       │  (type: "directory")       │       │  lspGotoDefinition       │
│                      │       │  (lightweight, no git)     │       │  lspFindReferences       │
│                      │       │                            │       │  lspCallHierarchy        │
│                      │       │  Both return: localPath    │       │                          │
└─────────────────────┘       └────────────────────────────┘       └──────────────────────────┘
```

---

## When to Clone vs Directory Fetch

| Scenario | Use GitHub Tools | Use Directory Fetch | Use Clone |
|----------|-----------------|--------------------|----|
| Quick file read | ✅ `githubGetFileContent` | Overkill | Overkill |
| Browse repo tree | ✅ `githubViewRepoStructure` | Overkill | Overkill |
| Find code pattern across repos | ✅ `githubSearchCode` | Overkill | Overkill |
| **Read all files in a directory** | ❌ One-by-one | ✅ `type: "directory"` | Overkill |
| **Search within a directory** | Limited | ✅ Directory fetch → `localSearchCode` | Also works |
| **Trace function call chains** | ❌ Not possible | ❌ Partial context | ✅ Clone → `lspCallHierarchy` |
| **Jump to symbol definitions** | ❌ Not possible | ❌ Partial context | ✅ Clone → `lspGotoDefinition` |
| **Find all usages of a type** | ❌ Not possible | ❌ Partial context | ✅ Clone → `lspFindReferences` |
| **Deep code search with regex** | Limited | ✅ If scope is small | ✅ Clone → `localSearchCode` |
| **Explore monorepo subtree** | Slow (many API calls) | ✅ For small dirs | ✅ Sparse clone for large dirs |

**Rule of thumb:**
- Need a **single directory**? → `githubGetFileContent` with `type: "directory"` (no git required)
- Need **semantic analysis** (definitions, references, call hierarchy)? → `githubCloneRepo` first
- Need a **large subtree or full project context**? → `githubCloneRepo` with `sparse_path`

---

## Two Clone Modes

### Mode 1: Full Clone

Best for general exploration where you need full project context (LSP works best with full repos).

```
githubCloneRepo:
  owner: "vercel"
  repo: "next.js"
  # branch omitted → auto-detects default branch
```

**Result:**
```yaml
owner: vercel
repo: next.js
branch: main
localPath: ~/.octocode/repos/vercel/next.js/main
```

### Mode 2: Sparse (Folder) Fetch

Best for large monorepos where you only need one package/directory. Dramatically faster.

```
githubCloneRepo:
  owner: "microsoft"
  repo: "TypeScript"
  sparse_path: "src/compiler"
```

**Result:**
```yaml
owner: microsoft
repo: TypeScript
branch: main
localPath: ~/.octocode/repos/microsoft/TypeScript/main__sp_a3f8c1
sparse_path: "src/compiler"
```

> **Note:** LSP may have limited cross-file resolution in sparse checkouts since not all source files are present. If you need full project context, clone without `sparse_path`.

---

## Step-by-Step Workflows

### Workflow 1: Browse a Cloned Repository Tree

**Goal:** Understand the structure of an external repo using local tools.

```
Step 1: Clone the repo
  githubCloneRepo(owner="facebook", repo="react")
  → localPath = "~/.octocode/repos/facebook/react/main"

Step 2: Browse the tree
  localViewStructure(path=localPath, depth=2)
  → See the full directory structure with file sizes and dates

Step 3: Drill into a directory
  localViewStructure(path=localPath + "/packages/react/src", depth=2)
  → See the subdirectory contents
```

### Workflow 2: Deep Code Analysis with LSP

**Goal:** Trace who calls a function in an external repo.

```
Step 1: Clone the repo
  githubCloneRepo(owner="vercel", repo="next.js", sparse_path="packages/next/src")
  → localPath

Step 2: Search for the function
  localSearchCode(path=localPath, pattern="handleRequest")
  → Get file paths and lineHint values

Step 3: Jump to definition
  lspGotoDefinition(uri=localPath+"/server/router.ts", symbolName="handleRequest", lineHint=42)
  → See the function definition

Step 4: Trace callers
  lspCallHierarchy(uri=..., symbolName="handleRequest", lineHint=42, direction="incoming")
  → See all functions that call handleRequest
```

### Workflow 3: From GitHub Browsing to Deep Local Analysis

**Goal:** You're browsing a repo on GitHub and want to go deeper.

```
Step 1: Browse on GitHub first (quick)
  githubViewRepoStructure(owner="pallets", repo="flask", depth=2)
  → See the tree, find interesting directory "src/flask"

Step 2: Clone for deep analysis
  githubCloneRepo(owner="pallets", repo="flask")
  → localPath

Step 3: Use full ripgrep power
  localSearchCode(path=localPath, pattern="def route\\(", type="py")
  → Full regex search, file type filtering, match context

Step 4: Use LSP
  lspFindReferences(uri=localPath+"/src/flask/app.py", symbolName="route", lineHint=...)
  → Find every file that uses the @route decorator
```

### Workflow 4: Sparse Fetch of a Monorepo Package

**Goal:** Analyze one package in a large monorepo without cloning the entire thing.

```
Step 1: Browse the monorepo structure on GitHub (quick discovery)
  githubViewRepoStructure(owner="microsoft", repo="TypeScript", path="src", depth=1)
  → See packages: compiler, services, harness, ...

Step 2: Clone only the compiler
  githubCloneRepo(owner="microsoft", repo="TypeScript", sparse_path="src/compiler")
  → localPath (only downloads src/compiler, much faster)

Step 3: Search within the fetched subtree
  localSearchCode(path=localPath, pattern="transformTypeScript")
  → Search only within the compiler code

Step 4: Find files by metadata
  localFindFiles(path=localPath, name="*.ts", modifiedWithin="30d")
  → Recently modified TypeScript files in the compiler
```

---

## Cache Behavior

| Behavior | Details |
|----------|---------|
| **TTL** | 24 hours by default (configurable via `OCTOCODE_CACHE_TTL_MS` env var) |
| **Location** | `~/.octocode/repos/{owner}/{repo}/{branch}/` |
| **Branch** | Auto-detected via GitHub API when omitted; resolved branch always included in path and result |
| **Sparse clones** | Separate cache: `{branch}__sp_{hash}/` |
| **Coexistence** | Full clone and sparse clones of the same repo can coexist |
| **Cache hit** | Returns instantly (no network call) |
| **Clone vs directory** | Clone-cache is never overwritten by directory fetch — if a git clone exists, directory fetch reuses it as-is |
| **Expired** | Automatically evicted by periodic GC (every 10 min) and on next request |
| **Force refresh** | Set `forceRefresh: true` in the query to bypass cache and re-clone/re-fetch |
| **Periodic GC** | Expired clones are cleaned up every 10 minutes (runs on server startup and periodically) |
| **Manual clear** | Delete the `localPath` directory to force re-clone |

---

## Path Validation: Why It Works

Local tools validate all paths against allowed roots. Cloned repos are accessible because:

1. **Clone destination**: `~/.octocode/repos/...` is under the octocode home directory
2. **PathValidator & ExecutionContextValidator**: Both automatically add `~/.octocode/` as an allowed root alongside the workspace directory
3. **Workspace root resolution**: Local tools validate paths against allowed roots, and LSP tools automatically choose project context from the target file path. If a cloned file is inside `WORKSPACE_ROOT`, Octocode keeps that root; otherwise it walks up from the file to the nearest project marker (`package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, `pyproject.toml`, etc.)
4. **Result**: Any `localPath` returned by `githubCloneRepo` or `githubGetFileContent` (directory mode) is automatically valid for all local + LSP tools, even when the cloned repo lives outside your current shell workspace

No extra configuration is needed beyond `ENABLE_LOCAL=true` (the default) and `ENABLE_CLONE=true`.

For TypeScript/JavaScript LSP:

- Octocode tries its bundled `typescript-language-server` first.
- If that bundled server is not available in your environment, install `typescript-language-server` + `typescript` on `PATH`, or set `OCTOCODE_TS_SERVER_PATH`.
- LSP can analyze bundled/minified `.js` files, but semantic quality is usually much better on original source trees than on large generated artifacts.

---

## Quick Reference

| Action | Tool | Key Parameter |
|--------|------|---------------|
| Clone entire repo | `githubCloneRepo` | `owner`, `repo` (branch auto-detected) |
| Clone specific branch | `githubCloneRepo` | `owner`, `repo`, `branch` |
| Clone one folder | `githubCloneRepo` | `owner`, `repo`, `sparse_path` (branch auto-detected) |
| Force re-clone | `githubCloneRepo` | `forceRefresh: true` (bypasses valid cache) |
| Browse cloned tree | `localViewStructure` | `path` = `localPath` |
| Search cloned code | `localSearchCode` | `path` = `localPath` |
| Read cloned file | `localGetFileContent` | `path` = `localPath + "/file.ts"` |
| Find files in clone | `localFindFiles` | `path` = `localPath` |
| Jump to definition | `lspGotoDefinition` | `uri` = file in `localPath` |
| Find all references | `lspFindReferences` | `uri` = file in `localPath` |
| Trace call chain | `lspCallHierarchy` | `uri` = file in `localPath` |

---

## Related Documentation

- [GitHub Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md) — Full `githubCloneRepo` parameter reference
- [Local & LSP Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md) — Full local + LSP tools documentation
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) — `ENABLE_LOCAL`, `ENABLE_CLONE`, and other settings
