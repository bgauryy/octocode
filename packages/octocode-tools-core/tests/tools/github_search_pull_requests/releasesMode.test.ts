import { beforeEach, describe, expect, it, vi } from 'vitest';

const fetchReleases = vi.fn();
vi.mock('../../../src/github/releases.js', () => ({
  fetchReleases: (...args: unknown[]) => fetchReleases(...args),
}));

import { searchMultipleGitHubPullRequests } from '../../../src/tools/github_search_pull_requests/execution.js';

function releasesData() {
  return {
    data: {
      type: 'releases',
      owner: 'microsoft',
      repo: 'TypeScript',
      releases: [
        {
          tagName: 'v6.0.3',
          publishedAt: '2026-04-16T23:43:08Z',
          latest: true,
          url: 'https://github.com/microsoft/TypeScript/releases/tag/v6.0.3',
        },
        {
          tagName: 'v6.0-rc',
          publishedAt: '2026-03-03T00:00:00Z',
          prerelease: true,
          url: 'https://github.com/microsoft/TypeScript/releases/tag/v6.0-rc',
        },
      ],
      latest: { tagName: 'v6.0.3', publishedAt: '2026-04-16T23:43:08Z' },
      pagination: { currentPage: 1, perPage: 30, hasMore: false },
    },
    status: 200,
  };
}

describe('ghHistoryResearch type:"releases"', () => {
  beforeEach(() => {
    fetchReleases.mockReset();
  });

  it('routes to fetchReleases and returns release rows with the latest marker', async () => {
    fetchReleases.mockResolvedValue(releasesData());
    const result = await searchMultipleGitHubPullRequests({
      queries: [{ type: 'releases', owner: 'microsoft', repo: 'TypeScript' }],
    } as never);

    expect(fetchReleases).toHaveBeenCalledWith(
      expect.objectContaining({
        owner: 'microsoft',
        repo: 'TypeScript',
        page: 1,
        perPage: 30,
      }),
      undefined
    );
    const text = JSON.stringify(result.structuredContent ?? result);
    expect(text).toContain('v6.0.3');
    expect(text).toContain('2026-04-16');
    expect(text).toContain('latest');
    expect(text).toContain('prerelease');
  });

  it('requires owner and repo', async () => {
    const result = await searchMultipleGitHubPullRequests({
      queries: [{ type: 'releases' }],
    } as never);
    expect(fetchReleases).not.toHaveBeenCalled();
    const text = JSON.stringify(result.structuredContent ?? result);
    expect(text).toContain('owner and repo are required for releases mode');
  });

  it('honors `limit` as a perPage alias when perPage was left at its default (regression)', async () => {
    fetchReleases.mockResolvedValue(releasesData());
    await searchMultipleGitHubPullRequests({
      queries: [
        { type: 'releases', owner: 'microsoft', repo: 'TypeScript', limit: 5 },
      ],
    } as never);

    expect(fetchReleases).toHaveBeenCalledWith(
      expect.objectContaining({ perPage: 5 }),
      undefined
    );
  });

  it('an explicit perPage always wins over limit', async () => {
    fetchReleases.mockResolvedValue(releasesData());
    await searchMultipleGitHubPullRequests({
      queries: [
        {
          type: 'releases',
          owner: 'microsoft',
          repo: 'TypeScript',
          limit: 5,
          perPage: 50,
        },
      ],
    } as never);

    expect(fetchReleases).toHaveBeenCalledWith(
      expect.objectContaining({ perPage: 50 }),
      undefined
    );
  });

  it('emits a next.nextPage continuation when there is another page (regression: releases used to dead-end)', async () => {
    const data = releasesData();
    data.data.pagination = { currentPage: 1, perPage: 30, hasMore: true } as never;
    (data.data.pagination as { nextPage?: number }).nextPage = 2;
    fetchReleases.mockResolvedValue(data);

    const result = await searchMultipleGitHubPullRequests({
      queries: [{ type: 'releases', owner: 'microsoft', repo: 'TypeScript' }],
    } as never);

    const text = JSON.stringify(result.structuredContent ?? result);
    expect(text).toContain('nextPage');
    expect(text).toContain('"page":2');
  });

  it('the local query schema accepts type:"releases"', async () => {
    const { GitHubPullRequestSearchQueryLocalSchema } = await import(
      '../../../src/tools/github_search_pull_requests/scheme.js'
    );
    const parsed = GitHubPullRequestSearchQueryLocalSchema.safeParse({
      type: 'releases',
      owner: 'o',
      repo: 'r',
    });
    expect(parsed.success).toBe(true);
  });
});
