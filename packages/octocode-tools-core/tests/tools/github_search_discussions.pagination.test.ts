import { beforeEach, describe, expect, it, vi } from 'vitest';

const fetchDiscussions = vi.fn();
vi.mock('../../src/github/discussions.js', () => ({
  fetchDiscussions: (...args: unknown[]) => fetchDiscussions(...args),
}));

import { searchMultipleGitHubDiscussions } from '../../src/tools/github_search_discussions/execution.js';
import { SearchDiscussionsBulkLocalSchema } from '../../src/tools/github_search_discussions/scheme.js';

function response(nextCursor?: string) {
  return {
    data: {
      type: 'discussions',
      owner: 'o',
      repo: 'r',
      totalCount: 3,
      discussions: [{ number: 1, title: 'Topic', url: 'https://example.test' }],
      pagination: {
        perPage: 1,
        hasMore: true,
        ...(nextCursor ? { nextCursor } : {}),
        ...(!nextCursor
          ? {
              continuationUnavailable: {
                reason: 'missingProviderCursor',
              },
            }
          : {}),
      },
    },
    status: 200,
  };
}

describe('ghSearchDiscussions pagination', () => {
  beforeEach(() => {
    fetchDiscussions.mockReset();
  });

  it('emits and replays the provider cursor as a public continuation', async () => {
    fetchDiscussions.mockResolvedValue(response('cursor-2'));
    const result = await searchMultipleGitHubDiscussions({
      queries: [
        {
          owner: 'o',
          repo: 'r',
          keywords: ['topic'],
          pageSize: 1,
          goal: 'find discussions',
          reasoning: 'inspect community context',
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
      tool: 'ghSearchDiscussions',
      query: {
        owner: 'o',
        repo: 'r',
        keywords: ['topic'],
        pageSize: 1,
        after: 'cursor-2',
      },
    });
    expect(continuation.query).not.toHaveProperty('goal');
    expect(continuation.query).not.toHaveProperty('reasoning');

    const replay = SearchDiscussionsBulkLocalSchema.parse({
      queries: [continuation.query],
    });
    await searchMultipleGitHubDiscussions({ queries: replay.queries } as never);
    expect(fetchDiscussions).toHaveBeenLastCalledWith(
      expect.objectContaining({ after: 'cursor-2' }),
      undefined,
      undefined
    );
  });

  it('keeps a typed partial marker when GitHub says more but omits a cursor', async () => {
    fetchDiscussions.mockResolvedValue(response());
    const result = await searchMultipleGitHubDiscussions({
      queries: [{ owner: 'o', repo: 'r', pageSize: 1 }],
    } as never);
    const row = (
      result.structuredContent as {
        results: Array<{ data: Record<string, any> }>;
      }
    ).results[0]!;
    expect(row.data.pagination).toMatchObject({
      hasMore: true,
      continuationUnavailable: { reason: 'missingProviderCursor' },
    });
    expect(row.data.terminalLimit).toBe(true);
    expect(row.data.next?.nextPage).toBeUndefined();
    expect(row.meta.diagnostics?.codes).toContain('terminalLimitReached');
    expect(row.meta.diagnostics?.codes).not.toContain('continuationMissing');
  });
});
