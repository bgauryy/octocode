import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import {
  initialize,
  cleanup,
  getGitHubToken,
  getTokenSource,
  getServerConfig,
  _setTokenResolvers,
  _resetTokenResolvers,
} from '../../octocode-tools-core/src/serverConfig.js';
import type { FullTokenResolution } from '@octocodeai/octocode-tools-core/credentials';

type ResolveTokenFullMock = Mock<
  (options?: {
    hostname?: string;
    clientId?: string;
  }) => Promise<FullTokenResolution | null>
>;

let mockResolveTokenFull: ResolveTokenFullMock;

function mockResult(
  token: string | null,
  source:
    | 'env:OCTOCODE_TOKEN'
    | 'env:GH_TOKEN'
    | 'env:GITHUB_TOKEN'
    | 'octocode-storage'
    | 'gh-cli'
    | null,
  extra?: Partial<FullTokenResolution>
): FullTokenResolution | null {
  if (!token) return null;
  return {
    token,
    source,
    wasRefreshed: false,
    ...extra,
  };
}

describe('Token Fallback Chain Behavior', () => {
  const savedEnvVars: Record<string, string | undefined> = {};

  beforeEach(() => {
    vi.clearAllMocks();
    cleanup();

    savedEnvVars.OCTOCODE_TOKEN = process.env.OCTOCODE_TOKEN;
    savedEnvVars.GH_TOKEN = process.env.GH_TOKEN;
    savedEnvVars.GITHUB_TOKEN = process.env.GITHUB_TOKEN;

    delete process.env.OCTOCODE_TOKEN;
    delete process.env.GH_TOKEN;
    delete process.env.GITHUB_TOKEN;

    mockResolveTokenFull = vi.fn(async () => null);
    _setTokenResolvers({ resolveTokenFull: mockResolveTokenFull });
  });

  afterEach(() => {
    for (const key of Object.keys(savedEnvVars)) {
      if (savedEnvVars[key] !== undefined) {
        process.env[key] = savedEnvVars[key];
      } else {
        delete process.env[key];
      }
    }
    cleanup();
    _resetTokenResolvers();
  });

  describe('Runtime Source Transitions', () => {
    it('should transition from env → storage when env token is removed', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(mockResult('env-token', 'env:GITHUB_TOKEN'))
        .mockResolvedValueOnce(mockResult('stored-token', 'octocode-storage'));

      const token1 = await getGitHubToken();
      expect(token1).toBe('env-token');

      const token2 = await getGitHubToken();
      expect(token2).toBe('stored-token');

      expect(mockResolveTokenFull).toHaveBeenCalledTimes(2);
    });

    it('should transition from env → gh-cli when env and storage fail', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(mockResult('env-token', 'env:GH_TOKEN'))
        .mockResolvedValueOnce(mockResult('cli-token', 'gh-cli'));

      expect(await getGitHubToken()).toBe('env-token');
      expect(await getTokenSource()).toBe('gh-cli');
    });

    it('should transition from storage → gh-cli when storage becomes empty', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(mockResult('stored-token', 'octocode-storage'))
        .mockResolvedValueOnce(mockResult('gh-cli-fallback', 'gh-cli'));

      expect(await getGitHubToken()).toBe('stored-token');
      expect(await getGitHubToken()).toBe('gh-cli-fallback');
    });

    it('should transition from gh-cli → none when CLI becomes unavailable', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(mockResult('cli-token', 'gh-cli'))
        .mockResolvedValueOnce(null);

      expect(await getGitHubToken()).toBe('cli-token');
      expect(await getGitHubToken()).toBeNull();
    });

    it('should transition across full chain: env → storage → gh-cli → none', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(mockResult('env-token', 'env:OCTOCODE_TOKEN'))
        .mockResolvedValueOnce(mockResult('stored-token', 'octocode-storage'))
        .mockResolvedValueOnce(mockResult('cli-token', 'gh-cli'))
        .mockResolvedValueOnce(null);

      expect(await getGitHubToken()).toBe('env-token');
      expect(await getGitHubToken()).toBe('stored-token');
      expect(await getGitHubToken()).toBe('cli-token');
      expect(await getGitHubToken()).toBeNull();
      expect(mockResolveTokenFull).toHaveBeenCalledTimes(4);
    });

    it('should recover when a previously-failed source becomes available', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(null)
        .mockResolvedValueOnce(
          mockResult('recovered-token', 'octocode-storage')
        )
        .mockResolvedValueOnce(null)
        .mockResolvedValueOnce(mockResult('new-env-token', 'env:GITHUB_TOKEN'));

      expect(await getGitHubToken()).toBeNull();
      expect(await getGitHubToken()).toBe('recovered-token');
      expect(await getGitHubToken()).toBeNull();
      expect(await getGitHubToken()).toBe('new-env-token');
    });
  });

  describe('Token Source Tracking Through Fallbacks', () => {
    it('should track source transitions accurately', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(mockResult('tok1', 'env:OCTOCODE_TOKEN'))
        .mockResolvedValueOnce(mockResult('tok2', 'env:GH_TOKEN'))
        .mockResolvedValueOnce(mockResult('tok3', 'env:GITHUB_TOKEN'))
        .mockResolvedValueOnce(mockResult('tok4', 'octocode-storage'))
        .mockResolvedValueOnce(mockResult('tok5', 'gh-cli'))
        .mockResolvedValueOnce(null);

      expect(await getTokenSource()).toBe('env:OCTOCODE_TOKEN');
      expect(await getTokenSource()).toBe('env:GH_TOKEN');
      expect(await getTokenSource()).toBe('env:GITHUB_TOKEN');
      expect(await getTokenSource()).toBe('octocode-storage');
      expect(await getTokenSource()).toBe('gh-cli');
      expect(await getTokenSource()).toBe('none');
    });

    it('should map "file" source to "octocode-storage" consistently', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockResult('stored-tok', 'octocode-storage')
      );

      expect(await getTokenSource()).toBe('octocode-storage');
      expect(await getTokenSource()).toBe('octocode-storage');
    });
  });

  describe('Refresh failure (no token from storage)', () => {
    it('should return null when resolveTokenFull returns null after storage refresh failure', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      const token = await getGitHubToken();
      expect(token).toBeNull();
    });

    it('should return "none" source when resolveTokenFull returns null', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      expect(await getTokenSource()).toBe('none');
      expect(await getGitHubToken()).toBeNull();
    });

    it('should handle null resolution then recovery via gh-cli', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(null)
        .mockResolvedValueOnce(mockResult('cli-rescue', 'gh-cli'));

      expect(await getGitHubToken()).toBeNull();
      expect(await getGitHubToken()).toBe('cli-rescue');
    });

    it('should handle wasRefreshed=true from storage (successful refresh)', async () => {
      mockResolveTokenFull.mockResolvedValue({
        token: 'refreshed-token',
        source: 'octocode-storage',
        wasRefreshed: true,
        username: 'testuser',
      });

      const token = await getGitHubToken();
      expect(token).toBe('refreshed-token');

      const source = await getTokenSource();
      expect(source).toBe('octocode-storage');
    });
  });

  describe('Error Recovery', () => {
    it('should recover gracefully after resolveTokenFull throws', async () => {
      mockResolveTokenFull
        .mockRejectedValueOnce(new Error('Network error'))
        .mockResolvedValueOnce(mockResult('recovered', 'env:GITHUB_TOKEN'));

      expect(await getGitHubToken()).toBeNull();
      expect(await getGitHubToken()).toBe('recovered');
    });

    it('should handle multiple consecutive errors before recovery', async () => {
      mockResolveTokenFull
        .mockRejectedValueOnce(new Error('Error 1'))
        .mockRejectedValueOnce(new Error('Error 2'))
        .mockRejectedValueOnce(new Error('Error 3'))
        .mockResolvedValueOnce(mockResult('finally-works', 'env:GH_TOKEN'));

      expect(await getGitHubToken()).toBeNull();
      expect(await getGitHubToken()).toBeNull();
      expect(await getGitHubToken()).toBeNull();
      expect(await getGitHubToken()).toBe('finally-works');
    });

    it('should not leak error state between calls', async () => {
      mockResolveTokenFull
        .mockRejectedValueOnce(new Error('Crash'))
        .mockResolvedValueOnce(mockResult('clean-token', 'env:GITHUB_TOKEN'));

      const source1 = await getTokenSource();
      expect(source1).toBe('none');

      const source2 = await getTokenSource();
      expect(source2).toBe('env:GITHUB_TOKEN');
    });
  });

  describe('Edge Cases', () => {
    it('should treat empty string token as no token', async () => {
      mockResolveTokenFull.mockResolvedValue({
        token: '',
        source: 'env:GITHUB_TOKEN',
        wasRefreshed: false,
      });

      expect(await getGitHubToken()).toBeNull();
    });

    it('should handle undefined source gracefully', async () => {
      mockResolveTokenFull.mockResolvedValue({
        token: 'some-token',
        source: undefined as unknown as null,
        wasRefreshed: false,
      });

      expect(await getGitHubToken()).toBe('some-token');
      expect(await getTokenSource()).toBe('none');
    });

    it('should handle unrecognized source string gracefully', async () => {
      mockResolveTokenFull.mockResolvedValue({
        token: 'some-token',
        source: 'unknown-source' as FullTokenResolution['source'],
        wasRefreshed: false,
      });

      expect(await getGitHubToken()).toBe('some-token');
      expect(await getTokenSource()).toBe('none');
    });

    it('should handle resolveTokenFull returning undefined', async () => {
      mockResolveTokenFull.mockResolvedValue(undefined as unknown as null);

      expect(await getGitHubToken()).toBeNull();
      expect(await getTokenSource()).toBe('none');
    });
  });

  describe('Concurrent Fallback Resolution', () => {
    it('should handle concurrent calls with different fallback results', async () => {
      let callCount = 0;
      mockResolveTokenFull.mockImplementation(async () => {
        callCount++;
        const currentCall = callCount;
        await new Promise(resolve => setTimeout(resolve, 10));

        if (currentCall === 1)
          return mockResult('env-token', 'env:GITHUB_TOKEN');
        if (currentCall === 2)
          return mockResult('stored-token', 'octocode-storage');
        return mockResult('cli-token', 'gh-cli');
      });

      const results = await Promise.all([
        getGitHubToken(),
        getGitHubToken(),
        getGitHubToken(),
      ]);

      expect(results).toContain('env-token');
      expect(results).toContain('stored-token');
      expect(results).toContain('cli-token');
    });

    it('should handle concurrent calls where some fail and some succeed', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(mockResult('success-1', 'env:GITHUB_TOKEN'))
        .mockRejectedValueOnce(new Error('Intermittent failure'))
        .mockResolvedValueOnce(mockResult('success-2', 'octocode-storage'))
        .mockRejectedValueOnce(new Error('Another failure'));

      const results = await Promise.all([
        getGitHubToken(),
        getGitHubToken(),
        getGitHubToken(),
        getGitHubToken(),
      ]);

      const successes = results.filter(t => t !== null);
      const failures = results.filter(t => t === null);
      expect(successes.length).toBe(2);
      expect(failures.length).toBe(2);
      expect(successes).toContain('success-1');
      expect(successes).toContain('success-2');
    });
  });

  describe('Initialize with Fallback', () => {
    it('should initialize successfully even when no token is available', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      await expect(initialize()).resolves.not.toThrow();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('none');
    });

    it('should initialize with storage fallback token', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockResult('init-stored-token', 'octocode-storage')
      );

      await initialize();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('octocode-storage');
    });

    it('should initialize with gh-cli fallback token', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockResult('init-cli-token', 'gh-cli')
      );

      await initialize();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('gh-cli');
    });

    it('should initialize even when resolveTokenFull throws during init', async () => {
      mockResolveTokenFull
        .mockRejectedValueOnce(new Error('Init failure'))
        .mockResolvedValueOnce(
          mockResult('post-init-token', 'env:GITHUB_TOKEN')
        );

      await expect(initialize()).resolves.not.toThrow();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('none');

      const token = await getGitHubToken();
      expect(token).toBe('post-init-token');
    });

    it('should use fresh resolution after initialize (not cached from init)', async () => {
      mockResolveTokenFull
        .mockResolvedValueOnce(mockResult('init-token', 'env:GITHUB_TOKEN'))
        .mockResolvedValueOnce(mockResult('fresh-cli-token', 'gh-cli'));

      await initialize();

      const freshToken = await getGitHubToken();
      expect(freshToken).toBe('fresh-cli-token');
    });
  });
});
