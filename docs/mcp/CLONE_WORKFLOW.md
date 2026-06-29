# Clone & Local Tools Workflow

> How to use `ghCloneRepo` and `ghGetFileContent` (directory mode) to bridge GitHub repositories with local + LSP tools for deep code analysis.

> **Prerequisites:** Requires `ENABLE_CLONE=true` and local tools not explicitly disabled.

---

## The Bridge: GitHub → Clone/Fetch → Local + LSP

Octocode MCP has two worlds of tools:

| World | Tools | Strengths | Limitations |
|-------|-------|-----------|-------------|
| **GitHub** | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure` | Fast, no disk usage, works on any repo | No LSP, no semantic analysis, API rate limits |
| **Local + LSP** | `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent`, `lspGetSemantics` | Semantic navigation, call tracing, full ripgrep power | Only works on files on disk |

**Two tools bridge these worlds** — they download content to `<octocode-home>/tmp/` so local and LSP tools can analyze it:

| Bridge Tool | When to Use | How it Works |
|-------------|-------------|--------------|
| **`ghCloneRepo`** | Full repo or sparse subtree | Uses `git clone` into `tmp/clone` (requires git) |
| **`ghGetFileContent`** (type: `"directory"`) | Single directory of files | Uses GitHub API + `download_url` into `tmp/tree` (no git needed) |

Clones and API-fetched trees use separate tmp buckets with the same 24-hour TTL policy: `<octocode-home>/tmp/clone/{owner}/{repo}/{branch}/` for git clones, and `<octocode-home>/tmp/tree/{owner}/{repo}/{branch}/` for file/tree materialization.

**Branch resolution:** Both tools auto-detect the repository's default branch via the GitHub API when no `branch` is specified (falls back to `main`). The resolved branch name is always included in the result and the cache path.

```
┌─────────────────────┐       ┌────────────────────────────┐       ┌──────────────────────────┐
│  GitHub (remote)     │       │  Bridge Tools              │       │  Local + LSP (on disk)   │
│                      │       │                            │       │                          │
│  ghSearchCode    │──────▶│  ghCloneRepo           │──────▶│  localSearchCode         │
│  githubViewStructure │       │  (full/sparse clone)       │       │  localViewStructure      │
│  ghGetFileContent│       │                            │       │  localGetFileContent     │
│                      │       │  ghGetFileContent      │       │  localFindFiles          │
│                      │       │  (type: "directory")       │       │  lspGetSemantics   │
│                      │       │  (lightweight, no git)     │       │                          │
│                      │       │  Both return localPath     │       │                          │
│                      │       │  + next (localSearch,      │       │                          │
│                      │       │    viewStructure) + location│       │                          │
└─────────────────────┘       └────────────────────────────┘       └──────────────────────────┘
```

---

## When to Clone vs Directory Fetch

| Scenario | Use GitHub Tools | Use Directory Fetch | Use Clone |
|----------|-----------------|--------------------|----|
| Quick file read | ✅ `ghGetFileContent` | Overkill | Overkill |
| Browse repo tree | ✅ `ghViewRepoStructure` | Overkill | Overkill |
| Find code pattern across repos | ✅ `ghSearchCode` | Overkill | Overkill |
| **Read all files in a directory** | ❌ One-by-one | ✅ `type: "directory"` | Overkill |
| **Search within a directory** | Limited | ✅ Directory fetch → `localSearchCode` | Also works |
| **Trace function call chains** | ❌ Not possible | ❌ Partial context | ✅ Clone → `lspGetSemantics(type="callers")` / `type="callees"` |
| **Jump to symbol definitions** | ❌ Not possible | ❌ Partial context | ✅ Clone → `lspGetSemantics(type="definition")` |
| **Find all usages of a type** | ❌ Not possible | ❌ Partial context | ✅ Clone → `lspGetSemantics(type="references")` |
| **Deep code search with regex** | Limited | ✅ If scope is small | ✅ Clone → `localSearchCode` |
| **Explore monorepo subtree** | Slow (many API calls) | ✅ For small dirs | ✅ Sparse clone for large dirs |

**Rule of thumb:**
- Need a **single directory**? → `ghGetFileContent` with `type: "directory"` (no git required)
- Need **semantic analysis** (definitions, references, call hierarchy)? → `ghCloneRepo` first
- Need a **large subtree or full project context**? → `ghCloneRepo` with `sparsePath`

---

## Two Clone Modes

### Mode 1: Full Clone

Best for general exploration where you need full project context (LSP works best with full repos).

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
    tool: localSearchCode
    query:
      path: <octocode-home>/tmp/clone/vercel/next.js/main
      mode: discovery
  viewStructure:
    tool: localViewStructure
    query:
      path: <octocode-home>/tmp/clone/vercel/next.js/main
```

Pass `next.localSearch.query` or `next.viewStructure.query` directly to the respective tool — the `path` is always absolute.

### Mode 2: Sparse (Folder) Fetch

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
    tool: localSearchCode
    query:
      path: <octocode-home>/tmp/clone/microsoft/TypeScript/main__sp_a3f8c1
      mode: discovery
  viewStructure:
    tool: localViewStructure
    query:
      path: <octocode-home>/tmp/clone/microsoft/TypeScript/main__sp_a3f8c1
```

> **Note:** LSP may have limited cross-file resolution in sparse checkouts since not all source files are present. If you need full project context, clone without `sparsePath`.

### Mode 3: Directory Fetch (ghGetFileContent type:"directory")

Lightweight alternative — no git required. Downloads individual files via the GitHub Contents API into `tmp/tree`.

```
ghGetFileContent:
  owner: "vercel"
  repo: "next.js"
  path: "packages/next/src/server"
  type: "directory"
```

**Result:**
```yaml
localPath: <octocode-home>/tmp/tree/vercel/next.js/main/packages/next/src/server
repoRoot: <octocode-home>/tmp/tree/vercel/next.js/main
fileCount: 12
complete: true
location:
  kind: directory
  localPath: <octocode-home>/tmp/tree/vercel/next.js/main/packages/next/src/server
  repoRoot: <octocode-home>/tmp/tree/vercel/next.js/main
  source: treeFetch
  cached: false
  complete: true
next:
  localSearch:
    tool: localSearchCode
    query:
      path: <octocode-home>/tmp/tree/vercel/next.js/main/packages/next/src/server
      mode: discovery
  viewStructure:
    tool: localViewStructure
    query:
      path: <octocode-home>/tmp/tree/vercel/next.js/main/packages/next/src/server
```

> **Note:** `complete: false` means some files were skipped (binary, oversized, or file-limit). Use `ghCloneRepo` when completeness matters.

---

## Step-by-Step Workflows

### Workflow 1: Browse a Cloned Repository Tree

**Goal:** Understand the structure of an external repo using local tools.

```
Step 1: Clone the repo
  ghCloneRepo(owner="facebook", repo="react")
  → localPath = "<octocode-home>/tmp/clone/facebook/react/main"

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
  ghCloneRepo(owner="vercel", repo="next.js", sparsePath="packages/next/src")
  → localPath

Step 2: Search for the function
  localSearchCode(path=localPath, pattern="handleRequest")
  → Get file paths and lineHint values

Step 3: Jump to definition
  lspGetSemantics(type="definition", uri=localPath+"/server/router.ts", symbolName="handleRequest", lineHint=42)
  → See the function definition

Step 4: Trace callers
  lspGetSemantics(type="callers", uri=..., symbolName="handleRequest", lineHint=42)
  → See all functions that call handleRequest
```

### Workflow 3: From GitHub Browsing to Deep Local Analysis

**Goal:** You're browsing a repo on GitHub and want to go deeper.

```
Step 1: Browse on GitHub first (quick)
  ghViewRepoStructure(owner="pallets", repo="flask", maxDepth=2)
  → See the tree, find interesting directory "src/flask"

Step 2: Clone for deep analysis
  ghCloneRepo(owner="pallets", repo="flask")
  → localPath

Step 3: Use full ripgrep power
  localSearchCode(path=localPath, pattern="def route\\(", type="py")
  → Full regex search, file type filtering, match context

Step 4: Use LSP
  lspGetSemantics(type="references", uri=localPath+"/src/flask/app.py", symbolName="route", lineHint=...)
  → Find every file that uses the @route decorator
```

### Workflow 4: Sparse Fetch of a Monorepo Package

**Goal:** Analyze one package in a large monorepo without cloning the entire thing.

```
Step 1: Browse the monorepo structure on GitHub (quick discovery)
  ghViewRepoStructure(owner="microsoft", repo="TypeScript", path="src", maxDepth=1)
  → See packages: compiler, services, harness, ...

Step 2: Clone only the compiler
  ghCloneRepo(owner="microsoft", repo="TypeScript", sparsePath="src/compiler")
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
| **Location** | clones: `<octocode-home>/tmp/clone/{owner}/{repo}/{branch}/`; file/tree fetches: `<octocode-home>/tmp/tree/{owner}/{repo}/{branch}/` |
| **Branch** | Auto-detected via GitHub API when omitted; resolved branch always included in path and result |
| **Sparse clones** | Separate cache: `{branch}__sp_{hash}/` |
| **Coexistence** | Full clone and sparse clones of the same repo can coexist |
| **Cache hit** | Returns instantly (no network call) |
| **Clone vs directory** | Clone-cache and directory/file materialization are separate; directory fetch never overwrites a git clone |
| **Expired** | Automatically evicted by periodic GC (every 10 min) and on next request |
| **Force refresh** | Set `forceRefresh: true` in the query to bypass cache and re-clone/re-fetch |
| **Periodic GC** | Expired clone/tree materializations are cleaned up every 10 minutes (runs on server startup and periodically) |
| **Manual clear** | Delete the `localPath` directory to force re-clone |

---

## Path Validation: Why It Works

Local tools validate all paths against allowed roots. Cloned repos are accessible because:

1. **Tmp destination**: `<octocode-home>/tmp/...` is under the Octocode home directory
2. **PathValidator & ExecutionContextValidator**: Both automatically add Octocode home as an allowed root alongside the workspace directory
3. **Workspace root resolution**: Local tools validate paths against allowed roots, and LSP tools automatically choose project context from the target file path. If a cloned file is inside `WORKSPACE_ROOT`, Octocode keeps that root; otherwise it walks up from the file to the nearest project marker (`package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, `pyproject.toml`, etc.)
4. **Result**: Any `localPath` returned by `ghCloneRepo` or `ghGetFileContent` (directory mode) is automatically valid for all local + LSP tools, even when the cloned repo lives outside your current shell workspace

For MCP, set `ENABLE_CLONE=true` and leave local tools enabled. The CLI defaults both local and clone support on unless explicitly disabled.

For TypeScript/JavaScript LSP:

- Octocode tries its bundled `typescript-language-server` first.
- If that bundled server is not available in your environment, install `typescript-language-server` + `typescript` on `PATH`, or set `OCTOCODE_TS_SERVER_PATH`.
- LSP can analyze bundled/minified `.js` files, but semantic quality is usually much better on original source trees than on large generated artifacts.

---

## Quick Reference

| Action | Tool | Key Parameter |
|--------|------|---------------|
| Clone entire repo | `ghCloneRepo` | `owner`, `repo` (branch auto-detected) |
| Clone specific branch | `ghCloneRepo` | `owner`, `repo`, `branch` |
| Clone one folder | `ghCloneRepo` | `owner`, `repo`, `sparsePath` (branch auto-detected) |
| Force re-clone | `ghCloneRepo` | `forceRefresh: true` (bypasses valid cache) |
| Browse cloned tree | `localViewStructure` | `path` = `localPath` |
| Search cloned code | `localSearchCode` | `path` = `localPath` |
| Read cloned file | `localGetFileContent` | `path` = `localPath + "/file.ts"` |
| Find files in clone | `localFindFiles` | `path` = `localPath` |
| Jump to definition | `lspGetSemantics` with `type="definition"` | `uri` = file in `localPath` |
| Find all references | `lspGetSemantics` with `type="references"` | `uri` = file in `localPath` |
| Trace callers/callees | `lspGetSemantics` with `type="callers"` or `type="callees"` | `uri` = file in `localPath` |

---

## Related Documentation

- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md) — Full `ghCloneRepo` parameter reference
- [Local Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md) — Local filesystem search, structure, metadata, and content tools
- [LSP Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md) — Semantic content tool
- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md) — `ENABLE_LOCAL`, `ENABLE_CLONE`, and other settings
