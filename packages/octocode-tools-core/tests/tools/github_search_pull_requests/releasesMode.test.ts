import { beforeEach, describe, expect, it, vi } from 'vitest';

const fetchReleases = vi.fn();
vi.mock('../../../src/github/releases.js', () => ({
  fetchReleases: (...args: unknown[]) => fetchReleases(...args),
}));

import { searchMultipleGitHubPullRequests } from '../../../src/tools/github_search_pull_requests/execution.js';
import { listMultipleGitHubReleases } from '../../../src/tools/github_search_pull_requests/splitExecutions.js';
import { ListReleasesBulkLocalSchema } from '../../../src/tools/github_search_pull_requests/splitSchemes.js';

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

describe('ghListReleases type:"releases"', () => {
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
        // The combined internal dispatcher uses the shared limit default.
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

  it('pageSize sets the release list page size', async () => {
    fetchReleases.mockResolvedValue(releasesData());
    await searchMultipleGitHubPullRequests({
      queries: [
        {
          type: 'releases',
          owner: 'microsoft',
          repo: 'TypeScript',
          pageSize: 5,
        },
      ],
    } as never);

    expect(fetchReleases).toHaveBeenCalledWith(
      expect.objectContaining({ perPage: 5 }),
      undefined
    );
  });

  it('emits a next.nextPage continuation when there is another page (regression: releases used to dead-end)', async () => {
    const data = releasesData();
    data.data.pagination = {
      currentPage: 1,
      perPage: 30,
      hasMore: true,
    } as never;
    (data.data.pagination as { nextPage?: number }).nextPage = 2;
    fetchReleases.mockResolvedValue(data);

    const result = await listMultipleGitHubReleases({
      queries: [
        {
          owner: 'microsoft',
          repo: 'TypeScript',
          includeAssets: true,
        },
      ],
    } as never);

    const row = (
      result.structuredContent as {
        results: Array<{ data: Record<string, any> }>;
      }
    ).results[0]!;
    const continuation = row.data.next.nextPage;
    expect(continuation).toMatchObject({
      tool: 'ghListReleases',
      query: {
        owner: 'microsoft',
        repo: 'TypeScript',
        page: 2,
        pageSize: 30,
        includeAssets: true,
      },
    });
    const replay = ListReleasesBulkLocalSchema.parse({
      queries: [continuation.query],
    });
    await expect(
      listMultipleGitHubReleases({ queries: replay.queries } as never)
    ).resolves.toBeDefined();
  });

  it('marks a next page beyond the public schema ceiling as terminal', async () => {
    const data = releasesData();
    data.data.pagination = {
      currentPage: 1000,
      perPage: 30,
      hasMore: true,
      nextPage: 1001,
    } as never;
    fetchReleases.mockResolvedValue(data);

    const result = await listMultipleGitHubReleases({
      queries: [{ owner: 'microsoft', repo: 'TypeScript', page: 1000 }],
    } as never);
    const row = (
      result.structuredContent as {
        results: Array<{
          data: Record<string, any>;
          meta: { diagnostics?: { codes?: string[] } };
        }>;
      }
    ).results[0]!;

    expect(row.data.terminalLimit).toBe(true);
    expect(row.data.pagination.continuationUnavailable).toMatchObject({
      reason: 'schemaPageLimit',
      maxPage: 1000,
    });
    expect(row.data.pagination.nextPage).toBeUndefined();
    expect(row.data.next?.nextPage).toBeUndefined();
    expect(row.meta.diagnostics?.codes).toContain('terminalLimitReached');
    expect(row.meta.diagnostics?.codes).not.toContain('continuationMissing');
  });

  it('marks provider cursor loss as terminal without an unusable continuation', async () => {
    const data = releasesData();
    data.data.pagination = {
      currentPage: 1,
      perPage: 30,
      hasMore: true,
    } as never;
    fetchReleases.mockResolvedValue(data);

    const result = await listMultipleGitHubReleases({
      queries: [{ owner: 'microsoft', repo: 'TypeScript' }],
    } as never);
    const row = (
      result.structuredContent as {
        results: Array<{
          data: Record<string, any>;
          meta: { diagnostics?: { codes?: string[] } };
        }>;
      }
    ).results[0]!;

    expect(row.data).toMatchObject({
      terminalLimit: true,
      pagination: {
        hasMore: true,
        continuationUnavailable: { reason: 'missingProviderCursor' },
      },
    });
    expect(row.data.next?.nextPage).toBeUndefined();
    expect(row.meta.diagnostics?.codes).toContain('terminalLimitReached');
    expect(row.meta.diagnostics?.codes).not.toContain('continuationMissing');
  });

  it('the local query schema accepts type:"releases"', async () => {
    const { GitHubPullRequestSearchQueryLocalSchema } =
      await import('../../../src/tools/github_search_pull_requests/scheme.js');
    const parsed = GitHubPullRequestSearchQueryLocalSchema.safeParse({
      type: 'releases',
      owner: 'o',
      repo: 'r',
    });
    expect(parsed.success).toBe(true);
  });
});
