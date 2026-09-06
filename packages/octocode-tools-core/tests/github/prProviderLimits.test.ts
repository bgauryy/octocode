import { fetchCommit } from '../../src/github/commit.js';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  listCommits: vi.fn(),
  listFiles: vi.fn(),
  getCommit: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      pulls: { listCommits: mocks.listCommits, listFiles: mocks.listFiles },
      repos: { getCommit: mocks.getCommit },
    },
  }),
  resolveCacheAuthFingerprint: async () => 'pr-provider-limits',
  OctokitWithThrottling: class {},
}));
import { transformPullRequestItemFromREST } from '../../src/github/prContentFetcher/transform.js';
import { clearAllCache } from '../../src/utils/http/cache/management.js';
import { formatPRForResponse } from '../../src/github/prTransformation.js';
import { transformPullRequestResult } from '../../src/providers/github/githubPullRequests.js';
import { mapPullRequestProviderResultData } from '../../src/tools/providerMappers/pullRequests.js';
import { shapePullRequestForContent } from '../../src/tools/github_search_pull_requests/contentResponse.js';

async function execute(
  content: Record<string, unknown>,
  collectionPages: Record<string, number> = {}
) {
  const params = {
    owner: 'o',
    repo: 'r',
    prNumber: 91,
    content,
    collectionPages,
  };
  const transformed = await transformPullRequestItemFromREST(
    { number: 91, title: 'PR', html_url: '', user: { login: 'a' } } as never,
    params,
    {} as never
  );
  const formatted = formatPRForResponse(transformed);
  const provider = transformPullRequestResult(
    { pullRequests: [formatted] } as never,
    {}
  );
  const mapped = mapPullRequestProviderResultData(provider);
  return {
    data: mapped.resultData,
    row: shapePullRequestForContent(mapped.pullRequests[0]!, { prNumber: 91 }, {
      patches: { mode: 'none' },
      ...content,
    } as never),
  };
}

describe('PR provider caps survive to public content', () => {
  beforeEach(() => {
    clearAllCache();
    vi.clearAllMocks();
  });

  it('reports the 250-commit terminal limit', async () => {
    const fixture = Array.from({ length: 250 }, (_, i) => ({
      sha: `limit-${i}`,
      commit: { message: 'm', author: null },
    }));
    mocks.listCommits.mockImplementation(async ({ page, per_page }) => ({
      data: fixture.slice((page - 1) * per_page, page * per_page),
      headers:
        page < 5 ? { link: '<https://api.github.com/x>; rel="next"' } : {},
    }));
    const { data, row } = await execute({ commits: {} }, { commits: 5 });
    expect(data).toMatchObject({
      isPartial: true,
      terminalLimit: true,
      partialReasons: ['providerResultCap'],
    });
    expect(row.providerLimits).toEqual([
      { reason: 'providerResultCap', surface: 'commits', maxResults: 250 },
    ]);
    expect(mocks.getCommit).not.toHaveBeenCalled();
  });

  it('reports the 3000-file terminal limit while keeping reachable local pages', async () => {
    mocks.listFiles.mockImplementation(async ({ page }) => ({
      data: Array.from({ length: 100 }, (_, i) => ({
        filename: `f-${page}-${i}`,
        status: 'added',
        additions: 1,
        deletions: 0,
      })),
      headers:
        page < 30 ? { link: '<https://api.github.com/x>; rel="next"' } : {},
    }));
    const { data, row } = await execute(
      { changedFiles: true },
      { changedFiles: 30 }
    );
    expect(data).toMatchObject({ isPartial: true, terminalLimit: true });
    expect(row.providerLimits).toEqual([
      {
        reason: 'providerResultCap',
        surface: 'changedFiles',
        maxResults: 3000,
      },
    ]);
    expect(row.contentPagination).toMatchObject({
      changedFiles: {
        hasMore: true,
        totalItems: 100,
        nextQuery: { filePage: 2 },
      },
    });
    expect(mocks.listFiles).toHaveBeenCalledTimes(1);
  });

  it('defers the nested commit file cap until the terminal provider batch is reached', async () => {
    mocks.listCommits.mockResolvedValue({
      data: [{ sha: 'capped-commit', commit: { message: 'm', author: null } }],
      headers: {},
    });
    mocks.getCommit.mockImplementation(async ({ page }) => ({
      data: {
        sha: 'capped-commit',
        commit: { message: 'm', author: null },
        parents: [],
        files: Array.from({ length: 100 }, (_, i) => ({
          filename: `f-${page}-${i}`,
          status: 'added',
          additions: 1,
          deletions: 0,
          patch: '+hi',
        })),
      },
      headers:
        page < 30 ? { link: '<https://api.github.com/x>; rel="next"' } : {},
    }));
    const { data, row } = await execute({ commits: { includeFiles: true } });
    expect(data.terminalLimit).toBeUndefined();
    expect((row.commits as any[])[0].files).toHaveLength(20);
    expect((row.commits as any[])[0].next.nextFilePage).toBeDefined();
    expect(mocks.getCommit).toHaveBeenCalledTimes(1);
    const terminal = await fetchCommit({
      owner: 'o',
      repo: 'r',
      ref: 'capped-commit',
      fileBatch: 30,
      includeDiff: true,
    });
    expect(terminal.data).toMatchObject({
      isPartial: true,
      terminalLimit: true,
      providerLimit: { maxFiles: 3000 },
    });
  });
});
