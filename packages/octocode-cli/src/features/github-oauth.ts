import { createOAuthDeviceAuth } from '@octokit/auth-oauth-device';
import { deleteToken } from '@octokit/oauth-methods';
import { request } from '@octokit/request';
import open from 'open';
import type {
  OAuthToken,
  StoredCredentials,
  OctocodeAuthStatus,
  TokenResult,
  TokenSource,
} from '../types/index.js';
import { checkGitHubAuth } from './gh-auth.js';
import {
  storeCredentials,
  getCredentials,
  deleteCredentials,
  isTokenExpired,
  getCredentialsFilePath,
  getCredentialsSync,
  getEnvTokenSource,
  hasEnvToken,
  resolveTokenFull,
  refreshAuthToken as sharedRefreshAuthToken,
  getTokenWithRefresh,
  getGhCliToken as sharedGetGhCliToken,
} from '../utils/token-storage.js';

const DEFAULT_CLIENT_ID = '178c6fc778ccc68e1d6a';

const DEFAULT_SCOPES = ['repo', 'read:org', 'gist'];

const DEFAULT_HOSTNAME = 'github.com';

interface LoginOptions {
  hostname?: string;

  scopes?: string[];

  gitProtocol?: 'ssh' | 'https';

  clientId?: string;

  onVerification?: (verification: VerificationInfo) => void;

  openBrowser?: boolean;
}

export interface VerificationInfo {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

interface LoginResult {
  success: boolean;
  username?: string;
  hostname?: string;
  error?: string;
}

interface LogoutResult {
  success: boolean;
  error?: string;
}

function getApiBaseUrl(hostname: string): string {
  if (hostname === 'github.com' || hostname === DEFAULT_HOSTNAME) {
    return 'https://api.github.com';
  }

  return `https://${hostname}/api/v3`;
}

async function getCurrentUser(
  token: string,
  hostname: string
): Promise<string> {
  const baseUrl = getApiBaseUrl(hostname);

  const response = await request('GET /user', {
    headers: {
      authorization: `token ${token}`,
    },
    baseUrl,
  });

  return response.data.login;
}

export async function login(options: LoginOptions = {}): Promise<LoginResult> {
  const {
    hostname = DEFAULT_HOSTNAME,
    scopes = DEFAULT_SCOPES,
    gitProtocol = 'https',
    clientId = DEFAULT_CLIENT_ID,
    onVerification,
    openBrowser = true,
  } = options;

  try {
    const auth = createOAuthDeviceAuth({
      clientType: 'oauth-app',
      clientId,
      scopes,
      onVerification: async verification => {
        if (onVerification) {
          onVerification(verification as VerificationInfo);
        }

        if (openBrowser) {
          try {
            await open(verification.verification_uri);
          } catch {
            console.log();
            console.log('  \u26A0 Could not open browser automatically.');
            console.log('  \u2192 Please open this URL manually:');
            console.log(`    ${verification.verification_uri}`);
            console.log();
          }
        }
      },
      request: request.defaults({
        baseUrl: getApiBaseUrl(hostname),
      }),
    });

    const tokenAuth = await auth({ type: 'oauth' });

    const username = await getCurrentUser(tokenAuth.token, hostname);

    const token: OAuthToken = {
      token: tokenAuth.token,
      tokenType: 'oauth',
      scopes: 'scopes' in tokenAuth ? tokenAuth.scopes : undefined,
    };

    if ('refreshToken' in tokenAuth && tokenAuth.refreshToken) {
      token.refreshToken = tokenAuth.refreshToken as string;
      token.expiresAt =
        'expiresAt' in tokenAuth ? (tokenAuth.expiresAt as string) : undefined;
      token.refreshTokenExpiresAt =
        'refreshTokenExpiresAt' in tokenAuth
          ? (tokenAuth.refreshTokenExpiresAt as string)
          : undefined;
    }

    const credentials: StoredCredentials = {
      hostname,
      username,
      token,
      gitProtocol,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };

    await storeCredentials(credentials);

    return {
      success: true,
      username,
      hostname,
    };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Authentication failed',
    };
  }
}

export async function logout(
  hostname: string = DEFAULT_HOSTNAME,
  options?: { clientSecret?: string }
): Promise<LogoutResult> {
  const credentials = await getCredentials(hostname);

  if (!credentials) {
    return {
      success: false,
      error: `Not logged in to ${hostname}`,
    };
  }

  if (options?.clientSecret) {
    try {
      await deleteToken({
        clientType: 'oauth-app',
        clientId: DEFAULT_CLIENT_ID,
        clientSecret: options.clientSecret,
        token: credentials.token.token,
        request: request.defaults({
          baseUrl: getApiBaseUrl(hostname),
        }),
      });
    } catch (error) {
      console.error(
        `[github-oauth] Token revocation failed: ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }

  await deleteCredentials(hostname);

  return { success: true };
}

export async function refreshAuthToken(
  hostname: string = DEFAULT_HOSTNAME
): Promise<LoginResult> {
  return sharedRefreshAuthToken(hostname, DEFAULT_CLIENT_ID);
}

export function getAuthStatus(
  hostname: string = DEFAULT_HOSTNAME
): OctocodeAuthStatus {
  if (hasEnvToken()) {
    const envSource = getEnvTokenSource();
    return {
      authenticated: true,
      hostname,
      username: undefined,
      tokenSource: 'env',

      envTokenSource: envSource ?? undefined,
    };
  }

  const credentials = getCredentialsSync(hostname);
  if (credentials) {
    const tokenExpired = isTokenExpired(credentials);
    return {
      authenticated: !tokenExpired,
      hostname: credentials.hostname,
      username: credentials.username,
      tokenExpired,
      tokenSource: 'octocode',
    };
  }

  const ghAuth = checkGitHubAuth();
  if (ghAuth.authenticated) {
    return {
      authenticated: true,
      hostname,
      username: ghAuth.username,
      tokenSource: 'gh-cli',
    };
  }

  return {
    authenticated: false,
    tokenSource: 'none',
  };
}

export async function getAuthStatusAsync(
  hostname: string = DEFAULT_HOSTNAME
): Promise<OctocodeAuthStatus> {
  if (hasEnvToken()) {
    const envSource = getEnvTokenSource();
    return {
      authenticated: true,
      hostname,
      username: undefined,
      tokenSource: 'env',
      envTokenSource: envSource ?? undefined,
    };
  }

  const credentials = await getCredentials(hostname);
  if (credentials) {
    const tokenExpired = isTokenExpired(credentials);
    return {
      authenticated: !tokenExpired,
      hostname: credentials.hostname,
      username: credentials.username,
      tokenExpired,
      tokenSource: 'octocode',
    };
  }

  const ghAuth = checkGitHubAuth();
  if (ghAuth.authenticated) {
    return {
      authenticated: true,
      hostname,
      username: ghAuth.username,
      tokenSource: 'gh-cli',
    };
  }

  return {
    authenticated: false,
    tokenSource: 'none',
  };
}

export async function getValidToken(
  hostname: string = DEFAULT_HOSTNAME
): Promise<string | null> {
  const result = await getTokenWithRefresh(hostname, DEFAULT_CLIENT_ID);
  return result.token;
}

export async function getOctocodeToken(
  hostname: string = DEFAULT_HOSTNAME
): Promise<TokenResult> {
  const result = await getTokenWithRefresh(hostname, DEFAULT_CLIENT_ID);

  if (result.token) {
    return {
      token: result.token,
      source: 'octocode',
      username: result.username,
    };
  }

  return {
    token: null,
    source: 'none',
  };
}

export async function getGhCliToken(
  hostname: string = DEFAULT_HOSTNAME
): Promise<TokenResult> {
  const ghToken = await sharedGetGhCliToken(hostname);

  if (ghToken) {
    const ghAuth = checkGitHubAuth();
    return {
      token: ghToken,
      source: 'gh-cli',
      username: ghAuth.username,
    };
  }

  return {
    token: null,
    source: 'none',
  };
}

type GetTokenSource = 'octocode' | 'gh' | 'auto';

export async function getToken(
  hostname: string = DEFAULT_HOSTNAME,
  preferredSource: GetTokenSource = 'auto'
): Promise<TokenResult> {
  if (preferredSource === 'octocode') {
    return getOctocodeToken(hostname);
  }

  if (preferredSource === 'gh') {
    return getGhCliToken(hostname);
  }

  const result = await resolveTokenFull({ hostname });

  if (result?.token) {
    const source: TokenSource =
      result.source === 'gh-cli'
        ? 'gh-cli'
        : result.source?.startsWith('env:')
          ? 'env'
          : 'octocode';

    return {
      token: result.token,
      source,
      username: result.username,
      envSource: result.source?.startsWith('env:') ? result.source : undefined,
    };
  }

  return {
    token: null,
    source: 'none',
  };
}

export function getStoragePath(): string {
  return getCredentialsFilePath();
}

export function getTokenType(source: TokenSource, envSource?: string): string {
  switch (source) {
    case 'env':
      return envSource ?? 'env:GITHUB_TOKEN';
    case 'gh-cli':
      return 'gh-cli';
    case 'octocode':
      return 'octocode-storage';
    case 'none':
    default:
      return 'none';
  }
}
