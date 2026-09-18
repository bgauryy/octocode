import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../../src/serverConfig.js', () => ({
  getGitHubToken: vi.fn(async () => undefined),
  getServerConfig: vi.fn(() => ({
    githubApiUrl: 'https://api.github.com',
    timeout: 30_000,
  })),
}));

vi.mock('../../src/session.js', () => ({ recordRateLimit: vi.fn() }));

import { clearOctokitInstances, getOctokit } from '../../src/github/client.js';

describe('Octokit retry and throttle policy together', () => {
  const fetchMock = vi.fn<typeof fetch>();

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-13T00:00:00Z'));
    clearOctokitInstances();
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    clearOctokitInstances();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it.each([403, 429])(
    'stops secondary HTTP %i when Retry-After exceeds the synchronous budget',
    async status => {
      fetchMock.mockImplementation(async () =>
        Response.json(
          { message: 'You have exceeded a secondary rate limit.' },
          { status, headers: { 'retry-after': '90' } }
        )
      );
      const client = await getOctokit();
      const result = client.rest.repos
        .get({ owner: 'o', repo: 'r' })
        .catch(error => error);
      await vi.advanceTimersByTimeAsync(30_000);

      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(await result).toMatchObject({
        status,
        response: { headers: { 'retry-after': '90' } },
      });
    }
  );

  it.each([403, 429])(
    'stops primary HTTP %i when reset exceeds the synchronous budget',
    async status => {
      const reset = String(Math.floor(Date.now() / 1000) + 90);
      fetchMock.mockImplementation(async () =>
        Response.json(
          { message: 'API rate limit exceeded' },
          {
            status,
            headers: {
              'x-ratelimit-remaining': '0',
              'x-ratelimit-reset': reset,
            },
          }
        )
      );
      const client = await getOctokit();
      const result = client.rest.repos
        .get({ owner: 'o', repo: 'r' })
        .catch(error => error);
      await vi.advanceTimersByTimeAsync(30_000);

      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(await result).toMatchObject({ status });
    }
  );

  it.each([
    ['secondary', 429, { 'retry-after': '1' }],
    [
      'primary',
      403,
      { 'x-ratelimit-remaining': '0', 'x-ratelimit-reset': '1789257601' },
    ],
    ['transient', 503, {}],
  ] as const)(
    'retains delayed recovery for a short %s failure',
    async (kind, status, headers) => {
      const attempts: number[] = [];
      fetchMock.mockImplementation(async () => {
        attempts.push(Date.now());
        return attempts.length === 1
          ? Response.json(
              {
                message:
                  kind === 'secondary'
                    ? 'You have exceeded a secondary rate limit.'
                    : 'Request failed',
              },
              { status, headers }
            )
          : Response.json({ default_branch: 'main' });
      });
      const client = await getOctokit();
      const result = client.rest.repos.get({ owner: 'o', repo: 'r' });
      await vi.advanceTimersByTimeAsync(10_000);

      expect((await result).data.default_branch).toBe('main');
      expect(attempts).toHaveLength(2);
      expect(attempts[1]! - attempts[0]!).toBeGreaterThanOrEqual(1000);
    }
  );
});
