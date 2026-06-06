import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { getOctokit, clearOctokitInstances } from '../../src/github/client.js';

vi.mock('../../src/serverConfig.js', () => ({
  getGitHubToken: vi.fn(() => Promise.resolve('test-token')),
  getServerConfig: vi.fn(() => ({
    timeout: 30000,
    version: '1.0.0',
    githubApiUrl: 'https://api.github.com',
    maxRetries: 3,
    loggingEnabled: true,
  })),
}));

vi.mock('octokit', () => {
  const mockOctokitClass = vi.fn(function (options: unknown) {
    return {
      options,
      rest: {
        repos: {
          get: vi.fn(),
        },
      },
    };
  });

  Object.assign(mockOctokitClass, {
    plugin: vi.fn(function () {
      return mockOctokitClass;
    }),
  });

  return {
    Octokit: mockOctokitClass,
  };
});

vi.mock('@octokit/plugin-throttling', () => ({
  throttling: {},
}));

import { getServerConfig } from '../../src/serverConfig.js';
import { Octokit } from 'octokit';

const mockGetServerConfig = vi.mocked(getServerConfig);
const mockOctokit = vi.mocked(Octokit);

describe('GitHub Client Branch Coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearOctokitInstances();
  });

  afterEach(() => {
    clearOctokitInstances();
  });

  describe('Timeout fallback (line 53)', () => {
    it('should use default timeout when config.timeout is undefined', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        timeout: undefined as unknown as number,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: true,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GH_TOKEN',
      });

      await getOctokit();

      expect(mockOctokit).toHaveBeenCalledWith(
        expect.objectContaining({
          request: { timeout: 30000 },
        })
      );
    });

    it('should use default timeout when config.timeout is 0', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        timeout: 0,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: true,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GH_TOKEN',
      });

      await getOctokit();

      expect(mockOctokit).toHaveBeenCalledWith(
        expect.objectContaining({
          request: { timeout: 30000 },
        })
      );
    });

    it('should use default timeout when config.timeout is null', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        timeout: null as unknown as number,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: true,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GH_TOKEN',
      });

      await getOctokit();

      expect(mockOctokit).toHaveBeenCalledWith(
        expect.objectContaining({
          request: { timeout: 30000 },
        })
      );
    });
  });

  describe('Auth token caching (line 68)', () => {
    it('should reuse cached instance for same auth token', async () => {
      const authInfo = {
        token: 'cached-token',
        clientId: 'test-client',
        scopes: [] as string[],
      };

      const instance1 = await getOctokit(authInfo);

      const instance2 = await getOctokit(authInfo);

      expect(instance1).toBe(instance2);

      expect(mockOctokit).toHaveBeenCalledTimes(1);
    });

    it('should create separate instances for different auth tokens', async () => {
      const auth1 = {
        token: 'token-a',
        clientId: 'client-a',
        scopes: [] as string[],
      };
      const auth2 = {
        token: 'token-b',
        clientId: 'client-b',
        scopes: [] as string[],
      };

      const instance1 = await getOctokit(auth1);
      const instance2 = await getOctokit(auth2);

      expect(instance1).not.toBe(instance2);
      expect(mockOctokit).toHaveBeenCalledTimes(2);
    });

    it('should reuse auth instance even after default instance is created', async () => {
      const authInfo = {
        token: 'reusable-token',
        clientId: 'reuse-client',
        scopes: [] as string[],
      };

      const authInstance1 = await getOctokit(authInfo);

      await getOctokit();

      const authInstance2 = await getOctokit(authInfo);

      expect(authInstance1).toBe(authInstance2);
      expect(mockOctokit).toHaveBeenCalledTimes(2);
    });

    it('should hash tokens and cache by hash', async () => {
      const authInfo = {
        token: 'my-secret-token-that-should-be-hashed',
        clientId: 'hash-client',
        scopes: [] as string[],
      };

      const instance1 = await getOctokit(authInfo);
      const instance2 = await getOctokit(authInfo);

      expect(instance1).toBe(instance2);
      expect(mockOctokit).toHaveBeenCalledTimes(1);
    });
  });

  describe('Expired instance replacement', () => {
    it('should create new instance when cached auth instance has expired', async () => {
      const authInfo = {
        token: 'expiring-token',
        clientId: 'test-client',
        scopes: [] as string[],
      };

      const originalNow = Date.now;
      let fakeTime = 1_000_000;
      Date.now = () => fakeTime;

      try {
        await getOctokit(authInfo);
        expect(mockOctokit).toHaveBeenCalledTimes(1);

        fakeTime += 5 * 60 * 1000 + 1;

        await getOctokit(authInfo);
        expect(mockOctokit).toHaveBeenCalledTimes(2);
      } finally {
        Date.now = originalNow;
      }
    });
  });

  describe('Token spread in options (line 55)', () => {
    it('should include auth in options when token is provided', async () => {
      const authInfo = {
        token: 'explicit-token',
        clientId: 'test',
        scopes: [] as string[],
      };

      await getOctokit(authInfo);

      expect(mockOctokit).toHaveBeenCalledWith(
        expect.objectContaining({
          auth: 'explicit-token',
        })
      );
    });

    it('should not include auth in options when token is empty string', async () => {
      vi.mocked(
        await import('../../src/serverConfig.js')
      ).getGitHubToken.mockResolvedValue('');

      await getOctokit();

      const callArgs = mockOctokit.mock.calls[0]?.[0] as { auth?: string };
      expect(callArgs.auth).toBeUndefined();
    });
  });

  describe('purgeExpiredInstances over capacity (lines 79-89)', () => {
    it('should evict oldest non-DEFAULT entries when over MAX_INSTANCES', async () => {
      const originalNow = Date.now;
      let fakeTime = 1_000_000;
      Date.now = () => fakeTime;

      try {
        for (let i = 0; i < 51; i++) {
          await getOctokit({
            token: `capacity-token-${i}`,
            clientId: `client-${i}`,
            scopes: [],
          });
          fakeTime += 10;
        }

        const instance = await getOctokit({
          token: 'post-eviction-token',
          clientId: 'post-eviction',
          scopes: [],
        });

        expect(instance).toBeDefined();
      } finally {
        Date.now = originalNow;
      }
    });

    it('should evict expired entries before LRU eviction during capacity check', async () => {
      const originalNow = Date.now;
      let fakeTime = 1_000_000;
      Date.now = () => fakeTime;

      try {
        for (let i = 0; i < 30; i++) {
          await getOctokit({
            token: `expire-capacity-${i}`,
            clientId: `expire-${i}`,
            scopes: [],
          });
          fakeTime += 10;
        }

        fakeTime += 5 * 60 * 1000 + 1;

        for (let i = 0; i < 20; i++) {
          await getOctokit({
            token: `fresh-capacity-${i}`,
            clientId: `fresh-${i}`,
            scopes: [],
          });
          fakeTime += 10;
        }

        const instance = await getOctokit({
          token: 'final-capacity-token',
          clientId: 'final',
          scopes: [],
        });

        expect(instance).toBeDefined();
      } finally {
        Date.now = originalNow;
      }
    });
  });
});
