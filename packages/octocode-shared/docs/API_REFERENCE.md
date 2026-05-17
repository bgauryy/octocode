# API Reference

> Complete API documentation for all `octocode-shared` modules.

## Package Overview

`octocode-shared` provides five modules, available via separate entry points:

```typescript
// All exports
import { ... } from 'octocode-shared';

// Module-specific imports (subpath exports)
import { ... } from 'octocode-shared/credentials';
import { ... } from 'octocode-shared/platform';
import { ... } from 'octocode-shared/session';
import { ... } from 'octocode-shared/config';

// Logger is available via the main entry point only
import { createLogger, setLogHandler } from 'octocode-shared';
```

---

## Credentials Module

### Types

#### `OAuthToken`

OAuth token structure for GitHub authentication.

```typescript
interface OAuthToken {
  token: string;                     // Access token
  tokenType: 'oauth';                // Token type identifier
  scopes?: string[];                 // OAuth scopes (e.g., ['repo', 'read:user'])
  refreshToken?: string;             // Refresh token (for Octocode OAuth)
  expiresAt?: string;                // ISO 8601 expiration timestamp
  refreshTokenExpiresAt?: string;    // Refresh token expiration
}
```

#### `StoredCredentials`

Complete credentials record as stored.

```typescript
interface StoredCredentials {
  hostname: string;           // Normalized hostname (e.g., 'github.com')
  username: string;           // GitHub username
  token: OAuthToken;          // Token data
  gitProtocol: 'ssh' | 'https';
  createdAt: string;          // ISO 8601 creation timestamp
  updatedAt: string;          // ISO 8601 last update timestamp
}
```

#### `TokenSource`

Indicates where a token was resolved from.

```typescript
type TokenSource =
  | 'env:OCTOCODE_TOKEN'
  | 'env:GH_TOKEN'
  | 'env:GITHUB_TOKEN'
  
  | 'file'
  | 'gh-cli'
  | null;
```

#### `StoreResult`

Result from storing credentials.

```typescript
interface StoreResult {
  success: boolean;
}
```

#### `DeleteResult`

Result from deleting credentials.

```typescript
interface DeleteResult {
  success: boolean;
  deletedFromFile: boolean;
}
```

---

### Token Resolution Functions

#### `resolveTokenFull(options)`

**Complete token resolution with all fallbacks.** This is the recommended function for most use cases.

```typescript
async function resolveTokenFull(options?: {
  hostname?: string;
  clientId?: string;
  getGhCliToken?: GhCliTokenGetter;
}): Promise<FullTokenResolution | null>

interface FullTokenResolution {
  token: string;
  source: TokenSource | 'gh-cli';
  wasRefreshed?: boolean;
  username?: string;
  refreshError?: string;
}
```

**Resolution Order**:
1. Environment variables (`OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`)
2. Stored credentials (file) with auto-refresh
3. gh CLI fallback

**Example**:
```typescript
const { token, source } = await resolveTokenFull({ hostname: 'github.com' });
if (token) {
  console.log(`Using token from ${source}`);
}
```

---

#### `getTokenWithRefresh(hostname?)`

Get token from stored credentials with automatic refresh.

```typescript
async function getTokenWithRefresh(
  hostname?: string,
  clientId?: string
): Promise<TokenWithRefreshResult>

interface TokenWithRefreshResult {
  token: string | null;
  source: 'stored' | 'refreshed' | 'none';
  username?: string;
  refreshError?: string;
}
```

**Example**:
```typescript
const result = await getTokenWithRefresh('github.com');
if (result.source === 'refreshed') {
  console.log('Token was refreshed');
}
```

---

#### `resolveToken(hostname?)`

Resolve token without refresh (sync-safe).

```typescript
async function resolveToken(hostname?: string): Promise<ResolvedToken>

interface ResolvedToken {
  token: string;
  source: TokenSource;
}
// Note: resolveToken() returns ResolvedToken | null
// The token field is non-null when the object is returned
```

---

#### `resolveTokenWithRefresh(hostname?)`

Resolve token with auto-refresh for Octocode tokens.

```typescript
async function resolveTokenWithRefresh(
  hostname?: string,
  clientId?: string
): Promise<ResolvedTokenWithRefresh | null>

interface ResolvedTokenWithRefresh {
  token: string | null;
  source: TokenSource;
  wasRefreshed?: boolean;
  username?: string;
  refreshError?: string;
}
```

---

### Credential Storage Functions

#### `storeCredentials(credentials)`

Store credentials securely (encrypted file storage).

```typescript
async function storeCredentials(
  credentials: StoredCredentials
): Promise<StoreResult>
```

**Example**:
```typescript
const result = await storeCredentials({
  hostname: 'github.com',
  username: 'octocat',
  token: {
    token: 'gho_xxxxxxxxxxxx',
    tokenType: 'oauth',
    scopes: ['repo'],
  },
  gitProtocol: 'https',
  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
});

```

---

#### `getCredentials(hostname?, options?)`

Retrieve credentials (async, with caching).

```typescript
async function getCredentials(
  hostname?: string,
  options?: GetCredentialsOptions
): Promise<StoredCredentials | null>

interface GetCredentialsOptions {
  bypassCache?: boolean;  // Skip in-memory cache
}
```

---

#### `getCredentialsSync(hostname?)`

Retrieve credentials synchronously (file storage only).

```typescript
function getCredentialsSync(hostname?: string): StoredCredentials | null
```

⚠️ **Note**: Cannot access keychain (async-only). Use for sync contexts where refresh isn't needed.

---

#### `deleteCredentials(hostname?)`

Delete credentials from all storage backends.

```typescript
async function deleteCredentials(hostname?: string): Promise<DeleteResult>
```

---

#### `updateToken(hostname, token)`

Update only the token for existing credentials.

```typescript
async function updateToken(
  hostname: string,
  token: OAuthToken
): Promise<StoreResult>
```

---

### Token Helpers

#### `getToken(hostname?)`

Get raw token string (async).

```typescript
async function getToken(hostname?: string): Promise<string | null>
```

---

#### `getTokenSync(hostname?)`

Get raw token string (sync, file only).

```typescript
function getTokenSync(hostname?: string): string | null
```

---

#### `refreshAuthToken(hostname?, clientId?)`

Manually refresh an OAuth token.

```typescript
async function refreshAuthToken(
  hostname?: string,
  clientId?: string
): Promise<RefreshResult>

interface RefreshResult {
  success: boolean;
  username?: string;
  hostname?: string;
  error?: string;
}
```

---

### Environment Token Functions

#### `getTokenFromEnv()`

Get token from environment variables.

```typescript
function getTokenFromEnv(): string | null
```

**Check Order**: `OCTOCODE_TOKEN` → `GH_TOKEN` → `GITHUB_TOKEN`

---

#### `getEnvTokenSource()`

Identify which environment variable has a token.

```typescript
function getEnvTokenSource(): TokenSource
```

---

#### `hasEnvToken()`

Check if any environment variable has a token.

```typescript
function hasEnvToken(): boolean
```

---

### Expiration Helpers

#### `isTokenExpired(credentials)`

Check if access token is expired.

```typescript
function isTokenExpired(credentials: StoredCredentials): boolean
```

---

#### `isRefreshTokenExpired(credentials)`

Check if refresh token is expired.

```typescript
function isRefreshTokenExpired(credentials: StoredCredentials): boolean
```

---

### Storage Management

#### `listStoredHosts()`

List all hostnames with stored credentials (async).

```typescript
async function listStoredHosts(): Promise<string[]>
```

---

#### `listStoredHostsSync()`

List all hostnames with stored credentials (sync, file only).

```typescript
function listStoredHostsSync(): string[]
```

---

#### `hasCredentials(hostname?)`

Check if credentials exist (async).

```typescript
async function hasCredentials(hostname?: string): Promise<boolean>
```

---

#### `hasCredentialsSync(hostname?)`

Check if credentials exist (sync, file only).

```typescript
function hasCredentialsSync(hostname?: string): boolean
```

---

### Cache Control

#### `invalidateCredentialsCache(hostname?)`

Invalidate credential cache.

```typescript
function invalidateCredentialsCache(hostname?: string): void
```

- Pass hostname to invalidate specific entry
- Call without args to clear entire cache

---

### Low-Level Functions

#### `encrypt(data)`

Encrypt string with AES-256-GCM.

```typescript
function encrypt(data: string): string
```

Returns format: `iv:authTag:ciphertext` (hex-encoded)

---

#### `decrypt(encryptedData)`

Decrypt AES-256-GCM encrypted string.

```typescript
function decrypt(encryptedData: string): string
```

---

#### `ensureOctocodeDir()`

Create `~/.octocode/` directory if it doesn't exist.

```typescript
function ensureOctocodeDir(): void
```

---

### Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `OCTOCODE_DIR` | `~/.octocode` | Config directory |
| `CREDENTIALS_FILE` | `~/.octocode/credentials.json` | Encrypted credentials |
| `KEY_FILE` | `~/.octocode/.key` | Encryption key |
| `ENV_TOKEN_VARS` | `['OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN']` | Env var priority |

---

---

## Platform Module

### Platform Detection

#### `isWindows`

```typescript
const isWindows: boolean;  // process.platform === 'win32'
```

---

#### `isMac`

```typescript
const isMac: boolean;  // process.platform === 'darwin'
```

---

#### `isLinux`

```typescript
const isLinux: boolean;  // process.platform === 'linux'
```

---

#### `HOME`

```typescript
const HOME: string;  // os.homedir()
```

---

### Path Functions

#### `getAppDataPath()`

Get platform-specific app data directory.

```typescript
function getAppDataPath(): string
```

| Platform | Path |
|----------|------|
| Windows | `%APPDATA%` or `~/AppData/Roaming` |
| macOS/Linux | `~` |

---

#### `getLocalAppDataPath()`

Get platform-specific local app data directory.

```typescript
function getLocalAppDataPath(): string
```

| Platform | Path |
|----------|------|
| Windows | `%LOCALAPPDATA%` or `~/AppData/Local` |
| macOS/Linux | `~` |

---

### Info Functions

#### `getPlatformName()`

Get human-readable platform name.

```typescript
function getPlatformName(): string
// Returns: 'macOS', 'Windows', 'Linux', or os.platform()
```

---

#### `getArchitecture()`

Get CPU architecture.

```typescript
function getArchitecture(): string
// Returns: 'arm64', 'x64', etc.
```

---

## Session Module

### Types

#### `PersistedSession`

Session data structure.

```typescript
interface PersistedSession {
  version: 1;            // Schema version
  sessionId: string;     // UUID v4
  createdAt: string;     // ISO 8601
  lastActiveAt: string;  // ISO 8601
  stats: SessionStats;   // Hydrated from stats.json at runtime
}
```

---

#### `PersistedStats`

Stats data structure stored in `~/.octocode/stats.json`.

```typescript
interface PersistedStats {
  version: 1;            // Schema version
  stats: SessionStats;
}
```

---

#### `SessionStats`

Usage statistics.

```typescript
interface SessionStats {
  toolCalls: number;
  promptCalls: number;
  errors: number;
  rateLimits: number;
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

---

#### `SessionUpdateResult`

Result from stat update operations.

```typescript
interface SessionUpdateResult {
  success: boolean;
  session: PersistedSession | null;
}
```

---

#### `SessionOptions`

Options for session creation.

```typescript
interface SessionOptions {
  forceNew?: boolean;  // Create new session even if one exists
}
```

---

### Session Management

#### `getOrCreateSession(options?)`

Get existing session or create new one.

```typescript
function getOrCreateSession(options?: SessionOptions): PersistedSession
```

---

#### `readSession()`

Read current session from cache or disk.

```typescript
function readSession(): PersistedSession | null
```

---

#### `writeSession(session)`

Write session (deferred to disk).

```typescript
function writeSession(session: PersistedSession): void
```

---

#### `getSessionId()`

Get current session ID.

```typescript
function getSessionId(): string | null
```

---

#### `deleteSession()`

Delete session completely.

```typescript
function deleteSession(): boolean
```

---

### Flush Control

#### `flushSession()`

Flush pending changes to disk (async-safe).

```typescript
function flushSession(): void
```

---

#### `flushSessionSync()`

Flush pending changes to disk (for exit handlers).

```typescript
function flushSessionSync(): void
```

---

### Statistics Functions

#### `updateSessionStats(updates)`

Batch update session statistics.

```typescript
function updateSessionStats(
  updates: Partial<SessionStats>
): SessionUpdateResult
```

**Note**: Values are **added** to existing counts, not replaced.

---

#### `incrementToolCalls(count?)`

Increment tool call counter.

```typescript
function incrementToolCalls(count?: number): SessionUpdateResult
```

---

#### `incrementPromptCalls(count?)`

Increment prompt call counter.

```typescript
function incrementPromptCalls(count?: number): SessionUpdateResult
```

---

#### `incrementErrors(count?)`

Increment error counter.

```typescript
function incrementErrors(count?: number): SessionUpdateResult
```

---

#### `incrementRateLimits(count?)`

Increment global provider rate-limit counter.

```typescript
function incrementRateLimits(count?: number): SessionUpdateResult
```

---

#### `incrementRateLimitByProvider(provider, count?)`

Increment global and per-provider rate-limit counters.

```typescript
function incrementRateLimitByProvider(
  provider: string,
  count?: number
): SessionUpdateResult
```

---

#### `incrementPackageRegistryFailures(registry, count?)`

Increment package-registry failure counters. These are separate from provider API rate limits.

```typescript
function incrementPackageRegistryFailures(
  registry: string,
  count?: number
): SessionUpdateResult
```

---

#### `resetSessionStats()`

Reset all statistics to zero.

```typescript
function resetSessionStats(): SessionUpdateResult
```

---

### Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `SESSION_FILE` | `~/.octocode/session.json` | Session file path |
| `STATS_FILE` | `~/.octocode/stats.json` | Stats file path |

---

## Testing Utilities

Functions prefixed with `_` are exported for testing purposes:

### Credentials

```typescript
// Get cache statistics
_getCacheStats(): { size: number; entries: Array<...> }

// Clear credential cache
_resetCredentialsCache(): void
```

### Session

```typescript
// Reset all session state (cache, timer, handlers)
_resetSessionState(): void
```

### Config

```typescript
// Reset configuration cache
_resetConfigCache(): void

// Get cache state for debugging
_getCacheState(): { hasConfig: boolean; ... }
```

### Logger

```typescript
// Get the current custom log handler (or null)
_getLogHandler(): ((entry: LogEntry) => void) | null
```

---

## Config Module

### Configuration Loading

#### `getConfig()`

Load and resolve configuration (async).

```typescript
async function getConfig(): Promise<ResolvedConfig>
```

---

#### `getConfigSync()`

Load and resolve configuration (sync).

```typescript
function getConfigSync(): ResolvedConfig
```

---

#### `reloadConfig()`

Force reload configuration from disk.

```typescript
async function reloadConfig(): Promise<ResolvedConfig>
```

---

#### `invalidateConfigCache()`

Invalidate the in-memory config cache.

```typescript
function invalidateConfigCache(): void
```

---

### Configuration Types

#### `OctocodeConfig`

User-facing configuration (from `.octocoderc`).

```typescript
interface OctocodeConfig {
  $schema?: string;
  version?: number;
  github?: { apiUrl?: string };
  gitlab?: { host?: string };
  local?: { enabled?: boolean; allowedPaths?: string[]; workspaceRoot?: string };
  tools?: { enabled?: string[] | null; enableAdditional?: string[] | null; disabled?: string[] | null; disablePrompts?: boolean };
  network?: { timeout?: number; maxRetries?: number };
  telemetry?: { logging?: boolean };
  lsp?: { configPath?: string };
  output?: { format?: 'yaml' | 'json' };
}
```

---

#### `ResolvedConfig`

Fully resolved configuration with all defaults applied.

```typescript
interface ResolvedConfig {
  version: number;
  github: { apiUrl: string };
  gitlab: { host: string };
  local: { enabled: boolean; allowedPaths: string[]; workspaceRoot: string | undefined };
  tools: { enabled: string[] | null; enableAdditional: string[] | null; disabled: string[] | null; disablePrompts: boolean };
  network: { timeout: number; maxRetries: number };
  telemetry: { logging: boolean };
  lsp: { configPath: string | undefined };
  output: { format: 'yaml' | 'json' };
  source: 'file' | 'defaults' | 'mixed';
  configPath?: string;
}
```

---

### Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `CONFIG_FILE_NAME` | `.octocoderc` | Config file name |

---

## Logger Module

### Types

#### `LogLevel`

```typescript
type LogLevel = 'debug' | 'info' | 'warn' | 'error';
```

#### `LogEntry`

```typescript
interface LogEntry {
  level: LogLevel;
  module: string;
  message: string;
  data?: Record<string, unknown>;
}
```

---

### Functions

#### `createLogger(module)`

Create a logger instance for a named module.

```typescript
function createLogger(module: string): {
  debug: (message: string, data?: Record<string, unknown>) => void;
  info: (message: string, data?: Record<string, unknown>) => void;
  warn: (message: string, data?: Record<string, unknown>) => void;
  error: (message: string, data?: Record<string, unknown>) => void;
}
```

**Example**:
```typescript
const logger = createLogger('my-module');
logger.info('Starting up', { version: '1.0.0' });
logger.error('Something failed', { code: 'ERR_001' });
```

---

#### `setLogHandler(handler)`

Override the default console log handler with a custom handler.

```typescript
function setLogHandler(handler: ((entry: LogEntry) => void) | null): void
```

Pass `null` to restore the default console handler.

---

## Import Patterns

### Recommended: Module-Specific Imports

```typescript
// Only credentials
import { getTokenWithRefresh, storeCredentials } from 'octocode-shared/credentials';

// Only platform
import { isMac, isWindows, HOME } from 'octocode-shared/platform';

// Only session
import { getOrCreateSession, incrementToolCalls } from 'octocode-shared/session';

// Only config
import { getConfigSync, reloadConfig } from 'octocode-shared/config';

// Logger (via main entry only - no subpath export)
import { createLogger } from 'octocode-shared';
```

### Alternative: Main Entry Point

```typescript
// All exports (larger bundle)
import { 
  getTokenWithRefresh,  // credentials
  isMac,                // platform
  getOrCreateSession,   // session
  getConfigSync,        // config
  createLogger,         // logger
} from 'octocode-shared';
```

---

## Related Documentation

- [CREDENTIALS_ARCHITECTURE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/docs/CREDENTIALS_ARCHITECTURE.md) - Deep dive into credential storage
- [SESSION_PERSISTENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/docs/SESSION_PERSISTENCE.md) - Session storage architecture

---

*Part of [octocode-shared](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/README.md) - Shared utilities for Octocode packages*
