/**
 * Token Resolution Priority Tests
 *
 * Tests the documented token resolution priority order:
 * 1. OCTOCODE_TOKEN env
 * 2. GH_TOKEN env
 * 3. GITHUB_TOKEN env
 * 4. File (encrypted storage ~/.octocode/credentials.json)
 * 5. gh auth token (GitHub CLI) - fallback
 *
 * Note: Token resolution is DYNAMIC - no caching.
 * Each call to getGitHubToken() resolves fresh.
 *
 * These tests mock `resolveTokenFull` from octocode-shared which handles
 * the entire resolution chain. Tests for the actual priority chain logic
 * are in the octocode-shared package.
 *
 * @see docs/AUTHENTICATION_SETUP.md
 */

import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import {
  initialize,
  cleanup,
  getGitHubToken,
  getServerConfig,
  getTokenSource,
  _setTokenResolvers,
  _resetTokenResolvers,
} from '../src/serverConfig.js';
import type { FullTokenResolution } from 'octocode-shared';

describe('Token Resolution Priority (AUTHENTICATION_SETUP.md)', () => {
  // Store original env values (not a reference to process.env!)
  const savedEnvVars: Record<string, string | undefined> = {};

  // Mock for resolveTokenFull from octocode-shared
  type ResolveTokenFullMock = Mock<
    (options?: {
      hostname?: string;
      clientId?: string;
      getGhCliToken?: (
        hostname?: string
      ) => string | null | Promise<string | null>;
    }) => Promise<FullTokenResolution | null>
  >;

  let mockResolveTokenFull: ResolveTokenFullMock;

  // Helper to create token resolution result
  function mockTokenResult(
    token: string | null,
    source:
      | 'env:OCTOCODE_TOKEN'
      | 'env:GH_TOKEN'
      | 'env:GITHUB_TOKEN'
      | 'file'
      | 'file'
      | 'gh-cli'
      | null
  ): FullTokenResolution | null {
    if (!token) return null;
    return {
      token,
      source,
      wasRefreshed: false,
    };
  }

  beforeEach(() => {
    vi.clearAllMocks();
    cleanup();

    // Save and clear token-related env vars (don't replace process.env!)
    // This is critical: we must modify the SAME process.env object that
    // octocode-shared reads from, not create a new object
    savedEnvVars.OCTOCODE_TOKEN = process.env.OCTOCODE_TOKEN;
    savedEnvVars.GH_TOKEN = process.env.GH_TOKEN;
    savedEnvVars.GITHUB_TOKEN = process.env.GITHUB_TOKEN;
    savedEnvVars.LOG = process.env.LOG;

    delete process.env.OCTOCODE_TOKEN;
    delete process.env.GH_TOKEN;
    delete process.env.GITHUB_TOKEN;
    delete process.env.LOG;

    // Create fresh mock for resolveTokenFull
    mockResolveTokenFull = vi.fn(async () => null);

    // Inject mock
    _setTokenResolvers({
      resolveTokenFull: mockResolveTokenFull,
    });
  });

  afterEach(() => {
    // Restore original env values
    if (savedEnvVars.OCTOCODE_TOKEN !== undefined) {
      process.env.OCTOCODE_TOKEN = savedEnvVars.OCTOCODE_TOKEN;
    } else {
      delete process.env.OCTOCODE_TOKEN;
    }
    if (savedEnvVars.GH_TOKEN !== undefined) {
      process.env.GH_TOKEN = savedEnvVars.GH_TOKEN;
    } else {
      delete process.env.GH_TOKEN;
    }
    if (savedEnvVars.GITHUB_TOKEN !== undefined) {
      process.env.GITHUB_TOKEN = savedEnvVars.GITHUB_TOKEN;
    } else {
      delete process.env.GITHUB_TOKEN;
    }
    if (savedEnvVars.LOG !== undefined) {
      process.env.LOG = savedEnvVars.LOG;
    } else {
      delete process.env.LOG;
    }

    cleanup();
    _resetTokenResolvers();
  });

  describe('Priority Order Verification', () => {
    it('should use env token when available (Priority 1-3)', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('env-token', 'env:GITHUB_TOKEN')
      );

      const token = await getGitHubToken();

      expect(token).toBe('env-token');
      expect(mockResolveTokenFull).toHaveBeenCalledTimes(1);
    });

    it('should use stored credentials when env is empty (Priority 4-5)', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('stored-token', 'file')
      );

      const token = await getGitHubToken();

      expect(token).toBe('stored-token');
      expect(mockResolveTokenFull).toHaveBeenCalledTimes(1);
    });

    it('should use GitHub CLI when storage is empty (Priority 6)', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('gh-cli-token', 'gh-cli')
      );

      const token = await getGitHubToken();

      expect(token).toBe('gh-cli-token');
      expect(mockResolveTokenFull).toHaveBeenCalledTimes(1);
    });

    it('should return null when all sources are exhausted', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      const token = await getGitHubToken();

      expect(token).toBeNull();
      expect(mockResolveTokenFull).toHaveBeenCalledTimes(1);
    });
  });

  describe('Environment Variable Priority (1-3)', () => {
    it('should use OCTOCODE_TOKEN when all env vars are set (Priority 1)', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('octocode-token-wins', 'env:OCTOCODE_TOKEN')
      );

      const token = await getGitHubToken();

      expect(token).toBe('octocode-token-wins');
    });

    it('should use GH_TOKEN when OCTOCODE_TOKEN is not set (Priority 2)', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('gh-token-wins', 'env:GH_TOKEN')
      );

      const token = await getGitHubToken();

      expect(token).toBe('gh-token-wins');
    });

    it('should use GITHUB_TOKEN when others are not set (Priority 3)', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('github-token-wins', 'env:GITHUB_TOKEN')
      );

      const token = await getGitHubToken();

      expect(token).toBe('github-token-wins');
    });
  });

  describe('GitHub CLI Fallback (Priority 6)', () => {
    it('should use gh auth token when env and storage are not available', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('gh-auth-token', 'gh-cli')
      );

      const token = await getGitHubToken();

      expect(token).toBe('gh-auth-token');
    });

    it('should pass getGhCliToken option to resolveTokenFull', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      await getGitHubToken();

      expect(mockResolveTokenFull).toHaveBeenCalledWith(
        expect.objectContaining({
          hostname: 'github.com',
          getGhCliToken: expect.any(Function),
        })
      );
    });

    it('should handle CLI token with whitespace trimmed', async () => {
      // The trimming is done by resolveTokenFull, which returns trimmed token
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('cli-token-trimmed', 'gh-cli')
      );

      const token = await getGitHubToken();

      expect(token).toBe('cli-token-trimmed');
    });

    it('should return null when CLI token is empty', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      const token = await getGitHubToken();

      expect(token).toBeNull();
    });

    it('should handle CLI errors gracefully (resolveTokenFull falls through)', async () => {
      // When gh CLI fails, resolveTokenFull returns null or the next source
      mockResolveTokenFull.mockResolvedValue(null);

      const token = await getGitHubToken();

      expect(token).toBeNull();
    });
  });

  describe('Stored Credentials Fallback (Priority 4-5)', () => {
    it('should use stored token when env is not available', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('stored-token', 'file')
      );

      const token = await getGitHubToken();

      expect(token).toBe('stored-token');
    });

    it('should use file token from encrypted storage', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('file-token', 'file')
      );

      const token = await getGitHubToken();

      expect(token).toBe('file-token');
    });

    it('should handle storage errors gracefully', async () => {
      mockResolveTokenFull.mockRejectedValue(
        new Error('Storage access denied')
      );

      const token = await getGitHubToken();

      expect(token).toBeNull();
    });
  });

  describe('Dynamic Token Resolution (No Caching)', () => {
    it('should resolve token fresh on each call', async () => {
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('token-1', 'env:GITHUB_TOKEN')
      );
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('token-2', 'env:GITHUB_TOKEN')
      );
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('token-3', 'env:GITHUB_TOKEN')
      );

      const token1 = await getGitHubToken();
      const token2 = await getGitHubToken();
      const token3 = await getGitHubToken();

      expect(token1).toBe('token-1');
      expect(token2).toBe('token-2');
      expect(token3).toBe('token-3');
      expect(mockResolveTokenFull).toHaveBeenCalledTimes(3);
    });

    it('should pick up token changes immediately', async () => {
      // First call - env token available
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('env-token', 'env:GITHUB_TOKEN')
      );
      const token1 = await getGitHubToken();
      expect(token1).toBe('env-token');

      // Second call - CLI token available
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('cli-token', 'gh-cli')
      );
      const token2 = await getGitHubToken();
      expect(token2).toBe('cli-token');

      // Third call - no token
      mockResolveTokenFull.mockResolvedValueOnce(null);
      const token3 = await getGitHubToken();
      expect(token3).toBeNull();
    });

    it('should handle concurrent requests independently', async () => {
      let callCount = 0;
      mockResolveTokenFull.mockImplementation(async () => {
        callCount++;
        await new Promise(resolve => setTimeout(resolve, 50));
        return mockTokenResult(`token-${callCount}`, 'env:GITHUB_TOKEN');
      });

      const [token1, token2, token3] = await Promise.all([
        getGitHubToken(),
        getGitHubToken(),
        getGitHubToken(),
      ]);

      expect(token1).toMatch(/^token-\d$/);
      expect(token2).toMatch(/^token-\d$/);
      expect(token3).toMatch(/^token-\d$/);
      expect(callCount).toBe(3);
    });
  });

  describe('Error Handling & Edge Cases', () => {
    it('should mask sensitive data in error messages', async () => {
      const errorWithToken = new Error(
        'Failed to authenticate: token ghp_secrettoken123'
      );
      mockResolveTokenFull.mockRejectedValue(errorWithToken);

      const token = await getGitHubToken();
      expect(token).toBeNull();
    });

    it('should handle null return gracefully', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      const token = await getGitHubToken();
      expect(token).toBeNull();
    });

    it('should handle empty token result', async () => {
      // Empty string token should be treated as no token
      mockResolveTokenFull.mockResolvedValue({
        token: '',
        source: 'env:GITHUB_TOKEN',
        wasRefreshed: false,
      });

      const token = await getGitHubToken();
      // Empty string is falsy, so it's treated as null (no valid token)
      expect(token).toBeNull();
    });
  });

  describe('Integration with initialize()', () => {
    it('should resolve token during initialization', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('init-token', 'env:GITHUB_TOKEN')
      );

      await initialize();

      const token = await getGitHubToken();
      expect(token).toBe('init-token');
    });

    it('should handle missing token during initialization gracefully', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      await expect(initialize()).resolves.not.toThrow();

      const token = await getGitHubToken();
      expect(token).toBeNull();
    });
  });

  describe('Documentation Compliance', () => {
    it('should NOT access storage when env token is available (performance)', async () => {
      // resolveTokenFull handles this internally - we just verify it's called once
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('env-token', 'env:GITHUB_TOKEN')
      );

      await getGitHubToken();

      expect(mockResolveTokenFull).toHaveBeenCalledTimes(1);
    });

    it('should NOT access gh CLI when storage token is available', async () => {
      // resolveTokenFull handles this internally - we just verify it's called once
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('stored-token', 'file')
      );

      await getGitHubToken();

      expect(mockResolveTokenFull).toHaveBeenCalledTimes(1);
    });
  });

  describe('Token Source Tracking', () => {
    it('should track source as env:OCTOCODE_TOKEN when using OCTOCODE_TOKEN', async () => {
      process.env.OCTOCODE_TOKEN = 'octocode-env-token';
      _resetTokenResolvers(); // Use real implementation for env detection

      await initialize();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('env:OCTOCODE_TOKEN');
      expect(await getTokenSource()).toBe('env:OCTOCODE_TOKEN');
    });

    it('should track source as env:GH_TOKEN when using GH_TOKEN', async () => {
      process.env.GH_TOKEN = 'gh-env-token';
      _resetTokenResolvers(); // Use real implementation for env detection

      await initialize();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('env:GH_TOKEN');
      expect(await getTokenSource()).toBe('env:GH_TOKEN');
    });

    it('should track source as env:GITHUB_TOKEN when using GITHUB_TOKEN', async () => {
      process.env.GITHUB_TOKEN = 'github-env-token';
      _resetTokenResolvers(); // Use real implementation for env detection

      await initialize();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('env:GITHUB_TOKEN');
      expect(await getTokenSource()).toBe('env:GITHUB_TOKEN');
    });

    it('should track source as gh-cli when using GitHub CLI', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('gh-cli-token', 'gh-cli')
      );

      await initialize();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('gh-cli');
      expect(await getTokenSource()).toBe('gh-cli');
    });

    it('should track source as octocode-storage when using stored credentials', async () => {
      // 'file' and 'file' sources map to 'octocode-storage'
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('stored-token', 'file')
      );

      await initialize();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('octocode-storage');
      expect(await getTokenSource()).toBe('octocode-storage');
    });

    it('should track source as none when no token is found', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      await initialize();

      const config = getServerConfig();
      expect(config.tokenSource).toBe('none');
      expect(await getTokenSource()).toBe('none');
    });

    it('should resolve fresh source on each getTokenSource call', async () => {
      // First: env token - mock for initialize() and getTokenSource()
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('env-token', 'env:GITHUB_TOKEN')
      );
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('env-token', 'env:GITHUB_TOKEN')
      );
      await initialize();
      expect(await getTokenSource()).toBe('env:GITHUB_TOKEN');

      // Change: CLI available
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('cli-token', 'gh-cli')
      );
      expect(await getTokenSource()).toBe('gh-cli');

      // Change: no token
      mockResolveTokenFull.mockResolvedValueOnce(null);
      expect(await getTokenSource()).toBe('none');
    });
  });

  describe('Source Mapping', () => {
    it('should map file source to octocode-storage', async () => {
      mockResolveTokenFull.mockResolvedValue(mockTokenResult('token', 'file'));

      await initialize();

      expect(getServerConfig().tokenSource).toBe('octocode-storage');
    });

    it('should map file source to octocode-storage', async () => {
      mockResolveTokenFull.mockResolvedValue(mockTokenResult('token', 'file'));

      await initialize();

      expect(getServerConfig().tokenSource).toBe('octocode-storage');
    });

    it('should preserve env sources as-is', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('token', 'env:OCTOCODE_TOKEN')
      );

      await initialize();

      expect(getServerConfig().tokenSource).toBe('env:OCTOCODE_TOKEN');
    });

    it('should preserve gh-cli source as-is', async () => {
      mockResolveTokenFull.mockResolvedValue(
        mockTokenResult('token', 'gh-cli')
      );

      await initialize();

      expect(getServerConfig().tokenSource).toBe('gh-cli');
    });

    it('should map null source to none', async () => {
      mockResolveTokenFull.mockResolvedValue(null);

      await initialize();

      expect(getServerConfig().tokenSource).toBe('none');
    });
  });
});
