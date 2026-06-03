/**
 * Credentials Module Exports
 */

// Types
export type {
  OAuthToken,
  StoredCredentials,
  StoreResult,
  DeleteResult,
  CredentialsStore,
  TokenSource,
} from './types.js';

// Storage functions
export {
  // CRUD operations
  storeCredentials,
  getCredentials,
  type GetCredentialsOptions,
  getCredentialsSync,
  deleteCredentials,
  updateToken,

  // Cache management
  invalidateCredentialsCache,

  // Token retrieval (convenience)
  getToken,
  getTokenSync,
  resolveToken,
  type ResolvedToken,

  // Token retrieval with refresh (recommended)
  getTokenWithRefresh,
  type TokenWithRefreshResult,
  resolveTokenWithRefresh,
  type ResolvedTokenWithRefresh,
  refreshAuthToken,
  type RefreshResult,

  // Full token resolution with gh CLI fallback (recommended for CLI/MCP)
  resolveTokenFull,
  type FullTokenResolution,
  type GhCliTokenGetter,

  // Reset resolution state (testing only — prefer octocode-shared/testing)
  resetTokenResolution,

  // List/check operations
  listStoredHosts,
  listStoredHostsSync,
  hasCredentials,
  hasCredentialsSync,

  // Token expiration checks
  isTokenExpired,
  isRefreshTokenExpired,

  // Utility
  getCredentialsFilePath,

  // File storage helpers (for advanced use cases)
  readCredentialsStore,
  encrypt,
  decrypt,
  ensureOctocodeDir,

  // Constants
  OCTOCODE_DIR,
  CREDENTIALS_FILE,
  KEY_FILE,
  ENV_TOKEN_VARS,

  // Environment variable support
  getTokenFromEnv,
  getEnvTokenSource,
  hasEnvToken,
} from './storage.js';

// gh CLI token getter (default used by resolveTokenFull)
export { getGhCliToken } from './ghCli.js';
