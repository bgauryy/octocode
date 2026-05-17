import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../../src/bitbucket/pullRequestSearch.js', () => ({
  searchBitbucketPRsAPI: vi.fn(),
  fetchBitbucketPRSupplementalData: vi.fn(),
}));

const mockLogRateLimit = vi.hoisted(() => vi.fn());
vi.mock('../../../src/session.js', () => ({
  logRateLimit: mockLogRateLimit,
}));

vi.mock('../../../src/providers/bitbucket/utils.js', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('../../../src/providers/bitbucket/utils.js')
    >();
  return {
    ...actual,
    handleBitbucketAPIResponse: actual.handleBitbucketAPIResponse,
    parseBitbucketProjectId: actual.parseBitbucketProjectId,
  };
});

import { searchPullRequests } from '../../../src/providers/bitbucket/bitbucketPullRequests.js';
import {
  searchBitbucketPRsAPI,
  fetchBitbucketPRSupplementalData,
} from '../../../src/bitbucket/pullRequestSearch.js';

const mockSearchAPI = vi.mocked(searchBitbucketPRsAPI);
const mockSupplementalData = vi.mocked(fetchBitbucketPRSupplementalData);

describe('searchPullRequests (provider delegate)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should return error when projectId is missing', async () => {
    const result = await searchPullRequests({});
    expect(result.error).toContain('Project ID');
    expect(result.status).toBe(400);
  });

  it('should return error for whitespace-only state', async () => {
    const result = await searchPullRequests({
      projectId: 'ws/repo',
      state: '   ',
    });
    expect(result.error).toContain('Invalid Bitbucket PR state');
    expect(result.status).toBe(400);
    expect(result.hints).toBeDefined();
  });

  it('should return error for invalid projectId format', async () => {
    const result = await searchPullRequests({
      projectId: 'no-slash',
      state: 'open',
    });
    expect(result.error).toContain('Invalid Bitbucket projectId');
    expect(result.status).toBe(400);
    expect(result.hints).toEqual([
      'Provide projectId as "workspace/repo_slug".',
    ]);
  });

  it('should return error when API returns no data', async () => {
    mockSearchAPI.mockResolvedValue({
      error: 'Not found',
      status: 404,
    } as ReturnType<typeof searchBitbucketPRsAPI> extends Promise<infer R>
      ? R
      : never);

    const result = await searchPullRequests({
      projectId: 'ws/repo',
    });
    expect(result.error).toBeDefined();
    expect(result.provider).toBe('bitbucket');
  });

  it('should return transformed PR result on success', async () => {
    mockSearchAPI.mockResolvedValue({
      data: {
        pullRequests: [
          {
            id: 1,
            title: 'Fix bug',
            description: 'A fix',
            state: 'OPEN',
            author: { display_name: 'john', username: 'john' },
            source: { branch: { name: 'fix' }, commit: { hash: 'abc' } },
            destination: { branch: { name: 'main' }, commit: { hash: 'def' } },
            created_on: '2024-01-01T00:00:00Z',
            updated_on: '2024-01-02T00:00:00Z',
            comment_count: 0,
            links: { html: { href: 'https://bb.org/ws/repo/pr/1' } },
          },
        ],
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: 1,
        },
      },
      status: 200,
    });

    const result = await searchPullRequests({
      projectId: 'ws/repo',
    });

    expect(result.data).toBeDefined();
    expect(result.data!.items).toHaveLength(1);
    expect(result.data!.items[0]!.title).toBe('Fix bug');
    expect(result.provider).toBe('bitbucket');
  });

  it('should fetch supplemental data when withComments is true', async () => {
    mockSearchAPI.mockResolvedValue({
      data: {
        pullRequests: [
          {
            id: 1,
            title: 'PR',
            state: 'OPEN',
            author: { display_name: 'user' },
            source: { branch: { name: 'feat' } },
            destination: { branch: { name: 'main' } },
            created_on: '2024-01-01',
            updated_on: '2024-01-01',
            comment_count: 2,
          },
        ],
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: 1,
        },
      },
      status: 200,
    });

    mockSupplementalData.mockResolvedValue({
      comments: [
        {
          id: 10,
          content: { raw: 'LGTM' },
          user: { display_name: 'reviewer', username: 'reviewer' },
          created_on: '2024-01-01',
          updated_on: '2024-01-01',
        },
      ],
    });

    const result = await searchPullRequests({
      projectId: 'ws/repo',
      withComments: true,
    });

    expect(mockSupplementalData).toHaveBeenCalledWith(
      expect.objectContaining({
        workspace: 'ws',
        repoSlug: 'repo',
        prNumber: 1,
        withComments: true,
      })
    );
    expect(result.data).toBeDefined();
  });

  it('should fetch supplemental data for file changes (type set)', async () => {
    mockSearchAPI.mockResolvedValue({
      data: {
        pullRequests: [
          {
            id: 2,
            title: 'Feature',
            state: 'MERGED',
            author: { display_name: 'user' },
            source: { branch: { name: 'feat' } },
            destination: { branch: { name: 'main' } },
            created_on: '2024-01-01',
            updated_on: '2024-01-02',
            comment_count: 0,
          },
        ],
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
        },
      },
      status: 200,
    });

    mockSupplementalData.mockResolvedValue({
      diffstat: [
        {
          type: 'diffstat',
          status: 'modified',
          new: { path: 'src/app.ts' },
          lines_added: 5,
          lines_removed: 2,
        },
      ],
      diff: '--- a/src/app.ts\n+++ b/src/app.ts',
    });

    const result = await searchPullRequests({
      projectId: 'ws/repo',
      type: 'fullContent',
    });

    expect(mockSupplementalData).toHaveBeenCalledWith(
      expect.objectContaining({
        withDiff: true,
        withDiffstat: true,
      })
    );
    expect(result.data).toBeDefined();
  });

  it('should handle supplemental data fetch failure gracefully', async () => {
    mockSearchAPI.mockResolvedValue({
      data: {
        pullRequests: [
          {
            id: 1,
            title: 'PR',
            state: 'OPEN',
            author: { display_name: 'user' },
            source: { branch: { name: 'feat' } },
            destination: { branch: { name: 'main' } },
            created_on: '2024-01-01',
            updated_on: '2024-01-01',
            comment_count: 0,
          },
        ],
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
        },
      },
      status: 200,
    });

    mockSupplementalData.mockRejectedValue(new Error('network error'));

    const result = await searchPullRequests({
      projectId: 'ws/repo',
      withComments: true,
    });

    expect(result.data).toBeDefined();
    expect(result.data!.items).toHaveLength(1);
  });

  it('should log rate limits from supplemental data fetch failures', async () => {
    mockSearchAPI.mockResolvedValue({
      data: {
        pullRequests: [
          {
            id: 1,
            title: 'PR',
            state: 'OPEN',
            author: { display_name: 'user' },
            source: { branch: { name: 'feat' } },
            destination: { branch: { name: 'main' } },
            created_on: '2024-01-01',
            updated_on: '2024-01-01',
            comment_count: 0,
          },
        ],
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
        },
      },
      status: 200,
    });

    const headers = new Headers();
    headers.set('retry-after', '42');
    mockSupplementalData.mockRejectedValue(
      Object.assign(new Error('Rate limited'), {
        status: 429,
        response: { headers },
      })
    );

    const result = await searchPullRequests({
      projectId: 'ws/repo',
      withComments: true,
    });

    expect(result.data).toBeDefined();
    expect(result.data!.items).toHaveLength(1);
    expect(mockLogRateLimit).toHaveBeenCalledWith(
      expect.objectContaining({
        provider: 'bitbucket',
        retry_after_seconds: 42,
      })
    );
  });

  it('should skip supplemental data for single PR fetch (prNumber set)', async () => {
    mockSearchAPI.mockResolvedValue({
      data: {
        pullRequests: [
          {
            id: 42,
            title: 'Single PR',
            state: 'OPEN',
            author: { display_name: 'user' },
            source: { branch: { name: 'feat' } },
            destination: { branch: { name: 'main' } },
            created_on: '2024-01-01',
            updated_on: '2024-01-01',
            comment_count: 0,
          },
        ],
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
        },
        comments: [
          {
            id: 1,
            content: { raw: 'ok' },
            user: { display_name: 'r' },
            created_on: '2024-01-01',
            updated_on: '2024-01-01',
          },
        ],
      },
      status: 200,
    });

    const result = await searchPullRequests({
      projectId: 'ws/repo',
      number: 42,
      withComments: true,
    });

    expect(mockSupplementalData).not.toHaveBeenCalled();
    expect(result.data).toBeDefined();
  });

  it('should handle "all" state correctly', async () => {
    mockSearchAPI.mockResolvedValue({
      data: {
        pullRequests: [],
        pagination: { currentPage: 1, totalPages: 1, hasMore: false },
      },
      status: 200,
    });

    const result = await searchPullRequests({
      projectId: 'ws/repo',
      state: 'all',
    });

    expect(result.data).toBeDefined();
    expect(mockSearchAPI).toHaveBeenCalledWith(
      expect.objectContaining({ state: undefined })
    );
  });

  it('should map "superseded" state to SUPERSEDED', async () => {
    mockSearchAPI.mockResolvedValue({
      data: {
        pullRequests: [],
        pagination: { currentPage: 1, totalPages: 1, hasMore: false },
      },
      status: 200,
    });

    await searchPullRequests({
      projectId: 'ws/repo',
      state: 'superseded',
    });

    expect(mockSearchAPI).toHaveBeenCalledWith(
      expect.objectContaining({ state: 'SUPERSEDED' })
    );
  });

  it('should pass all query parameters to the API', async () => {
    mockSearchAPI.mockResolvedValue({
      data: {
        pullRequests: [],
        pagination: { currentPage: 1, totalPages: 1, hasMore: false },
      },
      status: 200,
    });

    await searchPullRequests({
      projectId: 'ws/repo',
      state: 'open',
      author: 'alice',
      baseBranch: 'main',
      headBranch: 'feature',
      sort: 'updated',
      page: 2,
      limit: 5,
    });

    expect(mockSearchAPI).toHaveBeenCalledWith(
      expect.objectContaining({
        workspace: 'ws',
        repoSlug: 'repo',
        state: 'OPEN',
        author: 'alice',
        baseBranch: 'main',
        headBranch: 'feature',
        sort: 'updated',
        page: 2,
        limit: 5,
      })
    );
  });
});
