import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getOctokit: vi.fn(),
  resolveCacheAuthFingerprint: vi.fn(async () => 'auth-fingerprint'),
  generateCacheKey: vi.fn(() => 'discussions-cache-key'),
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

const { fetchDiscussions } = await import('../../src/github/discussions.js');

describe('fetchDiscussions transport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('maps GraphQL rows, search terms, cursor pagination, and cache identity', async () => {
    const graphql = vi.fn().mockResolvedValue({
      search: {
        discussionCount: 2,
        pageInfo: { hasNextPage: true, endCursor: 'cursor-2' },
        nodes: [
          {},
          {
            number: 11,
            title: 'Graph design',
            url: 'https://github.test/o/r/discussions/11',
            author: { login: 'octo' },
            category: { name: 'Q&A' },
            createdAt: '2026-08-01T00:00:00Z',
            updatedAt: '2026-08-02T00:00:00Z',
            answerChosenAt: '2026-08-03T00:00:00Z',
            upvoteCount: 4,
            comments: { totalCount: 3 },
          },
        ],
      },
      repository: { hasDiscussionsEnabled: true },
    });
    mocks.getOctokit.mockResolvedValue({ graphql });

    const result = await fetchDiscussions(
      {
        owner: 'o',
        repo: 'r',
        keywords: [' graph ', '', 'design'],
        perPage: 10,
        after: 'cursor-1',
      },
      undefined,
      'session-1'
    );

    expect(graphql).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({
        q: 'repo:o/r graph design',
        first: 10,
        after: 'cursor-1',
      })
    );
    expect(result.data).toMatchObject({
      totalCount: 2,
      hasDiscussionsEnabled: true,
      discussions: [
        {
          number: 11,
          author: 'octo',
          category: 'Q&A',
          answered: true,
          upvotes: 4,
          comments: 3,
        },
      ],
      pagination: { hasMore: true, nextCursor: 'cursor-2' },
    });
    expect(mocks.generateCacheKey).toHaveBeenCalledWith(
      'gh-api-discussions',
      expect.objectContaining({
        keywords: ' graph   design',
        auth: 'auth-fingerprint',
      }),
      'session-1'
    );
  });

  it('maps GraphQL transport failures through the shared GitHub error shape', async () => {
    mocks.getOctokit.mockResolvedValue({
      graphql: vi
        .fn()
        .mockRejectedValue(
          Object.assign(new Error('forbidden'), { status: 403 })
        ),
    });

    const result = await fetchDiscussions({
      owner: 'o',
      repo: 'r',
      perPage: 10,
    });

    expect(result).toEqual(
      expect.objectContaining({ error: expect.any(String) })
    );
  });
});
