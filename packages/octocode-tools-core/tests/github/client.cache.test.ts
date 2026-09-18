import type { AuthInfo } from '@modelcontextprotocol/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestError } from 'octokit';

const mocks = vi.hoisted(() => ({
  reposGet: vi.fn(),
  reposGetBranch: vi.fn(),
}));

vi.mock('../../src/serverConfig.js', () => ({
  getGitHubToken: vi.fn(async () => undefined),
  getServerConfig: vi.fn(() => ({
    githubApiUrl: 'https://api.github.com',
    timeout: 30_000,
  })),
}));

vi.mock('octokit', async importOriginal => {
  const actual = await importOriginal<typeof import('octokit')>();
  const MockOctokit = vi.fn(function (options?: { auth?: string }) {
    return {
      rest: {
        repos: {
          get: (params: unknown) => mocks.reposGet(options?.auth, params),
          getBranch: (params: unknown) =>
            mocks.reposGetBranch(options?.auth, params),
        },
      },
    };
  });
  Object.assign(MockOctokit, { plugin: vi.fn(() => MockOctokit) });
  return { ...actual, Octokit: MockOctokit };
});

vi.mock('@octokit/plugin-throttling', () => ({ throttling: {} }));

import {
  clearOctokitInstances,
  getOctokit,
  hashGitHubToken,
  resolveCacheAuthFingerprint,
  resolveDefaultBranch,
} from '../../src/github/client.js';
import { getServerConfig } from '../../src/serverConfig.js';

const mockGetServerConfig = vi.mocked(getServerConfig);

function auth(token: string): AuthInfo {
  return { token } as AuthInfo;
}

describe('resolveDefaultBranch cache workflow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.reposGet.mockReset();
    mocks.reposGetBranch.mockReset();
    clearOctokitInstances();
    mockGetServerConfig.mockReturnValue({
      githubApiUrl: 'https://api.github.com',
      timeout: 30_000,
    } as ReturnType<typeof getServerConfig>);
  });

  afterEach(() => {
    vi.useRealTimers();
    clearOctokitInstances();
  });

  it.each([401, 403, 404, 429, 500, 503])(
    'preserves metadata HTTP %i without probing other endpoints',
    async status => {
      const failure = new RequestError('Metadata request failed', status, {
        request: { method: 'GET', url: '/repos/o/r', headers: {} },
        response: {
          status,
          url: 'https://api.github.com/repos/o/r',
          headers: { 'retry-after': '120', 'x-ratelimit-remaining': '0' },
          data: {},
        },
      });
      mocks.reposGet.mockRejectedValue(failure);
      mocks.reposGetBranch.mockRejectedValue(failure);

      const result = resolveDefaultBranch('o', 'r');

      await expect(result).rejects.toBe(failure);
      expect(mocks.reposGet).toHaveBeenCalledTimes(1);
      expect(mocks.reposGetBranch).not.toHaveBeenCalled();
    }
  );

  it('preserves network failures without probing branch endpoints', async () => {
    const failure = new TypeError('fetch failed');
    mocks.reposGet.mockRejectedValue(failure);
    mocks.reposGetBranch.mockRejectedValue(failure);

    await expect(resolveDefaultBranch('o', 'r')).rejects.toBe(failure);
    expect(mocks.reposGet).toHaveBeenCalledTimes(1);
    expect(mocks.reposGetBranch).not.toHaveBeenCalled();
  });

  it('does not treat an ordinary permission denial as a missing branch', async () => {
    const failure = new RequestError('Resource not accessible', 403, {
      request: { method: 'GET', url: '/repos/o/r', headers: {} },
    });
    mocks.reposGet.mockRejectedValue(failure);

    await expect(resolveDefaultBranch('o', 'r')).rejects.toBe(failure);
    expect(mocks.reposGet).toHaveBeenCalledTimes(1);
    expect(mocks.reposGetBranch).not.toHaveBeenCalled();
  });

  it.each([undefined, '', '   '])(
    'probes and caches an existing branch when metadata has no usable default (%s)',
    async defaultBranch => {
      mocks.reposGet.mockResolvedValue({
        data: { default_branch: defaultBranch },
      });
      mocks.reposGetBranch
        .mockRejectedValueOnce(
          new RequestError('Branch not found', 404, {
            request: {
              method: 'GET',
              url: '/repos/o/r/branches/main',
              headers: {},
            },
          })
        )
        .mockResolvedValueOnce({ data: { name: 'master' } });

      await expect(resolveDefaultBranch('o', 'r')).resolves.toBe('master');
      await expect(resolveDefaultBranch('o', 'r')).resolves.toBe('master');
      expect(mocks.reposGet).toHaveBeenCalledTimes(1);
      expect(mocks.reposGetBranch.mock.calls).toEqual([
        [undefined, { owner: 'o', repo: 'r', branch: 'main' }],
        [undefined, { owner: 'o', repo: 'r', branch: 'master' }],
      ]);
    }
  );

  it('stops fallback probes when the first branch request is rate limited', async () => {
    mocks.reposGet.mockResolvedValue({ data: {} });
    const failure = new RequestError('Secondary rate limit', 429, {
      request: { method: 'GET', url: '/repos/o/r/branches/main', headers: {} },
    });
    mocks.reposGetBranch.mockRejectedValue(failure);

    await expect(resolveDefaultBranch('o', 'r')).rejects.toBe(failure);
    expect(mocks.reposGet).toHaveBeenCalledTimes(1);
    expect(mocks.reposGetBranch).toHaveBeenCalledTimes(1);
  });

  it('reuses a branch only within the same authentication identity', async () => {
    mocks.reposGet
      .mockResolvedValueOnce({ data: { default_branch: 'private-main' } })
      .mockResolvedValueOnce({ data: { default_branch: 'public-main' } });

    await expect(
      resolveDefaultBranch('octocode', 'core', auth('token-a'))
    ).resolves.toBe('private-main');
    await expect(
      resolveDefaultBranch('octocode', 'core', auth('token-a'))
    ).resolves.toBe('private-main');
    await expect(
      resolveDefaultBranch('octocode', 'core', auth('token-b'))
    ).resolves.toBe('public-main');

    expect(mocks.reposGet).toHaveBeenCalledTimes(2);
  });

  it('refreshes a cached default branch after five minutes', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-08-30T00:00:00Z'));
    mocks.reposGet
      .mockResolvedValueOnce({ data: { default_branch: 'main' } })
      .mockResolvedValueOnce({ data: { default_branch: 'next' } });

    await expect(
      resolveDefaultBranch('octocode', 'core', auth('token-a'))
    ).resolves.toBe('main');

    vi.advanceTimersByTime(5 * 60 * 1000 + 1);

    await expect(
      resolveDefaultBranch('octocode', 'core', auth('token-a'))
    ).resolves.toBe('next');
    expect(mocks.reposGet).toHaveBeenCalledTimes(2);
  });

  it('isolates the same repository across GitHub Enterprise hosts', async () => {
    mocks.reposGet
      .mockResolvedValueOnce({ data: { default_branch: 'enterprise-a' } })
      .mockResolvedValueOnce({ data: { default_branch: 'enterprise-b' } });

    mockGetServerConfig.mockReturnValue({
      githubApiUrl: 'https://github-a.example/api/v3',
      timeout: 30_000,
    } as ReturnType<typeof getServerConfig>);
    await expect(
      resolveDefaultBranch('octocode', 'core', auth('shared-token'))
    ).resolves.toBe('enterprise-a');

    mockGetServerConfig.mockReturnValue({
      githubApiUrl: 'https://github-b.example/api/v3',
      timeout: 30_000,
    } as ReturnType<typeof getServerConfig>);
    await expect(
      resolveDefaultBranch('octocode', 'core', auth('shared-token'))
    ).resolves.toBe('enterprise-b');

    expect(mocks.reposGet).toHaveBeenCalledTimes(2);
  });

  it.each([
    ['https://github-b.example/api/v3', 30_000],
    ['https://github-a.example/other/api/v3', 30_000],
    ['https://github-a.example/api/v3', 60_000],
  ])(
    'isolates client configuration %s with timeout %i',
    async (githubApiUrl, timeout) => {
      mockGetServerConfig.mockReturnValue({
        githubApiUrl: 'https://github-a.example/api/v3',
        timeout: 30_000,
      } as ReturnType<typeof getServerConfig>);
      const first = await getOctokit(auth('shared-token'));
      expect(await getOctokit(auth('shared-token'))).toBe(first);
      mockGetServerConfig.mockReturnValue({
        githubApiUrl,
        timeout,
      } as ReturnType<typeof getServerConfig>);
      const second = await getOctokit(auth('shared-token'));
      expect(second).not.toBe(first);
      expect(await getOctokit(auth('shared-token'))).toBe(second);
    }
  );

  it('separates branch and response cache identities for different API paths on one host', async () => {
    mocks.reposGet
      .mockResolvedValueOnce({ data: { default_branch: 'api-main' } })
      .mockResolvedValueOnce({ data: { default_branch: 'other-main' } });
    mockGetServerConfig.mockReturnValue({
      githubApiUrl: 'https://github.example/api/v3',
      timeout: 30_000,
    } as ReturnType<typeof getServerConfig>);
    const firstIdentity = await resolveCacheAuthFingerprint(
      auth('shared-token')
    );
    expect(await resolveDefaultBranch('o', 'r', auth('shared-token'))).toBe(
      'api-main'
    );
    mockGetServerConfig.mockReturnValue({
      githubApiUrl: 'https://github.example/other/api/v3',
      timeout: 30_000,
    } as ReturnType<typeof getServerConfig>);
    expect(await resolveCacheAuthFingerprint(auth('shared-token'))).not.toBe(
      firstIdentity
    );
    expect(await resolveDefaultBranch('o', 'r', auth('shared-token'))).toBe(
      'other-main'
    );
    expect(mocks.reposGet).toHaveBeenCalledTimes(2);
  });

  it('preserves the public cache key and normalizes equivalent endpoint URLs', async () => {
    const token = auth('shared-token');
    expect(await resolveCacheAuthFingerprint(token)).toBe(
      hashGitHubToken(token.token)
    );
    const firstClient = await getOctokit(token);
    mockGetServerConfig.mockReturnValue({
      githubApiUrl: 'https://API.GITHUB.COM:443/',
      timeout: 30_000,
    } as ReturnType<typeof getServerConfig>);
    expect(await resolveCacheAuthFingerprint(token)).toBe(
      hashGitHubToken(token.token)
    );
    expect(await getOctokit(token)).toBe(firstClient);
  });
});
