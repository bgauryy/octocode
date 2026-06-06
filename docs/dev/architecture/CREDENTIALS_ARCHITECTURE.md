# Credentials Architecture

> Technical deep-dive into the credential storage system in `octocode-shared`.

## Overview

The credentials module provides secure token storage with AES-256-GCM encryption, automatic token refresh, and multi-source resolution. It's designed to be the single source of truth for GitHub authentication across all Octocode packages.

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           TOKEN RESOLUTION FLOW                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────┐   Priority: 1 (Highest)                                   │
│  │ Environment  │   ─────────────────────                                   │
│  │   Variables  │   OCTOCODE_TOKEN → GH_TOKEN → GITHUB_TOKEN                │
│  │              │   ⚠️ No auto-refresh (user-managed)                        │
│  └──────┬───────┘                                                            │
│         │ Not found?                                                         │
│         ▼                                                                    │
│  ┌──────────────┐   Priority: 2                                             │
│  │  Encrypted   │   ──────────                                              │
│  │    File      │   ~/.octocode/credentials.json (AES-256-GCM)              │
│  │              │   ✅ Auto-refresh for Octocode OAuth tokens               │
│  └──────┬───────┘                                                            │
│         │ Not found?                                                         │
│         ▼                                                                    │
│  ┌──────────────┐   Priority: 3 (Lowest)                                    │
│  │   gh CLI     │   ─────────────────────                                   │
│  │   Fallback   │   `gh auth token` command                                 │
│  │              │   ⚠️ No auto-refresh (gh CLI manages its own)             │
│  └──────────────┘                                                            │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Storage Layer: Encrypted File Storage

All credentials are stored in encrypted files. This provides a pure JavaScript solution that works across all environments (CI, containers, SSH, desktop) without native dependencies.

**File Locations**:

| File | Purpose | Permissions |
|------|---------|-------------|
| `~/.octocode/credentials.json` | Encrypted credentials | `0600` |
| `~/.octocode/.key` | Encryption key (32 bytes, random) | `0600` |

**Encryption Algorithm**: AES-256-GCM

```typescript
const ALGORITHM = 'aes-256-gcm';
const IV_LENGTH = 16;

function encrypt(data: string): string {
  const key = getOrCreateKey();           // 256-bit from ~/.octocode/.key
  const iv = randomBytes(IV_LENGTH);      // Unique per encryption
  const cipher = createCipheriv(ALGORITHM, key, iv);
  
  let encrypted = cipher.update(data, 'utf8', 'hex');
  encrypted += cipher.final('hex');
  const authTag = cipher.getAuthTag();    // Authentication tag for integrity
  
  return `${iv.toString('hex')}:${authTag.toString('hex')}:${encrypted}`;
}
```

**Format**: `iv:authTag:ciphertext` (hex-encoded, colon-separated)

---

## Token Types & Refresh Policy

| Token Type | Has Expiry? | Has Refresh Token? | Auto-Refresh? |
|------------|-------------|-------------------|---------------|
| **Octocode OAuth** | ✅ 8 hours | ✅ 6 months | ✅ Yes |
| **GitHub PAT (classic)** | ❌ No | ❌ No | ❌ N/A |
| **GitHub PAT (fine-grained)** | ✅ Optional | ❌ No | ❌ No |
| **Environment Variables** | Unknown | Unknown | ❌ No (user-managed) |
| **gh CLI Token** | ✅ 8 hours | ✅ Managed by gh | ❌ No (gh manages) |

### Octocode OAuth Token Structure

```typescript
interface OAuthToken {
  token: string;              // Access token
  tokenType: 'oauth';
  scopes?: string[];          // e.g., ['repo', 'read:user']
  refreshToken?: string;      // For refreshing expired tokens
  expiresAt?: string;         // ISO 8601 timestamp
  refreshTokenExpiresAt?: string;
}
```

### Expiration Checking

```typescript
export function isTokenExpired(credentials: StoredCredentials): boolean {
  const { expiresAt } = credentials.token;
  if (!expiresAt) return false;  // No expiry = never expires
  return new Date(expiresAt) < new Date();
}

export function isRefreshTokenExpired(credentials: StoredCredentials): boolean {
  const { refreshTokenExpiresAt } = credentials.token;
  if (!refreshTokenExpiresAt) return false;
  return new Date(refreshTokenExpiresAt) < new Date();
}
```

---

## Token Refresh Flow

When a token is expired, `getTokenWithRefresh()` automatically refreshes it:

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│ Get Token   │────▶│ Check Expiry │────▶│ Token Valid?    │
└─────────────┘     └──────────────┘     └────────┬────────┘
                                                  │
                            ┌─────────────────────┴─────────────────────┐
                            │ Yes                                       │ No
                            ▼                                           ▼
                    ┌───────────────┐                          ┌────────────────┐
                    │ Return Token  │                          │ Has Refresh    │
                    └───────────────┘                          │ Token?         │
                                                               └───────┬────────┘
                                                                       │
                                              ┌────────────────────────┴────────┐
                                              │ Yes                             │ No
                                              ▼                                 ▼
                                     ┌────────────────┐              ┌──────────────┐
                                     │ Refresh via    │              │ Return       │
                                     │ @octokit/oauth │              │ Expired      │
                                     └───────┬────────┘              │ Token        │
                                             │                       └──────────────┘
                                             ▼
                                     ┌────────────────┐
                                     │ Store New      │
                                     │ Token          │
                                     └───────┬────────┘
                                             │
                                             ▼
                                     ┌────────────────┐
                                     │ Return Fresh   │
                                     │ Token          │
                                     └────────────────┘
```

**Refresh Implementation** (using `@octokit/oauth-methods`):

```typescript
import { refreshToken as octokitRefreshToken } from '@octokit/oauth-methods';

export async function refreshAuthToken(
  credentials: StoredCredentials,
  clientId: string = DEFAULT_CLIENT_ID
): Promise<RefreshResult> {
  const { refreshToken } = credentials.token;
  
  const { data } = await octokitRefreshToken({
    clientType: 'github-app',
    clientId,
    refreshToken,
  });
  
  // Update stored credentials with new token
  const updatedCredentials = {
    ...credentials,
    token: {
      ...credentials.token,
      token: data.access_token,
      expiresAt: data.expires_at,
      refreshToken: data.refresh_token,
      refreshTokenExpiresAt: data.refresh_token_expires_at,
    },
  };
  
  await storeCredentials(updatedCredentials);
  return { success: true, token: data.access_token };
}
```

---

## In-Memory Caching

To avoid repeated file reads, credentials are cached in memory:

```typescript
interface CachedCredentials {
  credentials: StoredCredentials;
  cachedAt: number;
}

const credentialsCache = new Map<string, CachedCredentials>();
const CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes

function isCacheValid(hostname: string): boolean {
  const cached = credentialsCache.get(hostname);
  if (!cached) return false;
  return Date.now() - cached.cachedAt < CACHE_TTL_MS;
}
```

**Cache Invalidation**:

```typescript
// Invalidate specific host
invalidateCredentialsCache('github.com');

// Invalidate all
invalidateCredentialsCache();
```

The cache is automatically invalidated when:
- `storeCredentials()` is called
- `deleteCredentials()` is called
- `updateToken()` is called

---

## Hostname Normalization

All hostnames are normalized before storage/lookup:

```typescript
function normalizeHostname(hostname: string): string {
  return hostname
    .toLowerCase()
    .replace(/^https?:\/\//, '')  // Remove protocol
    .replace(/\/$/, '');          // Remove trailing slash
}

// Examples:
// 'GitHub.com' → 'github.com'
// 'https://github.com/' → 'github.com'
// 'GITHUB.ENTERPRISE.COM' → 'github.enterprise.com'
```

---

## Security Considerations

### ✅ What We Do

1. **AES-256-GCM**: Authenticated encryption prevents tampering
2. **Unique IVs**: Each encryption uses a random initialization vector
3. **File Permissions**: `0600` (owner read/write only)
4. **Memory Cache TTL**: Credentials expire from memory after 5 minutes
5. **Random Key Generation**: 32-byte cryptographically random key

### ⚠️ Limitations

1. **File-based Key**: Encryption key is stored in `~/.octocode/.key`
2. **Single-User**: Designed for single-user workstations, not multi-tenant
3. **No HSM Support**: Does not integrate with hardware security modules
4. **Trust on First Use**: No certificate pinning for token refresh endpoints

---

## Error Handling

```typescript
// Corrupted credentials file
const store = readCredentialsStore();
// Returns { version: 1, credentials: {} } on parse errors
// Warns user: "Could not read credentials file. You may need to login again."
```

---

## Related Documentation

- [SESSION_PERSISTENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/SESSION_PERSISTENCE.md) - Session storage architecture
- [Shared API Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/SHARED_API_REFERENCE.md) - Complete API documentation

---

*Part of [octocode-shared](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/README.md) - Shared utilities for Octocode packages*
