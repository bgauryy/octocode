<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">

  <h1>Octocode Research Skill</h1>

  <p>HTTP server wrapping 13 Octocode MCP tools with intent-based prompt selection</p>

  [![Skill](https://img.shields.io/badge/skill-agentskills.io-purple)](https://agentskills.io/what-are-skills)
  [![License](https://img.shields.io/badge/license-MIT-blue)](https://github.com/bgauryy/octocode-mcp/blob/main/LICENSE)
  [![Port](https://img.shields.io/badge/port-1987-green)](http://localhost:1987)

</div>

---

Unified HTTP interface for local filesystem, LSP, GitHub, and package registry tools. The agent sends a question, the server selects a prompt, and tool responses include hints that guide the next call.

https://github.com/user-attachments/assets/d1260dbc-e7b6-4bec-909f-232ebee91ce9

---

## Why a Server?

The server is not just a pass-through — it provides a persistent runtime layer that makes research faster and more reliable:

| Capability | What it does |
|---|---|
| **MCP Cache** | Loads tool metadata, schemas, and system prompt once at startup. Every request reads from memory instead of re-initializing the MCP client. |
| **Circuit Breakers** | Per-service circuits (GitHub search, GitHub content, LSP navigation, LSP hierarchy, local, package) stop cascading failures. An open circuit fails fast instead of waiting for timeouts. |
| **Retry + Backoff** | Transient errors (rate limits, LSP cold starts, busy files) retry automatically with exponential backoff before surfacing to the agent. |
| **Request Timeout** | Every tool call is wrapped with a hard timeout (30–60s) to prevent hangs from blocking the agent. |
| **Detached Daemon** | The server runs as a detached process — no client owns it. Multiple agents/IDEs share one server instance. A PID file at `~/.octocode/research-server-{PORT}.pid` enables explicit stop. |
| **Idle Self-Stop** | After 30 minutes of inactivity the server exits on its own, freeing memory. The next `server-init` call spawns a fresh instance. |
| **Session Telemetry** | Tracks tool calls, repos accessed, and error rates per session for diagnostics (de-identified, opt-out with `LOG=false`). |
| **Readiness Gate** | Routes return **503** during MCP init (~1–3s), so agents never get partial data. `npm start` polls until ready. |

---

## Quick Start

```bash
npx add-skill https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-research
```

> Requires GitHub auth. See [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md).

Or manually:

```bash
cd skills/octocode-research
npm install && npm start
curl http://localhost:1987/health
```

Override defaults with `OCTOCODE_RESEARCH_HOST` / `OCTOCODE_RESEARCH_PORT`.

---

## Prompt Selection

The server picks a prompt based on what the agent asks:

| Question | Prompt | Scope |
|---|---|---|
| "How does React useState work?" | `research` | GitHub repos, packages |
| "Trace auth flow in our app" | `research_local` | Local codebase via LSP |
| "Review PR #123" | `reviewPR` | Diff + code review |
| "Plan adding caching" | `plan` | Architecture + steps |

---

## API

### Discovery

**Load order:** Call **`GET /tools/initContext`** first — it returns the system prompt and all tool JSON Schemas in one response. Use **`GET /tools/system`** when you only need the instructions.

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/health` | Health, uptime, memory, circuit breaker states |
| `GET` | `/tools/initContext` | **Load first** — system prompt + all tool schemas |
| `GET` | `/tools/system` | System instructions only |
| `GET` | `/tools/list` | All 13 tools (concise) |
| `GET` | `/tools/info` | All tools with full descriptions |
| `GET` | `/tools/info/:toolName` | One tool — JSON Schema + description + hints |
| `GET` | `/tools/metadata` | Raw metadata: instruction summary, tool/prompt counts |
| `GET` | `/tools/schemas` | All tool JSON Schemas (bulk) |
| `GET` | `/prompts/list` | All 7 prompts |
| `GET` | `/prompts/info/:promptName` | Prompt content and arguments |

### Execution

**`POST /tools/call/:toolName`** — every tool uses this single endpoint.

```json
{
  "queries": [{
    "id": "unique-query-id",
    "mainResearchGoal": "Overall objective",
    "researchGoal": "This query's goal",
    "reasoning": "Why this approach"
  }]
}
```

- `id` is required on every query (alphanumeric, dots, underscores, dashes).
- `mainResearchGoal` is required for GitHub/package tools; local/LSP tools need `researchGoal` + `reasoning`.
- Up to **3 queries** per request (bulk). Responses include `hints` — follow them.

---

## Tools

| Tool | Category | What it does |
|---|---|---|
| `localSearchCode` | Local | Ripgrep code search |
| `localGetFileContent` | Local | Read file content |
| `localFindFiles` | Local | Find files by pattern/metadata |
| `localViewStructure` | Local | Directory tree |
| `lspGotoDefinition` | LSP | Jump to definition |
| `lspFindReferences` | LSP | All usages of a symbol |
| `lspCallHierarchy` | LSP | Incoming/outgoing calls |
| `ghSearchCode` | GitHub | Search code across repos |
| `ghGetFileContent` | GitHub | Read file from repo |
| `ghViewRepoStructure` | GitHub | Repo directory tree |
| `ghSearchRepos` | GitHub | Search repos |
| `ghSearchPRs` | GitHub | Search PRs |
| `npmSearch` | Package | npm lookup |

---

## Example

Find where React defines `useState`, then read it:

```bash
curl -X POST http://localhost:1987/tools/call/ghSearchCode \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "id": "find-useState",
      "mainResearchGoal": "Understand React useState implementation",
      "researchGoal": "Find useState source location",
      "reasoning": "Locate the hook definition first",
      "owner": "facebook",
      "repo": "react",
      "keywordsToSearch": ["useState", "function"],
      "match": "file"
    }]
  }'
```

```bash
curl -X POST http://localhost:1987/tools/call/ghGetFileContent \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [{
      "id": "read-useState",
      "mainResearchGoal": "Understand React useState implementation",
      "researchGoal": "Read useState source",
      "reasoning": "Found location, reading implementation",
      "owner": "facebook",
      "repo": "react",
      "path": "packages/react/src/ReactHooks.js",
      "matchString": "useState"
    }]
  }'
```

---

## Resilience

Per-service circuit breakers with retry and backoff:

| Circuit | Failure Threshold | Reset Timeout | Retries | Max Backoff |
|---|---|---|---|---|
| `github:search` | 2 | 60s | 3 | 30s |
| `github:content` | 3 | 30s | 3 | 30s |
| `github:pulls` | 2 | 60s | 3 | 30s |
| `lsp:navigation` | 3 | 10s | 3 | 5s |
| `lsp:hierarchy` | 2 | 15s | 3 | 5s |
| `local` | 5 | 5s | 2 | 1s |
| `package` | 3 | 45s | 3 | 15s |

---

## Privacy

Collects de-identified telemetry (usage counts, error rates). Never collects source code, env vars, or PII. Opt out: `export LOG=false`. Local logs at `~/.octocode/logs/` are not uploaded.

[Privacy Policy](https://github.com/bgauryy/octocode-mcp/blob/main/PRIVACY.md) &middot; [Terms](https://github.com/bgauryy/octocode-mcp/blob/main/TERMS.md)

---

## Docs

| Document | Description |
|---|---|
| [SKILL.md](./SKILL.md) | Agent workflow guide |
| [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) | Architecture |
| [docs/API_REFERENCE.md](./docs/API_REFERENCE.md) | Full API reference |
| [docs/FLOWS.md](./docs/FLOWS.md) | Request flow diagrams |

---

MIT &copy; 2026 Octocode &mdash; [LICENSE](https://github.com/bgauryy/octocode-mcp/blob/main/LICENSE)
