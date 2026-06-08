# Credentials Architecture

`octocode-shared/credentials` is the shared GitHub auth layer for Octocode packages. It resolves tokens from environment variables, encrypted Octocode storage, and the GitHub CLI fallback.

## Token Resolution

Priority order:

1. `OCTOCODE_TOKEN`
2. `GH_TOKEN`
3. `GITHUB_TOKEN`
4. Encrypted Octocode credentials in `~/.octocode/credentials.json`
5. `gh auth token`

Environment and `gh` tokens are returned as-is. Stored Octocode credentials can be refreshed before they are returned.

## Storage

| File | Purpose | Permissions |
|------|---------|-------------|
| `~/.octocode/credentials.json` | Encrypted credential store | `0600` |
| `~/.octocode/.key` | 32-byte encryption key, hex encoded | `0600` |

`OCTOCODE_HOME` changes the base directory.

Stored shape:

```ts
interface StoredCredentials {
  hostname: string;
  username: string;
  token: OAuthToken;
  gitProtocol: 'ssh' | 'https';
  createdAt: string;
  updatedAt: string;
}
```

## Encryption

Credentials are encrypted with AES-256-GCM.

Format:

```text
iv:authTag:ciphertext
```

The key is generated once and stored in `.key`; every encryption uses a new random IV. GCM provides confidentiality and tamper detection.

## Refresh Policy

| Source | Auto-refresh | Notes |
|--------|--------------|-------|
| Environment token | No | User-managed. |
| Stored Octocode OAuth token | Yes, when expired and refresh token is valid | Uses `@octokit/oauth-methods`. |
| Stored token without refresh token | No | Expired token resolves as unavailable. |
| `gh` CLI | No | `gh` manages its own auth. |

Refresh errors are sanitized so token-like strings are masked before returning/logging.

## Cache

Credentials are cached in memory per normalized hostname for 5 minutes. Expired tokens invalidate their cache entry. Writes, deletes, and token updates invalidate the relevant cache.

Hostname normalization lowercases hostnames and strips protocol/trailing slash, so `https://GitHub.com/` and `github.com` share one key.

## Public Operations

| API | Behavior |
|-----|----------|
| `resolveTokenFull()` | Full resolution chain including env, storage refresh, and `gh` fallback. |
| `resolveToken()` | Env + storage only, no `gh` fallback. |
| `resolveTokenWithRefresh()` | Env + storage with refresh metadata. |
| `storeCredentials(credentials)` | Normalizes host, writes encrypted store, invalidates cache. |
| `getCredentials(hostname)` | Reads cached or encrypted stored credentials. |
| `getCredentialsSync(hostname)` | Synchronous stored-credential read. |
| `deleteCredentials(hostname)` | Removes credentials for one host. |
| `listStoredHosts()` | Lists hosts present in encrypted storage. |
| `hasCredentials(hostname)` | Checks encrypted storage. |
| `updateToken(hostname, token)` | Replaces token and updates `updatedAt`. |
| `refreshAuthToken(hostname, clientId?)` | Refreshes stored OAuth credentials. |
| `getTokenWithRefresh(hostname, clientId?)` | Returns stored token or refreshes if needed. |

## Failure Behavior

| Scenario | Behavior |
|----------|----------|
| Missing credentials file | Return an empty store. |
| Invalid encrypted payload or JSON | Warn and return an empty store. |
| Invalid store schema | Warn and return an empty store. |
| Expired stored token with no refresh token | Return no token with refresh error. |
| Expired refresh token | Return refresh failure and require login. |
| `gh` command failure | Ignore fallback and return no token if no prior source succeeded. |

## Security Notes

- File permissions are tightened to owner-only.
- AES-GCM auth tags detect tampering.
- Error messages mask GitHub-token-like strings.
- The encryption key is file-based, so this is for single-user workstations and CI contexts, not multi-tenant secret custody.
- Consumers must never log raw tokens.

## Related Documentation

- [Session Persistence](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/SESSION_PERSISTENCE.md)
- [octocode-shared README](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/README.md)
