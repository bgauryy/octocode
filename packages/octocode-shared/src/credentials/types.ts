/**
 * Credential Types
 *
 * Shared types for OAuth tokens and credential storage across octocode packages.
 */

/**
 * OAuth token structure
 */
export interface OAuthToken {
  token: string;
  tokenType: 'oauth';
  scopes?: string[];
  // For GitHub Apps with expiring tokens
  refreshToken?: string;
  expiresAt?: string;
  refreshTokenExpiresAt?: string;
}

/**
 * Stored credentials for a GitHub host
 */
export interface StoredCredentials {
  hostname: string;
  username: string;
  token: OAuthToken;
  gitProtocol: 'ssh' | 'https';
  createdAt: string;
  updatedAt: string;
}

/**
 * Result from storing credentials
 */
export interface StoreResult {
  success: boolean;
}

/**
 * Result from deleting credentials
 */
export interface DeleteResult {
  success: boolean;
  deletedFromFile: boolean;
}

/**
 * Storage interface for credentials (file storage)
 */
export interface CredentialsStore {
  version: number;
  credentials: Record<string, StoredCredentials>;
}

/**
 * Token source identifier for debugging and display
 *
 * Priority order:
 * 1. Environment variables (OCTOCODE_TOKEN > GH_TOKEN > GITHUB_TOKEN)
 * 2. Encrypted file storage (~/.octocode/credentials.json)
 * 3. gh CLI stored token (external fallback)
 */
export type TokenSource =
  | 'env:OCTOCODE_TOKEN'
  | 'env:GH_TOKEN'
  | 'env:GITHUB_TOKEN'
  | 'octocode-storage'
  | 'gh-cli'
  | null;
