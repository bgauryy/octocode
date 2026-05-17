# AGENTS.md - Octocode MCP Server

> **Location**: `packages/octocode-mcp/AGENTS.md`

AI agent guidance for the `octocode-mcp` package - Model Context Protocol server for GitHub and local code research.

This file **overrides** the root [`AGENTS.md`](https://github.com/bgauryy/octocode-mcp/blob/main/AGENTS.md) for work within this package.

---

## Overview

Octocode MCP is an MCP server providing AI agents with code exploration tools:

- **GitHub, GitLab & Bitbucket**: Search code, repositories, PRs/MRs, view structure, fetch content
- **Local Research**: Search code with ripgrep, browse directories, find files, read content
- **LSP Intelligence**: Semantic code navigation with goto definition, find references, call hierarchy
- **Package Discovery**: Search NPM/PyPI for packages and repository URLs

**Key Docs**: See [Key Documentation](#-key-documentation) below for the canonical package doc index.

---

## 🛠️ Commands

All commands run from this package directory (`packages/octocode-mcp/`).
For monorepo-wide setup and workflow commands, see [docs/DEVELOPMENT_GUIDE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/DEVELOPMENT_GUIDE.md).

| Task | Command | Description |
|------|---------|-------------|
| **Build** | `yarn build` | Lint + clean + bundle with tsdown |
| **Build (Dev)** | `yarn build:dev` | Build without lint |
| **Build (Watch)** | `yarn build:watch` | Watch mode for development |
| **Clean** | `yarn clean` | Remove `dist/` directory |
| **Test** | `yarn test` | Run tests with coverage report |
| **Test (Full)** | `yarn test:full` | Lint + typecheck + tests with coverage |
| **Test (Quiet)** | `yarn test:quiet` | Minimal test output |
| **Test (Watch)** | `yarn test:watch` | Watch mode for tests |
| **Test (UI)** | `yarn test:ui` | Vitest UI dashboard |
| **Typecheck** | `yarn typecheck` | TypeScript type checking |
| **Lint** | `yarn lint` | ESLint check |
| **Lint (Fix)** | `yarn lint:fix` | Auto-fix linting issues |
| **Format** | `yarn format` | Prettier format `src/` |
| **Format (Check)** | `yarn format:check` | Check formatting |
| **Debug** | `yarn debug` | Run with MCP Inspector |

### Binary Builds (Bun)

| Target | Command |
|--------|---------|
| Current platform | `yarn build:bin` |
| macOS ARM64 | `yarn build:bin:darwin-arm64` |
| macOS x64 | `yarn build:bin:darwin-x64` |
| Linux ARM64 | `yarn build:bin:linux-arm64` |
| Linux x64 | `yarn build:bin:linux-x64` |
| Linux x64 (musl) | `yarn build:bin:linux-x64-musl` |
| Windows x64 | `yarn build:bin:windows-x64` |
| All platforms | `yarn build:bin:all` |

---

## 📂 Package Structure

```
src/
├── index.ts                 # Entry point - server initialization
├── serverConfig.ts          # Configuration & GitHub token management
├── session.ts               # Session tracking & telemetry
├── responses.ts             # Response formatting utilities
├── errorCodes.ts            # Centralized error definitions
├── types.ts                 # Shared TypeScript types
├── public.ts                # Public API exports
│
├── hints/                   # 💡 Dynamic hint generation
│   ├── index.ts             # Hints module exports
│   ├── dynamic.ts           # Context-aware hints
│   ├── static.ts            # Predefined hints
│   ├── localBaseHints.ts    # Local tool base hints
│   └── types.ts             # Hint type definitions
│
├── scheme/                  # 📐 Shared schema utilities
│   └── baseSchema.ts        # Common schema patterns & bulk query builder
│
├── tools/                   # 🔧 Tool implementations (modular structure)
│   ├── toolConfig.ts        # Tool registry & configuration
│   ├── toolRegistry.ts      # Dynamic tool management (enable/disable/remove)
│   ├── toolMetadata.ts      # Dynamic metadata from API
│   ├── toolNames.ts         # Static tool name constants
│   ├── toolsManager.ts      # Tool registration orchestrator
│   ├── utils.ts             # Tool-specific utilities
│   │
│   ├── github_fetch_content/    # GitHub file content retrieval
│   │   ├── execution.ts         # Handler implementation
│   │   ├── github_fetch_content.ts  # Tool registration
│   │   ├── scheme.ts            # Zod schema
│   │   └── types.ts             # Type definitions
│   │
│   ├── github_search_code/      # GitHub code search
│   │   ├── execution.ts
│   │   ├── github_search_code.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   ├── github_search_pull_requests/  # GitHub PR search
│   │   ├── execution.ts
│   │   ├── github_search_pull_requests.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   ├── github_search_repos/     # GitHub repository search
│   │   ├── execution.ts
│   │   ├── github_search_repos.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   ├── github_view_repo_structure/  # GitHub repo tree
│   │   ├── execution.ts
│   │   ├── github_view_repo_structure.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   ├── local_fetch_content/     # Local file content
│   │   ├── execution.ts
│   │   ├── fetchContent.ts      # Core implementation
│   │   ├── index.ts
│   │   ├── register.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   ├── local_find_files/        # Local file finder
│   │   ├── execution.ts
│   │   ├── findFiles.ts
│   │   ├── index.ts
│   │   ├── register.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   ├── local_ripgrep/           # Local code search (ripgrep)
│   │   ├── execution.ts
│   │   ├── index.ts
│   │   ├── register.ts
│   │   ├── scheme.ts
│   │   ├── searchContentRipgrep.ts
│   │   └── types.ts
│   │
│   ├── local_view_structure/    # Local directory browser
│   │   ├── execution.ts
│   │   ├── index.ts
│   │   ├── local_view_structure.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   ├── lsp_call_hierarchy/      # LSP call hierarchy
│   │   ├── callHierarchy.ts
│   │   ├── execution.ts
│   │   ├── index.ts
│   │   ├── register.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   ├── lsp_find_references/     # LSP find references
│   │   ├── execution.ts
│   │   ├── index.ts
│   │   ├── lsp_find_references.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   ├── lsp_goto_definition/     # LSP goto definition
│   │   ├── execution.ts
│   │   ├── lsp_goto_definition.ts
│   │   ├── scheme.ts
│   │   └── types.ts
│   │
│   └── package_search/          # NPM/PyPI package search
│       ├── execution.ts
│       ├── package_search.ts
│       ├── scheme.ts
│       └── types.ts
│
├── github/                  # 🐙 GitHub API layer
│   ├── index.ts             # GitHub module exports
│   ├── client.ts            # Octokit client with throttling
│   ├── githubAPI.ts         # Core API types & interfaces
│   ├── codeSearch.ts        # Code search operations
│   ├── fileContent.ts       # File content retrieval
│   ├── fileOperations.ts    # File operation utilities
│   ├── repoSearch.ts        # Repository search
│   ├── repoStructure.ts     # Repository tree exploration
│   ├── pullRequestSearch.ts # PR search & diff retrieval
│   ├── queryBuilders.ts     # GitHub search query construction
│   ├── errors.ts            # GitHub error handling
│   └── errorConstants.ts    # GitHub-specific error codes
│
├── gitlab/                  # 🦊 GitLab API layer
│   ├── index.ts             # GitLab module exports
│   ├── client.ts            # GitLab API client
│   ├── GitLabProvider.ts    # GitLab provider implementation
│   ├── codeSearch.ts        # Code search operations
│   ├── fileContent.ts       # File content retrieval
│   ├── repoSearch.ts        # Repository search
│   ├── repoStructure.ts     # Repository tree exploration
│   ├── mergeRequestSearch.ts # MR search & diff retrieval
│   └── errors.ts            # GitLab error handling
│
├── providers/               # 🔌 Multi-provider abstraction
│   ├── index.ts             # Provider module exports
│   ├── factory.ts           # Provider factory & registry
│   ├── types.ts             # Provider type definitions
│   ├── github/              # GitHub provider
│   │   ├── GitHubProvider.ts
│   │   └── github*.ts       # GitHub provider delegates
│   ├── gitlab/              # GitLab provider
│   │   ├── GitLabProvider.ts
│   │   └── gitlab*.ts       # GitLab provider delegates
│   └── bitbucket/           # Bitbucket provider
│       ├── BitbucketProvider.ts
│       └── bitbucket*.ts    # Bitbucket provider delegates
│
├── lsp/                     # 🔤 Language Server Protocol
│   ├── index.ts             # LSP module exports
│   ├── client.ts            # LSP client (spawns servers, JSON-RPC)
│   ├── config.ts            # Language server configurations
│   ├── manager.ts           # LSP server lifecycle management
│   ├── resolver.ts          # Symbol resolution utilities
│   ├── symbols.ts           # Symbol type utilities
│   ├── types.ts             # LSP type definitions
│   ├── uri.ts               # URI handling utilities
│   └── validation.ts        # LSP input validation
│
├── security/                # 🔒 Security layer
│   ├── withSecurityValidation.ts  # Security wrapper for tools
│   ├── contentSanitizer.ts  # Secret detection & redaction
│   ├── pathValidator.ts     # Path traversal prevention
│   ├── commandValidator.ts  # Command injection prevention
│   ├── executionContextValidator.ts # Execution context validation
│   ├── ignoredPathFilter.ts # Sensitive path filtering
│   ├── regexes.ts           # Re-exports from regexes/
│   ├── regexes/             # Secret detection patterns (200+)
│   │   ├── index.ts         # Combined exports
│   │   ├── ai-providers.ts  # AI/LLM API keys
│   │   ├── cloud-infrastructure.ts # AWS, GCP, Azure, databases
│   │   ├── auth-crypto.ts   # JWT, OAuth, private keys
│   │   ├── dev-tools-vcs.ts # CI/CD, GitHub, GitLab
│   │   ├── payments-commerce.ts # Stripe, PayPal, crypto
│   │   └── communications.ts # Slack, social, messaging
│   ├── mask.ts              # Data masking utilities
│   ├── patternsConstants.ts # Security pattern definitions
│   └── securityConstants.ts # Security configuration
│
├── commands/                # 🖥️ CLI command builders
│   ├── BaseCommandBuilder.ts    # Abstract command builder
│   ├── RipgrepCommandBuilder.ts # ripgrep (rg) command builder — single search engine, bundled via @vscode/ripgrep
│   ├── FindCommandBuilder.ts    # find command builder
│   └── LsCommandBuilder.ts      # ls command builder
│
├── utils/                   # 🛠️ Shared utilities (organized by domain)
│   ├── core/                # Core utilities
│   │   ├── constants.ts     # Global constants
│   │   ├── logger.ts        # MCP logging integration
│   │   ├── promise.ts       # Async/promise utilities
│   │   └── types.ts         # Core type definitions
│   │
│   ├── credentials/         # Credential utilities
│   │   └── index.ts         # Credential management re-exports
│   │
│   ├── environment/         # Environment detection
│   │   └── environmentDetection.ts # Runtime environment detection
│   │
│   ├── exec/                # Command execution
│   │   ├── index.ts         # Module exports
│   │   ├── safe.ts          # Safe command execution
│   │   ├── spawn.ts         # Process spawning
│   │   ├── npm.ts           # NPM command utilities
│   │   └── commandAvailability.ts # Command detection
│   │
│   ├── file/                # File operations
│   │   ├── byteOffset.ts    # Byte offset calculations
│   │   ├── filters.ts       # File filtering utilities
│   │   ├── size.ts          # File size utilities
│   │   ├── toolHelpers.ts   # Tool-specific helpers
│   │   └── types.ts         # File type definitions
│   │
│   ├── http/                # HTTP utilities
│   │   ├── cache.ts         # Response caching
│   │   └── fetch.ts         # Fetch with retries
│   │
│   ├── minifier/            # Content minification
│   │   ├── index.ts         # Module exports
│   │   ├── minifier.ts      # File-type aware minification
│   │   └── jsonToYamlString.ts # YAML conversion
│   │
│   ├── package/             # Package utilities
│   │   ├── common.ts        # Shared package utilities
│   │   ├── npm.ts           # NPM package search
│   │   └── python.ts        # PyPI package search
│   │
│   ├── pagination/          # Pagination utilities
│   │   ├── index.ts         # Module exports
│   │   ├── core.ts          # Core pagination logic
│   │   ├── hints.ts         # Pagination hints
│   │   └── types.ts         # Pagination types
│   │
│   ├── parsers/             # Output parsers
│   │   ├── diff.ts          # Diff parsing
│   │   └── ripgrep.ts       # Ripgrep output parsing
│   │
│   └── response/            # Response utilities
│       ├── bulk.ts          # Bulk operation responses
│       └── error.ts         # Error response formatting
│
├── prompts/                 # 💬 MCP prompts
│   └── prompts.ts           # Prompt registration
│
└── types/                   # 📝 Type definitions
    ├── metadata.ts          # Metadata types
    ├── toolTypes.ts         # Tool-specific types
    └── markdown.d.ts        # Markdown type declarations
```

### Tests Structure

```
tests/
├── index.*.test.ts          # Server lifecycle tests
├── serverConfig.*.test.ts   # Configuration tests
├── session.*.test.ts        # Session/telemetry tests
├── errorCodes.test.ts       # Error codes tests
├── commands/                # Command builder tests
├── errors/                  # Error handling tests
├── github/                  # GitHub API tests (29 files)
├── lsp/                     # LSP client tests (9 files)
├── security/                # Security tests (15 files)
├── scheme/                  # Schema validation tests
├── hints/                   # Hints system tests
├── tools/                   # Tool implementation tests (54 files)
├── utils/                   # Utility tests (37 files)
├── integration/             # End-to-end tests
├── helpers/                 # Test utilities & mocks
└── fixtures/                # Test fixtures
```

---

## 🧰 Available Tools

| Tool | Type | Local | Description |
|------|------|-------|-------------|
| `githubSearchCode` | search | ❌ | Search code across GitHub/GitLab |
| `githubGetFileContent` | content | ❌ | Fetch file content or directory to disk (`type: "directory"` requires `ENABLE_CLONE`) |
| `githubViewRepoStructure` | content | ❌ | Browse GitHub/GitLab repository tree |
| `githubSearchRepositories` | search | ❌ | Search GitHub/GitLab repositories |
| `githubSearchPullRequests` | history | ❌ | Search PRs/MRs and view diffs |
| `packageSearch` | search | ❌ | Search NPM/PyPI packages |
| `localSearchCode` | search | ✅ | Search code with ripgrep |
| `localViewStructure` | content | ✅ | Browse local directories |
| `localFindFiles` | search | ✅ | Find files by metadata |
| `localGetFileContent` | content | ✅ | Read local file content |
| `lspGotoDefinition` | LSP | ✅ | Jump to symbol definition |
| `lspFindReferences` | LSP | ✅ | Find all usages of a symbol |
| `lspCallHierarchy` | LSP | ✅ | Trace function call relationships |

### LSP Tools

LSP (Language Server Protocol) tools provide **semantic** code intelligence:

- **No IDE required** - Works standalone via spawned language servers
- **TypeScript/JavaScript bundled** - Works out-of-box
- **30+ languages supported** - Python, Go, Rust, Java, C/C++, etc. (requires server installation)
- **Cross-platform** - macOS, Linux, Windows

See the Local Tools reference document in `./docs/` for full documentation.

---

## 📦 Package Guidelines

These are the core principles for this MCP server:

1. **Security First**: Validate all inputs and paths. Sanitize all outputs.
2. **Bulk Operations**: Support 1-5 items per tool call for efficiency (3 for GitHub, 5 for local).
3. **Token Efficiency**: Minimize response size for LLMs via minification and YAML output.
4. **Graceful Degradation**: Always return usable results; never crash. Isolate errors per query.
5. **Research Context**: Every query requires `mainResearchGoal`, `researchGoal`, `reasoning`.

---

## 🏗️ Architecture Patterns

### Tool Module Structure

Each tool is organized as a self-contained module:

```
tools/<tool_name>/
├── execution.ts         # Handler implementation (bulk logic)
├── <tool_name>.ts       # Tool registration with MCP server
├── scheme.ts            # Zod schema for input validation
├── types.ts             # TypeScript type definitions
├── index.ts             # Module exports (local tools)
└── register.ts          # Registration helper (local tools)
```

### Tool Registration Flow

```
Schema (Zod) → registerTool() → Security Wrapper → Bulk Handler → Implementation → Sanitizer → Response
```

1. **Schema Validation** (`<tool>/scheme.ts`) - Zod validates inputs
2. **Registration** (`server.registerTool()`) - `inputSchema`, `outputSchema`, `annotations`
3. **Security Wrapper** (`withSecurityValidation.ts`) - Input sanitization, secret detection
4. **Bulk Operations** (`<tool>/execution.ts`) - Parallel query execution (1-5 queries)
5. **Tool Implementation** - Business logic, API calls
6. **Content Sanitizer** (`contentSanitizer.ts`) - Output secret redaction
7. **Response** (`responses.ts`) - YAML/JSON text + `structuredContent`

Tools return `structuredContent` validated by the SDK against `outputSchema`. `RegisteredTool` handles are stored in `toolRegistry.ts` for runtime `enable()`/`disable()`/`remove()`. The server advertises `listChanged: true` and defers background work to the `oninitialized` callback.

### Key Design Decisions

- **Modular Tools**: Each tool is a self-contained directory with scheme, types, execution, and registration
- **Bulk Queries**: All tools accept 1-5 queries per request
- **Research Context**: Every query requires `mainResearchGoal`, `researchGoal`, `reasoning`
- **Security First**: All I/O sanitized, secrets redacted, paths validated
- **Single-Engine Search**: `localSearchCode` uses bundled `@vscode/ripgrep` only — no grep fallback. Errors are isolated per query and surface actionable install hints when the bundled binary is missing.
- **Pooled LSP Clients**: All LSP tools acquire clients through `LspClientPool` so tsserver stays warm across requests; callers MUST NOT call `client.stop()`.
- **Token Efficiency**: Content minification, YAML output, response prioritization

---

## 🛡️ Safety & Permissions

### Package-Level Access

| Path | Access | Description |
|------|--------|-------------|
| `src/` | ✅ FULL | Source code |
| `tests/` | ✅ FULL | Test files |
| `docs/` | ✅ EDIT | Documentation |
| `*.json`, `*.config.*` | ⚠️ ASK | Package configs |
| `dist/`, `coverage/` | ❌ NEVER | Generated files |

### Protected Files

- **Never Modify**: `dist/`, `coverage/`, `node_modules/`
- **Ask Before Modifying**: `package.json`, `tsconfig.json`, `vitest.config.ts`, `tsdown.config.ts`

---

## 🧪 Testing Protocol

### Requirements

- **Coverage**: 90% required (Statements, Branches, Functions, Lines)
- **Framework**: Vitest with v8 coverage

### Test Categories

| Category | Path | Purpose |
|----------|------|---------|
| Unit | `tests/<module>.test.ts` | Individual function tests |
| Integration | `tests/integration/` | End-to-end tool tests |
| Security | `tests/security/` | Penetration & bypass tests |
| GitHub API | `tests/github/` | API mocking & validation |
| LSP | `tests/lsp/` | LSP client & tool tests |
| Hints | `tests/hints/` | Hints system tests |

### Running Tests

```bash
# Full test suite with coverage
yarn test

# Quick feedback loop
yarn test:quiet

# Development mode
yarn test:watch

# Visual debugging
yarn test:ui
```

---

## ⚙️ Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `GITHUB_TOKEN` | GitHub personal access token | - |
| `GITHUB_API_URL` | GitHub API base URL | `https://api.github.com` |
| `OCTOCODE_TOKEN` | Octocode-specific GitHub token (highest priority) | - |
| `GH_TOKEN` | GitHub CLI compatible token | - |
| `GITLAB_TOKEN` | GitLab personal access token | - |
| `GL_TOKEN` | GitLab token (fallback) | - |
| `GITLAB_HOST` | GitLab instance URL | `https://gitlab.com` |
| `ENABLE_LOCAL` | Enable local filesystem tools | `true` |
| `ENABLE_CLONE` | Enable `githubCloneRepo` tool and `githubGetFileContent` directory mode (requires `ENABLE_LOCAL`) | `false` |
| `OCTOCODE_CACHE_TTL_MS` | Cache TTL for cloned repos in milliseconds | `86400000` (24h) |
| `DISABLE_PROMPTS` | Disable prompts/slash commands | `false` |
| `LOG` | Enable session logging | `true` |
| `REQUEST_TIMEOUT` | API request timeout (ms) | `30000` |
| `MAX_RETRIES` | Maximum retry attempts | `3` |
| `TOOLS_TO_RUN` | Comma-separated tool whitelist | - |
| `ENABLE_TOOLS` | Comma-separated tools to enable | - |
| `DISABLE_TOOLS` | Comma-separated tools to disable | - |
| `OCTOCODE_OUTPUT_FORMAT` | Response format: `yaml` (default) or `json` | `yaml` |

---

## 📚 Key Documentation

| Document | Description |
|----------|-------------|
| [`GITHUB_GITLAB_TOOLS_REFERENCE.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/docs/GITHUB_GITLAB_TOOLS_REFERENCE.md) | GitHub/GitLab tools: search code/repos/PRs, content, packages |
| [`LOCAL_TOOLS_REFERENCE.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/docs/LOCAL_TOOLS_REFERENCE.md) | Local + LSP tools: search, structure, files, semantic analysis |
| [`CLONE_AND_LOCAL_TOOLS_WORKFLOW.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/docs/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md) | Clone repos → use local + LSP tools for deep analysis |
| [`AUTHENTICATION_SETUP.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/docs/AUTHENTICATION_SETUP.md) | GitHub/GitLab authentication setup |
| [`README.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/README.md) | Installation, usage, configuration |
| [MCP Spec](https://modelcontextprotocol.io/) | Model Context Protocol specification |
| [GitHub REST API](https://docs.github.com/en/rest) | GitHub API reference |
| [LSP Spec](https://microsoft.github.io/language-server-protocol/) | Language Server Protocol specification |

---

## 🔑 Key Files Reference

| Purpose | File(s) |
|---------|---------|
| Entry point | `src/index.ts` |
| Tool registration | `src/tools/toolsManager.ts`, `src/tools/toolConfig.ts` |
| Tool registry | `src/tools/toolRegistry.ts` |
| Tool modules | `src/tools/<tool_name>/` (scheme.ts, execution.ts, types.ts) |
| Hints system | `src/hints/` |
| Security wrapper | `src/security/withSecurityValidation.ts` |
| Output sanitization | `src/utils/secureServer.ts` |
| Secret detection | `src/security/contentSanitizer.ts`, `src/security/regexes/` |
| Path validation | `src/security/pathValidator.ts` |
| GitHub client | `src/github/client.ts` |
| GitLab client | `src/gitlab/client.ts` |
| Provider factory | `src/providers/factory.ts` |
| LSP client | `src/lsp/client.ts` |
| LSP config | `src/lsp/config.ts`, `src/lsp/manager.ts` |
| Bulk operations | `src/utils/response/bulk.ts` |
| Response formatting | `src/responses.ts` |
| Error codes | `src/errorCodes.ts` |
| Package search | `src/utils/package/npm.ts`, `src/utils/package/python.ts` |

---

*Package-level AGENTS.md for octocode-mcp v11.x*
