import type { AuthInfo } from '@modelcontextprotocol/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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

vi.mock('octokit', () => {
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
  return { Octokit: MockOctokit };
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
