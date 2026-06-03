import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('octocode-shared', () => ({
  storeCredentials: vi.fn().mockResolvedValue({ success: true }),
  getCredentials: vi.fn().mockResolvedValue(null),
  getCredentialsSync: vi.fn().mockReturnValue(null),
  deleteCredentials: vi.fn().mockResolvedValue({
    success: true,
    deletedFromFile: false,
  }),
  updateToken: vi.fn().mockResolvedValue(true),
  listStoredHosts: vi.fn().mockResolvedValue([]),
  listStoredHostsSync: vi.fn().mockReturnValue([]),
  hasCredentials: vi.fn().mockResolvedValue(false),
  hasCredentialsSync: vi.fn().mockReturnValue(false),
  isTokenExpired: vi.fn().mockReturnValue(false),
  isRefreshTokenExpired: vi.fn().mockReturnValue(false),
  refreshAuthToken: vi
    .fn()
    .mockResolvedValue({ success: false, error: 'Mock' }),
  getTokenWithRefresh: vi
    .fn()
    .mockResolvedValue({ token: null, source: 'none' }),
  getCredentialsFilePath: vi
    .fn()
    .mockReturnValue('/mock/.octocode/credentials.json'),
  getTokenFromEnv: vi.fn().mockReturnValue(null),
  getEnvTokenSource: vi.fn().mockReturnValue(null),
  hasEnvToken: vi.fn().mockReturnValue(false),
  ENV_TOKEN_VARS: ['OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN'],
  resolveTokenFull: vi.fn().mockResolvedValue(null),

  isWindows: false,
  isMac: true,
  isLinux: false,
  HOME: '/Users/test',
}));

function createTestCredentials(overrides = {}) {
  return {
    hostname: 'github.com',
    username: 'testuser',
    token: {
      token: 'test-token',
      tokenType: 'oauth' as const,
    },
    gitProtocol: 'https' as const,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...overrides,
  };
}

describe('Token Storage (CLI re-exports from octocode-shared)', () => {
  beforeEach(async () => {
    vi.clearAllMocks();

    const shared = await import('octocode-shared');
    vi.mocked(shared.storeCredentials).mockResolvedValue({ success: true });
    vi.mocked(shared.getCredentials).mockResolvedValue(null);
    vi.mocked(shared.getCredentialsSync).mockReturnValue(null);
    vi.mocked(shared.deleteCredentials).mockResolvedValue({
      success: true,
      deletedFromFile: false,
    });
    vi.mocked(shared.updateToken).mockResolvedValue(true);
    vi.mocked(shared.listStoredHosts).mockResolvedValue([]);
    vi.mocked(shared.listStoredHostsSync).mockReturnValue([]);
    vi.mocked(shared.hasCredentials).mockResolvedValue(false);
    vi.mocked(shared.hasCredentialsSync).mockReturnValue(false);
    vi.mocked(shared.isTokenExpired).mockReturnValue(false);
    vi.mocked(shared.isRefreshTokenExpired).mockReturnValue(false);
    vi.mocked(shared.refreshAuthToken).mockResolvedValue({
      success: false,
      error: 'Mock',
    });
    vi.mocked(shared.getTokenWithRefresh).mockResolvedValue({
      token: null,
      source: 'none',
    });
    vi.mocked(shared.getCredentialsFilePath).mockReturnValue(
      '/mock/.octocode/credentials.json'
    );
    vi.mocked(shared.getTokenFromEnv).mockReturnValue(null);
    vi.mocked(shared.getEnvTokenSource).mockReturnValue(null);
    vi.mocked(shared.hasEnvToken).mockReturnValue(false);
    vi.mocked(shared.resolveTokenFull).mockResolvedValue(null);
  });

  describe('storeCredentials', () => {
    it('should delegate to shared package', async () => {
      const shared = await import('octocode-shared');
      const { storeCredentials } =
        await import('../../src/utils/token-storage.js');

      const creds = createTestCredentials();
      vi.mocked(shared.storeCredentials).mockResolvedValue({ success: true });

      const result = await storeCredentials(creds);

      expect(result.success).toBe(true);
      expect(shared.storeCredentials).toHaveBeenCalledWith(creds);
    });
  });

  describe('getCredentials', () => {
    it('should return null when no credentials exist', async () => {
      const { getCredentials } =
        await import('../../src/utils/token-storage.js');

      const result = await getCredentials('github.com');

      expect(result).toBeNull();
    });

    it('should return credentials when they exist', async () => {
      const shared = await import('octocode-shared');
      const { getCredentials } =
        await import('../../src/utils/token-storage.js');

      const creds = createTestCredentials();
      vi.mocked(shared.getCredentials).mockResolvedValue(creds);

      const result = await getCredentials('github.com');

      expect(result).toEqual(creds);
    });
  });

  describe('getCredentialsSync', () => {
    it('should delegate to shared package', async () => {
      const shared = await import('octocode-shared');
      const { getCredentialsSync } =
        await import('../../src/utils/token-storage.js');

      getCredentialsSync('github.com');

      expect(shared.getCredentialsSync).toHaveBeenCalledWith('github.com');
    });
  });

  describe('deleteCredentials', () => {
    it('should return result from shared package', async () => {
      const shared = await import('octocode-shared');
      const { deleteCredentials } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.deleteCredentials).mockResolvedValue({
        success: true,
        deletedFromFile: true,
      });

      const result = await deleteCredentials('github.com');

      expect(result.success).toBe(true);
      expect(result.deletedFromFile).toBe(true);
    });
  });

  describe('isTokenExpired', () => {
    it('should return false for non-expiring tokens', async () => {
      const shared = await import('octocode-shared');
      const { isTokenExpired } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.isTokenExpired).mockReturnValue(false);

      const creds = createTestCredentials();
      const result = isTokenExpired(creds);

      expect(result).toBe(false);
    });

    it('should return true for expired tokens', async () => {
      const shared = await import('octocode-shared');
      const { isTokenExpired } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.isTokenExpired).mockReturnValue(true);

      const creds = createTestCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
        },
      });

      const result = isTokenExpired(creds);

      expect(result).toBe(true);
    });
  });

  describe('refreshAuthToken', () => {
    it('should delegate to shared package', async () => {
      const shared = await import('octocode-shared');
      const { refreshAuthToken } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.refreshAuthToken).mockResolvedValue({
        success: true,
        username: 'testuser',
        hostname: 'github.com',
      });

      const result = await refreshAuthToken('github.com');

      expect(result.success).toBe(true);
      expect(shared.refreshAuthToken).toHaveBeenCalled();
    });
  });

  describe('getTokenWithRefresh', () => {
    it('should return token when available', async () => {
      const shared = await import('octocode-shared');
      const { getTokenWithRefresh } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.getTokenWithRefresh).mockResolvedValue({
        token: 'test-token',
        source: 'stored',
        username: 'testuser',
      });

      const result = await getTokenWithRefresh('github.com');

      expect(result.token).toBe('test-token');
      expect(result.source).toBe('stored');
    });

    it('should return null when no token available', async () => {
      const shared = await import('octocode-shared');
      const { getTokenWithRefresh } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.getTokenWithRefresh).mockResolvedValue({
        token: null,
        source: 'none',
      });

      const result = await getTokenWithRefresh('github.com');

      expect(result.token).toBeNull();
    });
  });

  describe('getCredentialsFilePath', () => {
    it('should return path from shared package', async () => {
      const shared = await import('octocode-shared');
      const { getCredentialsFilePath } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.getCredentialsFilePath).mockReturnValue(
        '/home/user/.octocode/credentials.json'
      );

      const result = getCredentialsFilePath();

      expect(result).toBe('/home/user/.octocode/credentials.json');
    });
  });

  describe('getEnvTokenSource', () => {
    it('should return source from shared package', async () => {
      const shared = await import('octocode-shared');
      const { getEnvTokenSource } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.getEnvTokenSource).mockReturnValue('env:GITHUB_TOKEN');

      const result = getEnvTokenSource();

      expect(result).toBe('env:GITHUB_TOKEN');
    });
  });

  describe('hasEnvToken', () => {
    it('should return result from shared package', async () => {
      const shared = await import('octocode-shared');
      const { hasEnvToken } = await import('../../src/utils/token-storage.js');

      vi.mocked(shared.hasEnvToken).mockReturnValue(true);

      const result = hasEnvToken();

      expect(result).toBe(true);
    });
  });

  describe('resolveTokenFull', () => {
    it('should return token with source', async () => {
      const shared = await import('octocode-shared');
      const { resolveTokenFull } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.resolveTokenFull).mockResolvedValue({
        token: 'full-token',
        source: 'octocode-storage',
        wasRefreshed: false,
      });

      const result = await resolveTokenFull();

      expect(result?.token).toBe('full-token');
      expect(result?.source).toBe('octocode-storage');
    });

    it('should return null when no token', async () => {
      const shared = await import('octocode-shared');
      const { resolveTokenFull } =
        await import('../../src/utils/token-storage.js');

      vi.mocked(shared.resolveTokenFull).mockResolvedValue(null);

      const result = await resolveTokenFull();

      expect(result).toBeNull();
    });
  });
});
