# Octocode Architecture & Developer Skill

## What Is Octocode

Octocode is a code-research platform with **two interfaces** over the same tool implementations:

- **MCP server** (`packages/octocode-mcp`) — served via `StdioServerTransport`, registered in MCP clients (Claude, VS Code, etc.)
- **CLI** (`packages/octocode-cli`) — direct tool invocation from the terminal without an MCP client

Both interfaces call into **`packages/octocode-tools-core`** for all tool logic.

---

## Package Map

```
octocode-mcp/ (monorepo root)
├── packages/
│   ├── octocode-tools-core/    # All 13 tool implementations + execution (TypeScript)
│   ├── octocode-mcp/           # MCP server (thin wrapper over tools-core)
│   ├── octocode-cli/           # CLI wrapper (thin wrapper over tools-core)
│   ├── octocode-lsp/           # LSP client/server lifecycle (Rust + napi)
│   ├── octocode-context-utils/ # FS queries, ripgrep parsing, YAML (Rust + napi)
│   ├── octocode-security/      # Path validation, command allowlist, secrets (Rust + napi)
│   ├── octocode-shared/        # Credentials, sessions, platform detection
│   └── octocode-vscode/        # VS Code extension
│
octocode-mcp-host/ (SEPARATE REPO — tool metadata source)
└── packages/octocode-core/
    ├── src/resources/tools/    # Tool descriptions + schema field texts (ToolSpec)
    ├── src/schemas/            # Zod input schemas (canonical MCP contracts)
    └── src/resources/systemPrompt.ts  # MCP system prompt
```

**Critical**: `octocode-core` (from the `octocode-mcp-host` repo) is the **only** source for tool descriptions, schema field texts, and the system prompt. It is consumed by `octocode-tools-core` as a `file://` path dep during local dev.

---

## 13 Tools — Routing Guide

### GitHub Tools (remote, requires token)

| Tool | When to use |
|------|-------------|
| `ghSearchCode` | Find code snippets across GitHub by keywords, owner, repo, extension, language, path |
| `ghGetFileContent` | Read a specific file (or region) from a GitHub repo |
| `ghViewRepoStructure` | Browse a repo's directory tree |
| `ghCloneRepo` | Clone a repo/subtree to disk for local + LSP work (`ENABLE_CLONE=true` required) |
| `ghSearchRepos` | Discover repos by name, keywords, topic, language, stars |
| `ghHistoryResearch` | Unified PR + commit history research. LIST mode (no `prNumber`): PR search by keyword/author/state or commit log (`type:"commits"`) for file/repo/subtree. DETAIL mode (`prNumber` required): full PR — body, changedFiles, patches, comments, reviews, approvals. `reviewMode:"full"` fetches all surfaces in one call. Batch up to 5 queries per call. |

### Local Tools (filesystem, `ENABLE_LOCAL=true` by default)

| Tool | When to use |
|------|-------------|
| `localSearchCode` | ripgrep search — file+line, regex, modes: paginated/discovery/detailed; countLinesPerFile/countMatchesPerFile for count-only |
| `localGetFileContent` | Read a local file or region (matchString, startLine/endLine, charOffset pagination) |
| `localViewStructure` | Browse local directories |
| `localFindFiles` | Find files by name pattern, metadata, extension |

### Semantic / LSP

| Tool | When to use |
|------|-------------|
| `lspGetSemantics` | Typed semantic navigation: `definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`, `implementation` |

### Package

| Tool | When to use |
|------|-------------|
| `npmSearch` | npm package lookup with metadata and source-repo handoff |

### Routing decision tree

```
Is the target a local path / workspace?
  → local tools (localSearchCode → localGetFileContent → lspGetSemantics)

Is it a symbol you need to understand semantically?
  → localSearchCode first (get uri + lineHint) → lspGetSemantics

Is it an npm package?
  → npmSearch → ghViewRepoStructure → ghSearchCode

Is it GitHub code?
  → ghSearchRepos → ghViewRepoStructure → ghSearchCode → ghGetFileContent

Trace file/feature/repo history (who changed what, when, rename tracking)?
  → ghHistoryResearch(owner, repo, path, type:"commits")             — file commit log
  → ghHistoryResearch(owner, repo, path:"src/auth/", type:"commits") — dir subtree
  → ghHistoryResearch(owner, repo, type:"commits", includeDiff:true) — whole-repo with diffs
  Bulk: queries:[{type:"commits",path:"A",...},{type:"commits",path:"B",...}] — up to 5 paths per call
  → commits with (#NNN) in messageHeadline → ghHistoryResearch(owner, repo, prNumber:NNN) for full PR
  Note: there is NO separate ghHistory tool — ghHistoryResearch handles both PR and commit history.

Search or deep-dive PRs (any state, author, topic)?
  → ghHistoryResearch(owner, repo, keywordsToSearch:[...], state:"open"|"closed"|"merged")
  → ghHistoryResearch(owner, repo, author:"username")              — PRs by contributor
  → ghHistoryResearch(owner, repo, prNumber, reviewMode:"full")    — full PR in one call
  → ghHistoryResearch(owner, repo, prNumber, content:{patches:{mode:"selected",files:[...]}}) — targeted diffs

Need deep cross-package LSP analysis? (requires ENABLE_CLONE=true server config)
  → ghCloneRepo → localViewStructure(localPath) → localSearchCode → lspGetSemantics
  → fallback without clone: ghViewRepoStructure → ghSearchCode → ghGetFileContent
```

### LSP type routing

```
documentSymbols  → file outline (uri only, no symbolName needed)
hover            → signature + JSDoc
definition       → jump-to-declaration
typeDefinition   → generic type resolution
implementation   → abstract member impl (member name, not class)
references       → same-package usages (bounded by TS server open files)
callers          → cross-package incoming calls (TS/JS/Go/Rust only)
callees          → outgoing calls
callHierarchy    → both directions
```

---

## Call Structure

Every tool call uses a **bulk queries envelope**:

```json
{
  "queries": [
    {
      "mainResearchGoal": "Shared goal across all queries in this batch",
      "researchGoal": "What this specific query answers",
      "reasoning": "Why this query is needed",
      // ... tool-specific fields
    }
  ]
}
```

`mainResearchGoal`, `researchGoal`, and `reasoning` are **required** on the MCP wire. The CLI (`octocode tools`) auto-fills all three when omitted — only GitHub/Package tools require `mainResearchGoal` explicitly via CLI.

---

## Minify Decision Table

Pick by goal, not habit. Wrong default = 2–5× extra tokens.

| Goal | Tool | Use |
|------|------|-----|
| Orient on an unknown file | `localGetFileContent`, `ghGetFileContent` | `minify:"symbols"` — skeleton + line numbers, 55–97% smaller |
| Normal investigation | Any content read | `minify:"standard"` (default) — strips comments/blanks |
| Quote exact text / match whitespace / raw diff | Any content read | `minify:"none"` |
| PR review | `ghHistoryResearch` detail | `minify:"standard"` always; `minify:"none"` only for raw diff quoting |
| PR search — `"symbols"` | `ghHistoryResearch` | **Not available** — Zod error if attempted |
| Call tree navigation | `lspGetSemantics` callers/callees/callHierarchy | `format:"compact"` — ~50% fewer tokens (different flag name) |
| Repo discovery | `ghSearchRepos` | `verbose:false` (default) lean strings; `verbose:true` only when filtering on stars/topics/dates |
| File-existence check | `ghSearchCode` | `match:"path"` — ~10× cheaper than file content scan |

---

## Data Flow (MCP path)

```
MCP client call
  → octocode-mcp/src/index.ts (StdioServerTransport)
  → registerTool() → Security wrapper
  → octocode-tools-core bulk handler
  → tool execution.ts
  → ContentSanitizer → response envelope (YAML/JSON)
  → structuredContent back to client
```

---

## Local Development Workflow

### 1. Edit `octocode-core` (tool metadata / schemas)

`octocode-core` lives in the **separate** `octocode-mcp-host` repo:

```
/Users/guybary/Documents/octocode-mcp-host/packages/octocode-core/
  src/resources/tools/   # Edit ToolSpec descriptions here
  src/schemas/           # Edit Zod input schemas here
  src/resources/systemPrompt.ts
```

After editing, build it:

```bash
cd /Users/guybary/Documents/octocode-mcp-host/packages/octocode-core
yarn build
```

### 2. Wire the local build into `octocode-tools-core`

`packages/octocode-tools-core/package.json` must point to the local build:

```json
"@octocodeai/octocode-core": "file:///Users/guybary/Documents/octocode-mcp-host/packages/octocode-core"
```

Then sync and build:

```bash
cd /Users/guybary/Documents/octocode-mcp/packages/octocode-tools-core
yarn          # re-links the file: dep
yarn build    # compiles tools-core with local octocode-core
```

### 3. Propagate to MCP or CLI

After rebuilding `octocode-tools-core`, rebuild whichever interface you are testing:

```bash
# MCP
cd /Users/guybary/Documents/octocode-mcp/packages/octocode-mcp
yarn build

# CLI
cd /Users/guybary/Documents/octocode-mcp/packages/octocode-cli
yarn build
```

### 4. Test tools via CLI (fastest loop)

For workspace-internal testing, set all internal deps to `workspace:^` first (so they resolve from the monorepo, not npm), build all, then invoke directly:

```bash
# From monorepo root
yarn build   # builds all packages in dependency order

# Call a tool directly (no MCP client needed)
node /Users/guybary/Documents/octocode-mcp/packages/octocode-cli/out/octocode-cli.js \
  tools <tool-name> \
  --queries '[{"mainResearchGoal":"...","researchGoal":"...","reasoning":"...","<field>":"<value>"}]'
```

Example — test `localSearchCode`:

```bash
node packages/octocode-cli/out/octocode-cli.js tools localSearchCode \
  --queries '[{"mainResearchGoal":"find tool config","researchGoal":"locate toolConfig.ts","reasoning":"need entrypoint","keywords":"toolConfig","path":"/Users/guybary/Documents/octocode-mcp/packages"}]'
```

Note: the CLI command is `octocode tools <name>` (not `octocode <name>` directly). The `keywords` field for `localSearchCode` is a **string**, not an array — multi-word terms go in a single string.

---

## Native (Rust-accelerated) Packages

`octocode-tools-core` is pure TypeScript but consumes three native Rust packages at runtime:

| Package | Rust role |
|---------|-----------|
| `octocode-lsp` | LSP client/server lifecycle, symbol resolution, JSON-RPC |
| `octocode-context-utils` | File system queries (`queryFileSystem`), ripgrep output parsing, YAML serialisation |
| `octocode-security` | Path validation, command allowlist enforcement, secret redaction regexes |

`octocode-tools-core` lazy-loads `octocode-context-utils` via `createRequire` (see `src/utils/contextUtils.ts`). If the native `.node` binary is absent the module load fails with a clear `ContextUtilsLoadError`. All three are `workspace:^` dependencies.

---

## Environment Variables (key ones)

| Variable | Default | Notes |
|----------|---------|-------|
| `OCTOCODE_TOKEN` / `GH_TOKEN` / `GITHUB_TOKEN` | — | GitHub auth (priority: OCTOCODE > GH > GITHUB) |
| `ENABLE_LOCAL` | `true` | Enables local filesystem tools |
| `ENABLE_CLONE` | `false` | Enables `ghCloneRepo` + directory mode |
| `ALLOWED_PATHS` | `[]` (all) | Restrict local tools to comma-separated absolute paths |
| `OCTOCODE_OUTPUT_FORMAT` | `yaml` | `yaml` or `json` |

---

## Pagination Pattern

- Page only when response includes `hasMore: true` or `nextPage`
- Use `charOffset` + `charLength` for byte-level continuation on large files/PR bodies
- `localSearchCode` uses `matchPage` for per-file match pagination
- Narrow (add filters / keywords) before paging noisy results

---

## Evidence Gate

Every response includes an `evidence` object. Check it before issuing follow-up calls.

```yaml
evidence:
  answerReady: true   # → STOP — result is sufficient, do not issue follow-up calls
  complete: true      # → no more pages; false → paginate with charOffset/page
  confidence: high    # high | medium | low — calibrate trust
  kind: code          # content type
```

**Hard rules:**
- `answerReady:true` → stop. Do not confirm with a follow-up read.
- `complete:false` → paginate first (charOffset/page) before issuing a different query.
- Call `localGetFileContent` only when the search snippet is insufficient to answer the question, or when you need exact text. A snippet proving the symbol exists is proof — `minify:"none"` confirmation is not required.

**LSP prerequisites (hard — not soft):**
- `documentSymbols` → needs only `uri`
- All other types → need `uri` + `symbolName` + `lineHint` from a real `localSearchCode` match
- Never estimate `lineHint`. A wrong value returns empty results silently.

---

## Research Patterns

Chains are evidence-driven, not fixed sequences. Start from what you know, skip steps you don't need.

**Local — symbol investigation**
```
# If file path unknown:
localSearchCode(keywords, path, mode:"discovery")     → file list (cheap)
localSearchCode(keywords, path, include:[known files]) → targeted snippets
localGetFileContent(path, matchString)                 → read when snippet insufficient
lspGetSemantics(uri, symbolName, lineHint=match.line)  → semantic navigation

# If file path already known: skip directly to localGetFileContent or lspGetSemantics
```

**Local — temporal / metadata queries** (use localFindFiles, not localSearchCode)
```
localFindFiles(path, modifiedWithin:"24h", sortBy:"modified")     → recently changed files
localFindFiles(path, names:["*.test.ts"], pathPattern:"src/**")    → scoped file discovery
localFindFiles(path, regex:"^index\\.", entryType:"f")             → all index files
```

**GitHub — code investigation**
```
ghSearchCode(keywords:[...], owner, repo, match:"path")           → file existence check (~10× cheaper)
ghSearchRepos(keywords:[...])                                      → discover owner/repo if unknown
ghViewRepoStructure(owner, repo, path, maxDepth)                  → tree when path structure is unknown
ghGetFileContent(owner, repo, path, minify:"symbols")             → orient on unknown files first
```

**GitHub — PR review**
```
# Single PR:
ghHistoryResearch(owner, repo, prNumber, minify:"standard", charLength:20000,
  content:{metadata:true, body:true, changedFiles:true})
→ ghGetFileContent for current source state

# Multiple known PR numbers — batch in ONE call (saves N-1 round trips):
queries:[
  {owner, repo, prNumber:409, minify:"standard", content:{metadata:true, changedFiles:true}},
  {owner, repo, prNumber:360, minify:"standard", content:{metadata:true, changedFiles:true}},
  {owner, repo, prNumber:336, minify:"standard", content:{metadata:true, changedFiles:true}}
]  ← max 5 per call

# Multiple known file paths — batch reads in ONE call:
queries:[{owner, repo, path:"src/foo.ts"}, {owner, repo, path:"src/bar.ts"}]
```

**GitHub — commit history → PR deep dive (the canonical chain)**

Use `ghHistoryResearch` for both commit history (`type:"commits"`) and PR search/deep-read. There is no separate `ghHistory` tool.

```
# Step 1 — trace who changed a file and when:
ghHistoryResearch(owner, repo, path:"src/auth/session.ts", type:"commits")
  → result includes: messageHeadline:"Feature X (#420)" with embedded PR ref

# Step 2 — extract PR number from messageHeadline (#NNN) → deep read immediately:
ghHistoryResearch(owner, repo, prNumber:420, reviewMode:"full")
  → body, changedFiles, patches, comments, reviews in one call

# Find a PR by topic/keyword/author (all states):
ghHistoryResearch(owner, repo, keywordsToSearch:["pagination", "cursor"], state:"merged")
ghHistoryResearch(owner, repo, author:"username")  — PRs by any contributor (open OR merged)
  → returns lean PR metadata (number, title, author, date)
  → then re-call with prNumber:<N>, reviewMode:"full" to read it

# Batch — trace multiple files in one call (saves N-1 round trips):
queries:[
  {owner, repo, path:"src/auth.ts", type:"commits"},
  {owner, repo, path:"src/session.ts", type:"commits"},
  {owner, repo, path:"src/auth/", type:"commits"}
]  ← max 5 per call

# Repo/dir activity:
ghHistoryResearch(owner, repo, type:"commits", since:"<ISO8601>")           — whole-repo
ghHistoryResearch(owner, repo, path:"src/auth/", type:"commits")            — dir subtree only
  → paginate with page:nextPage when hasMore:true

# With diffs (N+1 API calls — use sparingly):
ghHistoryResearch(owner, repo, path:"src/auth/session.ts", type:"commits", includeDiff:true)
```

**Deep cross-package LSP** (ENABLE_CLONE=true required; fallback: ghViewRepoStructure → ghSearchCode → ghGetFileContent)
```
ghCloneRepo → localViewStructure(localPath) → localSearchCode → lspGetSemantics
```

**Package → source**
```
npmSearch(packageName) → ghViewRepoStructure(owner, repo) → ghSearchCode → ghGetFileContent
```

---

## Response Budget Rules

Any call that might return >5 files or a full PR diff **must** cap its output. No cap = silent truncation or token flood.

| Field | Scope | When to set |
|-------|-------|-------------|
| `responseCharLength` | Entire response (all queries) | Default for unknown-size ops: `10000` |
| `charLength` | Single file or PR body | Always set on file reads >~200 lines |
| `maxMatchesPerFile` | localSearchCode per-file cap | Set when searching noisy files (e.g. generated code) |
| `maxFiles` | localSearchCode total file cap | Set when `mode:"paginated"` on a broad path |
| `itemsPerPage` | PR content paginators | Lower when fetching comments/commits on large PRs |

---

## Discovery-First Pattern

Never run a full `paginated` search as the first call on an unknown codebase. Cheap → targeted → read.

```
Step 1: localSearchCode(keywords, path, mode:"discovery")
        → file list only, near-zero output tokens

Step 2: localSearchCode(keywords, path, include:[files from step 1])
        → targeted paginated, bounded result

Step 3: localGetFileContent(path, matchString)
        → focused read only if snippet insufficient
```

For count-only orientation (which files, how many hits):
```
localSearchCode(keywords, path, countLinesPerFile:true)
→ file + line-count table, ~5 tokens per file
```

---

## Parallel vs Serial Batching

Max 5 queries per bulk call. Batch independent queries; serialize dependent ones.

```
✅ Batch in one call (independent):
  Multiple ghSearchCode queries for the same research goal
  localSearchCode + localFindFiles (different search dimensions)
  ghGetFileContent on N known file paths

❌ Must serialize (output of step N feeds step N+1):
  localSearchCode → lspGetSemantics  (need lineHint from search result)
  ghSearchRepos  → ghViewRepoStructure  (need owner/repo from repos result)
  ghCloneRepo → localViewStructure  (need localPath from clone result)
  localSearchCode → localGetFileContent  (need path from search result)
```
