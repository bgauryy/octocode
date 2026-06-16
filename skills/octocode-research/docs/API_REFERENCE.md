# Octocode Research API Reference

> HTTP API on `localhost:1987` by default (configurable via `OCTOCODE_RESEARCH_HOST` / `OCTOCODE_RESEARCH_PORT`) - All tools via POST `/tools/call/:toolName`
> **v2.1.0** - Process managed by PM2

## Quick Start

```bash
# Development
npm run dev                                  # Start with hot reload

# Production (PM2)
npm run pm2:start                            # Start with PM2
npm run pm2:logs                             # View logs
npm run pm2:monit                            # Dashboard

# Health & Discovery
curl http://localhost:1987/health            # Health check
curl http://localhost:1987/tools/list        # List tools
curl http://localhost:1987/tools/info/localSearchCode  # Get schema (BEFORE calling!)
```

---

## Routes Overview

| Route | Method | Purpose |
|-------|--------|---------|
| `/health` | GET | Server health & circuit status |
| `/tools/list` | GET | List all 13 tools (concise) |
| `/tools/info` | GET | All tools with details |
| `/tools/info/:toolName` | GET | Specific tool schema (**call before using!**) |
| `/tools/system` | GET | System instructions (**load first**) |
| `/tools/metadata` | GET | Raw metadata summary |
| `/tools/call/:toolName` | POST | Execute any tool via JSON body |
| `/prompts/list` | GET | List all prompts |
| `/prompts/info/:promptName` | GET | Specific prompt content |

---

## Health & Discovery

### `GET /health`
```bash
curl http://localhost:1987/health
```
Response:
```json
{
  "status": "ok",
  "port": 1987,
  "version": "2.1.0",
  "uptime": 123,
  "processManager": "pm2",
  "memory": { "heapUsed": 27, "heapTotal": 42, "rss": 132 },
  "circuits": {},
  "errors": { "queueSize": 0, "recentErrors": [] }
}
```

| Field | Description |
|-------|-------------|
| `status` | `"ok"` when ready, `"initializing"` during startup |
| `processManager` | Always `"pm2"` - server is managed by PM2 |
| `uptime` | Seconds since process start |
| `memory` | Heap and RSS memory usage in MB |
| `circuits` | Circuit breaker states (empty when healthy) |
| `errors` | Recent error queue for debugging |

### `GET /tools/list`
```bash
curl http://localhost:1987/tools/list
```

### `GET /tools/info`
All tools with optional details:
```bash
curl "http://localhost:1987/tools/info?schema=true&hints=true"
```

### `GET /tools/info/:toolName`
**⚠️ Call this BEFORE using any tool!**
```bash
curl http://localhost:1987/tools/info/localSearchCode
```

### `GET /tools/system`
**Load this FIRST into agent context:**
```bash
curl http://localhost:1987/tools/system
```

### `GET /tools/metadata`
```bash
curl http://localhost:1987/tools/metadata
```

### `GET /prompts/list`
```bash
curl http://localhost:1987/prompts/list
```

### `GET /prompts/info/:promptName`
```bash
curl http://localhost:1987/prompts/info/research
```

---

## Calling Tools

All tools are called via **POST `/tools/call/:toolName`** with a JSON body.

### Request Format
```json
{
  "queries": [{
    "mainResearchGoal": "High-level goal",
    "researchGoal": "Specific goal for this query",
    "reasoning": "Why this query helps",
    ...toolSpecificParams
  }]
}
```

### Response Format
```json
{
  "tool": "toolName",
  "success": true,
  "data": { ... },
  "hints": ["Next step suggestions..."],
  "research": { "mainResearchGoal": "...", "researchGoal": "...", "reasoning": "..." }
}
```

### Limits
- Max 3 queries per request
- All queries require: `mainResearchGoal`, `researchGoal`, `reasoning`

---

## Local Tools

### localSearchCode

Search code with ripgrep.

| Param | Required | Description |
|-------|----------|-------------|
| `pattern` | ✅ | Search pattern |
| `path` | ✅ | Directory to search |
| `type` | | File type (ts, js, py) |
| `filesOnly` | | Return only file paths |
| `limit` | | Max results |

```bash
curl -X POST http://localhost:1987/tools/call/localSearchCode \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Find exports",
      "researchGoal": "Locate export statements",
      "reasoning": "Mapping module exports",
      "pattern": "export",
      "path": "/project",
      "filesOnly": true
    }]
  }'
```

---

### localGetFileContent

Read local file content.

| Param | Required | Description |
|-------|----------|-------------|
| `path` | ✅ | File path |
| `startLine` | | Start line number |
| `endLine` | | End line number |
| `matchString` | | Extract around pattern |

```bash
curl -X POST http://localhost:1987/tools/call/localGetFileContent \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Read file",
      "researchGoal": "Get index.ts content",
      "reasoning": "Understanding entry point",
      "path": "/project/src/index.ts",
      "startLine": 1,
      "endLine": 50
    }]
  }'
```

---

### localFindFiles

Find files by metadata.

| Param | Required | Description |
|-------|----------|-------------|
| `path` | ✅ | Directory to search |
| `name` | | Filename pattern (glob) |
| `type` | | "file" / "directory" |
| `modifiedWithin` | | Time filter: "1d", "2h" |

```bash
curl -X POST http://localhost:1987/tools/call/localFindFiles \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Find TypeScript files",
      "researchGoal": "List all .ts files",
      "reasoning": "Mapping source files",
      "path": "/project",
      "name": "*.ts",
      "type": "file"
    }]
  }'
```

---

### localViewStructure

View directory tree.

| Param | Required | Description |
|-------|----------|-------------|
| `path` | ✅ | Directory path |
| `depth` | | Tree depth (1-5) |
| `filesOnly` | | Show only files |
| `hidden` | | Include hidden files |

```bash
curl -X POST http://localhost:1987/tools/call/localViewStructure \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Explore structure",
      "researchGoal": "View src directory",
      "reasoning": "Understanding project layout",
      "path": "/project/src",
      "depth": 2
    }]
  }'
```

---

## LSP Tools

**Note:** All LSP tools require `lineHint` from `localSearchCode` results.

### lspGotoDefinition

Jump to symbol definition.

| Param | Required | Description |
|-------|----------|-------------|
| `uri` | ✅ | File path |
| `symbolName` | ✅ | Symbol name |
| `lineHint` | ✅ | Line number (1-indexed) |
| `contextLines` | | Lines of context (default: 5) |

```bash
curl -X POST http://localhost:1987/tools/call/lspGotoDefinition \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Find definition",
      "researchGoal": "Locate createServer",
      "reasoning": "Understanding function source",
      "uri": "/project/src/index.ts",
      "symbolName": "createServer",
      "lineHint": 10
    }]
  }'
```

---

### lspFindReferences

Find all symbol usages.

| Param | Required | Description |
|-------|----------|-------------|
| `uri` | ✅ | File path |
| `symbolName` | ✅ | Symbol name |
| `lineHint` | ✅ | Line number (1-indexed) |
| `includeDeclaration` | | Include definition (default: true) |
| `page` | | Pagination |

```bash
curl -X POST http://localhost:1987/tools/call/lspFindReferences \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Find usages",
      "researchGoal": "All handleRequest references",
      "reasoning": "Impact analysis",
      "uri": "/project/src/index.ts",
      "symbolName": "handleRequest",
      "lineHint": 25
    }]
  }'
```

---

### lspCallHierarchy

Trace function call relationships.

| Param | Required | Description |
|-------|----------|-------------|
| `uri` | ✅ | File path |
| `symbolName` | ✅ | Symbol name |
| `lineHint` | ✅ | Line number (1-indexed) |
| `direction` | ✅ | "incoming" (who calls) / "outgoing" (what it calls) |
| `depth` | | Call depth (1-3, default: 1) |

```bash
curl -X POST http://localhost:1987/tools/call/lspCallHierarchy \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Trace calls",
      "researchGoal": "Who calls handleRequest",
      "reasoning": "Understanding call flow",
      "uri": "/project/src/router.ts",
      "symbolName": "handleRequest",
      "lineHint": 42,
      "direction": "incoming"
    }]
  }'
```

---

## GitHub Tools

**Note:** Requires `GITHUB_TOKEN` environment variable.

### ghSearchCode

Search code across GitHub repositories.

| Param | Required | Description |
|-------|----------|-------------|
| `keywordsToSearch` | ✅ | Array of keywords |
| `owner` | | Repository owner |
| `repo` | | Repository name |
| `path` | | Path filter |
| `extension` | | File extension |

```bash
curl -X POST http://localhost:1987/tools/call/ghSearchCode \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Find React hooks",
      "researchGoal": "useState and useEffect usage",
      "reasoning": "Learning hook patterns",
      "keywordsToSearch": ["useState", "useEffect"],
      "owner": "facebook",
      "repo": "react"
    }]
  }'
```

---

### ghGetFileContent

Read file from GitHub repository.

| Param | Required | Description |
|-------|----------|-------------|
| `owner` | ✅ | Repository owner |
| `repo` | ✅ | Repository name |
| `path` | ✅ | File path |
| `branch` | | Branch name |
| `startLine` | | Start line |
| `endLine` | | End line |

```bash
curl -X POST http://localhost:1987/tools/call/ghGetFileContent \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Read React source",
      "researchGoal": "Get React.js entry",
      "reasoning": "Understanding React core",
      "owner": "facebook",
      "repo": "react",
      "path": "packages/react/src/React.js"
    }]
  }'
```

---

### ghSearchRepos

Search GitHub repositories.

| Param | Required | Description |
|-------|----------|-------------|
| `keywordsToSearch` | ✅* | Search keywords |
| `topicsToSearch` | ✅* | Topic tags |
| `owner` | | Filter by owner |
| `stars` | | Star filter: ">1000", "100..500" |
| `sort` | | "stars" / "forks" / "updated" |

*At least one of `keywordsToSearch` or `topicsToSearch` required.

```bash
curl -X POST http://localhost:1987/tools/call/ghSearchRepos \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Find CLI tools",
      "researchGoal": "TypeScript CLI repos",
      "reasoning": "Research CLI patterns",
      "topicsToSearch": ["typescript", "cli"],
      "stars": ">1000"
    }]
  }'
```

---

### ghViewRepoStructure

View repository tree structure.

| Param | Required | Description |
|-------|----------|-------------|
| `owner` | ✅ | Repository owner |
| `repo` | ✅ | Repository name |
| `branch` | ✅ | Branch name |
| `path` | | Subdirectory path |
| `depth` | | Tree depth (1-2) |

```bash
curl -X POST http://localhost:1987/tools/call/ghViewRepoStructure \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Explore React",
      "researchGoal": "View packages structure",
      "reasoning": "Understanding monorepo layout",
      "owner": "facebook",
      "repo": "react",
      "branch": "main",
      "path": "packages"
    }]
  }'
```

---

### ghSearchPRs

Search pull requests.

| Param | Required | Description |
|-------|----------|-------------|
| `owner` | | Repository owner |
| `repo` | | Repository name |
| `prNumber` | | Specific PR number |
| `state` | | "open" / "closed" |
| `merged` | | Filter merged PRs |
| `type` | | "metadata" / "fullContent" / "partialContent" |

```bash
curl -X POST http://localhost:1987/tools/call/ghSearchPRs \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Find merged PRs",
      "researchGoal": "Recent merged React PRs",
      "reasoning": "Understanding recent changes",
      "owner": "facebook",
      "repo": "react",
      "state": "closed",
      "merged": true,
      "limit": 5
    }]
  }'
```

---

## Package Search

### npmSearch

Search npm packages.

| Param | Required | Description |
|-------|----------|-------------|
| `name` | ✅ | Package name |
| `ecosystem` | | "npm" (default) / "python" |
| `searchLimit` | | Max results |

```bash
curl -X POST http://localhost:1987/tools/call/npmSearch \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "mainResearchGoal": "Find package",
      "researchGoal": "Get express info",
      "reasoning": "Checking package details",
      "name": "express",
      "ecosystem": "npm"
    }]
  }'
```

---

## Error Responses

| Code | Description |
|------|-------------|
| 400 | Validation error - check required params |
| 404 | Route/tool not found - check spelling |
| 503 | Circuit open - service temporarily unavailable |

### 404 Response Example
```json
{
  "success": false,
  "error": {
    "message": "Route not found: GET /invalidRoute",
    "code": "NOT_FOUND",
    "availableRoutes": [
      "GET  /health",
      "GET  /tools/list",
      "GET  /tools/info/:toolName",
      "GET  /tools/system",
      "POST /tools/call/:toolName",
      "GET  /prompts/list",
      "GET  /prompts/info/:promptName"
    ],
    "hint": "All tools are called via POST /tools/call/{toolName}"
  }
}
```

---

*Octocode Research v2.1.0*
