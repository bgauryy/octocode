# Authentication

GitHub is the only supported Octocode provider. This page covers every aspect of token resolution — priority, storage, refresh, Octokit caching, and the MCP/CLI shared flow.

## Quick Start

```bash
# Recommended: OAuth login via Octocode CLI
npx octocode auth login

# Or use GitHub CLI credentials (already logged in)
gh auth login

# Or provide a token via environment variable
export GITHUB_TOKEN="ghp_your_token_here"

# Check current auth status
npx octocode status
```

Start the MCP server after any of the above:

```bash
npx octocode-mcp
```

## Token Priority

`resolveTokenFull()` in `octocode-tools-core/src/shared/credentials/tokenResolution.ts` is the single resolution path shared by both the MCP server and the CLI. It tries sources in order and stops at the first non-empty result:

| Priority | Source | How it is checked |
|----------|--------|--------------------|
| 1 | `OCTOCODE_TOKEN` env var | `process.env.OCTOCODE_TOKEN` |
| 2 | `GH_TOKEN` env var | `process.env.GH_TOKEN` |
| 3 | `GITHUB_TOKEN` env var | `process.env.GITHUB_TOKEN` |
| 4 | `GITHUB_PERSONAL_ACCESS_TOKEN` env var | `process.env.GITHUB_PERSONAL_ACCESS_TOKEN` |
| 5 | Octocode encrypted storage | `~/.octocode/credentials.json` (with optional auto-refresh) |
| 6 | `gh` CLI | `gh auth token <hostname>` |

All env-token lookups read `process.env` directly every call — no startup caching. Rotating or unsetting an env variable takes effect on the next API request with no server restart required.

> **Source labels** returned in results: `'env:OCTOCODE_TOKEN'`, `'env:GH_TOKEN'`, `'env:GITHUB_TOKEN'`, `'env:GITHUB_PERSONAL_ACCESS_TOKEN'`, `'octocode-storage'`, `'gh-cli'`.

## Authentication Methods

### 1. Octocode OAuth Login (recommended)

```bash
npx octocode auth login
```

Opens a browser for GitHub's OAuth Device Flow. After approval the token is stored encrypted in `<octocode-home>/credentials.json`. The OAuth App client ID is `178c6fc778ccc68e1d6a` and the default scopes are `repo`, `read:org`, and `gist`.

### 2. Environment Variables

Set any one of the four env vars above. `OCTOCODE_TOKEN` is the most specific (highest priority). The standard `GITHUB_TOKEN` and `GITHUB_PERSONAL_ACCESS_TOKEN` names are also accepted, which covers CI environments (GitHub Actions sets `GITHUB_TOKEN` automatically) and MCP client env blocks.

```bash
export OCTOCODE_TOKEN="ghp_your_token"       # highest priority
export GH_TOKEN="ghp_your_token"             # second
export GITHUB_TOKEN="ghp_your_token"         # third
export GITHUB_PERSONAL_ACCESS_TOKEN="ghp_…" # fourth (lowest env priority)
```

#### MCP client config example

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "@octocodeai/mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_your_token_here"
      }
    }
  }
}
```

### 3. GitHub CLI

If `gh` is installed and authenticated, its token is used as fallback (priority 6). Octocode runs `gh auth token <hostname>` and trims the result.

```bash
gh auth login
```

No extra configuration required. `gh` manages its own token refresh.

## Credential Storage

Stored credentials (from `npx octocode auth login`) live in two files:

| File | Purpose | Permissions |
|------|---------|-------------|
| `<octocode-home>/credentials.json` | Encrypted credential store | `0600` |
| `<octocode-home>/.key` | 32-byte AES key, hex-encoded | `0600` |

Default home directories:

| Platform | Default path |
|----------|-------------|
| macOS | `~/.octocode` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/.octocode` |
| Windows | `%APPDATA%\.octocode` |

### Encryption

Credentials are encrypted with **AES-256-GCM**. The format of every `credentials.json` entry is:

```
iv:authTag:ciphertext        (all hex-encoded)
```

A new random IV is generated for each write. The GCM auth tag detects tampering. The key is created once on first login and stored in `.key` at mode `0600`.

### Stored shape

```ts
interface StoredCredentials {
  hostname: string;          // normalized, e.g. "github.com"
  username: string;          // GitHub login
  token: {
    token: string;
    tokenType: 'oauth';
    scopes?: string[];
    refreshToken?: string;   // present for GitHub App tokens only
    expiresAt?: string;      // ISO 8601; absent for non-expiring OAuth App tokens
    refreshTokenExpiresAt?: string;
  };
  gitProtocol: 'ssh' | 'https';
  createdAt: string;         // ISO 8601
  updatedAt: string;         // ISO 8601
}
```

## Token Refresh

| Token type | Auto-refresh | Notes |
|------------|-------------|-------|
| Environment variable | No | User-managed; re-read fresh from `process.env` each call. |
| Stored OAuth App token | No | OAuth App tokens do not expire; no refresh token is issued. |
| Stored GitHub App token | Yes | Refreshed via `@octokit/oauth-methods` when expired and a valid refresh token exists. |
| `gh` CLI token | No | `gh` manages its own refresh. |

**Expiry guard**: a stored token is considered expired when `expiresAt` is within **5 minutes** of now. This allows the refresh to complete before any in-flight request sees an expired token.

**Refresh token expiry**: if `refreshTokenExpiresAt` is in the past, refresh is blocked and the user must run `npx octocode auth login` again.

Error messages from the refresh path are sanitized — GitHub-token-like strings are masked before they appear in any log or return value.

## Octokit Instance Cache

`getOctokit()` in `octocode-tools-core/src/github/client.ts` resolves the token on every call, then keys the Octokit instance cache by the SHA-256 hash of the token (first 16 hex characters), or `'ANONYMOUS'` for unauthenticated requests.

| Property | Value |
|----------|-------|
| Cache key | SHA-256(token), first 16 hex chars |
| Instance TTL | 5 minutes |
| Max instances | 50 |
| Background purge interval | 1 minute |

When the resolved token changes (e.g., an env var is updated), the old cached instance is not reused — a new one is created immediately. When the cache is full, expired entries are evicted first; if still full, the oldest entries are dropped.

## MCP Startup Flow

At MCP server startup, `initialize()` in `serverConfig.ts` runs once:

1. Reads configuration from `~/.octocode/.octocoderc` and environment.
2. Calls `resolveGitHubToken()` to **snapshot** `tokenSource` for the status log.
3. Stores the snapshot in `ServerConfig.tokenSource`.

After startup, `getGitHubToken()` (called by `getOctokit()` before each GitHub API request) **always re-resolves** via `resolveTokenFull()`. The snapshotted `tokenSource` in the config is only used for display — actual token lookup is always live.

## CLI Auth Commands

### `auth login`

```bash
npx octocode auth login                        # interactive menu (TTY)
npx octocode auth login --json                 # non-interactive OAuth (prints JSON steps)
npx octocode auth login --hostname myhost.com  # GitHub Enterprise
npx octocode auth login --git-protocol ssh     # request SSH git protocol
npx octocode auth login --force                # re-authenticate even if already signed in
```

**Interactive menu** (default when stdout is a TTY and no `--force`/`--json`):

```
  GitHub Authentication
  ─────────────────────
  Using GITHUB_TOKEN (takes priority)       ← shown only when an env token is active
  Switch Octocode account (@alice)          ← shown when stored credentials exist
  Delete Octocode token (@alice)
  Sign in via gh CLI                        ← or "Delete gh CLI token" if already signed in
  ─────────────────────────────────────────
  Or set OCTOCODE_TOKEN / GITHUB_TOKEN / GITHUB_PERSONAL_ACCESS_TOKEN env var
  Back
```

The menu reads live auth state (`getAuthStatusAsync`) before rendering. The presence of env token, stored credentials, and gh CLI auth each independently affect which choices appear.

**JSON mode** (`--json`) emits one JSON line per step:

```jsonc
{ "step": "warning",      "envVar": "GITHUB_TOKEN", "message": "..." } // if env token active
{ "step": "verification", "userCode": "ABCD-1234",  "verificationUri": "https://..." }
{ "step": "result",       "success": true,          "username": "alice" }
```

### Switching Accounts

**Option A — interactive menu:** run `npx octocode auth login` and select "Switch Octocode account". This signs out the current stored token then immediately starts a new OAuth flow.

**Option B — command line:**

```bash
npx octocode auth login --force
```

`--force` signs out any existing stored token for the hostname, then runs the OAuth device flow again. It does not affect env tokens or gh CLI auth.

> **Env token in effect?** When `OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, or `GITHUB_PERSONAL_ACCESS_TOKEN` is set, it takes priority over stored credentials. The `--force` flow stores a new token but it won't be used until you unset the env var. The CLI warns you when this is the case.

### `auth logout`

```bash
npx octocode auth logout                   # confirm then delete stored Octocode token
npx octocode auth logout --yes             # skip confirmation
npx octocode auth logout --json            # JSON output
npx octocode auth logout --hostname myhost # Enterprise host
```

`auth logout` removes the stored OAuth credentials for the given hostname from `<octocode-home>/credentials.json`. It does **not** revoke the GitHub token server-side (no client secret is available), and it does **not** affect env tokens or gh CLI auth.

To sign out of the gh CLI use `gh auth logout`.

### Other `auth` subcommands

```bash
npx octocode auth login
npx octocode auth logout
npx octocode auth status              # print auth status (human-readable)
npx octocode auth status --json       # print auth status (JSON)
npx octocode auth refresh             # refresh a stored GitHub App token
```

`auth refresh` is only valid for GitHub App OAuth tokens (those with a `refreshToken`). OAuth App tokens (standard `ghp_*` personal access tokens) do not expire and cannot be refreshed. Tokens from env vars or gh CLI are not refreshable through this command — the CLI says so explicitly.

### Auth Status Shape

`getAuthStatusAsync()` (used internally by all auth commands) resolves to:

```ts
{
  authenticated: boolean;
  hostname?: string;
  username?: string;
  tokenSource: 'env' | 'octocode' | 'gh-cli' | 'none';
  tokenExpired?: boolean;      // set when stored token is expired
  envTokenSource?: string;     // e.g. 'env:GITHUB_TOKEN'
}
```

The CLI `tokenSource` field collapses all four env variants into `'env'` with the specific variable in `envTokenSource`. The underlying `resolveTokenFull()` returns the precise source label (e.g. `'env:OCTOCODE_TOKEN'`).

## In-Memory Credential Cache

Credential reads are cached per normalized hostname. Cache entries expire after **5 minutes**. A write, delete, or token update immediately invalidates the relevant hostname entry.

Hostname normalization:
- lowercased
- protocol prefix stripped (`https://`, `http://`)
- trailing slash removed

So `https://GitHub.com/` and `github.com` map to the same cache entry.

## Credential Architecture API

`@octocodeai/octocode-tools-core/credentials` is the shared GitHub auth layer for the MCP server and CLI. Its public operations are:

| API | Behavior |
|-----|----------|
| `resolveTokenFull()` | Full resolution chain including env, encrypted storage refresh, and `gh` CLI fallback. |
| `resolveToken()` | Env + encrypted storage only, no `gh` fallback. |
| `resolveTokenWithRefresh()` | Env + encrypted storage with refresh metadata. |
| `storeCredentials(credentials)` | Normalizes host, writes encrypted store, invalidates cache. |
| `getCredentials(hostname)` | Reads cached or encrypted stored credentials. |
| `getCredentialsSync(hostname)` | Synchronous stored-credential read. |
| `deleteCredentials(hostname)` | Removes credentials for one host. |
| `listStoredHosts()` | Lists hosts present in encrypted storage. |
| `hasCredentials(hostname)` | Checks encrypted storage. |
| `updateToken(hostname, token)` | Replaces token and updates `updatedAt`. |
| `refreshAuthToken(hostname, clientId?)` | Refreshes stored OAuth credentials. |
| `getTokenWithRefresh(hostname, clientId?)` | Returns stored token or refreshes if needed. |

## Credential Failure Behavior

| Scenario | Behavior |
|----------|----------|
| Missing credentials file | Return an empty store. |
| Invalid encrypted payload or JSON | Warn and return an empty store. |
| Invalid store schema | Warn and return an empty store. |
| Expired stored token with no refresh token | Return no token with refresh error. |
| Expired refresh token | Return refresh failure and require login. |
| `gh` command failure | Ignore fallback and return no token if no prior source succeeded. |

## Credential Security Notes

- File permissions are tightened to owner-only.
- AES-GCM auth tags detect tampering.
- Error messages mask GitHub-token-like strings.
- The encryption key is file-based, so this is for single-user workstations and CI contexts, not multi-tenant secret custody.
- Consumers must never expose raw tokens.

## GitHub Enterprise

Set `GITHUB_API_URL` to the Enterprise API endpoint:

```bash
export GITHUB_TOKEN="ghp_your_enterprise_token"
export GITHUB_API_URL="https://github.mycompany.com/api/v3"
```

The default API URL is `https://api.github.com`. The CLI and MCP server use `GITHUB_API_URL` automatically when set.

OAuth login for Enterprise:

```bash
npx octocode auth login --hostname github.mycompany.com
```

The refresh API base URL is derived from the stored hostname: `github.com` uses `https://api.github.com`, any other host uses `https://<hostname>/api/v3`.

## Clone and Local Tools

Remote GitHub tools work with token auth alone. The clone-backed tools (`ghCloneRepo`, `ghGetFileContent type:"directory"`) also require:

```bash
export ENABLE_CLONE=true
```

This can also be set in `~/.octocode/.octocoderc`. Local tools stay enabled by default unless `ENABLE_LOCAL=false` is set.

## Troubleshooting

| Symptom | Check |
|---------|-------|
| No token found | Run `npx octocode status`, then `npx octocode auth login`, or set an env var. |
| 401 Unauthorized | Verify token scopes and the selected GitHub host. |
| Enterprise requests hit github.com | Set `GITHUB_API_URL` to the Enterprise API URL. |
| Clone tools unavailable | Set `ENABLE_CLONE=true` and make sure `ENABLE_LOCAL` is not set to `false`. |
| Stored token expired, no refresh | Token is an OAuth App token that cannot refresh — run `npx octocode auth login` again. |
| Refresh token expired | `refreshTokenExpiresAt` is past — run `npx octocode auth login`. |
| Env token is active but OAuth was just saved | Env tokens take priority; unset the env var to use the stored token. |
| Wrong account | Run `npx octocode auth logout` then `npx octocode auth login`, or update `gh` with `gh auth switch`. |

## Related

- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) — all env vars and config file options
- [Octocode MCP Server](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md)
- [Octocode Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
- [Octocode CLI Guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)
