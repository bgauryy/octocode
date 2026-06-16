# Octocode Research Skill - Research Document

**Generated:** 2026-01-18  
**Version:** 2.0.0

---

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Core Components](#core-components)
- [API Reference](#api-reference)
- [Tool Registry](#tool-registry)
- [Resilience Patterns](#resilience-patterns)
- [Request Flow](#request-flow)
- [Response Formats](#response-formats)
- [File Structure](#file-structure)

---

## Overview

The **octocode-research** skill is an Express.js HTTP server that wraps `octocode-mcp` tools as REST endpoints for code research. It listens to **localhost** on port **1987** and provides:

| Feature | Description |
|---------|-------------|
| **Unified Tool API** | All tools via `POST /tools/call/:toolName` |
| **MCP-compatible** | Prompts, tools, and system context following MCP patterns |
| **Resilience** | Circuit breaker + retry with exponential backoff |
| **Throttling** | Gradual slowdown for high-frequency requests |
| **Session Telemetry** | Tool call tracking for analytics |

### Key Design Principle

> **All tools are executed via a unified `POST /tools/call/:toolName` endpoint, NOT individual GET routes.**

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          OCTOCODE RESEARCH SKILL                            │
├─────────────────────────────────────────────────────────────────────────────┤
│  HTTP Clients (curl/fetch)  ←──────→  Express Server (port 1987)            │
│                                            │                                 │
│                     ┌──────────────────────┼───────────────────────┐        │
│                     │                      │                       │        │
│                     ▼                      ▼                       ▼        │
│              Local Tools            LSP Tools              GitHub Tools     │
│              (ripgrep, fs)     (semantic analysis)           (API)          │
│                                                                              │
│              localSearchCode       lspGotoDefinition      ghSearchCode  │
│              localGetFileContent   lspFindReferences      githubGetFile...  │
│              localFindFiles        lspCallHierarchy       githubViewRepo... │
│              localViewStructure                           githubSearchPR... │
│                                                           npmSearch     │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           EXTERNAL SYSTEMS                                   │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │ Filesystem  │  │ GitHub API  │  │ NPM         │  │ LSP Server  │        │
│  │  (ripgrep)  │  │  (Octokit)  │  │   APIs      │  │             │        │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘        │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Core Components

### 1. Server (`src/server.ts`)

The Express HTTP server that:
- Initializes MCP content cache at startup via `initializeMcpContent()`
- Initializes providers for GitHub token resolution via `initializeProviders()`
- Mounts `/tools` and `/prompts` route handlers
- Exposes `/health` endpoint for monitoring with circuit breaker states
- Handles graceful shutdown with `SIGTERM`/`SIGINT` handlers

**Key functions:**
- `createServer()` - Creates and configures the Express app
- `startServer()` - Starts the HTTP server on port 1987
- `gracefulShutdown()` - Cleans up contexts and circuits on shutdown

### 2. MCP Cache (`src/mcpCache.ts`)

Singleton cache that loads tool metadata **ONCE** at startup:

```typescript
export async function initializeMcpContent(): Promise<CompleteMetadata> {
  if (mcpContent) return mcpContent;
  
  initPromise = (async () => {
    await initialize();
    const content = await loadToolContent();
    mcpContent = content;
    return content;
  })();
  
  return initPromise;
}

export function getMcpContent(): CompleteMetadata {
  if (!mcpContent) {
    throw new Error('mcpContent not initialized');
  }
  return mcpContent;
}
```

### 3. Index (`src/index.ts`)

Re-exports layer that maps `octocode-mcp` functions to skill-friendly names:

| Export | Source |
|--------|--------|
| `ghSearchCode` | `searchMultipleGitHubCode` |
| `ghGetFileContent` | `fetchMultipleGitHubFileContents` |
| `localSearchCode` | `executeRipgrepSearch` |
| `localGetFileContent` | `executeFetchContent` |
| `lspGotoDefinition` | `executeGotoDefinition` |
| `lspFindReferences` | `executeFindReferences` |
| `lspCallHierarchy` | `executeCallHierarchy` |
| `npmSearch` | `searchPackages` |

### 4. Routes (`src/routes/`)

| File | Mounted | Purpose |
|------|---------|---------|
| `tools.ts` | ✅ `/tools/*` | **Main API** - Tool discovery & execution |
| `prompts.ts` | ✅ `/prompts/*` | Prompt discovery |
| `local.ts` | ❌ | Handler logic (used in tests) |
| `lsp.ts` | ❌ | Handler logic (used in tests) |
| `github.ts` | ❌ | Handler logic (used in tests) |
| `package.ts` | ❌ | Handler logic (used in tests) |

---

## API Reference

### Discovery Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Server health, memory usage, circuit breaker states |
| `/tools/list` | GET | List all tools (concise - name + description) |
| `/tools/info/:toolName` | GET | Get tool JSON schema + hints |
| `/tools/info` | GET | List all tools with details |
| `/tools/system` | GET | Get system prompt (load FIRST) |
| `/tools/initContext` | GET | Combined system prompt + all tool schemas (recommended for init) |
| `/tools/metadata` | GET | Get raw metadata (advanced) |
| `/prompts/list` | GET | List all prompts (MCP-compatible format) |
| `/prompts/info/:promptName` | GET | Get prompt content and arguments |

### Execution Endpoint

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/tools/call/:toolName` | **POST** | **Execute any tool** |

**Request format:**
```json
{
  "queries": [{
    "mainResearchGoal": "Overall research objective",
    "researchGoal": "This specific query's goal",
    "reasoning": "Why this approach helps",
    // ... tool-specific parameters
  }]
}
```

**Constraints:**
- `queries` must be an array with 1-3 items
- Each query must include research context parameters

---

## Tool Registry

The `TOOL_REGISTRY` in `src/routes/tools.ts` maps tool names to their functions and resilience wrappers:

```typescript
const TOOL_REGISTRY: Record<string, ToolEntry> = {
  // GitHub tools
  ghSearchCode: { fn: ghSearchCode, resilience: withGitHubResilience, category: 'github' },
  ghGetFileContent: { fn: ghGetFileContent, resilience: withGitHubResilience, category: 'github' },
  ghViewRepoStructure: { fn: ghViewRepoStructure, resilience: withGitHubResilience, category: 'github' },
  ghSearchRepos: { fn: ghSearchRepos, resilience: withGitHubResilience, category: 'github' },
  ghSearchPRs: { fn: ghSearchPRs, resilience: withGitHubResilience, category: 'github' },

  // Local tools
  localSearchCode: { fn: localSearchCode, resilience: withLocalResilience, category: 'local' },
  localGetFileContent: { fn: localGetFileContent, resilience: withLocalResilience, category: 'local' },
  localFindFiles: { fn: localFindFiles, resilience: withLocalResilience, category: 'local' },
  localViewStructure: { fn: localViewStructure, resilience: withLocalResilience, category: 'local' },

  // LSP tools
  lspGotoDefinition: { fn: lspGotoDefinition, resilience: withLspResilience, category: 'lsp' },
  lspFindReferences: { fn: lspFindReferences, resilience: withLspResilience, category: 'lsp' },
  lspCallHierarchy: { fn: lspCallHierarchy, resilience: withLspResilience, category: 'lsp' },

  // Package tools
  npmSearch: { fn: npmSearch, resilience: withPackageResilience, category: 'package' },
};
```

### Available Tools (13 total)

| Tool | Category | Description |
|------|----------|-------------|
| `localSearchCode` | Local | Search code with ripgrep |
| `localGetFileContent` | Local | Read local file content |
| `localFindFiles` | Local | Find files by pattern/metadata |
| `localViewStructure` | Local | View directory tree |
| `lspGotoDefinition` | LSP | Go to symbol definition |
| `lspFindReferences` | LSP | Find all symbol references |
| `lspCallHierarchy` | LSP | Get call hierarchy (incoming/outgoing) |
| `ghSearchCode` | GitHub | Search code in repos |
| `ghGetFileContent` | GitHub | Read file from repo |
| `ghViewRepoStructure` | GitHub | View repo tree |
| `ghSearchRepos` | GitHub | Search repositories |
| `ghSearchPRs` | GitHub | Search pull requests |
| `npmSearch` | Package | Search npm |

---

## Resilience Patterns

### 1. Circuit Breaker (`src/utils/circuitBreaker.ts`)

Prevents cascading failures with three states:

```
CLOSED (normal) ──[failures >= threshold]──► OPEN (reject all)
       ▲                                          │
       │                                     [timeout]
       │                                          │
       └──[successes >= threshold]─── HALF-OPEN ◄─┘
                                        (probe)
```

**Default configuration:**
```typescript
const DEFAULT_CONFIG: CircuitBreakerConfig = {
  failureThreshold: 3,    // Failures before opening
  successThreshold: 2,    // Successes to close from half-open
  resetTimeoutMs: 30000,  // Time before half-open probe
};
```

**Per-service overrides:**

| Service | Failure Threshold | Success Threshold | Timeout |
|---------|-------------------|-------------------|---------|
| LSP | 3 | 1 | 10s |
| GitHub | 2 | 1 | 60s |

**Key functions:**
- `withCircuitBreaker(name, operation, fallback?)` - Execute with protection
- `getCircuitState(name)` - Monitor circuit health
- `configureCircuit(name, config)` - Customize thresholds
- `getAllCircuitStates()` - Health dashboard (used in `/health`)

### 2. Retry with Exponential Backoff (`src/utils/retry.ts`)

```typescript
const RETRY_CONFIGS = {
  lsp: {
    maxAttempts: 3,
    initialDelayMs: 500,
    maxDelayMs: 5000,
    backoffMultiplier: 2,
  },
  github: {
    maxAttempts: 3,
    initialDelayMs: 1000,
    maxDelayMs: 30000,
    backoffMultiplier: 3,
  },
  package: {
    maxAttempts: 3,
    initialDelayMs: 1000,
    maxDelayMs: 15000,
    backoffMultiplier: 2,
  },
  local: {
    maxAttempts: 2,
    initialDelayMs: 100,
    maxDelayMs: 1000,
    backoffMultiplier: 2,
  },
};
```

### 3. Combined Resilience (`src/utils/resilience.ts`)

Four pre-configured wrappers that combine circuit breaker + retry:

```typescript
export async function withGitHubResilience<T>(operation: () => Promise<T>, toolName: string): Promise<T>
export async function withLspResilience<T>(operation: () => Promise<T>, toolName: string): Promise<T>
export async function withLocalResilience<T>(operation: () => Promise<T>, toolName: string): Promise<T>
export async function withPackageResilience<T>(operation: () => Promise<T>, toolName: string): Promise<T>
```

### 4. Readiness Check (`src/middleware/readiness.ts`)

Middleware that verifies the server is ready to handle requests before processing them. Returns 503 Service Unavailable if the server is still initializing.

---

## Request Flow

### Tool Execution Flow

```
POST /tools/call/localSearchCode
Body: { "queries": [{ "pattern": "auth", "path": "src", ... }] }
        │
        ▼
┌─────────────────────────────────────┐
│  requestLogger Middleware           │
│  → Log: tool, params, timestamp     │
│  → Write to ~/.octocode/logs/       │
└────────┬────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│  Route Handler (routes/tools.ts)    │
│  POST /tools/call/:toolName         │
│                                     │
│  1. Lookup tool in TOOL_REGISTRY    │
│  2. Validate queries array (1-3)    │
│  3. Get resilience wrapper          │
└────────┬────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│  Resilience Wrapper                 │
│                                     │
│  toolEntry.resilience(fn, toolName) │
│   ├─ Circuit Breaker Check          │
│   │   └─ OPEN? → Fail fast          │
│   │   └─ CLOSED/HALF-OPEN? → Proceed│
│   ├─ Retry with Backoff             │
│   └─ Execute Tool Function          │
└────────┬────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│  octocode-mcp Tool Function         │
│                                     │
│  localSearchCode({ queries: [...] })│
│   ├─ Execute ripgrep search         │
│   ├─ Parse results                  │
│   ├─ Apply hints                    │
│   └─ Return CallToolResult          │
└────────┬────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│  Response Transformation            │
│                                     │
│  parseToolResponse(rawResult)       │
│   ├─ Extract data, hints, research  │
│   └─ Detect errors                  │
└────────┬────────────────────────────┘
         │
         ▼
HTTP Response (JSON)
```

---

## Response Formats

### Single Query Response

```json
{
  "tool": "localSearchCode",
  "success": true,
  "data": {
    "files": [...],
    "totalMatches": 10,
    "pagination": { "page": 1, "hasMore": false }
  },
  "hints": [
    "Use lineHint for LSP tools",
    "Consider narrowing search with path filter"
  ],
  "research": {
    "mainResearchGoal": "Find authentication handlers",
    "researchGoal": "Locate auth middleware",
    "reasoning": "Understanding auth flow"
  }
}
```

### Bulk Query Response (2-3 queries)

```json
{
  "tool": "localSearchCode",
  "bulk": true,
  "success": true,
  "instructions": "Review results by status...",
  "results": [
    { "id": 1, "status": "hasResults", "data": {...}, "research": {...} },
    { "id": 2, "status": "empty", "data": {}, "research": {...} }
  ],
  "hints": {
    "hasResults": ["Follow up with LSP for semantic analysis"],
    "empty": ["Try broader search pattern"],
    "error": []
  },
  "counts": {
    "total": 2,
    "hasResults": 1,
    "empty": 1,
    "error": 0
  }
}
```

### Error Response

```json
{
  "tool": "localSearchCode",
  "success": false,
  "data": {},
  "hints": [
    "Circuit breaker is open - retry in 30s",
    "Consider using fallback tool"
  ],
  "research": {
    "mainResearchGoal": "...",
    "researchGoal": "...",
    "reasoning": "..."
  }
}
```

---

## File Structure

```
octocode-research/
├── src/
│   ├── server.ts              # Express server entry point
│   ├── index.ts               # Re-exports from octocode-mcp
│   ├── mcpCache.ts            # MCP client caching
│   ├── routes/
│   │   ├── tools.ts           # /tools/* - MAIN tool API
│   │   ├── prompts.ts         # /prompts/* - prompt discovery
│   │   ├── local.ts           # Handler logic (tests)
│   │   ├── lsp.ts             # Handler logic (tests)
│   │   ├── github.ts          # Handler logic (tests)
│   │   └── package.ts         # Handler logic (tests)
│   ├── middleware/
│   │   ├── errorHandler.ts    # Error response formatting
│   │   ├── logger.ts          # Request/response logging
│   │   ├── queryParser.ts     # Zod validation
│   │   └── readiness.ts       # Server readiness check
│   ├── validation/
│   │   ├── index.ts           # Schema exports
│   │   ├── schemas.ts         # HTTP schemas (from octocode-mcp)
│   │   └── httpPreprocess.ts  # Query string conversion
│   ├── utils/
│   │   ├── circuitBreaker.ts  # Circuit breaker pattern
│   │   ├── colors.ts          # Console color functions
│   │   ├── logger.ts          # File-based logging
│   │   ├── resilience.ts      # Combined resilience wrappers
│   │   ├── responseBuilder.ts # Research response formatting
│   │   ├── responseFactory.ts # Safe data extraction
│   │   ├── responseParser.ts  # MCP response parsing
│   │   ├── retry.ts           # Retry with backoff
│   │   └── routeFactory.ts    # Route handler factory
│   ├── types/
│   │   ├── express.d.ts       # Express type extensions
│   │   ├── guards.ts          # Type guard functions
│   │   ├── mcp.ts             # MCP protocol types
│   │   ├── responses.ts       # Response type definitions
│   │   └── toolTypes.ts       # Tool parameter types
│   └── __tests__/
│       ├── integration/       # Integration tests
│       └── unit/              # Unit tests
├── docs/
│   ├── API_REFERENCE.md       # HTTP API reference
│   ├── ARCHITECTURE.md        # Architecture documentation
│   ├── FLOWS.md               # Flow diagrams
│   └── OVERVIEW.md            # This document
├── references/
│   └── GUARDRAILS.md          # Safety guardrails
├── scripts/
│   ├── server.js              # Bundled server (tsdown)
│   └── server.d.ts            # Type declarations
├── SKILL.md                   # Skill definition for agents
├── AGENTS.md                  # Development guide
├── README.md                  # Quick start
├── package.json
├── tsconfig.json
├── tsdown.config.ts
└── vitest.config.ts
```

---

## Quick Start

```bash
# Build
npm run build

# Start server
npm start

# Development with hot reload
npm run dev

# Health check
curl http://localhost:1987/health

# List tools
curl http://localhost:1987/tools/list

# Get tool schema
curl http://localhost:1987/tools/info/localSearchCode

# Execute tool
curl -X POST http://localhost:1987/tools/call/localSearchCode \
  -H "Content-Type: application/json" \
  -d '{"queries": [{
    "mainResearchGoal": "Find auth",
    "researchGoal": "Search handlers",
    "reasoning": "Testing",
    "pattern": "authenticate",
    "path": "/src"
  }]}'
```

---

## Related Documentation

| Document | Purpose |
|----------|---------|
| [SKILL.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/octocode-research/SKILL.md) | Agent workflow guide |
| [AGENTS.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/octocode-research/AGENTS.md) | Development guidelines |
| [ARCHITECTURE.md](./ARCHITECTURE.md) | Full architecture details |
| [FLOWS.md](./FLOWS.md) | Request flow diagrams |
| [API_REFERENCE.md](./API_REFERENCE.md) | HTTP API reference |
| [GUARDRAILS.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/octocode-research/references/GUARDRAILS.md) | Safety rules |

---

*Generated by Octocode Research Analysis*
