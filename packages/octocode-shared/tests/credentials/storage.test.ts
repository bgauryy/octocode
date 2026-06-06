import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as fs from 'node:fs';
import * as crypto from 'node:crypto';

vi.mock('@octokit/oauth-methods', () => ({
  refreshToken: vi.fn(),
}));

vi.mock('@octokit/request', () => ({
  request: {
    defaults: vi.fn().mockReturnValue(vi.fn()),
  },
}));

vi.mock('node:fs', () => ({
  existsSync: vi.fn(),
  mkdirSync: vi.fn(),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  unlinkSync: vi.fn(),
  statSync: vi.fn(),
  chmodSync: vi.fn(),
}));

vi.mock('node:crypto', () => ({
  randomBytes: vi.fn(),
  createCipheriv: vi.fn(),
  createDecipheriv: vi.fn(),
}));

function assertFsIsMocked(): void {
  if (!vi.isMockFunction(fs.writeFileSync)) {
    throw new Error(
      'SAFETY: fs.writeFileSync is NOT mocked! Tests would write to real files.'
    );
  }

  if (!vi.isMockFunction(fs.existsSync)) {
    throw new Error(
      'SAFETY: fs.existsSync is NOT mocked! Tests would access real files.'
    );
  }
}

function createMockCredentials(overrides = {}) {
  return {
    hostname: 'github.com',
    username: '__mock_user__',
    token: {
      token: 'ghp_MOCK_TOKEN_00000000000000000000',
      tokenType: 'oauth' as const,
    },
    gitProtocol: 'https' as const,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...overrides,
  };
}

function createMockCipher() {
  return {
    update: vi.fn().mockReturnValue('encrypted'),
    final: vi.fn().mockReturnValue(''),
    getAuthTag: vi.fn().mockReturnValue(Buffer.from('authtag1234567')),
  };
}

describe('Token Storage', () => {
  const mockKey = Buffer.alloc(32, 'a');
  const mockIv = Buffer.alloc(16, 'b');

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();

    assertFsIsMocked();

    vi.mocked(crypto.randomBytes).mockReturnValue(mockIv as unknown as void);

    vi.mocked(fs.statSync).mockReturnValue({
      mode: 0o100600,
    } as ReturnType<typeof fs.statSync>);
  });

  afterEach(() => {
    vi.resetAllMocks();

    const writeFileCalls = vi.mocked(fs.writeFileSync).mock.calls;
    for (const call of writeFileCalls) {
      const path = String(call[0]);

      if (path.includes('/Users/') || path.includes('/home/')) {
        console.warn(
          `⚠️ Test called writeFileSync with user path: ${path} (mocked, not real)`
        );
      }
    }
  });

  describe('storeCredentials', () => {
    it('should write encrypted credentials to file', async () => {
      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockReturnValue(mockKey.toString('hex'));

      const mockCipher = createMockCipher();
      vi.mocked(crypto.createCipheriv).mockReturnValue(
        mockCipher as unknown as crypto.CipherGCM
      );

      const { storeCredentials } =
        await import('../../src/credentials/storage.js');

      const result = await storeCredentials(createMockCredentials());

      expect(result.success).toBe(true);
    });

    it('should create .octocode directory if it does not exist', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);
      vi.mocked(fs.readFileSync).mockReturnValue(mockKey.toString('hex'));

      const mockCipher = createMockCipher();
      vi.mocked(crypto.createCipheriv).mockReturnValue(
        mockCipher as unknown as crypto.CipherGCM
      );

      const { storeCredentials } =
        await import('../../src/credentials/storage.js');

      await storeCredentials(createMockCredentials());

      expect(fs.mkdirSync).toHaveBeenCalled();
    });
  });

  describe('getCredentials', () => {
    it('should return null when credentials file does not exist', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { getCredentials } =
        await import('../../src/credentials/storage.js');
      const result = await getCredentials('github.com');

      expect(result).toBeNull();
    });

    it('should cache missing credentials to avoid repeated file reads', async () => {
      const storedCreds = createMockCredentials({
        hostname: 'github.enterprise.example',
      });
      const store = {
        version: 1,
        credentials: { 'github.enterprise.example': storedCreds },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getCredentials } =
        await import('../../src/credentials/storage.js');

      expect(await getCredentials('github.com')).toBeNull();
      const readCountAfterFirstLookup = vi.mocked(fs.readFileSync).mock.calls
        .length;

      expect(await getCredentials('github.com')).toBeNull();
      expect(vi.mocked(fs.readFileSync).mock.calls).toHaveLength(
        readCountAfterFirstLookup
      );
    });

    it('should return credentials when they exist in file', async () => {
      const storedCreds = createMockCredentials();
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getCredentials } =
        await import('../../src/credentials/storage.js');
      const result = await getCredentials('github.com');

      expect(result).toEqual(storedCreds);
    });
  });

  describe('getToken', () => {
    it('should return token string when credentials exist', async () => {
      const storedCreds = createMockCredentials();
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getToken } = await import('../../src/credentials/storage.js');
      const result = await getToken('github.com');

      expect(result).toBe('ghp_MOCK_TOKEN_00000000000000000000');
    });

    it('should return null when credentials do not exist', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { getToken } = await import('../../src/credentials/storage.js');
      const result = await getToken('github.com');

      expect(result).toBeNull();
    });
  });

  describe('isTokenExpired', () => {
    it('should return false for non-expiring tokens', async () => {
      const { isTokenExpired } =
        await import('../../src/credentials/storage.js');

      const credentials = createMockCredentials();
      expect(isTokenExpired(credentials)).toBe(false);
    });

    it('should return true for expired tokens', async () => {
      const { isTokenExpired } =
        await import('../../src/credentials/storage.js');

      const credentials = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
        },
      });

      expect(isTokenExpired(credentials)).toBe(true);
    });

    it('should return false for tokens expiring more than 5 minutes from now', async () => {
      const { isTokenExpired } =
        await import('../../src/credentials/storage.js');

      const futureDate = new Date(Date.now() + 10 * 60 * 1000);
      const credentials = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          expiresAt: futureDate.toISOString(),
        },
      });

      expect(isTokenExpired(credentials)).toBe(false);
    });

    it('should return true for tokens expiring in less than 5 minutes', async () => {
      const { isTokenExpired } =
        await import('../../src/credentials/storage.js');

      const nearFuture = new Date(Date.now() + 2 * 60 * 1000);
      const credentials = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          expiresAt: nearFuture.toISOString(),
        },
      });

      expect(isTokenExpired(credentials)).toBe(true);
    });

    it('should return true for invalid date strings', async () => {
      const { isTokenExpired } =
        await import('../../src/credentials/storage.js');

      const credentials = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          expiresAt: 'invalid-date',
        },
      });

      expect(isTokenExpired(credentials)).toBe(true);
    });
  });

  describe('isRefreshTokenExpired', () => {
    it('should return false when no refresh token expiry', async () => {
      const { isRefreshTokenExpired } =
        await import('../../src/credentials/storage.js');

      const credentials = createMockCredentials();
      expect(isRefreshTokenExpired(credentials)).toBe(false);
    });

    it('should return true for expired refresh token', async () => {
      const { isRefreshTokenExpired } =
        await import('../../src/credentials/storage.js');

      const credentials = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          refreshToken: 'refresh-token',
          refreshTokenExpiresAt: '2020-01-01T00:00:00.000Z',
        },
      });

      expect(isRefreshTokenExpired(credentials)).toBe(true);
    });

    it('should return false for valid refresh token', async () => {
      const { isRefreshTokenExpired } =
        await import('../../src/credentials/storage.js');

      const futureDate = new Date(Date.now() + 24 * 60 * 60 * 1000);
      const credentials = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          refreshToken: 'refresh-token',
          refreshTokenExpiresAt: futureDate.toISOString(),
        },
      });

      expect(isRefreshTokenExpired(credentials)).toBe(false);
    });

    it('should return true for invalid refresh token date strings', async () => {
      const { isRefreshTokenExpired } =
        await import('../../src/credentials/storage.js');

      const credentials = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          refreshToken: 'refresh-token',
          refreshTokenExpiresAt: 'invalid-date',
        },
      });

      expect(isRefreshTokenExpired(credentials)).toBe(true);
    });
  });

  describe('constants', () => {
    it('should export storage path constants', async () => {
      const { OCTOCODE_DIR, CREDENTIALS_FILE, KEY_FILE } =
        await import('../../src/credentials/storage.js');

      expect(OCTOCODE_DIR).toContain('.octocode');
      expect(CREDENTIALS_FILE).toContain('credentials.json');
      expect(KEY_FILE).toContain('.key');
    });
  });

  describe('getTokenFromEnv', () => {
    const originalEnv = process.env;

    beforeEach(() => {
      process.env = { ...originalEnv };
      delete process.env.OCTOCODE_TOKEN;
      delete process.env.GH_TOKEN;
      delete process.env.GITHUB_TOKEN;
    });

    afterEach(() => {
      process.env = originalEnv;
    });

    it('should return OCTOCODE_TOKEN when set', async () => {
      process.env.OCTOCODE_TOKEN = 'octocode-test-token';

      const { getTokenFromEnv } =
        await import('../../src/credentials/storage.js');
      expect(getTokenFromEnv()).toBe('octocode-test-token');
    });

    it('should return GH_TOKEN when OCTOCODE_TOKEN is not set', async () => {
      process.env.GH_TOKEN = 'gh-test-token';

      const { getTokenFromEnv } =
        await import('../../src/credentials/storage.js');
      expect(getTokenFromEnv()).toBe('gh-test-token');
    });

    it('should return GITHUB_TOKEN when others are not set', async () => {
      process.env.GITHUB_TOKEN = 'github-test-token';

      const { getTokenFromEnv } =
        await import('../../src/credentials/storage.js');
      expect(getTokenFromEnv()).toBe('github-test-token');
    });

    it('should prioritize OCTOCODE_TOKEN over GH_TOKEN', async () => {
      process.env.OCTOCODE_TOKEN = 'octocode-priority';
      process.env.GH_TOKEN = 'gh-lower-priority';

      const { getTokenFromEnv } =
        await import('../../src/credentials/storage.js');
      expect(getTokenFromEnv()).toBe('octocode-priority');
    });

    it('should prioritize OCTOCODE_TOKEN over GITHUB_TOKEN', async () => {
      process.env.OCTOCODE_TOKEN = 'octocode-priority';
      process.env.GITHUB_TOKEN = 'github-lower-priority';

      const { getTokenFromEnv } =
        await import('../../src/credentials/storage.js');
      expect(getTokenFromEnv()).toBe('octocode-priority');
    });

    it('should prioritize GH_TOKEN over GITHUB_TOKEN', async () => {
      process.env.GH_TOKEN = 'gh-priority';
      process.env.GITHUB_TOKEN = 'github-lower-priority';

      const { getTokenFromEnv } =
        await import('../../src/credentials/storage.js');
      expect(getTokenFromEnv()).toBe('gh-priority');
    });

    it('should return null when no env vars are set', async () => {
      const { getTokenFromEnv } =
        await import('../../src/credentials/storage.js');
      expect(getTokenFromEnv()).toBeNull();
    });

    it('should trim whitespace from token values', async () => {
      process.env.OCTOCODE_TOKEN = '  trimmed-token  ';

      const { getTokenFromEnv } =
        await import('../../src/credentials/storage.js');
      expect(getTokenFromEnv()).toBe('trimmed-token');
    });

    it('should skip empty or whitespace-only tokens', async () => {
      process.env.OCTOCODE_TOKEN = '   ';
      process.env.GH_TOKEN = 'fallback-token';

      const { getTokenFromEnv } =
        await import('../../src/credentials/storage.js');
      expect(getTokenFromEnv()).toBe('fallback-token');
    });
  });

  describe('getEnvTokenSource', () => {
    const originalEnv = process.env;

    beforeEach(() => {
      process.env = { ...originalEnv };
      delete process.env.OCTOCODE_TOKEN;
      delete process.env.GH_TOKEN;
      delete process.env.GITHUB_TOKEN;
    });

    afterEach(() => {
      process.env = originalEnv;
    });

    it('should return env:OCTOCODE_TOKEN when set', async () => {
      process.env.OCTOCODE_TOKEN = 'test-token';

      const { getEnvTokenSource } =
        await import('../../src/credentials/storage.js');
      expect(getEnvTokenSource()).toBe('env:OCTOCODE_TOKEN');
    });

    it('should return env:GH_TOKEN when OCTOCODE_TOKEN is not set', async () => {
      process.env.GH_TOKEN = 'test-token';

      const { getEnvTokenSource } =
        await import('../../src/credentials/storage.js');
      expect(getEnvTokenSource()).toBe('env:GH_TOKEN');
    });

    it('should return env:GITHUB_TOKEN when others are not set', async () => {
      process.env.GITHUB_TOKEN = 'test-token';

      const { getEnvTokenSource } =
        await import('../../src/credentials/storage.js');
      expect(getEnvTokenSource()).toBe('env:GITHUB_TOKEN');
    });

    it('should return null when no env vars are set', async () => {
      const { getEnvTokenSource } =
        await import('../../src/credentials/storage.js');
      expect(getEnvTokenSource()).toBeNull();
    });
  });

  describe('hasEnvToken', () => {
    const originalEnv = process.env;

    beforeEach(() => {
      process.env = { ...originalEnv };
      delete process.env.OCTOCODE_TOKEN;
      delete process.env.GH_TOKEN;
      delete process.env.GITHUB_TOKEN;
    });

    afterEach(() => {
      process.env = originalEnv;
    });

    it('should return true when OCTOCODE_TOKEN is set', async () => {
      process.env.OCTOCODE_TOKEN = 'test-token';

      const { hasEnvToken } = await import('../../src/credentials/storage.js');
      expect(hasEnvToken()).toBe(true);
    });

    it('should return true when GH_TOKEN is set', async () => {
      process.env.GH_TOKEN = 'test-token';

      const { hasEnvToken } = await import('../../src/credentials/storage.js');
      expect(hasEnvToken()).toBe(true);
    });

    it('should return true when GITHUB_TOKEN is set', async () => {
      process.env.GITHUB_TOKEN = 'test-token';

      const { hasEnvToken } = await import('../../src/credentials/storage.js');
      expect(hasEnvToken()).toBe(true);
    });

    it('should return false when no env vars are set', async () => {
      const { hasEnvToken } = await import('../../src/credentials/storage.js');
      expect(hasEnvToken()).toBe(false);
    });
  });

  describe('resolveToken', () => {
    const originalEnv = process.env;

    beforeEach(async () => {
      process.env = { ...originalEnv };
      delete process.env.OCTOCODE_TOKEN;
      delete process.env.GH_TOKEN;
      delete process.env.GITHUB_TOKEN;

      await import('../../src/credentials/storage.js');
    });

    afterEach(() => {
      process.env = originalEnv;
    });

    describe('Priority 1-3: Environment Variables', () => {
      it('should return OCTOCODE_TOKEN with source env:OCTOCODE_TOKEN', async () => {
        process.env.OCTOCODE_TOKEN = 'env-octocode-token';

        const { resolveToken } =
          await import('../../src/credentials/storage.js');
        const result = await resolveToken();

        expect(result).toEqual({
          token: 'env-octocode-token',
          source: 'env:OCTOCODE_TOKEN',
        });
      });

      it('should return GH_TOKEN with source env:GH_TOKEN', async () => {
        process.env.GH_TOKEN = 'env-gh-token';

        const { resolveToken } =
          await import('../../src/credentials/storage.js');
        const result = await resolveToken();

        expect(result).toEqual({
          token: 'env-gh-token',
          source: 'env:GH_TOKEN',
        });
      });

      it('should return GITHUB_TOKEN with source env:GITHUB_TOKEN', async () => {
        process.env.GITHUB_TOKEN = 'env-github-token';

        const { resolveToken } =
          await import('../../src/credentials/storage.js');
        const result = await resolveToken();

        expect(result).toEqual({
          token: 'env-github-token',
          source: 'env:GITHUB_TOKEN',
        });
      });

      it('should prioritize OCTOCODE_TOKEN over all other env vars', async () => {
        process.env.OCTOCODE_TOKEN = 'octocode-wins';
        process.env.GH_TOKEN = 'gh-loses';
        process.env.GITHUB_TOKEN = 'github-loses';

        const { resolveToken } =
          await import('../../src/credentials/storage.js');
        const result = await resolveToken();

        expect(result?.token).toBe('octocode-wins');
        expect(result?.source).toBe('env:OCTOCODE_TOKEN');
      });

      it('should prioritize GH_TOKEN over GITHUB_TOKEN', async () => {
        process.env.GH_TOKEN = 'gh-wins';
        process.env.GITHUB_TOKEN = 'github-loses';

        const { resolveToken } =
          await import('../../src/credentials/storage.js');
        const result = await resolveToken();

        expect(result?.token).toBe('gh-wins');
        expect(result?.source).toBe('env:GH_TOKEN');
      });
    });

    describe('Priority 4: Stored Credentials (File)', () => {
      it('should fall back to stored credentials when no env vars', async () => {
        const storedCreds = createMockCredentials();
        const store = {
          version: 1,
          credentials: { 'github.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const { resolveToken } =
          await import('../../src/credentials/storage.js');
        const result = await resolveToken();

        expect(result).toEqual({
          token: 'ghp_MOCK_TOKEN_00000000000000000000',
          source: 'octocode-storage',
        });
      });

      it('should return file source for stored credentials', async () => {
        const storedCreds = createMockCredentials();
        const store = {
          version: 1,
          credentials: { 'github.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const { resolveToken } =
          await import('../../src/credentials/storage.js');

        const result = await resolveToken();

        expect(result?.token).toBe('ghp_MOCK_TOKEN_00000000000000000000');
        expect(result?.source).toBe('octocode-storage');
      });

      it('should return null when no token found anywhere', async () => {
        vi.mocked(fs.existsSync).mockReturnValue(false);

        const { resolveToken } =
          await import('../../src/credentials/storage.js');
        const result = await resolveToken();

        expect(result).toBeNull();
      });
    });

    describe('Environment Variables Skip Storage', () => {
      it('should NOT check storage when env var token is available', async () => {
        process.env.GITHUB_TOKEN = 'fast-env-token';

        const { resolveToken } =
          await import('../../src/credentials/storage.js');
        const result = await resolveToken();

        expect(fs.existsSync).not.toHaveBeenCalled();
        expect(fs.readFileSync).not.toHaveBeenCalled();
        expect(result?.token).toBe('fast-env-token');
      });
    });

    describe('Custom Hostname', () => {
      it('should use custom hostname for storage lookup', async () => {
        const storedCreds = createMockCredentials({
          hostname: 'github.mycompany.com',
        });
        const store = {
          version: 1,
          credentials: { 'github.mycompany.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const { resolveToken } =
          await import('../../src/credentials/storage.js');

        const defaultResult = await resolveToken('github.com');
        expect(defaultResult).toBeNull();

        const customResult = await resolveToken('github.mycompany.com');
        expect(customResult?.token).toBe('ghp_MOCK_TOKEN_00000000000000000000');
      });
    });

    describe('Expired Token Handling', () => {
      it('should return null for expired stored token', async () => {
        const storedCreds = createMockCredentials({
          token: {
            token: 'expired-token',
            tokenType: 'oauth' as const,
            expiresAt: '2020-01-01T00:00:00.000Z',
          },
        });
        const store = {
          version: 1,
          credentials: { 'github.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const { resolveToken } =
          await import('../../src/credentials/storage.js');
        const result = await resolveToken();

        expect(result).toBeNull();
      });
    });
  });

  describe('getTokenSync', () => {
    it('should return token string when credentials exist', async () => {
      const storedCreds = createMockCredentials();
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getTokenSync } = await import('../../src/credentials/storage.js');
      const result = getTokenSync('github.com');

      expect(result).toBe('ghp_MOCK_TOKEN_00000000000000000000');
    });

    it('should return null when credentials do not exist', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { getTokenSync } = await import('../../src/credentials/storage.js');
      const result = getTokenSync('github.com');

      expect(result).toBeNull();
    });

    it('should return null when token is expired', async () => {
      const storedCreds = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getTokenSync } = await import('../../src/credentials/storage.js');
      const result = getTokenSync('github.com');

      expect(result).toBeNull();
    });

    it('should return null when credentials exist but token is missing', async () => {
      const storedCreds = {
        ...createMockCredentials(),
        token: undefined,
      };
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getTokenSync } = await import('../../src/credentials/storage.js');
      const result = getTokenSync('github.com');

      expect(result).toBeNull();
    });
  });

  describe('getToken', () => {
    it('should return null when token is expired', async () => {
      const storedCreds = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getToken } = await import('../../src/credentials/storage.js');
      const result = await getToken('github.com');

      expect(result).toBeNull();
    });
  });

  describe('refreshAuthToken', () => {
    it('should return error when not logged in', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { refreshAuthToken } =
        await import('../../src/credentials/storage.js');
      const result = await refreshAuthToken('github.com');

      expect(result.success).toBe(false);
      expect(result.error).toContain('Not logged in');
    });

    it('should return error when token has no refresh token', async () => {
      const storedCreds = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { refreshAuthToken } =
        await import('../../src/credentials/storage.js');
      const result = await refreshAuthToken('github.com');

      expect(result.success).toBe(false);
      expect(result.error).toContain('does not support refresh');
    });

    it('should return error when refresh token is expired', async () => {
      const storedCreds = createMockCredentials({
        token: {
          token: 'test-token',
          tokenType: 'oauth' as const,
          refreshToken: 'expired-refresh-token',
          refreshTokenExpiresAt: '2020-01-01T00:00:00.000Z',
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { refreshAuthToken } =
        await import('../../src/credentials/storage.js');
      const result = await refreshAuthToken('github.com');

      expect(result.success).toBe(false);
      expect(result.error).toContain('Refresh token has expired');
    });
  });

  describe('getTokenWithRefresh', () => {
    it('should return token when not expired', async () => {
      const storedCreds = createMockCredentials({
        token: {
          token: 'valid-token',
          tokenType: 'oauth' as const,
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getTokenWithRefresh } =
        await import('../../src/credentials/storage.js');
      const result = await getTokenWithRefresh('github.com');

      expect(result.token).toBe('valid-token');
      expect(result.source).toBe('stored');
      expect(result.username).toBe('__mock_user__');
    });

    it('should return null with error when expired and no refresh token', async () => {
      const storedCreds = createMockCredentials({
        token: {
          token: 'expired-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getTokenWithRefresh } =
        await import('../../src/credentials/storage.js');
      const result = await getTokenWithRefresh('github.com');

      expect(result.token).toBeNull();
      expect(result.source).toBe('none');
      expect(result.refreshError).toContain('no refresh token');
    });

    it('should return null when no credentials exist', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { getTokenWithRefresh } =
        await import('../../src/credentials/storage.js');
      const result = await getTokenWithRefresh('github.com');

      expect(result.token).toBeNull();
      expect(result.source).toBe('none');
    });
  });

  describe('resolveTokenWithRefresh', () => {
    const originalEnv = process.env;

    beforeEach(async () => {
      process.env = { ...originalEnv };
      delete process.env.OCTOCODE_TOKEN;
      delete process.env.GH_TOKEN;
      delete process.env.GITHUB_TOKEN;

      await import('../../src/credentials/storage.js');
    });

    afterEach(() => {
      process.env = originalEnv;
    });

    it('should return env token without refresh attempt', async () => {
      process.env.GITHUB_TOKEN = 'env-token';

      const { resolveTokenWithRefresh } =
        await import('../../src/credentials/storage.js');
      const result = await resolveTokenWithRefresh();

      expect(result?.token).toBe('env-token');
      expect(result?.source).toBe('env:GITHUB_TOKEN');
      expect(result?.wasRefreshed).toBe(false);
    });

    it('should return stored token with wasRefreshed=false when valid', async () => {
      const storedCreds = createMockCredentials({
        token: {
          token: 'stored-token',
          tokenType: 'oauth' as const,
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { resolveTokenWithRefresh } =
        await import('../../src/credentials/storage.js');
      const result = await resolveTokenWithRefresh();

      expect(result?.token).toBe('stored-token');
      expect(result?.wasRefreshed).toBe(false);
      expect(result?.username).toBe('__mock_user__');
    });

    it('should return null when no token found', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { resolveTokenWithRefresh } =
        await import('../../src/credentials/storage.js');
      const result = await resolveTokenWithRefresh();

      expect(result).toBeNull();
    });

    it('should return null when token expired and no refresh token available', async () => {
      const storedCreds = createMockCredentials({
        token: {
          token: 'expired-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { resolveTokenWithRefresh } =
        await import('../../src/credentials/storage.js');
      const result = await resolveTokenWithRefresh();

      expect(result).toBeNull();
    });
  });

  describe('resolveTokenFull', () => {
    const originalEnv = process.env;

    beforeEach(async () => {
      process.env = { ...originalEnv };
      delete process.env.OCTOCODE_TOKEN;
      delete process.env.GH_TOKEN;
      delete process.env.GITHUB_TOKEN;

      await import('../../src/credentials/storage.js');
    });

    afterEach(() => {
      process.env = originalEnv;
    });

    describe('Priority 1-3: Environment Variables', () => {
      it('should return OCTOCODE_TOKEN with highest priority', async () => {
        process.env.OCTOCODE_TOKEN = 'octocode-env-token';
        process.env.GH_TOKEN = 'gh-env-token';
        process.env.GITHUB_TOKEN = 'github-env-token';

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull();

        expect(result?.token).toBe('octocode-env-token');
        expect(result?.source).toBe('env:OCTOCODE_TOKEN');
        expect(result?.wasRefreshed).toBe(false);
      });

      it('should return GH_TOKEN when OCTOCODE_TOKEN not set', async () => {
        process.env.GH_TOKEN = 'gh-env-token';
        process.env.GITHUB_TOKEN = 'github-env-token';

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull();

        expect(result?.token).toBe('gh-env-token');
        expect(result?.source).toBe('env:GH_TOKEN');
      });

      it('should return GITHUB_TOKEN when others not set', async () => {
        process.env.GITHUB_TOKEN = 'github-env-token';

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull();

        expect(result?.token).toBe('github-env-token');
        expect(result?.source).toBe('env:GITHUB_TOKEN');
      });

      it('should skip storage check when env token available', async () => {
        process.env.GITHUB_TOKEN = 'fast-env-token';

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull();

        expect(fs.existsSync).not.toHaveBeenCalled();
        expect(result?.token).toBe('fast-env-token');
      });
    });

    describe('Priority 4-5: Stored Credentials with Refresh', () => {
      it('should return stored token when env vars not set', async () => {
        const storedCreds = createMockCredentials({
          token: {
            token: 'stored-token',
            tokenType: 'oauth' as const,
          },
        });
        const store = {
          version: 1,
          credentials: { 'github.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull();

        expect(result?.token).toBe('stored-token');
        expect(result?.source).toBe('octocode-storage');
        expect(result?.username).toBe('__mock_user__');
      });

      it('should return file source for stored credentials', async () => {
        const storedCreds = createMockCredentials();
        const store = {
          version: 1,
          credentials: { 'github.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');

        const result = await resolveTokenFull();

        expect(result?.token).toBe('ghp_MOCK_TOKEN_00000000000000000000');
        expect(result?.source).toBe('octocode-storage');
      });
    });

    describe('Priority 5: gh CLI Fallback', () => {
      it('should call getGhCliToken when no env or stored token', async () => {
        vi.mocked(fs.existsSync).mockReturnValue(false);

        const mockGetGhCliToken = vi.fn().mockReturnValue('gh-cli-token');

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          getGhCliToken: mockGetGhCliToken,
        });

        expect(mockGetGhCliToken).toHaveBeenCalledWith('github.com');
        expect(result?.token).toBe('gh-cli-token');
        expect(result?.source).toBe('gh-cli');
      });

      it('should pass custom hostname to getGhCliToken', async () => {
        vi.mocked(fs.existsSync).mockReturnValue(false);

        const mockGetGhCliToken = vi.fn().mockReturnValue('enterprise-token');

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          hostname: 'github.mycompany.com',
          getGhCliToken: mockGetGhCliToken,
        });

        expect(mockGetGhCliToken).toHaveBeenCalledWith('github.mycompany.com');
        expect(result?.token).toBe('enterprise-token');
      });

      it('should handle async getGhCliToken callback', async () => {
        vi.mocked(fs.existsSync).mockReturnValue(false);

        const mockGetGhCliToken = vi
          .fn()
          .mockResolvedValue('async-gh-cli-token');

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          getGhCliToken: mockGetGhCliToken,
        });

        expect(result?.token).toBe('async-gh-cli-token');
        expect(result?.source).toBe('gh-cli');
      });

      it('should trim whitespace from gh CLI token', async () => {
        vi.mocked(fs.existsSync).mockReturnValue(false);

        const mockGetGhCliToken = vi
          .fn()
          .mockReturnValue('  gh-token-with-whitespace  ');

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          getGhCliToken: mockGetGhCliToken,
        });

        expect(result?.token).toBe('gh-token-with-whitespace');
      });

      it('should skip gh CLI when it returns null', async () => {
        vi.mocked(fs.existsSync).mockReturnValue(false);

        const mockGetGhCliToken = vi.fn().mockReturnValue(null);

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          getGhCliToken: mockGetGhCliToken,
        });

        expect(result).toBeNull();
      });

      it('should skip gh CLI when it returns empty string', async () => {
        vi.mocked(fs.existsSync).mockReturnValue(false);

        const mockGetGhCliToken = vi.fn().mockReturnValue('   ');

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          getGhCliToken: mockGetGhCliToken,
        });

        expect(result).toBeNull();
      });

      it('should handle gh CLI errors gracefully', async () => {
        vi.mocked(fs.existsSync).mockReturnValue(false);

        const mockGetGhCliToken = vi
          .fn()
          .mockRejectedValue(new Error('gh CLI not installed'));

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          getGhCliToken: mockGetGhCliToken,
        });

        expect(result).toBeNull();
      });

      it('should NOT call getGhCliToken when env token available', async () => {
        process.env.GITHUB_TOKEN = 'env-token';

        const mockGetGhCliToken = vi.fn().mockReturnValue('gh-cli-token');

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          getGhCliToken: mockGetGhCliToken,
        });

        expect(mockGetGhCliToken).not.toHaveBeenCalled();
        expect(result?.token).toBe('env-token');
      });

      it('should NOT call getGhCliToken when stored token available', async () => {
        const storedCreds = createMockCredentials();
        const store = {
          version: 1,
          credentials: { 'github.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const mockGetGhCliToken = vi.fn().mockReturnValue('gh-cli-token');

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          getGhCliToken: mockGetGhCliToken,
        });

        expect(mockGetGhCliToken).not.toHaveBeenCalled();
        expect(result?.token).toBe('ghp_MOCK_TOKEN_00000000000000000000');
      });

      it('should fall back to gh CLI when storage token is expired', async () => {
        const storedCreds = createMockCredentials({
          token: {
            token: 'expired-token',
            tokenType: 'oauth' as const,
            expiresAt: '2020-01-01T00:00:00.000Z',
          },
        });
        const store = {
          version: 1,
          credentials: { 'github.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const mockGetGhCliToken = vi.fn().mockReturnValue('gh-cli-fallback');

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({
          getGhCliToken: mockGetGhCliToken,
        });

        expect(result?.token).toBe('gh-cli-fallback');
        expect(result?.source).toBe('gh-cli');
      });
    });

    describe('No Token Found', () => {
      it('should return null when all sources exhausted', async () => {
        vi.mocked(fs.existsSync).mockReturnValue(false);

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({ getGhCliToken: () => null });

        expect(result).toBeNull();
      });

      it('should return null when token expired, no refresh token, and no gh CLI', async () => {
        const storedCreds = createMockCredentials({
          token: {
            token: 'expired-token',
            tokenType: 'oauth' as const,
            expiresAt: '2020-01-01T00:00:00.000Z',
          },
        });
        const store = {
          version: 1,
          credentials: { 'github.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({ getGhCliToken: () => null });

        expect(result).toBeNull();
      });
    });

    describe('Custom Options', () => {
      it('should use default hostname when not specified', async () => {
        process.env.GITHUB_TOKEN = 'env-token';

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');
        const result = await resolveTokenFull({});

        expect(result?.token).toBe('env-token');
      });

      it('should use custom hostname for storage lookup', async () => {
        const storedCreds = createMockCredentials({
          hostname: 'github.enterprise.com',
        });
        const store = {
          version: 1,
          credentials: { 'github.enterprise.com': storedCreds },
        };

        vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return true;
          if (String(path).includes('credentials.json')) return true;
          return false;
        });
        vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
          if (String(path).includes('.key')) return mockKey.toString('hex');
          return 'iv:authtag:encrypted';
        });

        const mockDecipher = {
          update: vi.fn().mockReturnValue(JSON.stringify(store)),
          final: vi.fn().mockReturnValue(''),
          setAuthTag: vi.fn(),
        };
        vi.mocked(crypto.createDecipheriv).mockReturnValue(
          mockDecipher as unknown as crypto.DecipherGCM
        );

        const { resolveTokenFull } =
          await import('../../src/credentials/storage.js');

        const defaultResult = await resolveTokenFull({
          getGhCliToken: () => null,
        });
        expect(defaultResult).toBeNull();

        const customResult = await resolveTokenFull({
          hostname: 'github.enterprise.com',
        });
        expect(customResult?.token).toBe('ghp_MOCK_TOKEN_00000000000000000000');
      });
    });
  });

  describe('listStoredHosts', () => {
    it('should list hosts from file storage', async () => {
      const credentials = createMockCredentials();
      const store = {
        version: 1,
        credentials: {
          'github.com': credentials,
          'github.enterprise.com': {
            ...credentials,
            hostname: 'github.enterprise.com',
          },
        },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { listStoredHosts } =
        await import('../../src/credentials/storage.js');
      const hosts = await listStoredHosts();

      expect(hosts).toContain('github.com');
      expect(hosts).toContain('github.enterprise.com');
      expect(hosts.length).toBe(2);
    });

    it('should return empty array when no credentials exist', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { listStoredHosts } =
        await import('../../src/credentials/storage.js');
      const hosts = await listStoredHosts();

      expect(hosts).toEqual([]);
    });
  });

  describe('listStoredHostsSync', () => {
    it('should list hosts from file storage synchronously', async () => {
      const credentials = createMockCredentials();
      const store = {
        version: 1,
        credentials: {
          'github.com': credentials,
          'custom.host.com': { ...credentials, hostname: 'custom.host.com' },
        },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { listStoredHostsSync } =
        await import('../../src/credentials/storage.js');
      const hosts = listStoredHostsSync();

      expect(hosts).toContain('github.com');
      expect(hosts).toContain('custom.host.com');
    });

    it('should return empty array when no credentials file', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { listStoredHostsSync } =
        await import('../../src/credentials/storage.js');
      const hosts = listStoredHostsSync();

      expect(hosts).toEqual([]);
    });
  });

  describe('hasCredentials', () => {
    it('should return true when credentials exist', async () => {
      const credentials = createMockCredentials();
      const store = { version: 1, credentials: { 'github.com': credentials } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { hasCredentials } =
        await import('../../src/credentials/storage.js');
      const result = await hasCredentials('github.com');

      expect(result).toBe(true);
    });

    it('should return false when credentials do not exist', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { hasCredentials } =
        await import('../../src/credentials/storage.js');
      const result = await hasCredentials('github.com');

      expect(result).toBe(false);
    });

    it('should use default hostname when not specified', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { hasCredentials } =
        await import('../../src/credentials/storage.js');
      const result = await hasCredentials();

      expect(result).toBe(false);
    });
  });

  describe('hasCredentialsSync', () => {
    it('should return true when credentials exist in file', async () => {
      const credentials = createMockCredentials();
      const store = { version: 1, credentials: { 'github.com': credentials } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { hasCredentialsSync } =
        await import('../../src/credentials/storage.js');
      const result = hasCredentialsSync('github.com');

      expect(result).toBe(true);
    });

    it('should return false when no credentials file exists', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { hasCredentialsSync } =
        await import('../../src/credentials/storage.js');
      const result = hasCredentialsSync();

      expect(result).toBe(false);
    });
  });

  describe('updateToken', () => {
    it('should update token for existing credentials', async () => {
      const credentials = createMockCredentials();
      const store = { version: 1, credentials: { 'github.com': credentials } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const mockCipher = createMockCipher();
      vi.mocked(crypto.createCipheriv).mockReturnValue(
        mockCipher as unknown as crypto.CipherGCM
      );

      const { updateToken } = await import('../../src/credentials/storage.js');
      const result = await updateToken('github.com', {
        token: 'new-token-value',
        tokenType: 'oauth',
      });

      expect(result).toBe(true);
      expect(fs.writeFileSync).toHaveBeenCalled();
    });

    it('should return false when credentials do not exist', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { updateToken } = await import('../../src/credentials/storage.js');
      const result = await updateToken('github.com', {
        token: 'new-token',
        tokenType: 'oauth',
      });

      expect(result).toBe(false);
    });
  });

  describe('getCredentialsFilePath', () => {
    it('should return file path', async () => {
      const { getCredentialsFilePath, CREDENTIALS_FILE } =
        await import('../../src/credentials/storage.js');

      const path = getCredentialsFilePath();

      expect(path).toBe(CREDENTIALS_FILE);
    });
  });

  describe('refreshAuthToken - successful refresh', () => {
    it('should successfully refresh token and update storage', async () => {
      const { refreshToken: mockRefreshToken } =
        await import('@octokit/oauth-methods');

      const futureDate = new Date(Date.now() + 8 * 60 * 60 * 1000);
      const futureRefreshDate = new Date(
        Date.now() + 6 * 30 * 24 * 60 * 60 * 1000
      );

      const storedCreds = createMockCredentials({
        token: {
          token: 'expired-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
          refreshToken: 'valid-refresh-token',
          refreshTokenExpiresAt: futureRefreshDate.toISOString(),
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const mockCipher = createMockCipher();
      vi.mocked(crypto.createCipheriv).mockReturnValue(
        mockCipher as unknown as crypto.CipherGCM
      );

      vi.mocked(mockRefreshToken).mockResolvedValue({
        authentication: {
          token: 'new-refreshed-token',
          refreshToken: 'new-refresh-token',
          expiresAt: futureDate.toISOString(),
          refreshTokenExpiresAt: futureRefreshDate.toISOString(),
          type: 'token',
          tokenType: 'oauth',
        },
      } as unknown as Awaited<ReturnType<typeof mockRefreshToken>>);

      const { refreshAuthToken, _resetCredentialsCache } =
        await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      const result = await refreshAuthToken('github.com');

      expect(result.success).toBe(true);
      expect(result.username).toBe('__mock_user__');
      expect(result.hostname).toBe('github.com');
      expect(mockRefreshToken).toHaveBeenCalled();
    });

    it('should return error when refresh API call fails', async () => {
      const { refreshToken: mockRefreshToken } =
        await import('@octokit/oauth-methods');

      const futureRefreshDate = new Date(
        Date.now() + 6 * 30 * 24 * 60 * 60 * 1000
      );

      const storedCreds = createMockCredentials({
        token: {
          token: 'expired-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
          refreshToken: 'valid-refresh-token',
          refreshTokenExpiresAt: futureRefreshDate.toISOString(),
        },
      });
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      vi.mocked(mockRefreshToken).mockRejectedValue(new Error('API error'));

      const { refreshAuthToken, _resetCredentialsCache } =
        await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      const result = await refreshAuthToken('github.com');

      expect(result.success).toBe(false);
      expect(result.error).toBe('API error');
    });

    it('should use enterprise API base URL for enterprise hostname', async () => {
      const { refreshToken: mockRefreshToken } =
        await import('@octokit/oauth-methods');
      const { request: mockRequest } = await import('@octokit/request');

      const futureRefreshDate = new Date(
        Date.now() + 6 * 30 * 24 * 60 * 60 * 1000
      );

      const storedCreds = createMockCredentials({
        hostname: 'github.mycompany.com',
        token: {
          token: 'expired-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
          refreshToken: 'valid-refresh-token',
          refreshTokenExpiresAt: futureRefreshDate.toISOString(),
        },
      });
      const store = {
        version: 1,
        credentials: { 'github.mycompany.com': storedCreds },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      vi.mocked(mockRequest.defaults).mockReturnValue(
        vi.fn() as unknown as ReturnType<typeof mockRequest.defaults>
      );
      vi.mocked(mockRefreshToken).mockRejectedValue(new Error('Expected'));

      const { refreshAuthToken, _resetCredentialsCache } =
        await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      await refreshAuthToken('github.mycompany.com');

      expect(mockRequest.defaults).toHaveBeenCalledWith({
        baseUrl: 'https://github.mycompany.com/api/v3',
      });
    });
  });

  describe('getTokenWithRefresh - successful refresh', () => {
    it('should refresh expired token and return new token', async () => {
      const { refreshToken: mockRefreshToken } =
        await import('@octokit/oauth-methods');

      const futureDate = new Date(Date.now() + 8 * 60 * 60 * 1000);
      const futureRefreshDate = new Date(
        Date.now() + 6 * 30 * 24 * 60 * 60 * 1000
      );

      const expiredCreds = createMockCredentials({
        token: {
          token: 'expired-token',
          tokenType: 'oauth' as const,
          expiresAt: '2020-01-01T00:00:00.000Z',
          refreshToken: 'valid-refresh-token',
          refreshTokenExpiresAt: futureRefreshDate.toISOString(),
        },
      });

      const refreshedCreds = createMockCredentials({
        token: {
          token: 'new-refreshed-token',
          tokenType: 'oauth' as const,
          expiresAt: futureDate.toISOString(),
          refreshToken: 'new-refresh-token',
          refreshTokenExpiresAt: futureRefreshDate.toISOString(),
        },
      });

      let callCount = 0;

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockImplementation(() => {
          callCount++;
          if (callCount <= 2) {
            return JSON.stringify({
              version: 1,
              credentials: { 'github.com': expiredCreds },
            });
          }
          return JSON.stringify({
            version: 1,
            credentials: { 'github.com': refreshedCreds },
          });
        }),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const mockCipher = createMockCipher();
      vi.mocked(crypto.createCipheriv).mockReturnValue(
        mockCipher as unknown as crypto.CipherGCM
      );

      vi.mocked(mockRefreshToken).mockResolvedValue({
        authentication: {
          token: 'new-refreshed-token',
          refreshToken: 'new-refresh-token',
          expiresAt: futureDate.toISOString(),
          refreshTokenExpiresAt: futureRefreshDate.toISOString(),
          type: 'token',
          tokenType: 'oauth',
        },
      } as unknown as Awaited<ReturnType<typeof mockRefreshToken>>);

      const { getTokenWithRefresh, _resetCredentialsCache } =
        await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      const result = await getTokenWithRefresh('github.com');

      expect(result.token).toBe('new-refreshed-token');
      expect(result.source).toBe('refreshed');
      expect(result.username).toBe('__mock_user__');
    });
  });

  describe('deleteCredentials', () => {
    it('should delete credentials and cleanup when last credential', async () => {
      const credentials = createMockCredentials();
      const store = { version: 1, credentials: { 'github.com': credentials } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { deleteCredentials } =
        await import('../../src/credentials/storage.js');
      const result = await deleteCredentials('github.com');

      expect(result.success).toBe(true);
      expect(result.deletedFromFile).toBe(true);
      expect(fs.unlinkSync).toHaveBeenCalled();
    });

    it('should return false when credentials do not exist', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { deleteCredentials } =
        await import('../../src/credentials/storage.js');
      const result = await deleteCredentials('github.com');

      expect(result.success).toBe(false);
      expect(result.deletedFromFile).toBe(false);
    });
  });

  describe('getCredentialsSync', () => {
    it('should return credentials from file storage', async () => {
      const storedCreds = createMockCredentials();
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getCredentialsSync } =
        await import('../../src/credentials/storage.js');
      const result = getCredentialsSync('github.com');

      expect(result).toEqual(storedCreds);
    });

    it('should return null for non-existent hostname', async () => {
      const storedCreds = createMockCredentials();
      const store = { version: 1, credentials: { 'github.com': storedCreds } };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getCredentialsSync } =
        await import('../../src/credentials/storage.js');
      const result = getCredentialsSync('other-host.com');

      expect(result).toBeNull();
    });
  });

  describe('Credentials Cache', () => {
    it('should cache credentials after first fetch', async () => {
      const credentials = createMockCredentials();
      const store = {
        version: 1,
        credentials: { 'github.com': credentials },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getCredentials, _getCacheStats, _resetCredentialsCache } =
        await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      const result1 = await getCredentials('github.com');
      expect(result1?.token.token).toBe('ghp_MOCK_TOKEN_00000000000000000000');

      const stats1 = _getCacheStats();
      expect(stats1.size).toBe(1);
      expect(stats1.entries[0].hostname).toBe('github.com');
      expect(stats1.entries[0].valid).toBe(true);

      vi.mocked(crypto.createDecipheriv).mockClear();
      const result2 = await getCredentials('github.com');
      expect(result2?.token.token).toBe('ghp_MOCK_TOKEN_00000000000000000000');
    });

    it('should bypass cache when option is set', async () => {
      const credentials = createMockCredentials();
      const store = {
        version: 1,
        credentials: { 'github.com': credentials },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getCredentials, _resetCredentialsCache } =
        await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      await getCredentials('github.com');

      vi.mocked(crypto.createDecipheriv).mockClear();
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const result = await getCredentials('github.com', { bypassCache: true });
      expect(result?.token.token).toBe('ghp_MOCK_TOKEN_00000000000000000000');

      expect(crypto.createDecipheriv).toHaveBeenCalled();
    });

    it('should invalidate cache on storeCredentials', async () => {
      const credentials = createMockCredentials();
      const store = {
        version: 1,
        credentials: { 'github.com': credentials },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const mockCipher = createMockCipher();
      vi.mocked(crypto.createCipheriv).mockReturnValue(
        mockCipher as unknown as crypto.CipherGCM
      );

      const {
        getCredentials,
        storeCredentials,
        _getCacheStats,
        _resetCredentialsCache,
      } = await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      await getCredentials('github.com');
      expect(_getCacheStats().size).toBe(1);

      await storeCredentials(credentials);

      const stats = _getCacheStats();
      expect(
        stats.entries.find(e => e.hostname === 'github.com')
      ).toBeUndefined();
    });

    it('should invalidate cache on deleteCredentials', async () => {
      const credentials = createMockCredentials();
      const store = {
        version: 1,
        credentials: { 'github.com': credentials },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const mockCipher = createMockCipher();
      vi.mocked(crypto.createCipheriv).mockReturnValue(
        mockCipher as unknown as crypto.CipherGCM
      );

      const {
        getCredentials,
        deleteCredentials,
        _getCacheStats,
        _resetCredentialsCache,
      } = await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      await getCredentials('github.com');
      expect(_getCacheStats().size).toBe(1);

      await deleteCredentials('github.com');

      const stats = _getCacheStats();
      expect(
        stats.entries.find(e => e.hostname === 'github.com')
      ).toBeUndefined();
    });

    it('should invalidate all cache entries with invalidateCredentialsCache()', async () => {
      const credentials1 = createMockCredentials({ hostname: 'github.com' });
      const credentials2 = createMockCredentials({
        hostname: 'github.enterprise.com',
      });
      const store = {
        version: 1,
        credentials: {
          'github.com': credentials1,
          'github.enterprise.com': credentials2,
        },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const {
        getCredentials,
        invalidateCredentialsCache,
        _getCacheStats,
        _resetCredentialsCache,
      } = await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      await getCredentials('github.com');
      await getCredentials('github.enterprise.com');
      expect(_getCacheStats().size).toBe(2);

      invalidateCredentialsCache();

      expect(_getCacheStats().size).toBe(0);
    });

    it('should invalidate specific hostname with invalidateCredentialsCache(hostname)', async () => {
      const credentials1 = createMockCredentials({ hostname: 'github.com' });
      const credentials2 = createMockCredentials({
        hostname: 'github.enterprise.com',
      });
      const store = {
        version: 1,
        credentials: {
          'github.com': credentials1,
          'github.enterprise.com': credentials2,
        },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const {
        getCredentials,
        invalidateCredentialsCache,
        _getCacheStats,
        _resetCredentialsCache,
      } = await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      await getCredentials('github.com');
      await getCredentials('github.enterprise.com');
      expect(_getCacheStats().size).toBe(2);

      invalidateCredentialsCache('github.com');

      const stats = _getCacheStats();
      expect(stats.size).toBe(1);
      expect(stats.entries[0].hostname).toBe('github.enterprise.com');
    });

    it('should not serve expired tokens from cache even within TTL', async () => {
      const nearFuture = new Date(Date.now() + 2 * 60 * 1000);
      const credentials = createMockCredentials({
        token: {
          token: 'ghp_EXPIRING_TOKEN_000000000000000000',
          tokenType: 'oauth' as const,
          expiresAt: nearFuture.toISOString(),
        },
      });
      const store = {
        version: 1,
        credentials: { 'github.com': credentials },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(store)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { getCredentials, _resetCredentialsCache, _getCacheStats } =
        await import('../../src/credentials/storage.js');

      _resetCredentialsCache();

      await getCredentials('github.com');
      expect(_getCacheStats().size).toBe(1);

      const stats = _getCacheStats();
      expect(stats.entries[0].valid).toBe(false);

      vi.mocked(crypto.createDecipheriv).mockClear();
      await getCredentials('github.com');
      expect(crypto.createDecipheriv).toHaveBeenCalled();
    });
  });

  describe('Key File Permissions', () => {
    it('should fix key file permissions if too permissive', async () => {
      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const { statSync, chmodSync } = await import('node:fs');
      vi.mocked(statSync).mockReturnValue({
        mode: 0o100644,
      } as ReturnType<typeof statSync>);

      const mockDecipher = {
        update: vi
          .fn()
          .mockReturnValue(JSON.stringify({ version: 1, credentials: {} })),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { readCredentialsStore } =
        await import('../../src/credentials/storage.js');

      readCredentialsStore();

      expect(chmodSync).toHaveBeenCalledWith(
        expect.stringContaining('.key'),
        0o600
      );
    });

    it('should not call chmod when key file has correct permissions', async () => {
      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const { statSync, chmodSync } = await import('node:fs');
      vi.mocked(statSync).mockReturnValue({
        mode: 0o100600,
      } as ReturnType<typeof statSync>);

      const mockDecipher = {
        update: vi
          .fn()
          .mockReturnValue(JSON.stringify({ version: 1, credentials: {} })),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { readCredentialsStore } =
        await import('../../src/credentials/storage.js');

      readCredentialsStore();

      expect(chmodSync).not.toHaveBeenCalled();
    });
  });

  describe('decrypt invalid encrypted format', () => {
    it('throws Invalid encrypted data format for empty, invalid, and two-part payloads', async () => {
      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockReturnValue(mockKey.toString('hex'));

      const { decrypt } =
        await import('../../src/credentials/credentialEncryption.js');

      expect(() => decrypt('')).toThrow('Invalid encrypted data format');
      expect(() => decrypt('invalid')).toThrow('Invalid encrypted data format');
      expect(() => decrypt('a:b')).toThrow('Invalid encrypted data format');
    });
  });

  describe('readCredentialsStore read failures', () => {
    it('returns empty store and masks GitHub token in logged reason when readFileSync throws', async () => {
      const tokenSuffix = 'ghp_abc123456789012345678901234567890123';
      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        if (String(path).includes('credentials.json')) {
          throw new Error(`corrupt read ${tokenSuffix}`);
        }
        return '';
      });

      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      const { readCredentialsStore } =
        await import('../../src/credentials/storage.js');

      const result = readCredentialsStore();

      expect(result).toEqual({ version: 1, credentials: {} });
      const stderrOutput = stderrSpy.mock.calls.map(c => String(c[0])).join('');
      expect(stderrOutput).toContain('Could not read credentials file');
      expect(stderrOutput).toContain('***MASKED***');
      expect(stderrOutput).not.toContain(tokenSuffix);

      stderrSpy.mockRestore();
    });

    it('returns empty store when readFileSync throws a non-Error value', async () => {
      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        if (String(path).includes('credentials.json')) {
          throw 'not an Error instance';
        }
        return '';
      });

      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      const { readCredentialsStore } =
        await import('../../src/credentials/storage.js');

      const result = readCredentialsStore();

      expect(result).toEqual({ version: 1, credentials: {} });
      const stderrOutput = stderrSpy.mock.calls.map(c => String(c[0])).join('');
      expect(stderrOutput).toContain('Could not read credentials file');

      stderrSpy.mockRestore();
    });
  });

  describe('Credential Store Zod Validation', () => {
    it('should reject credentials with invalid structure', async () => {
      const invalidStore = {
        version: 'not-a-number',
        credentials: {},
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const { statSync } = await import('node:fs');
      vi.mocked(statSync).mockReturnValue({
        mode: 0o100600,
      } as ReturnType<typeof statSync>);

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(invalidStore)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      const { readCredentialsStore } =
        await import('../../src/credentials/storage.js');

      const result = readCredentialsStore();

      expect(result).toEqual({ version: 1, credentials: {} });
      expect(stderrSpy).toHaveBeenCalledWith(
        expect.stringContaining('Credentials file has invalid format')
      );

      stderrSpy.mockRestore();
    });

    it('should reject credentials with missing required fields', async () => {
      const storeWithBadCreds = {
        version: 1,
        credentials: {
          'github.com': {
            hostname: 'github.com',
          },
        },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const { statSync } = await import('node:fs');
      vi.mocked(statSync).mockReturnValue({
        mode: 0o100600,
      } as ReturnType<typeof statSync>);

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(storeWithBadCreds)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      const { readCredentialsStore } =
        await import('../../src/credentials/storage.js');

      const result = readCredentialsStore();

      expect(result).toEqual({ version: 1, credentials: {} });

      stderrSpy.mockRestore();
    });

    it('should accept valid credential store data', async () => {
      const validStore = {
        version: 1,
        credentials: {
          'github.com': createMockCredentials(),
        },
      };

      vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return true;
        if (String(path).includes('credentials.json')) return true;
        return false;
      });
      vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
        if (String(path).includes('.key')) return mockKey.toString('hex');
        return 'iv:authtag:encrypted';
      });

      const { statSync } = await import('node:fs');
      vi.mocked(statSync).mockReturnValue({
        mode: 0o100600,
      } as ReturnType<typeof statSync>);

      const mockDecipher = {
        update: vi.fn().mockReturnValue(JSON.stringify(validStore)),
        final: vi.fn().mockReturnValue(''),
        setAuthTag: vi.fn(),
      };
      vi.mocked(crypto.createDecipheriv).mockReturnValue(
        mockDecipher as unknown as crypto.DecipherGCM
      );

      const { readCredentialsStore } =
        await import('../../src/credentials/storage.js');

      const result = readCredentialsStore();

      expect(result.version).toBe(1);
      expect(result.credentials['github.com'].hostname).toBe('github.com');
      expect(result.credentials['github.com'].token.token).toBe(
        'ghp_MOCK_TOKEN_00000000000000000000'
      );
    });
  });

  describe('tokenRefresh dependency injection', () => {
    it('refreshAuthToken accepts injected deps and calls getCredentials', async () => {
      const { refreshAuthToken: refreshAuthTokenCore } =
        await import('../../src/credentials/tokenRefresh.js');

      const mockGetCredentials = vi.fn().mockResolvedValue(null);
      const mockUpdateToken = vi.fn();

      const result = await refreshAuthTokenCore(
        { getCredentials: mockGetCredentials, updateToken: mockUpdateToken },
        'github.com'
      );

      expect(mockGetCredentials).toHaveBeenCalledWith('github.com');
      expect(result.success).toBe(false);
      expect(result.error).toContain('Not logged in');
    });

    it('refreshAuthToken uses injected getCredentials to find credentials', async () => {
      const { refreshAuthToken: refreshAuthTokenCore } =
        await import('../../src/credentials/tokenRefresh.js');

      const mockGetCredentials = vi.fn().mockResolvedValue({
        hostname: 'github.com',
        username: 'testuser',
        token: {
          token: 'expired-token',
          tokenType: 'oauth',
        },
      });
      const mockUpdateToken = vi.fn();

      const result = await refreshAuthTokenCore(
        { getCredentials: mockGetCredentials, updateToken: mockUpdateToken },
        'github.com'
      );

      expect(mockGetCredentials).toHaveBeenCalledWith('github.com');
      expect(result.success).toBe(false);
      expect(result.error).toContain('does not support refresh');
      expect(mockUpdateToken).not.toHaveBeenCalled();
    });

    it('getTokenWithRefresh returns none when injected getCredentials returns null', async () => {
      const { getTokenWithRefresh: getTokenWithRefreshCore } =
        await import('../../src/credentials/tokenRefresh.js');

      const mockGetCredentials = vi.fn().mockResolvedValue(null);
      const mockUpdateToken = vi.fn();

      const result = await getTokenWithRefreshCore(
        { getCredentials: mockGetCredentials, updateToken: mockUpdateToken },
        'github.com'
      );

      expect(mockGetCredentials).toHaveBeenCalledWith('github.com');
      expect(result.token).toBeNull();
      expect(result.source).toBe('none');
    });

    it('getTokenWithRefresh returns stored token via injected deps', async () => {
      const { getTokenWithRefresh: getTokenWithRefreshCore } =
        await import('../../src/credentials/tokenRefresh.js');

      const futureDate = new Date(
        Date.now() + 24 * 60 * 60 * 1000
      ).toISOString();
      const mockGetCredentials = vi.fn().mockResolvedValue({
        hostname: 'github.com',
        username: 'testuser',
        token: {
          token: 'valid-token',
          tokenType: 'oauth',
          expiresAt: futureDate,
        },
      });
      const mockUpdateToken = vi.fn();

      const result = await getTokenWithRefreshCore(
        { getCredentials: mockGetCredentials, updateToken: mockUpdateToken },
        'github.com'
      );

      expect(result.token).toBe('valid-token');
      expect(result.source).toBe('stored');
      expect(result.username).toBe('testuser');
      expect(mockUpdateToken).not.toHaveBeenCalled();
    });

    it('storage.ts wrapper binds its own getCredentials/updateToken', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { refreshAuthToken } =
        await import('../../src/credentials/storage.js');

      const result = await refreshAuthToken('github.com');
      expect(result.success).toBe(false);
      expect(result.error).toContain('Not logged in');
    });
  });
});
