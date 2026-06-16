# Octocode Research - Main Flows & Architecture

> Understanding how the octocode-research skill works from server startup to tool execution.
> **v2.1.0** - Process managed by PM2

## Table of Contents

- [Overview](#overview)
- [Architecture Diagram](#architecture-diagram)
- [Main Components](#main-components)
- [Core Flows](#core-flows)
  - [Server Startup Flow](#1-server-startup-flow)
  - [Tool Execution Flow](#2-tool-execution-flow)
  - [Discovery Flow](#3-discovery-flow)
- [PM2 Process Management](#pm2-process-management)
- [Component Connections](#component-connections)
- [Quick Reference](#quick-reference)

---

## Overview

The **octocode-research** skill is a lightweight HTTP API server that wraps `octocode-mcp` tools for code research. It provides:

- **HTTP Interface**: REST API on `localhost:1987` by default (configurable via `OCTOCODE_RESEARCH_HOST` / `OCTOCODE_RESEARCH_PORT`)
- **HTTP Clients**: curl, fetch, or any HTTP client
- **Unified Tool API**: All tools via `POST /tools/call/:toolName`
- **Resilience**: Circuit breaker + retry patterns for reliability

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          OCTOCODE RESEARCH SKILL                            │
├─────────────────────────────────────────────────────────────────────────────┤
│  HTTP Clients (curl/fetch)  ←──────→  HTTP Server (port 1987)  ←→ octocode-mcp │
│                                            │                                 │
│                                            ├─→ Local Tools (ripgrep, fs)    │
│                                            ├─→ LSP Tools (semantic analysis)│
│                                            ├─→ GitHub Tools (API)           │
│                                            └─→ Package Tools (npm)          │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Architecture Diagram

```
                              ┌───────────────────────────┐
                              │      AI Agent / User      │
                              └─────────────┬─────────────┘
                                            │
                          ┌─────────────────┼─────────────────┐
                          │                 │                 │
                          ▼                 ▼                 ▼
                    ┌─────────┐       ┌─────────┐       ┌─────────┐
                    │  curl   │       │  fetch  │       │ HTTP    │
                    │         │       │         │       │ Client  │
                    └────┬────┘       └────┬────┘       └────┬────┘
                         │                 │                 │
                         └─────────────────┼─────────────────┘
                                           │ HTTP Request
                                           ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                           EXPRESS SERVER (port 1987)                          │
├──────────────────────────────────────────────────────────────────────────────┤
│ ┌──────────────────────────────────────────────────────────────────────────┐ │
│ │                           MIDDLEWARE LAYER                                │ │
│ │  ┌─────────────┐  ┌───────────────┐  ┌─────────────┐  ┌───────────────┐  │ │
│ │  │requestLogger│→│  express.json │→│contextPropag│→│ errorHandler  │  │ │
│ │  └─────────────┘  └───────────────┘  └─────────────┘  └───────────────┘  │ │
│ └──────────────────────────────────────────────────────────────────────────┘ │
│                                     │                                        │
│                                     ▼                                        │
│ ┌──────────────────────────────────────────────────────────────────────────┐ │
│ │                            ROUTE HANDLERS                                 │ │
│ │  ┌─────────────────────────────────────────────────────────────────────┐ │ │
│ │  │                         /tools/*                                     │ │ │
│ │  │  • GET  /tools/list           - List all tools                      │ │ │
│ │  │  • GET  /tools/info/:name     - Get tool schema                     │ │ │
│ │  │  • GET  /tools/system         - Get system prompt                   │ │ │
│ │  │  • POST /tools/call/:name     - Execute any tool                    │ │ │
│ │  └─────────────────────────────────────────────────────────────────────┘ │ │
│ │  ┌─────────────────────────────────────────────────────────────────────┐ │ │
│ │  │                        /prompts/*                                   │ │ │
│ │  │  • GET  /prompts/list         - List all prompts                    │ │ │
│ │  │  • GET  /prompts/info/:name   - Get prompt details                  │ │ │
│ │  └─────────────────────────────────────────────────────────────────────┘ │ │
│ │        │                                                                 │ │
│ │  ┌─────┴─────────────────────────────────────────────────────┐          │ │
│ │  │                    RESILIENCE LAYER                        │          │ │
│ │  │    (Circuit Breaker + Retry + Rate Limit Handling)        │          │ │
│ │  └───────────────────────────┬────────────────────────────────┘          │ │
│ └──────────────────────────────┼────────────────────────────────────────────┘ │
└────────────────────────────────┼────────────────────────────────────────────┘
                                 │
                                 ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                            OCTOCODE-MCP PACKAGE                              │
├──────────────────────────────────────────────────────────────────────────────┤
│  src/index.ts (re-exports)  →  Tool Execution Functions                      │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │ localSearchCode, localGetFileContent, localFindFiles, localViewStructure│  │
│  │ lspGotoDefinition, lspFindReferences, lspCallHierarchy                 │  │
│  │ ghSearchCode, ghGetFileContent, ghViewRepoStructure, etc.  │  │
│  │ npmSearch                                                          │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                           EXTERNAL SYSTEMS                                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │ Filesystem  │  │ GitHub API  │  │ NPM         │  │ LSP Server  │         │
│  │  (ripgrep)  │  │  (Octokit)  │  │   APIs      │  │             │         │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘         │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Main Components

### 1. **Server (\`src/server.ts\`)**

The Express HTTP server that:
- Initializes MCP content cache at startup
- Mounts \`/tools\` and \`/prompts\` route handlers
- Handles graceful shutdown
- Exposes \`/health\` endpoint for monitoring

### 2. **MCP Cache (\`src/mcpCache.ts\`)**

Singleton cache that:
- Loads tool metadata ONCE at startup
- Provides fast access to tool schemas
- Avoids repeated initialization costs

### 3. **Index (\`src/index.ts\`)**

Re-exports layer that:
- Maps \`octocode-mcp\` functions to skill-friendly names
- Provides type exports for TypeScript consumers
- Centralizes all tool imports

### 4. **Routes (\`src/routes/\`)**

| File | Endpoints | Purpose |
|------|-----------|---------|
| \`tools.ts\` | \`/tools/list\`, \`/tools/info/:name\`, \`/tools/call/:name\`, \`/tools/system\` | **Main API** - Tool discovery & execution |
| \`prompts.ts\` | \`/prompts/list\`, \`/prompts/info/:name\` | Prompt discovery |
| \`local.ts\` | *(Not mounted - used in tests only)* | Filesystem operations handlers |
| \`lsp.ts\` | *(Not mounted - used in tests only)* | Semantic analysis handlers |
| \`github.ts\` | *(Not mounted - used in tests only)* | GitHub API handlers |
| \`package.ts\` | *(Not mounted - used in tests only)* | npm search handlers |

> **Note**: Only \`/tools/*\` and \`/prompts/*\` are mounted in production. The individual route files contain handler logic used by the unified \`/tools/call/:toolName\` endpoint.

### 5. **Build Output (\`scripts/\`)**

The bundled server output from tsdown:

| File | Purpose |
|------|---------|
| \`server.js\` | Bundled server (standalone, all deps included) |
| \`server.d.ts\` | TypeScript type declarations |

---

## Core Flows

### 1. Server Startup Flow

```
┌────────────────────────────────────────────────────────────────────────────┐
│                   SERVER STARTUP FLOW (PM2 Managed)                         │
└────────────────────────────────────────────────────────────────────────────┘

   npm run pm2:start (or pm2 start ecosystem.config.cjs)
          │
          ▼
   ┌──────────────────────────────────────┐
   │     PM2 Process Manager              │
   │  - Reads ecosystem.config.cjs        │
   │  - Spawns node scripts/server.js     │
   │  - Sets wait_ready: true             │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │        src/server.ts                  │
   │        startServer()                  │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │ 1. createServer()                    │
   │  ├─ initializeLogger()               │
   │  ├─ initializeSession()              │
   │  ├─ express()                        │
   │  ├─ use(requestLogger)               │
   │  ├─ mount(/health)                   │
   │  ├─ mount(/tools, toolsRoutes)       │
   │  ├─ mount(/prompts, promptsRoutes)   │
   │  └─ use(errorHandler)                │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │ 2. app.listen(1987)                  │
   │  └─ HTTP server listening            │
   │  └─ Status: "initializing"           │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │ 3. Background initialization (async) │
   │  ├─ initializeMcpContent()           │
   │  │   ├─ initialize() (octocode-mcp)  │
   │  │   └─ loadToolContent() → cache    │
   │  ├─ initializeProviders()            │
   │  │   └─ Resolve GitHub token         │
   │  └─ process.send('ready')  ◄─────────│── PM2 ready signal
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │ 4. PM2 marks process "online"        │
   │  └─ Status: "ok"                     │
   │  └─ Server Ready!                    │
   └──────────────────────────────────────┘
```

### 2. Tool Execution Flow

```
┌────────────────────────────────────────────────────────────────────────────┐
│                        TOOL EXECUTION FLOW                                  │
└────────────────────────────────────────────────────────────────────────────┘

   POST /tools/call/localSearchCode
   Body: { "queries": [{ "pattern": "auth", "path": "src", ... }] }
          │
          ▼
   ┌──────────────────────────────────────┐
   │  requestLogger Middleware            │
   │  → Log: tool, params, timestamp      │
   │  → Write to ~/.octocode/logs/        │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │  Route Handler (routes/tools.ts)     │
   │  POST /tools/call/:toolName          │
   │                                       │
   │  1. Lookup tool in TOOL_REGISTRY     │
   │  2. Validate queries array           │
   │  3. Get resilience wrapper           │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │  Body Validation                     │
   │                                       │
   │  Input:  { queries: [...] }          │
   │  Checks:                             │
   │   - queries is array                 │
   │   - 1-3 queries per request          │
   │   - Tool exists in registry          │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │  Resilience Wrapper                  │
   │                                       │
   │  toolEntry.resilience(fn, toolName)  │
   │   ├─ Circuit Breaker Check           │
   │   │   └─ OPEN? → Fail fast           │
   │   │   └─ CLOSED/HALF-OPEN? → Proceed │
   │   ├─ Retry with Backoff              │
   │   │   └─ Config per tool category    │
   │   └─ Execute Tool Function           │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │  octocode-mcp Tool Function          │
   │                                       │
   │  localSearchCode({ queries: [...] }) │
   │   ├─ Execute ripgrep search          │
   │   ├─ Parse results                   │
   │   ├─ Apply hints                     │
   │   └─ Return CallToolResult           │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │  Response Transformation             │
   │                                       │
   │  parseToolResponse(rawResult)        │
   │   ├─ Extract data, hints, research   │
   │   └─ Detect errors                   │
   └────────┬─────────────────────────────┘
            │
            ▼
   ┌──────────────────────────────────────┐
   │  HTTP Response                       │
   │                                       │
   │  {                                   │
   │    tool: "localSearchCode",          │
   │    success: true,                    │
   │    data: { ... },                    │
   │    hints: [...],                     │
   │    research: { ... }                 │
   │  }                                   │
   └──────────────────────────────────────┘
```

### 3. Discovery Flow

```
┌────────────────────────────────────────────────────────────────────────────┐
│                          DISCOVERY FLOW                                     │
└────────────────────────────────────────────────────────────────────────────┘

   GET /tools/list                       GET /tools/info/localSearchCode
          │                                       │
          ▼                                       ▼
   ┌──────────────────────┐              ┌──────────────────────┐
   │  Static tool list    │              │  getMcpContent()     │
   │  (hardcoded in       │              │   └─ Return cached   │
   │   tools.ts)          │              │      metadata        │
   └────────┬─────────────┘              └────────┬─────────────┘
            │                                      │
            ▼                                      ▼
   ┌──────────────────────┐              ┌──────────────────────┐
   │  Return tool list:   │              │  Find tool by name:  │
   │   - name             │              │   - name             │
   │   - description      │              │   - description      │
   │   - _hint: use       │              │   - inputSchema (Zod)│
   │     /tools/info      │              │   - hints            │
   │                      │              │                      │
   │  (concise discovery) │              │  (full schema)       │
   └──────────────────────┘              └──────────────────────┘
```

---

## Component Connections

```
┌────────────────────────────────────────────────────────────────────────────┐
│                       COMPONENT DEPENDENCY GRAPH                            │
└────────────────────────────────────────────────────────────────────────────┘

                         ┌─────────────────┐
                         │   package.json  │
                         │    (scripts)    │
                         └────────┬────────┘
                                  │
              ┌───────────────────┼───────────────────┐
              │                   │                   │
              ▼                   ▼                   ▼
      ┌───────────────┐  ┌───────────────┐  ┌───────────────┐
      │ scripts/      │  │ src/server.ts │  │ src/cli.ts    │
      │ server.ts     │──│  (entry)      │  │  (entry)      │
      └───────┬───────┘  └───────┬───────┘  └───────────────┘
              │                  │
              │                  ├─────────────────────────────┐
              │                  │                             │
              ▼                  ▼                             ▼
      ┌───────────────┐  ┌───────────────┐            ┌───────────────┐
      │ octocode-     │  │ src/mcpCache  │            │ src/routes/   │
      │ shared        │  │ .ts           │◄───────────│ tools.ts      │
      │ (session)     │  └───────┬───────┘            │ prompts.ts    │
      └───────────────┘          │                    └───────┬───────┘
                                 │                            │
                                 ▼                            ▼
                         ┌───────────────┐            ┌───────────────┐
                         │ src/index.ts  │◄───────────│ src/utils/    │
                         │ (re-exports)  │            │ resilience    │
                         └───────┬───────┘            └───────┬───────┘
                                 │                            │
                                 │                            │
                                 ▼                            ▼
                         ┌───────────────┐            ┌───────────────┐
                         │ octocode-mcp  │            │ src/utils/    │
                         │ (tools)       │            │ circuitBreaker│
                         └───────────────┘            │ retry.ts      │
                                                      └───────────────┘
```

---

## PM2 Process Management

The server is managed by PM2 for automatic restarts, monitoring, and log management.

### PM2 Features

| Feature | Configuration | Description |
|---------|---------------|-------------|
| **Cron Restart** | `0 * * * *` | Restarts every hour for memory hygiene |
| **Memory Guard** | `500M` | Auto-restart if memory exceeds threshold |
| **Ready Signal** | `wait_ready: true` | PM2 waits for `process.send('ready')` |
| **Kill Timeout** | `10000ms` | Graceful shutdown timeout |
| **Auto Restart** | `max_restarts: 10` | Restart on crash with backoff |

### PM2 Commands

```bash
# NPM Scripts
npm run pm2:start          # Start with PM2
npm run pm2:stop           # Stop gracefully
npm run pm2:restart        # Restart
npm run pm2:reload         # Zero-downtime reload
npm run pm2:delete         # Remove from PM2
npm run pm2:logs           # View logs
npm run pm2:monit          # Dashboard

# Direct PM2 Commands
pm2 status                 # List processes
pm2 logs octocode-research # Tail logs
pm2 describe octocode-research  # Process details
```

### Lifecycle Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           PM2 LIFECYCLE                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  pm2 start ──▶ [STARTING] ──▶ [WAITING READY] ──▶ [ONLINE]                 │
│                                      │               │                      │
│                    process.send('ready')             │                      │
│                                                      │                      │
│  ┌──────────────────────────────────────────────────┴──────────────────┐   │
│  │                         RUNNING                                      │   │
│  │  - Cron restart every hour                                          │   │
│  │  - Memory check (restart if > 500MB)                                │   │
│  │  - Auto-restart on crash (with backoff)                             │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│  SIGINT/SIGTERM ──────────────────▶│                                        │
│                                    │                                        │
│                    ┌───────────────┴───────────────┐                       │
│                    │     gracefulShutdown()        │                       │
│                    │  1. stopCircuitCleanup()      │                       │
│                    │  2. clearAllCircuits()        │                       │
│                    │  3. server.close()            │                       │
│                    │  4. process.exit(0)           │                       │
│                    └───────────────────────────────┘                       │
│                                    │                                        │
│                                    ▼                                        │
│                           PM2 Auto-Restart                                  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Quick Reference

### Start Server

```bash
# Development (with hot reload)
npm run dev

# Production (PM2 managed - RECOMMENDED)
npm run pm2:start
npm run pm2:logs           # View logs

# Direct (no PM2)
npm start
# Or: node scripts/server.js
```

### HTTP Examples

```bash
# Discovery
curl http://localhost:1987/health                        # Health check
curl http://localhost:1987/tools/list                    # List all tools
curl http://localhost:1987/tools/info/localSearchCode    # Get tool schema
curl http://localhost:1987/tools/system                  # Load system prompt
curl http://localhost:1987/prompts/list                  # List all prompts

# Tool Execution (ALL tools via POST /tools/call/:toolName)
curl -X POST http://localhost:1987/tools/call/localSearchCode \
  -H "Content-Type: application/json" \
  -d '{"queries": [{"mainResearchGoal": "Find auth", "researchGoal": "Search", "reasoning": "Test", "pattern": "auth", "path": "src"}]}'

curl -X POST http://localhost:1987/tools/call/lspGotoDefinition \
  -H "Content-Type: application/json" \
  -d '{"queries": [{"mainResearchGoal": "Find def", "researchGoal": "Locate", "reasoning": "Test", "uri": "file:///path/file.ts", "symbolName": "createServer", "lineHint": 20}]}'
```

### HTTP Endpoints

| Category | Endpoint | Method | Description |
|----------|----------|--------|-------------|
| **Health** | \`/health\` | GET | Server health + circuit states |
| **Discovery** | \`/tools/list\` | GET | List all tools (concise) |
| | \`/tools/info/:name\` | GET | Get tool schema + hints |
| | \`/tools/system\` | GET | Get system prompt |
| | \`/prompts/list\` | GET | List all prompts |
| | \`/prompts/info/:name\` | GET | Get prompt details |
| **Execution** | \`/tools/call/:toolName\` | POST | **Execute any tool** |

### Available Tools (via \`/tools/call/:toolName\`)

| Tool Name | Category | Description |
|-----------|----------|-------------|
| \`localSearchCode\` | Local | Search code with ripgrep |
| \`localGetFileContent\` | Local | Read local file content |
| \`localFindFiles\` | Local | Find files by pattern/metadata |
| \`localViewStructure\` | Local | View local directory tree |
| \`lspGotoDefinition\` | LSP | Go to symbol definition |
| \`lspFindReferences\` | LSP | Find all symbol references |
| \`lspCallHierarchy\` | LSP | Get call hierarchy |
| \`ghSearchCode\` | GitHub | Search code in GitHub repos |
| \`ghGetFileContent\` | GitHub | Read file from GitHub repo |
| \`ghViewRepoStructure\` | GitHub | View GitHub repo tree |
| \`ghSearchRepos\` | GitHub | Search GitHub repositories |
| \`ghSearchPRs\` | GitHub | Search pull requests |
| \`npmSearch\` | Package | Search npm packages |

### Resilience Configuration

| Service | Max Attempts | Initial Delay | Max Delay | Backoff |
|---------|--------------|---------------|-----------|---------|
| GitHub | 3 | 1000ms | 30000ms | 3x |
| LSP | 3 | 500ms | 5000ms | 2x |
| Local | 2 | 100ms | 1000ms | 2x |
| Package | 3 | 1000ms | 15000ms | 2x |

### Circuit Breaker States

```
CLOSED (normal) ──[3 failures]──► OPEN (reject all)
       ▲                              │
       │                         [30s timeout]
       │                              │
       └──[1 success]─── HALF-OPEN ◄──┘
                          (probe)
```

**Per-service configuration:**
- **LSP**: 3 failures, 1 success, 10s timeout
- **GitHub**: 2 failures, 1 success, 60s timeout

---

## Files Summary

| File | Purpose |
|------|---------|
| `ecosystem.config.cjs` | **PM2 configuration** - restart strategies, memory limits, ready signal |
| `src/server.ts` | Express server, route mounting (`/tools`, `/prompts`), graceful shutdown |
| `src/index.ts` | Re-exports from octocode-mcp |
| `src/mcpCache.ts` | Tool metadata caching |
| `src/routes/tools.ts` | **Main API** - `/tools/call/:toolName` and discovery |
| `src/routes/prompts.ts` | Prompt discovery |
| `src/routes/local.ts` | Handler logic (used by tools.ts registry) |
| `src/routes/lsp.ts` | Handler logic (used by tools.ts registry) |
| `src/routes/github.ts` | Handler logic (used by tools.ts registry) |
| `src/routes/package.ts` | Handler logic (used by tools.ts registry) |
| `src/middleware/*.ts` | Logging, validation, error handling |
| `src/utils/*.ts` | Resilience, formatting, parsing |
| `src/validation/schemas.ts` | Zod validation schemas |
| `scripts/server.js` | Bundled server (tsdown output) |

---

*Created by Octocode Research Skill v2.1.0*
*Last validated: 2026-01-20*
