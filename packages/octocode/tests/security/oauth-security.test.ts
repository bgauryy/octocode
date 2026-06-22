import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('node:fs', () => ({
  existsSync: vi.fn().mockReturnValue(false),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  unlinkSync: vi.fn(),
  rmSync: vi.fn(),
  statSync: vi.fn(),
  promises: {
    readFile: vi.fn(),
    writeFile: vi.fn(),
    mkdir: vi.fn(),
    unlink: vi.fn(),
    stat: vi.fn(),
  },
}));

vi.mock('node:crypto', () => ({
  randomBytes: vi.fn().mockReturnValue(Buffer.alloc(32)),
  createCipheriv: vi.fn().mockReturnValue({
    update: vi.fn().mockReturnValue('encrypted'),
    final: vi.fn().mockReturnValue(''),
    getAuthTag: vi.fn().mockReturnValue(Buffer.alloc(16)),
  }),
  createDecipheriv: vi.fn().mockReturnValue({
    update: vi.fn().mockReturnValue('{}'),
    final: vi.fn().mockReturnValue(''),
    setAuthTag: vi.fn(),
  }),
}));

vi.mock('@octokit/auth-oauth-device', () => ({
  createOAuthDeviceAuth: vi.fn(),
}));

const mockDeleteToken = vi.fn();
vi.mock('@octokit/oauth-methods', () => ({
  refreshToken: vi.fn(),
  deleteToken: mockDeleteToken,
  checkToken: vi.fn(),
}));

const mockRequest = vi.fn().mockResolvedValue({
  data: { login: 'testuser' },
  status: 200,
  headers: {},
  url: 'https://api.github.com/user',
}) as ReturnType<typeof vi.fn> & { defaults: ReturnType<typeof vi.fn> };
mockRequest.defaults = vi.fn().mockReturnValue(mockRequest);
vi.mock('@octokit/request', () => ({ request: mockRequest }));

vi.mock('open', () => ({ default: vi.fn().mockResolvedValue(undefined) }));

const ENV_TOKEN_VARS = ['OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN'];

const mockGetCredentials = vi.fn().mockResolvedValue(null);
const mockDeleteCredentials = vi.fn().mockResolvedValue({ success: true });
const mockIsTokenExpired = vi.fn().mockReturnValue(false);
const mockGetTokenWithRefresh = vi
  .fn()
  .mockResolvedValue({ token: null, source: 'none' });

vi.mock('../../src/utils/token-storage.js', () => ({
  storeCredentials: vi.fn().mockResolvedValue({ success: true }),
  getCredentials: mockGetCredentials,
  getCredentialsSync: vi.fn().mockReturnValue(null),
  deleteCredentials: mockDeleteCredentials,
  isTokenExpired: mockIsTokenExpired,
  isRefreshTokenExpired: vi.fn(),
  updateToken: vi.fn().mockResolvedValue(true),
  getCredentialsFilePath: vi
    .fn()
    .mockReturnValue('/home/test/.octocode/credentials.json'),
  hasEnvToken: vi.fn().mockImplementation(() =>
    ENV_TOKEN_VARS.some(v => {
      const t = process.env[v];
      return t && t.trim();
    })
  ),
  getTokenFromEnv: vi.fn().mockImplementation(() => {
    for (const v of ENV_TOKEN_VARS) {
      const t = process.env[v];
      if (t?.trim()) return t.trim();
    }
    return null;
  }),
  getEnvTokenSource: vi.fn().mockImplementation(() => {
    for (const v of ENV_TOKEN_VARS) {
      const t = process.env[v];
      if (t?.trim()) return `env:${v}`;
    }
    return null;
  }),
  resolveTokenFull: vi.fn(),
  refreshAuthToken: vi.fn(),
  getTokenWithRefresh: mockGetTokenWithRefresh,
  getGhCliToken: vi.fn().mockReturnValue(null),
}));

vi.mock('../../src/features/gh-auth.js', () => ({
  checkGitHubAuth: vi
    .fn()
    .mockReturnValue({ authenticated: false, username: null }),
}));

const savedEnv: Record<string, string | undefined> = {};

function saveAndClearEnv() {
  for (const v of ENV_TOKEN_VARS) {
    savedEnv[v] = process.env[v];
    delete process.env[v];
  }
}
function restoreEnv() {
  for (const v of ENV_TOKEN_VARS) {
    if (savedEnv[v] !== undefined) process.env[v] = savedEnv[v];
    else delete process.env[v];
  }
}

describe('OAuth Security Gaps', () => {
  beforeEach(() => {
    saveAndClearEnv();
    vi.clearAllMocks();
  });
  afterEach(restoreEnv);

  describe('getAuthStatusAsync', () => {
    it('returns env source when env token is set', async () => {
      process.env.GH_TOKEN = 'env-tok';
      const { getAuthStatusAsync } =
        await import('../../src/features/github-oauth.js');
      const status = await getAuthStatusAsync();
      expect(status.authenticated).toBe(true);
      expect(status.tokenSource).toBe('env');
      expect(status.envTokenSource).toBe('env:GH_TOKEN');
    });

    it('returns octocode source from async getCredentials', async () => {
      mockGetCredentials.mockResolvedValueOnce({
        hostname: 'github.com',
        username: 'asyncuser',
        token: { token: 'gho_abc', type: 'oauth' },
      });
      mockIsTokenExpired.mockReturnValueOnce(false);

      const { getAuthStatusAsync } =
        await import('../../src/features/github-oauth.js');
      const status = await getAuthStatusAsync();
      expect(status.authenticated).toBe(true);
      expect(status.username).toBe('asyncuser');
      expect(status.tokenSource).toBe('octocode');
    });

    it('reports not authenticated when token is expired', async () => {
      mockGetCredentials.mockResolvedValueOnce({
        hostname: 'github.com',
        username: 'expireduser',
        token: { token: 'old', type: 'oauth', expiresAt: '2020-01-01' },
      });
      mockIsTokenExpired.mockReturnValueOnce(true);

      const { getAuthStatusAsync } =
        await import('../../src/features/github-oauth.js');
      const status = await getAuthStatusAsync();
      expect(status.authenticated).toBe(false);
      expect(status.tokenExpired).toBe(true);
    });

    it('returns not authenticated when no source available', async () => {
      const { getAuthStatusAsync } =
        await import('../../src/features/github-oauth.js');
      const status = await getAuthStatusAsync();
      expect(status.authenticated).toBe(false);
      expect(status.tokenSource).toBe('none');
    });
  });

  describe('logout with token revocation', () => {
    it('calls deleteToken when clientSecret provided', async () => {
      mockGetCredentials.mockResolvedValueOnce({
        hostname: 'github.com',
        username: 'user1',
        token: { token: 'gho_secret123', type: 'oauth' },
      });

      const { logout } = await import('../../src/features/github-oauth.js');
      const result = await logout('github.com', {
        clientSecret: 'client-sec',
      });

      expect(result.success).toBe(true);
      expect(mockDeleteToken).toHaveBeenCalledOnce();
      expect(mockDeleteCredentials).toHaveBeenCalledWith('github.com');
    });

    it('still deletes credentials when revocation fails', async () => {
      mockGetCredentials.mockResolvedValueOnce({
        hostname: 'github.com',
        username: 'user1',
        token: { token: 'tok', type: 'oauth' },
      });
      mockDeleteToken.mockRejectedValueOnce(new Error('API error'));

      const stderrSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

      const { logout } = await import('../../src/features/github-oauth.js');
      const result = await logout('github.com', {
        clientSecret: 'sec',
      });

      expect(result.success).toBe(true);
      expect(mockDeleteCredentials).toHaveBeenCalledWith('github.com');
      stderrSpy.mockRestore();
    });

    it('skips revocation when no clientSecret', async () => {
      mockGetCredentials.mockResolvedValueOnce({
        hostname: 'github.com',
        username: 'user1',
        token: { token: 'tok', type: 'oauth' },
      });

      const { logout } = await import('../../src/features/github-oauth.js');
      await logout('github.com');

      expect(mockDeleteToken).not.toHaveBeenCalled();
      expect(mockDeleteCredentials).toHaveBeenCalledWith('github.com');
    });

    it('returns error when not signed in', async () => {
      const { logout } = await import('../../src/features/github-oauth.js');
      const result = await logout('github.com');
      expect(result.success).toBe(false);
      expect(result.error).toContain('Not signed in');
    });
  });

  describe('getOctocodeToken', () => {
    it('returns token with source=octocode when available', async () => {
      mockGetTokenWithRefresh.mockResolvedValueOnce({
        token: 'gho_valid',
        source: 'cache',
        username: 'tokuser',
      });

      const { getOctocodeToken } =
        await import('../../src/features/github-oauth.js');
      const result = await getOctocodeToken();
      expect(result).toMatchObject({
        token: 'gho_valid',
        source: 'octocode',
        username: 'tokuser',
      });
    });

    it('returns source=none when no token', async () => {
      const { getOctocodeToken } =
        await import('../../src/features/github-oauth.js');
      const result = await getOctocodeToken();
      expect(result.token).toBeNull();
      expect(result.source).toBe('none');
    });
  });

  describe('getGhCliToken', () => {
    it('returns gh-cli source when gh auth provides token', async () => {
      const tokenStorage = await import('../../src/utils/token-storage.js');
      vi.mocked(tokenStorage.getGhCliToken).mockResolvedValue('ghp_clitoken');

      const { getGhCliToken } =
        await import('../../src/features/github-oauth.js');
      const result = await getGhCliToken();
      expect(result.source).toBe('gh-cli');
      expect(result.token).toBe('ghp_clitoken');
    });

    it('returns source=none when gh CLI has no token', async () => {
      const tokenStorage = await import('../../src/utils/token-storage.js');
      vi.mocked(tokenStorage.getGhCliToken).mockResolvedValue(null);

      const { getGhCliToken } =
        await import('../../src/features/github-oauth.js');
      const result = await getGhCliToken();
      expect(result.source).toBe('none');
      expect(result.token).toBeNull();
    });
  });

  describe('Token leak prevention in error messages', () => {
    it('logout error does not expose token in console.error', async () => {
      const secretToken = 'gho_SUPERSECRET12345678901234567890ab';
      mockGetCredentials.mockResolvedValueOnce({
        hostname: 'github.com',
        username: 'u',
        token: { token: secretToken, type: 'oauth' },
      });
      mockDeleteToken.mockRejectedValueOnce(
        new Error(`API error for token ${secretToken}`)
      );

      const errors: string[] = [];
      const stderrSpy = vi
        .spyOn(console, 'error')
        .mockImplementation((...args: unknown[]) => {
          errors.push(args.map(String).join(' '));
        });

      const { logout } = await import('../../src/features/github-oauth.js');
      await logout('github.com', { clientSecret: 'sec' });

      for (const msg of errors) {
        expect(msg).toContain('Token revocation failed');
      }

      stderrSpy.mockRestore();
    });
  });
});
