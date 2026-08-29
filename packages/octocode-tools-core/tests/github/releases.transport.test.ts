import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getOctokit: vi.fn(),
  resolveCacheAuthFingerprint: vi.fn(async () => 'auth-fingerprint'),
  generateCacheKey: vi.fn(() => 'releases-cache-key'),
  withDataCache: vi.fn(async (_key: string, producer: () => Promise<unknown>) =>
    producer()
  ),
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: mocks.getOctokit,
  resolveCacheAuthFingerprint: mocks.resolveCacheAuthFingerprint,
}));

vi.mock('../../src/utils/http/cache.js', () => ({
  generateCacheKey: mocks.generateCacheKey,
  withDataCache: mocks.withDataCache,
}));

const { fetchReleases } = await import('../../src/github/releases.js');

describe('fetchReleases transport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('maps releases, assets, latest metadata, pagination, and cache identity', async () => {
    const listReleases = vi.fn().mockResolvedValue({
      data: [
        {
          id: 7,
          tag_name: 'v2.0.0',
          name: 'Version 2',
          published_at: '2026-08-01T00:00:00Z',
          prerelease: false,
          draft: false,
          assets: [
            {
              name: 'tool.tgz',
              size: 42,
              download_count: 9,
              browser_download_url: 'https://example.test/tool.tgz',
            },
          ],
        },
      ],
      headers: {
        link: '<https://api.github.test/repos/o/r/releases?page=2>; rel="next"',
      },
    });
    const getLatestRelease = vi.fn().mockResolvedValue({
      data: {
        id: 7,
        tag_name: 'v2.0.0',
        published_at: '2026-08-01T00:00:00Z',
      },
    });
    mocks.getOctokit.mockResolvedValue({
      rest: { repos: { listReleases, getLatestRelease } },
    });

    const result = await fetchReleases(
      { owner: 'o', repo: 'r', page: 1, perPage: 10, includeAssets: true },
      undefined,
      'session-1'
    );

    expect(listReleases).toHaveBeenCalledWith({
      owner: 'o',
      repo: 'r',
      page: 1,
      per_page: 10,
    });
    expect(result.data).toMatchObject({
      latest: { tagName: 'v2.0.0' },
      releases: [
        {
          tagName: 'v2.0.0',
          name: 'Version 2',
          latest: true,
          assets: [{ name: 'tool.tgz', downloadCount: 9 }],
        },
      ],
      pagination: { hasMore: true, nextPage: 2 },
    });
    expect(mocks.generateCacheKey).toHaveBeenCalledWith(
      'gh-api-releases',
      expect.objectContaining({
        auth: 'auth-fingerprint',
        includeAssets: true,
      }),
      'session-1'
    );
  });

  it('treats a missing latest release as non-fatal and maps list failures', async () => {
    mocks.getOctokit.mockResolvedValueOnce({
      rest: {
        repos: {
          listReleases: vi.fn().mockResolvedValue({ data: [], headers: {} }),
          getLatestRelease: vi.fn().mockRejectedValue(new Error('not found')),
        },
      },
    });
    const noLatest = await fetchReleases({
      owner: 'o',
      repo: 'r',
      page: 1,
      perPage: 10,
    });
    expect(noLatest.data?.latest).toBeUndefined();
    expect(noLatest.status).toBe(200);

    mocks.getOctokit.mockRejectedValueOnce(
      Object.assign(new Error('forbidden'), { status: 403 })
    );
    const failure = await fetchReleases({
      owner: 'o',
      repo: 'r2',
      page: 1,
      perPage: 10,
    });
    expect(failure).toEqual(
      expect.objectContaining({ error: expect.any(String) })
    );
  });
});
