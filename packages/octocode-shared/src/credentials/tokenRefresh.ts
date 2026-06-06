import { refreshToken as octokitRefreshToken } from '@octokit/oauth-methods';
import { request } from '@octokit/request';
import type { OAuthToken, StoredCredentials } from './types.js';
import { isRefreshTokenExpired, isTokenExpired } from './credentialUtils.js';
import {
  OCTOCODE_GITHUB_APP_CLIENT_ID,
  DEFAULT_HOSTNAME,
} from './constants.js';

function maskErrorMessage(message: string): string {
  return message
    .replace(/\b(ghp_|gho_|ghu_|ghs_|ghr_)[a-zA-Z0-9]{36,}\b/g, '***MASKED***')
    .replace(/\b[a-zA-Z0-9]{40,}\b/g, '***MASKED***');
}

function getApiBaseUrl(hostname: string): string {
  if (hostname === 'github.com' || hostname === DEFAULT_HOSTNAME) {
    return 'https://api.github.com';
  }
  return `https://${hostname}/api/v3`;
}

export interface RefreshResult {
  success: boolean;
  username?: string;
  hostname?: string;
  error?: string;
}

type GetCredentialsFn = (
  hostname?: string
) => Promise<StoredCredentials | null>;

type UpdateTokenFn = (hostname: string, token: OAuthToken) => Promise<boolean>;

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
      clientSecret: '',
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

export interface TokenWithRefreshResult {
  token: string | null;
  source: 'stored' | 'refreshed' | 'none';
  username?: string;
  refreshError?: string;
}

export async function getTokenWithRefresh(
  deps: { getCredentials: GetCredentialsFn; updateToken: UpdateTokenFn },
  hostname: string = DEFAULT_HOSTNAME,
  clientId: string = OCTOCODE_GITHUB_APP_CLIENT_ID
): Promise<TokenWithRefreshResult> {
  const credentials = await deps.getCredentials(hostname);

  if (!credentials || !credentials.token) {
    return { token: null, source: 'none' };
  }

  if (!isTokenExpired(credentials)) {
    return {
      token: credentials.token.token,
      source: 'stored',
      username: credentials.username,
    };
  }

  if (credentials.token.refreshToken) {
    const refreshResult = await refreshAuthToken(deps, hostname, clientId);

    if (refreshResult.success) {
      const updatedCredentials = await deps.getCredentials(hostname);
      if (updatedCredentials?.token.token) {
        return {
          token: updatedCredentials.token.token,
          source: 'refreshed',
          username: updatedCredentials.username,
        };
      }
    }

    return {
      token: null,
      source: 'none',
      refreshError: refreshResult.error,
    };
  }

  return {
    token: null,
    source: 'none',
    refreshError: 'Token expired and no refresh token available',
  };
}
