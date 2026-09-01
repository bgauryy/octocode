# Octocode MCP server

The Octocode MCP server exposes Octocode's research tools to AI coding clients through the Model Context Protocol over stdio. It is intentionally thin: the server registers schemas and transports requests, while tool behavior lives in `@octocodeai/octocode-tools-core` and native primitives live in `@octocodeai/octocode-engine`.

Use this page for the MCP mental model, startup lifecycle, client configuration entry points, and session persistence. For every tool, see [Octocode tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md). For settings, GitHub tokens, and encrypted credential storage, see [Octocode configuration and authentication](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

## What MCP adds

MCP gives assistants a stable tool catalog instead of making them shell out by hand. In Octocode, MCP and CLI share the same schemas, runners, security validation, response envelope, pagination, and secret redaction path. A query researched through an assistant and a query run through `npx octocode tools …` exercise the same core implementation.

| Layer | Responsibility |
|-------|----------------|
| MCP server | stdio lifecycle, tool registration, client-facing descriptions, output sanitization boundary |
| Tools core | GitHub/package/local/LSP runners, credentials, config, session, pagination, response shaping |
| Engine | native ripgrep, structural AST search, minify/signatures, secret scan, LSP orchestration |

## Quick start

Install through the CLI helper when you can:

```bash
npx octocode install --ide cursor
```

Otherwise, configure an MCP client directly to run `octocode-mcp`:

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

Set tokens through environment variables or run `npx octocode auth login`. Don't put tokens in `.octocoderc`. For more information, see the [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#authentication) section of the configuration reference.

## Startup lifecycle

The MCP entrypoint runs these steps in order:

```text
initialize
  -> configureSecurity
  -> initializeProviders
  -> loadToolContent
  -> initializeSession
  -> register tools
  -> stdio connect
```

At startup, Octocode reads configuration from environment variables and `<octocode-home>/.octocoderc`, initializes local security and provider clients, loads repository-owned tool metadata plus the external shared system prompt, opens the session store, and registers the final enabled tool set. Octocode looks the GitHub token up live on every request, so changing an environment token can affect the next API call even though the startup status log keeps its original token-source snapshot.

## Tool catalog

With no environment variables set, the MCP server registers 10 tools:

| Family | Tools |
|--------|-------|
| GitHub | `ghSearch`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` |
| Local | `localSearch`, `localGetFileContent`, `localAnalyzeGraph`, `lspGetSemantics` |
| Package | `npmSearch` |

To read the live CLI catalog, run `octocode tools --json`.

`ghSearch` is the sole GitHub discovery entry point. Its strict
`operation: "code" | "repositories" | "tree"` branches reject fields from
other operations and removed compatibility names cannot be re-enabled.

Every tool accepts bulk input through `queries`, with up to 5 items per call. Responses use a structured bulk envelope with per-query success, empty, and error states, plus pagination hints when more content is available. For more information, see the [Octocode tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md).

## Configuration and auth

Use environment variables for per-client or per-project settings. Use `<octocode-home>/.octocoderc` for machine-level defaults. Environment variables win over file values.

The following table lists the settings that matter most for MCP:

| Setting | Why it matters |
|---------|----------------|
| `GITHUB_TOKEN` / `GH_TOKEN` / `OCTOCODE_TOKEN` | GitHub API auth. |
| `GITHUB_API_URL` | GitHub Enterprise API endpoint. |
| `ENABLE_LOCAL` | Turns local filesystem and LSP tools on or off. Defaults to `true`; set it to `false` to disable them. |
| `ENABLE_CLONE` | Controls `ghCloneRepo` and directory materialization. Defaults to `true`; set it to `false` to disable clone workflows. |
| `TOOLS_TO_RUN`, `DISABLE_TOOLS` | Control which tools the MCP server registers. |
| `WORKSPACE_ROOT`, `ALLOWED_PATHS` | Bound local path resolution and validation. |

For full details, see the [Octocode configuration and authentication](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) reference.

## Materialization and response cache

The MCP server shares the same on-disk cache as the CLI under the configured Octocode home:

| Bucket | Path | Contents |
|---|---|---|
| Clone | `tmp/clone/{owner}/{repo}/{branch}` | Reusable Git checkouts |
| Tree | `tmp/tree/{owner}/{repo}/{commitSha}` | Materialized repository trees |
| Response | `tmp/response/` | Eligible GitHub and npm response payloads |

Initialization performs a persisted maintenance due-check. After the transport connects, MCP schedules the next persisted deadline with an unreferenced timer, so the timer does not keep the process alive. A cross-process lock prevents concurrent sweeps when CLI and MCP processes start together. Cleanup is best effort and is cancelled during shutdown; a cleanup failure does not block server startup or tool execution.

Maintenance removes expired owned cache entries while preserving unrelated files under `tmp`. See [Cache storage and lifecycle](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#cache-storage-and-lifecycle) for the 24-hour gate, expiry rules, limits, and manual controls, and [Cache behavior](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#cache-behavior) for tool-level semantics.

## Session persistence

`@octocodeai/octocode-tools-core/session` keeps lightweight runtime identity and usage stats across Octocode runs. It stays small: one in-memory session, deferred disk writes, and a synchronous flush on process exit.

### Storage

| File | Purpose | Notes |
|------|---------|-------|
| `<octocode-home>/session.json` | Session identity | `version`, `sessionId`, `createdAt`, `lastActiveAt`. |
| `<octocode-home>/stats.json` | Usage counters | Tool calls, errors, rate limits, char savings, cache hits, package registry failures. |

`OCTOCODE_HOME` changes the base directory for both files. Without it, Octocode uses `.octocode` inside the OS home directory on every platform: `~/.octocode` on macOS and Linux, `%USERPROFILE%\.octocode` on Windows.

### Data model

```ts
interface PersistedSession {
  version: 1;
  sessionId: string;
  createdAt: string;
  lastActiveAt: string;
  stats: SessionStats;
}
```

The runtime object includes `stats`. On disk, Octocode splits stats into `stats.json` so session identity stays compact.

### Write strategy

1. Read session once and keep it in memory.
2. Mark the cache dirty when stats or timestamps change.
3. Flush dirty state every 60 seconds with an `unref()` timer.
4. Flush synchronously on `exit`, `SIGINT`, and `SIGTERM`.
5. Write JSON through a temp file and atomic `rename()`.

This avoids a write on every counter increment while still preserving data on normal shutdown.

### Public operations

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

Testing helper: `_resetSessionState()` clears the cache, the timer, and the exit handlers.

### Failure behavior

| Scenario | Behavior |
|----------|----------|
| Missing session file | Create a new session. |
| Invalid session JSON/schema | Ignore the file and create a new session. |
| Missing or invalid stats file | Use default zeroed stats. |
| Write failure during normal flush | The calling context logs the error when it surfaces. |
| Write failure during exit flush | Octocode suppresses the error so shutdown continues. |

### Design rules

- Do not write session files directly from consumers.
- Prefer increment helpers over manually building stats updates.
- Keep stats additive; `updateSessionStats` adds to current counters.
- Call `flushSession()` in explicit shutdown paths when possible.
- Use `_resetSessionState()` in tests that touch session state.

### Related documentation

- [Token priority order](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#token-priority-order)
- [Tools core package](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/README.md)

## See also

- [Octocode tools reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
- [Octocode configuration and authentication](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [Octocode CLI guide](https://github.com/bgauryy/octocode/blob/main/packages/octocode/docs/OCTOCODE_CLI.md)
- [Security](https://github.com/bgauryy/octocode/blob/main/docs/SECURITY.md)
