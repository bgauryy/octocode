import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const ENV_VARS = ['OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN'] as const;
const savedEnv: Record<string, string | undefined> = {};

function saveAndClearEnv() {
  for (const v of ENV_VARS) {
    savedEnv[v] = process.env[v];
    delete process.env[v];
  }
}

function restoreEnv() {
  for (const v of ENV_VARS) {
    if (savedEnv[v] !== undefined) {
      process.env[v] = savedEnv[v];
    } else {
      delete process.env[v];
    }
  }
}

describe('tokenResolution', () => {
  beforeEach(() => {
    saveAndClearEnv();
    vi.resetModules();
  });

  afterEach(restoreEnv);

  async function loadModule() {
    return import('../../src/credentials/tokenResolution.js');
  }

  describe('initTokenResolution / resetTokenResolution', () => {
    it('throws when resolveTokenFull called before init', async () => {
      const mod = await loadModule();
      await expect(mod.resolveTokenFull()).rejects.toThrow(
        'Token resolution not initialized'
      );
    });

    it('succeeds after initTokenResolution is called', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });
      const result = await mod.resolveTokenFull({ getGhCliToken: () => null });
      expect(result).toBeNull();
    });

    it('throws again after resetTokenResolution', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });
      mod.resetTokenResolution();
      await expect(mod.resolveTokenFull()).rejects.toThrow(
        'Token resolution not initialized'
      );
    });
  });

  describe('resolveTokenFull', () => {
    it('prefers env over storage over gh-cli', async () => {
      process.env.GITHUB_TOKEN = 'env-val';
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi.fn(),
      });

      const result = await mod.resolveTokenFull({
        getGhCliToken: vi.fn().mockReturnValue('gh-cli-val'),
      });
      expect(result?.source).toBe('env:GITHUB_TOKEN');
    });

    it('OCTOCODE_TOKEN wins over all other env vars', async () => {
      process.env.OCTOCODE_TOKEN = 'oc-wins';
      process.env.GH_TOKEN = 'gh-loses';
      process.env.GITHUB_TOKEN = 'github-loses';
      const mod = await loadModule();
      mod.initTokenResolution({ getTokenWithRefresh: vi.fn() });

      const result = await mod.resolveTokenFull({ getGhCliToken: () => null });
      expect(result).toMatchObject({
        token: 'oc-wins',
        source: 'env:OCTOCODE_TOKEN',
      });
    });

    it('falls back to storage when no env token', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi.fn().mockResolvedValue({
          token: 'stored',
          source: 'stored',
          username: 'user1',
        }),
      });

      const result = await mod.resolveTokenFull({ getGhCliToken: () => null });
      expect(result).toMatchObject({
        token: 'stored',
        source: 'octocode-storage',
        wasRefreshed: false,
        username: 'user1',
      });
    });

    it('reports wasRefreshed=true when token was refreshed', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi.fn().mockResolvedValue({
          token: 'new-tok',
          source: 'refreshed',
          username: 'user1',
        }),
      });

      const result = await mod.resolveTokenFull({ getGhCliToken: () => null });
      expect(result?.wasRefreshed).toBe(true);
    });

    it('falls back to gh-cli when storage has no token', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });

      const result = await mod.resolveTokenFull({
        getGhCliToken: vi.fn().mockReturnValue('gh-cli-token'),
      });
      expect(result).toMatchObject({
        token: 'gh-cli-token',
        source: 'gh-cli',
      });
    });

    it('trims whitespace from gh-cli token', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });

      const result = await mod.resolveTokenFull({
        getGhCliToken: () => '  spaced  ',
      });
      expect(result?.token).toBe('spaced');
    });

    it('handles gh-cli throwing error gracefully', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });

      const result = await mod.resolveTokenFull({
        getGhCliToken: () => {
          throw new Error('gh not found');
        },
      });
      expect(result).toBeNull();
    });

    it('returns null when no token source available', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });

      expect(
        await mod.resolveTokenFull({ getGhCliToken: () => null })
      ).toBeNull();
    });

    it('returns null when storage refresh fails and gh-cli also fails', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi.fn().mockResolvedValue({
          token: null,
          source: 'none',
          refreshError: 'token expired',
        }),
      });

      const result = await mod.resolveTokenFull({
        getGhCliToken: () => null,
      });
      expect(result).toBeNull();
    });

    it('passes hostname to gh-cli getter', async () => {
      const mod = await loadModule();
      const ghCliGetter = vi.fn().mockReturnValue(null);
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });

      await mod.resolveTokenFull({
        hostname: 'enterprise.example.com',
        getGhCliToken: ghCliGetter,
      });
      expect(ghCliGetter).toHaveBeenCalledWith('enterprise.example.com');
    });

    it('supports async gh-cli getter', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });

      const result = await mod.resolveTokenFull({
        getGhCliToken: async () => 'async-gh-token',
      });
      expect(result?.token).toBe('async-gh-token');
    });
  });

  describe('resolveToken (deprecated wrapper)', () => {
    it('returns env token with highest priority', async () => {
      process.env.OCTOCODE_TOKEN = 'env-token';
      const mod = await loadModule();
      mod.initTokenResolution({ getTokenWithRefresh: vi.fn() });

      const result = await mod.resolveToken();
      expect(result).toEqual({
        token: 'env-token',
        source: 'env:OCTOCODE_TOKEN',
      });
    });

    it('falls back to storage when no env token', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi.fn().mockResolvedValue({
          token: 'stored-token',
          source: 'stored',
        }),
      });

      const result = await mod.resolveToken('github.com');
      expect(result).toMatchObject({
        token: 'stored-token',
        source: 'octocode-storage',
      });
    });

    it('returns null when neither env nor storage has token', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });

      expect(await mod.resolveToken()).toBeNull();
    });

    it('does not use gh-cli', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });

      expect(await mod.resolveToken()).toBeNull();
    });
  });

  describe('resolveTokenWithRefresh (deprecated wrapper)', () => {
    it('returns env token without refresh', async () => {
      process.env.GH_TOKEN = 'env-gh';
      const mod = await loadModule();
      mod.initTokenResolution({ getTokenWithRefresh: vi.fn() });

      const result = await mod.resolveTokenWithRefresh();
      expect(result).toMatchObject({
        token: 'env-gh',
        source: 'env:GH_TOKEN',
        wasRefreshed: false,
      });
    });

    it('returns stored token with wasRefreshed=false when not refreshed', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi.fn().mockResolvedValue({
          token: 'stored',
          source: 'stored',
          username: 'user1',
        }),
      });

      const result = await mod.resolveTokenWithRefresh();
      expect(result).toMatchObject({
        token: 'stored',
        source: 'octocode-storage',
        wasRefreshed: false,
        username: 'user1',
      });
    });

    it('returns wasRefreshed=true when token was refreshed', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi.fn().mockResolvedValue({
          token: 'refreshed-tok',
          source: 'refreshed',
          username: 'user1',
        }),
      });

      const result = await mod.resolveTokenWithRefresh();
      expect(result?.wasRefreshed).toBe(true);
    });

    it('returns null when no token and no refresh token available', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi.fn().mockResolvedValue({
          token: null,
          source: 'none',
          refreshError: 'Token expired and no refresh token available',
        }),
      });

      expect(await mod.resolveTokenWithRefresh()).toBeNull();
    });

    it('returns null when no token and no refresh error', async () => {
      const mod = await loadModule();
      mod.initTokenResolution({
        getTokenWithRefresh: vi
          .fn()
          .mockResolvedValue({ token: null, source: 'none' }),
      });

      expect(await mod.resolveTokenWithRefresh()).toBeNull();
    });
  });
});
