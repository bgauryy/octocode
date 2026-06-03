/**
 * Token Refresh
 *
 * OAuth token refresh logic for expired tokens.
 *
 * Dependencies (getCredentials, updateToken) are injected via parameters
 * to avoid a circular import with storage.ts.
 */

import { refreshToken as octokitRefreshToken } from '@octokit/oauth-methods';
import { request } from '@octokit/request';
import type { OAuthToken, StoredCredentials } from './types.js';
import { isRefreshTokenExpired, isTokenExpired } from './credentialUtils.js';
import {
  OCTOCODE_GITHUB_APP_CLIENT_ID,
  DEFAULT_HOSTNAME,
} from './constants.js';

/**
 * Mask sensitive data in error messages to prevent token leakage in logs.
 * Matches common token patterns (GitHub tokens, OAuth tokens, etc.)
 */
function maskErrorMessage(message: string): string {
  // Mask GitHub tokens (ghp_, gho_, ghu_, ghs_, ghr_ prefixes)
  // Mask generic long alphanumeric strings that look like tokens
  return message
    .replace(/\b(ghp_|gho_|ghu_|ghs_|ghr_)[a-zA-Z0-9]{36,}\b/g, '***MASKED***')
    .replace(/\b[a-zA-Z0-9]{40,}\b/g, '***MASKED***');
}

/**
 * Get GitHub API base URL for a hostname
 */
function getApiBaseUrl(hostname: string): string {
  if (hostname === 'github.com' || hostname === DEFAULT_HOSTNAME) {
    return 'https://api.github.com';
  }
  return `https://${hostname}/api/v3`;
}

/**
 * Result of a token refresh operation
 */
export interface RefreshResult {
  success: boolean;
  username?: string;
  hostname?: string;
  error?: string;
}

/** Dependency: function to get credentials from storage */
type GetCredentialsFn = (
  hostname?: string
) => Promise<StoredCredentials | null>;

/** Dependency: function to update a token in storage */
type UpdateTokenFn = (hostname: string, token: OAuthToken) => Promise<boolean>;

/**
 * Refresh an expired OAuth token using the refresh token
 *
 * @param deps - Injected dependencies (getCredentials, updateToken)
 * @param hostname - GitHub hostname (default: 'github.com')
 * @param clientId - OAuth client ID (default: octocode client ID)
 * @returns RefreshResult with success status and error details
 */
export async function refreshAuthToken(
  deps: { getCredentials: GetCredentialsFn; updateToken: UpdateTokenFn },
  hostname: string = DEFAULT_HOSTNAME,
  clientId: string = OCTOCODE_GITHUB_APP_CLIENT_ID
): Promise<RefreshResult> {
  const credentials = await deps.getCredentials(hostname);

  if (!credentials) {
    return {
      success: false,
      error: `Not logged in to ${hostname}`,
    };
  }

  if (!credentials.token.refreshToken) {
    return {
      success: false,
      error: 'Token does not support refresh (OAuth App tokens do not expire)',
    };
  }

  if (isRefreshTokenExpired(credentials)) {
    return {
      success: false,
      error: 'Refresh token has expired. Please login again.',
    };
  }

  try {
    const response = await octokitRefreshToken({
      clientType: 'github-app',
      clientId,
      clientSecret: '', // Empty for OAuth apps
      refreshToken: credentials.token.refreshToken,
      request: request.defaults({
        baseUrl: getApiBaseUrl(hostname),
      }),
    } as Parameters<typeof octokitRefreshToken>[0]);

    const newToken: OAuthToken = {
      token: response.authentication.token,
      tokenType: 'oauth',
      refreshToken: response.authentication.refreshToken,
      expiresAt: response.authentication.expiresAt,
      refreshTokenExpiresAt: response.authentication.refreshTokenExpiresAt,
    };

    await deps.updateToken(hostname, newToken);

    return {
      success: true,
      username: credentials.username,
      hostname,
    };
  } catch (error) {
    // Mask potential sensitive data in error messages
    const errorMsg =
      error instanceof Error
        ? maskErrorMessage(error.message)
        : 'Token refresh failed';
    return {
      success: false,
      error: errorMsg,
    };
  }
}

/**
 * Result of getting a token with refresh capability
 */
export interface TokenWithRefreshResult {
  token: string | null;
  source: 'stored' | 'refreshed' | 'none';
  username?: string;
  refreshError?: string;
}

/**
 * Get token with automatic refresh for expired tokens
 *
 * This is the recommended function for getting stored tokens. It will:
 * 1. Check if credentials exist
 * 2. If token is expired and has a refresh token, attempt to refresh
 * 3. Return the valid token or null
 *
 * NOTE: This does NOT check environment variables. Use resolveTokenWithRefresh()
 * for full resolution including env vars.
 *
 * @param deps - Injected dependencies (getCredentials, updateToken)
 * @param hostname - GitHub hostname (default: 'github.com')
 * @param clientId - OAuth client ID for refresh (default: octocode client ID)
 * @returns TokenWithRefreshResult with token, source, and any refresh errors
 */
export async function getTokenWithRefresh(
  deps: { getCredentials: GetCredentialsFn; updateToken: UpdateTokenFn },
  hostname: string = DEFAULT_HOSTNAME,
  clientId: string = OCTOCODE_GITHUB_APP_CLIENT_ID
): Promise<TokenWithRefreshResult> {
  const credentials = await deps.getCredentials(hostname);

  if (!credentials || !credentials.token) {
    return { token: null, source: 'none' };
  }

  // Token is valid - return it
  if (!isTokenExpired(credentials)) {
    return {
      token: credentials.token.token,
      source: 'stored',
      username: credentials.username,
    };
  }

  // Token is expired - try to refresh if we have a refresh token
  if (credentials.token.refreshToken) {
    const refreshResult = await refreshAuthToken(deps, hostname, clientId);

    if (refreshResult.success) {
      // Get the updated credentials after refresh
      const updatedCredentials = await deps.getCredentials(hostname);
      if (updatedCredentials?.token.token) {
        return {
          token: updatedCredentials.token.token,
          source: 'refreshed',
          username: updatedCredentials.username,
        };
      }
    }

    // Refresh failed
    return {
      token: null,
      source: 'none',
      refreshError: refreshResult.error,
    };
  }

  // No refresh token available and token is expired
  return {
    token: null,
    source: 'none',
    refreshError: 'Token expired and no refresh token available',
  };
}
