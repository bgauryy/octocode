# Session Persistence

> Technical documentation for the session storage system in `octocode-shared`.

## Overview

The session module provides persistent session state with deferred writes, in-memory caching, and automatic exit handlers. It tracks session lifecycle and usage statistics across MCP server runs.

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          SESSION STORAGE FLOW                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│    Application                                                               │
│        │                                                                     │
│        ▼                                                                     │
│  ┌─────────────────┐                                                         │
│  │  getOrCreate    │──────┐                                                  │
│  │    Session()    │      │                                                  │
│  └─────────────────┘      │                                                  │
│                           ▼                                                  │
│                    ┌──────────────┐     ┌──────────────┐                    │
│                    │ In-Memory    │◀───▶│  Disk Files  │                    │
│                    │   Cache      │     │ session/stats│                    │
│                    └──────┬───────┘     └──────────────┘                    │
│                           │                    ▲                             │
│                           │                    │                             │
│  ┌─────────────────┐      │                    │                             │
│  │  updateStats()  │──────┘                    │                             │
│  │ incrementXxx()  │                           │                             │
│  └─────────────────┘                           │                             │
│                                                │                             │
│  ┌─────────────────────────────────────────────┴────────────────────────┐   │
│  │                      Deferred Write System                           │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐   │   │
│  │  │ Dirty Flag   │  │ Flush Timer  │  │ Exit Handlers            │   │   │
│  │  │ (isDirty)    │  │ (60s cycle)  │  │ SIGINT, SIGTERM, exit    │   │   │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Session Data Structure

### File Location

```
~/.octocode/session.json
~/.octocode/stats.json
```

### Schema

```typescript
interface PersistedSession {
  version: 1;              // Schema version for migrations
  sessionId: string;       // UUID v4 identifier
  createdAt: string;       // ISO 8601 timestamp
  lastActiveAt: string;    // Updated on every interaction
}

interface PersistedStats {
  version: 1;              // Schema version for migrations
  stats: SessionStats;
}

interface SessionStats {
  toolCalls: number;       // MCP tool invocations
  promptCalls: number;     // MCP prompt invocations  
  errors: number;          // Error count
  rateLimits: number;      // Provider API rate-limit encounters
  rateLimitsByProvider?: Record<string, number>;
  charsSavedByTool?: Record<string, ToolCharSavingsStats>;
  githubCacheHits?: {
    hits: Record<string, number>;
    rateLimits: number;
  };
  packageRegistryFailures?: Record<string, number>;
  totalUsage?: SessionTotalUsageStats;
}
```

### Example Session File

```json
{
  "version": 1,
  "sessionId": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "createdAt": "2025-01-11T10:00:00.000Z",
  "lastActiveAt": "2025-01-11T15:30:45.123Z"
}
```

### Example Stats File

```json
{
  "version": 1,
  "stats": {
    "toolCalls": 142,
    "promptCalls": 3,
    "errors": 2,
    "rateLimits": 0,
    "rateLimitsByProvider": {},
    "charsSavedByTool": {},
    "githubCacheHits": {
      "hits": {},
      "rateLimits": 0
    },
    "packageRegistryFailures": {},
    "totalUsage": {
      "toolCalls": 142,
      "promptCalls": 3,
      "errors": 2,
      "rateLimits": 0,
      "rateLimitsByProvider": {},
      "rawChars": 0,
      "responseChars": 0,
      "savedChars": 0,
      "charSavingsCalls": 0,
      "githubCacheHits": 0,
      "githubCacheRateLimits": 0,
      "packageRegistryFailures": 0,
      "packageRegistryFailuresByRegistry": {}
    }
  }
}
```

---

## Deferred Write System

### Problem

Writing to disk on every stat increment would:
- Create excessive I/O operations
- Cause performance degradation
- Wear out SSDs unnecessarily

### Solution: Write Batching

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│ Update 1    │    │ Update 2    │    │ Update 3    │
│ (in-memory) │    │ (in-memory) │    │ (in-memory) │
└──────┬──────┘    └──────┬──────┘    └──────┬──────┘
       │                  │                  │
       │                  │                  │
       ▼                  ▼                  ▼
┌─────────────────────────────────────────────────────┐
│              In-Memory Cache (isDirty = true)       │
└─────────────────────────────────────────────────────┘
                          │
                          │ Every 60 seconds OR on exit
                          ▼
┌─────────────────────────────────────────────────────┐
│                 Batched Disk Write                  │
│        session.json + stats.json under ~/.octocode    │
└─────────────────────────────────────────────────────┘
```

### Implementation

```typescript
const FLUSH_INTERVAL_MS = 60_000; // 60 seconds
let cachedSession: PersistedSession | null = null;
let isDirty = false;
let flushTimer: ReturnType<typeof setInterval> | null = null;

// Timer-based flush (non-blocking, unref'd)
function startFlushTimer(): void {
  if (flushTimer) return;
  flushTimer = setInterval(() => {
    if (isDirty && cachedSession) {
      writeSessionToDisk(cachedSession);
      isDirty = false;
    }
  }, FLUSH_INTERVAL_MS);
  flushTimer.unref();  // Don't keep process alive
}
```

---

## Exit Handlers

To prevent data loss on process termination, synchronous flush handlers are registered:

```typescript
function registerExitHandlers(): void {
  // Normal exit
  process.on('exit', () => flushSessionSync());
  
  // Ctrl+C
  process.once('SIGINT', () => flushSessionSync());
  
  // Kill signal
  process.once('SIGTERM', () => flushSessionSync());
}
```

### Why `once()` for Signals?

Signals use `once()` to allow the default handler to run after our cleanup:

```
User presses Ctrl+C
        │
        ▼
┌───────────────────┐
│ Our SIGINT handler│
│ flushSessionSync()│
└─────────┬─────────┘
          │
          │ Handler removed (used once)
          ▼
┌───────────────────┐
│ Default handler   │
│ (process exits)   │
└───────────────────┘
```

---

## Atomic Writes

To prevent corruption from partial writes, we use a rename-based atomic write:

```typescript
function writeSessionToDisk(session: PersistedSession): void {
  ensureOctocodeDir();
  
  const tempFile = `${SESSION_FILE}.tmp`;
  
  // Step 1: Write to temp file
  writeFileSync(tempFile, JSON.stringify(session, null, 2), {
    mode: 0o600,
  });
  
  // Step 2: Atomic rename
  renameSync(tempFile, SESSION_FILE);
}
```

**Why This Works**:
- `rename()` is atomic on POSIX systems
- Either the old file exists, or the new one does
- No partial writes visible

---

## Session Lifecycle

### Creating/Retrieving Sessions

```typescript
// Get existing or create new
const session = getOrCreateSession();

// Force new session (ignore existing)
const freshSession = getOrCreateSession({ forceNew: true });
```

### Session ID

```typescript
const sessionId = getSessionId();
// Returns: "a1b2c3d4-e5f6-7890-abcd-ef1234567890" or null
```

### Deleting Sessions

```typescript
const deleted = deleteSession();
// Clears cache, stops timer, unregisters handlers, deletes file
```

---

## Statistics Tracking

### Increment Functions

```typescript
// Increment by 1 (default)
incrementToolCalls();
incrementPromptCalls();
incrementErrors();
incrementRateLimits();
incrementRateLimitByProvider('gitlab');
incrementPackageRegistryFailures('npm');

// Increment by N
incrementToolCalls(5);
```

### Batch Update

```typescript
updateSessionStats({
  toolCalls: 10,
  errors: 1,
});
// Adds to existing counts (doesn't replace)
```

### Reset Statistics

```typescript
resetSessionStats();
// Sets all counters to 0, keeps session ID
```

---

## API Reference

### Session Management

| Function | Purpose | Returns |
|----------|---------|---------|
| `getOrCreateSession(opts?)` | Get or create session | `PersistedSession` |
| `readSession()` | Read current session | `PersistedSession \| null` |
| `writeSession(session)` | Write session (deferred) | `void` |
| `deleteSession()` | Delete session completely | `boolean` |
| `getSessionId()` | Get current session ID | `string \| null` |

### Flush Control

| Function | Purpose | Returns |
|----------|---------|---------|
| `flushSession()` | Async flush to disk | `void` |
| `flushSessionSync()` | Sync flush (for exit handlers) | `void` |

### Statistics

| Function | Purpose | Returns |
|----------|---------|---------|
| `updateSessionStats(updates)` | Batch update stats | `SessionUpdateResult` |
| `incrementToolCalls(n?)` | Increment tool calls | `SessionUpdateResult` |
| `incrementPromptCalls(n?)` | Increment prompt calls | `SessionUpdateResult` |
| `incrementErrors(n?)` | Increment error count | `SessionUpdateResult` |
| `incrementRateLimits(n?)` | Increment global provider rate-limit count | `SessionUpdateResult` |
| `incrementRateLimitByProvider(provider, n?)` | Increment global and per-provider rate-limit counts | `SessionUpdateResult` |
| `incrementPackageRegistryFailures(registry, n?)` | Increment package-registry failure counts separately from rate limits | `SessionUpdateResult` |
| `resetSessionStats()` | Reset all stats to 0 | `SessionUpdateResult` |

### Constants

| Export | Value | Purpose |
|--------|-------|---------|
| `SESSION_FILE` | `~/.octocode/session.json` | Session file path |
| `STATS_FILE` | `~/.octocode/stats.json` | Stats file path |

---

## Performance Characteristics

| Operation | Time Complexity | I/O |
|-----------|-----------------|-----|
| `readSession()` (cached) | O(1) | None |
| `readSession()` (cold) | O(1) | 1 read |
| `incrementXxx()` | O(1) | None (deferred) |
| `flushSession()` | O(1) | 1 write |
| `getOrCreateSession()` | O(1) | 0-1 read, 1 write |

### Memory Usage

- Session cache: ~500 bytes per session
- No memory growth over time (single session per process)

---

## Validation & Error Handling

### Schema Validation

```typescript
function readSessionFromDisk(): PersistedSession | null {
  const content = readFileSync(SESSION_FILE, 'utf8');
  const session = JSON.parse(content);
  
  // Version check
  if (session.version !== CURRENT_VERSION) {
    return null;  // Incompatible version
  }
  
  // Required fields
  if (!session.sessionId || !session.createdAt) {
    return null;  // Invalid session
  }
  
  return session;
}
```

### Error Recovery

| Scenario | Behavior |
|----------|----------|
| File doesn't exist | Creates new session |
| Invalid JSON | Creates new session |
| Wrong version | Creates new session |
| Missing fields | Creates new session |
| Write failure | Logs error, continues |

---

## Integration Example

```typescript
import {
  getOrCreateSession,
  incrementToolCalls,
  incrementErrors,
  getSessionId,
} from 'octocode-shared/session';

// At MCP server startup
const session = getOrCreateSession();
console.log(`Session ID: ${session.sessionId}`);

// On each tool call
async function handleToolCall(name: string) {
  incrementToolCalls();
  try {
    // ... execute tool
  } catch (error) {
    incrementErrors();
    throw error;
  }
}

// Session is automatically flushed on exit
// No manual cleanup needed
```

---

## Testing Utilities

For testing, internal reset functions are exported:

```typescript
import { _resetSessionState } from 'octocode-shared/session';

beforeEach(() => {
  _resetSessionState();  // Clears cache, stops timer, unregisters handlers
});
```

---

## Related Documentation

- [CREDENTIALS_ARCHITECTURE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/docs/CREDENTIALS_ARCHITECTURE.md) - Credential storage system
- [API_REFERENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/docs/API_REFERENCE.md) - Complete API documentation

---

*Part of [octocode-shared](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/README.md) - Shared utilities for Octocode packages*
