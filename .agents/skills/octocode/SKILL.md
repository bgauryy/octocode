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
│   ├── octocode-tools-core/    # All 12 tool implementations + execution (TypeScript)
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

## 12 Tools — Routing Guide

### GitHub Tools (remote, requires token)

| Tool | When to use |
|------|-------------|
| `ghSearchCode` | Find code snippets across GitHub by keywords, owner, repo, extension, language, path |
| `ghGetFileContent` | Read a specific file (or region) from a GitHub repo |
| `ghViewRepoStructure` | Browse a repo's directory tree |
| `ghCloneRepo` | Clone a repo/subtree to disk for local + LSP work (`ENABLE_CLONE=true` required) |
| `ghSearchRepos` | Discover repos by name, keywords, topic, language, stars |
| `ghSearchPRs` | Search PRs, review diffs, fetch patches/comments/reviews |

### Local Tools (filesystem, `ENABLE_LOCAL=true` by default)

| Tool | When to use |
|------|-------------|
| `localSearchCode` | ripgrep search — file+line, regex, modes (paginated/discovery/detailed/count) |
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

Is it GitHub code / history?
  → ghSearchRepos → ghViewRepoStructure → ghSearchCode → ghGetFileContent

Need deep cross-package LSP analysis?
  → ghCloneRepo → local + LSP on localPath
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

## Minify Modes

| Value | Meaning |
|-------|---------|
| `"none"` | Exact raw text — preserves comments, formatting (use for quoting or exact diffs) |
| `"standard"` | Strips comments + blank lines — token-efficient reads |
| `"symbols"` | Skeleton/gutter only — fastest orientation, skips matchString/charLength |

Default is `"standard"` for `localGetFileContent`, `ghGetFileContent`, and `ghSearchPRs`. Use `"none"` explicitly when you need exact text, comments, or raw diffs.

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
| `WORKSPACE_ROOT` | `process.cwd()` | Root for resolving relative paths |
| `ALLOWED_PATHS` | `[]` (all) | Restrict local tools to comma-separated paths |
| `OCTOCODE_OUTPUT_FORMAT` | `yaml` | `yaml` or `json` |

---

## Pagination Pattern

- Page only when response includes `hasMore: true` or `nextPage`
- Use `charOffset` + `charLength` for byte-level continuation on large files/PR bodies
- `localSearchCode` uses `matchPage` for per-file match pagination
- Narrow (add filters / keywords) before paging noisy results

---

## Evidence Pattern

```
snippets from search = discovery (not proof)
proof = getFileContent(matchString=exact-text, minify:"none")
LSP needs uri + symbolName + lineHint from a prior localSearchCode hit
documentSymbols only needs uri
```

---

## Common Research Chains

**Local symbol investigation**
```
localViewStructure → localSearchCode → localGetFileContent → lspGetSemantics
```

**GitHub code investigation**
```
ghSearchRepos → ghViewRepoStructure → ghSearchCode → ghGetFileContent
```

**Package → source**
```
npmSearch → ghViewRepoStructure → ghSearchCode → ghGetFileContent
```

**Deep cross-package LSP**
```
ghCloneRepo → localViewStructure(localPath) → localSearchCode → lspGetSemantics
```

**PR review**
```
ghSearchPRs(prNumber, reviewMode="full") → ghGetFileContent for current source
```
