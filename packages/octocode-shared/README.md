# octocode-shared

Shared utilities for Octocode packages - credential management, session persistence, configuration, and platform detection.

## Installation

```bash
npm install octocode-shared
# or
yarn add octocode-shared
```

**Requirements:** Node.js >= 20.0.0

## Quick Start

```typescript
// Import all modules
import { getCredentials, getConfig, getPlatformName } from 'octocode-shared';

// Or import specific modules
import { getCredentials, storeCredentials } from 'octocode-shared/credentials';
import { getConfig, loadConfig } from 'octocode-shared/config';
import { getSessionId, updateSessionStats } from 'octocode-shared/session';
import { isWindows, isMac, isLinux } from 'octocode-shared/platform';
```

## Modules

### GitHub Token

**You don't need to configure anything.** Octocode finds your token automatically.

It checks these sources in order, using the first one it finds:

1. `OCTOCODE_TOKEN` env var
2. `GH_TOKEN` env var
3. `GITHUB_TOKEN` env var
4. Credentials stored by `octocode auth login`
5. Your existing `gh` CLI login (`gh auth login`)

**The easiest way to get started** — if you already use the GitHub CLI, just run your tool. It will pick up your `gh` session with no extra setup.

**To set a specific token:**
```bash
export GITHUB_TOKEN=ghp_your_token_here
```

**To store credentials permanently:**
```bash
octocode-cli auth login
```

---

### Credentials

Secure credential storage with AES-256-GCM encryption.

```typescript
import {
  storeCredentials,
  getCredentials,
  getToken,
  deleteCredentials,
  hasCredentials,
} from 'octocode-shared/credentials';

// Store credentials securely
await storeCredentials('github.com', {
  username: 'user',
  token: { token: 'SOME_TOKEN', tokenType: 'oauth' },
  gitProtocol: 'https',
});

// Retrieve credentials
const creds = await getCredentials('github.com');

// Get token with automatic refresh
const token = await getToken('github.com');

// Check if credentials exist
const exists = await hasCredentials('github.com');

// Delete credentials
await deleteCredentials('github.com');
```

### Config

Hierarchical configuration management with validation and caching.

```typescript
import {
  getConfig,
  loadConfig,
  validateConfig,
  resolveConfig,
} from 'octocode-shared/config';

// Get resolved configuration (cached)
const config = await getConfig();

// Access specific config values
const githubConfig = config.github;
const networkTimeout = config.network.timeout;

// Load raw config from file
const rawConfig = await loadConfig();

// Validate configuration
const result = validateConfig(rawConfig);
if (!result.valid) {
  console.error(result.errors);
}
```

### Session

Session persistence with deferred writes and usage statistics.

```typescript
import {
  getSessionId,
  getOrCreateSession,
  updateSessionStats,
  incrementToolCalls,
  flushSession,
} from 'octocode-shared/session';

// Get or create session
const session = await getOrCreateSession();

// Get session ID
const sessionId = await getSessionId();

// Update statistics
await incrementToolCalls();

// Flush session to disk
await flushSession();
```

### Platform

Cross-platform utilities for path and environment detection.

```typescript
import {
  isWindows,
  isMac,
  isLinux,
  HOME,
  getAppDataPath,
  getPlatformName,
  getArchitecture,
} from 'octocode-shared/platform';

// Platform detection
if (isMac) {
  console.log('Running on macOS');
}

// Get platform-specific paths
const appData = getAppDataPath(); // ~/.config on Linux/macOS, %APPDATA% on Windows

// Get platform info
console.log(getPlatformName());  // 'darwin', 'linux', or 'win32'
console.log(getArchitecture());  // 'x64', 'arm64', etc.
```

## Configuration File

Octocode looks for configuration in `~/.octocode/.octocoderc`:

```json
{
  "version": 1,
  "github": {
    "apiUrl": "https://api.github.com"
  },
  "network": {
    "timeout": 30000,
    "maxRetries": 3
  },
  "telemetry": {
    "logging": true
  }
}
```

## Storage Locations

| Data | Location |
|------|----------|
| Config | `~/.octocode/.octocoderc` |
| Credentials | `~/.octocode/credentials.json` (encrypted) |
| Session | `~/.octocode/session.json` |
| Encryption Key | `~/.octocode/.key` |

## Docs

- [Docs Index](https://github.com/bgauryy/octocode-mcp/blob/main/docs/README.md) — all Octocode docs
- [Shared API Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/SHARED_API_REFERENCE.md) — exports, types, and import patterns
- [Credentials Architecture](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/CREDENTIALS_ARCHITECTURE.md) — secure token storage design
- [Session Persistence](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/SESSION_PERSISTENCE.md) — session lifecycle and deferred writes
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) — env vars and `.octocoderc`

## Development

```bash
# Install dependencies
yarn install

# Build
yarn build

# Run tests
yarn test

# Type check
yarn typecheck

# Lint
yarn lint
```

## License

MIT
