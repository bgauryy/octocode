# Session Persistence

`octocode-shared/session` keeps lightweight runtime identity and usage stats across Octocode runs. It is intentionally small: one in-memory session, deferred disk writes, and synchronous flush on process exit.

## Storage

| File | Purpose | Notes |
|------|---------|-------|
| `~/.octocode/session.json` | Session identity | `version`, `sessionId`, `createdAt`, `lastActiveAt`. |
| `~/.octocode/stats.json` | Usage counters | Tool calls, errors, rate limits, char savings, cache hits, package registry failures. |

`OCTOCODE_HOME` changes the base directory for both files.

## Data Model

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

## Write Strategy

1. Read session once and keep it in memory.
2. Mark the cache dirty when stats or timestamps change.
3. Flush dirty state every 60 seconds with an `unref()` timer.
4. Flush synchronously on `exit`, `SIGINT`, and `SIGTERM`.
5. Write JSON through a temp file and atomic `rename()`.

This avoids writing on every counter increment while still preserving data on normal shutdown.

## Public Operations

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

## Failure Behavior

| Scenario | Behavior |
|----------|----------|
| Missing session file | Create a new session. |
| Invalid session JSON/schema | Ignore the file and create a new session. |
| Missing or invalid stats file | Use default zeroed stats. |
| Write failure during normal flush | Error is logged by caller context when surfaced. |
| Write failure during exit flush | Suppressed so shutdown can continue. |

## Design Rules

- Do not write session files directly from consumers.
- Prefer increment helpers over manually building stats updates.
- Keep stats additive; `updateSessionStats` adds to current counters.
- Call `flushSession()` in explicit shutdown paths when possible.
- Use `_resetSessionState()` in tests that touch session state.

## Related Documentation

- [Credentials Architecture](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/CREDENTIALS_ARCHITECTURE.md)
- [octocode-shared README](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/README.md)
