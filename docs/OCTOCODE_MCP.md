# Octocode MCP Server

The Octocode MCP server exposes Octocode's research tools to AI coding clients through the Model Context Protocol over stdio. It is intentionally thin: the server registers schemas and transports requests, while the actual tool behavior lives in `@octocodeai/octocode-tools-core` and native primitives live in `@octocodeai/octocode-engine`.

Use this page for the MCP mental model, startup lifecycle, client configuration entry points, and session persistence. Use [Octocode Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) for every tool, [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) for settings, and [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) for GitHub tokens and encrypted credential storage.

## What MCP Adds

MCP gives assistants a stable tool catalog instead of asking them to shell out manually. In Octocode, MCP and CLI share the same schemas, runners, security validation, response envelope, pagination, and secret redaction path. That means a query researched through an assistant and a query run through `npx octocode tools ...` exercise the same core implementation.

| Layer | Responsibility |
|-------|----------------|
| MCP server | stdio lifecycle, tool registration, client-facing descriptions, output sanitization boundary |
| Tools core | GitHub/package/local/LSP/OQL runners, credentials, config, session, pagination, response shaping |
| Engine | native ripgrep, structural AST search, minify/signatures, binary inspection, secret scan, LSP orchestration |

## Quick Start

Install through the CLI helper when possible:

```bash
npx octocode install --ide cursor
```

Or configure an MCP client directly. Pick the package that matches the version you want:

**New Octocode (Rust-powered engine)** — use `@octocodeai/mcp`:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "@octocodeai/mcp@latest"]
    }
  }
}
```

**Classic octocode-mcp** — use `octocode-mcp`:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"]
    }
  }
}
```

Set tokens through environment variables or run `npx octocode auth login`. Do not put tokens in `.octocoderc`; see [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md).

## Startup Lifecycle

The MCP entrypoint follows this order:

```text
initialize
  -> configureSecurity
  -> initializeProviders
  -> loadToolContent
  -> initializeSession
  -> register tools
  -> stdio connect
```

At startup, Octocode reads configuration from environment variables and `<octocode-home>/.octocoderc`, initializes local security and provider clients, loads tool metadata from `@octocodeai/octocode-core`, opens the session store, and registers the final enabled tool set. Actual GitHub token lookup is live per request, so changing an env token can affect the next API call even though the startup status log keeps its original token-source snapshot.

## Tool Catalog

The MCP server registers the same 14 research tools exposed by the raw CLI tool runner:

| Family | Tools |
|--------|-------|
| GitHub | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`, `ghHistoryResearch`, `ghCloneRepo` |
| Package | `npmSearch` |
| Local | `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent`, `localBinaryInspect` |
| LSP | `lspGetSemantics` |
| OQL | `oqlSearch` |

Every tool accepts bulk input via `queries` with up to 5 items. Responses use a structured bulk envelope with per-query success, empty, and error states, plus pagination hints when more content is available. See [Octocode Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md).

## Configuration And Auth

Use environment variables for per-client or per-project settings. Use `<octocode-home>/.octocoderc` for machine-level defaults. Environment variables win over file values.

Important MCP settings:

| Setting | Why it matters |
|---------|----------------|
| `GITHUB_TOKEN` / `GH_TOKEN` / `OCTOCODE_TOKEN` | GitHub API auth. |
| `GITHUB_API_URL` | GitHub Enterprise API endpoint. |
| `ENABLE_LOCAL` | Enables local filesystem and LSP tools; defaults on. |
| `ENABLE_CLONE` | Enables `ghCloneRepo` and directory materialization for MCP; must be `true` for clone workflows. |
| `TOOLS_TO_RUN`, `ENABLE_TOOLS`, `DISABLE_TOOLS` | Control which tools the MCP server registers. |
| `WORKSPACE_ROOT`, `ALLOWED_PATHS` | Bound local path resolution and validation. |

Full details live in [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

## Session Persistence

`@octocodeai/octocode-tools-core/session` keeps lightweight runtime identity and usage stats across Octocode runs. It is intentionally small: one in-memory session, deferred disk writes, and synchronous flush on process exit.

### Storage

| File | Purpose | Notes |
|------|---------|-------|
| `<octocode-home>/session.json` | Session identity | `version`, `sessionId`, `createdAt`, `lastActiveAt`. |
| `<octocode-home>/stats.json` | Usage counters | Tool calls, errors, rate limits, char savings, cache hits, package registry failures. |

`OCTOCODE_HOME` changes the base directory for both files. Without it, Octocode uses the platform default: macOS `~/.octocode`, Windows `%APPDATA%\octocode`, Linux `${XDG_CONFIG_HOME:-~/.config}/octocode`.

### Data Model

```ts
interface PersistedSession {
  version: 1;
  sessionId: string;
  createdAt: string;
  lastActiveAt: string;
  stats: SessionStats;
}
```

The runtime object includes `stats`; disk storage splits stats into `stats.json` so session identity can remain compact.

### Write Strategy

1. Read session once and keep it in memory.
2. Mark the cache dirty when stats or timestamps change.
3. Flush dirty state every 60 seconds with an `unref()` timer.
4. Flush synchronously on `exit`, `SIGINT`, and `SIGTERM`.
5. Write JSON through a temp file and atomic `rename()`.

This avoids writing on every counter increment while still preserving data on normal shutdown.

### Public Operations

| API | Behavior |
|-----|----------|
| `getOrCreateSession({ forceNew? })` | Reads existing session or creates a new UUID session. |
| `getSessionId()` | Returns cached session id, or `null` if no session is loaded. |
| `updateSessionStats(partial)` | Adds counters to current stats and updates `lastActiveAt`. |
| `incrementToolCalls`, `incrementErrors`, `incrementRateLimits` | Convenience counter increments. |
| `incrementRateLimitByProvider(provider)` | Tracks provider-specific rate limits. |
| `incrementToolCharSavings(tool, rawChars, responseChars)` | Tracks raw/response/saved char totals. |
| `incrementGitHubCacheHits`, `incrementGitHubCacheRateLimits` | Tracks GitHub cache behavior. |
| `incrementPackageRegistryFailures(registry)` | Tracks package-registry failure counts. |
| `resetSessionStats()` | Resets counters but keeps the session id. |
| `flushSession()` / `flushSessionSync()` | Writes dirty cache to disk. |
| `deleteSession()` | Clears cache and deletes session/stat files. |

Testing helper: `_resetSessionState()` clears cache, timer, and exit handlers.

### Failure Behavior

| Scenario | Behavior |
|----------|----------|
| Missing session file | Create a new session. |
| Invalid session JSON/schema | Ignore the file and create a new session. |
| Missing or invalid stats file | Use default zeroed stats. |
| Write failure during normal flush | Error is logged by caller context when surfaced. |
| Write failure during exit flush | Suppressed so shutdown can continue. |

### Design Rules

- Do not write session files directly from consumers.
- Prefer increment helpers over manually building stats updates.
- Keep stats additive; `updateSessionStats` adds to current counters.
- Call `flushSession()` in explicit shutdown paths when possible.
- Use `_resetSessionState()` in tests that touch session state.

### Related Documentation

- [Credentials Architecture](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md#credential-architecture-api)
- [Tools Core package](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/README.md)

## See Also

- [Octocode Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)
- [Octocode CLI Guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)
