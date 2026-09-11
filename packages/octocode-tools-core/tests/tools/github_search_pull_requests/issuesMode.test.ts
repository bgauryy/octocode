import { beforeEach, describe, expect, it, vi } from 'vitest';

const fetchIssues = vi.fn();
vi.mock('../../../src/github/issues/orchestrator.js', () => ({
  fetchIssues: (...args: unknown[]) => fetchIssues(...args),
}));

import {
  getMultipleGitHubHistoryItems,
  searchMultipleGitHubHistory,
} from '../../../src/tools/github_search_pull_requests/historyExecutions.js';

function issuesData() {
  return {
    data: {
      type: 'issues',
      owner: 'microsoft',
      repo: 'TypeScript',
      issues: [
        {
          number: 42,
          title: 'Crash on startup',
          state: 'open',
          author: 'someone',
          labels: ['bug'],
          created_at: '2026-01-01T00:00:00Z',
          updated_at: '2026-02-01T00:00:00Z',
          url: 'https://github.com/microsoft/TypeScript/issues/42',
        },
      ],
      total_count: 1,
      pagination: { currentPage: 1, perPage: 30, hasMore: false },
    },
    status: 200,
  };
}

describe('ghSearchIssues type:"issues"', () => {
  beforeEach(() => {
    fetchIssues.mockReset();
  });

  it('routes to fetchIssues and returns issue rows', async () => {
    fetchIssues.mockResolvedValue(issuesData());
    const result = await searchMultipleGitHubHistory({
      queries: [
        {
          operation: 'issues',
          owner: 'microsoft',
          repo: 'TypeScript',
          keywords: ['crash'],
          state: 'open',
        },
      ],
    } as never);

    expect(fetchIssues).toHaveBeenCalledWith(
      expect.objectContaining({
        owner: 'microsoft',
        repo: 'TypeScript',
        keywordsToSearch: ['crash'],
        state: 'open',
        page: 1,
      }),
      undefined
    );
    const text = JSON.stringify(result.structuredContent ?? result);
    expect(text).toContain('Crash on startup');
    expect(text).toContain('"type":"issues"');
    expect(text).not.toContain('"readIssue"');
    expect(text).not.toContain('"operation":"issue"');
    expect(text).toContain('"number":42');
    expect(text).not.toContain('"issueNumber"');
    expect(text).not.toContain('"searchRepositoryCode"');
    expect(text).not.toContain('"searchCode"');
    expect(text).not.toContain('"tool":"ghSearch"');
    expect(text).not.toContain('"operation":"code"');
    expect(text).not.toContain('github.code');
  });

  it('keeps the PR-only page explanation on the public empty response', async () => {
    fetchIssues.mockResolvedValue({
      data: {
        type: 'issues',
        owner: 'microsoft',
        repo: 'TypeScript',
        issues: [],
        warnings: [
          'This page contained only pull requests; follow pagination.nextPage.',
        ],
        pagination: {
          currentPage: 1,
          perPage: 1,
          hasMore: true,
          nextPage: 2,
        },
      },
      status: 200,
    });

    const result = await searchMultipleGitHubHistory({
      queries: [
        {
          operation: 'issues',
          owner: 'microsoft',
          repo: 'TypeScript',
          pageSize: 1,
        },
      ],
    } as never);

    expect(result.structuredContent).toMatchObject({
      results: [
        {
          status: 'empty',
          data: {
            hints: [expect.stringContaining('only pull requests')],
            pagination: { hasMore: true, nextPage: 2 },
          },
        },
      ],
    });
  });

  it('omits optional next-tool suggestions for successful issue details', async () => {
    fetchIssues.mockResolvedValue({
      data: {
        type: 'issues',
        owner: 'microsoft',
        repo: 'TypeScript',
        issues: [
          {
            number: 42,
            title: 'Crash on startup',
            state: 'open',
            author: 'someone',
            labels: [],
            created_at: '2026-01-01T00:00:00Z',
            updated_at: '2026-02-01T00:00:00Z',
            url: 'https://github.com/microsoft/TypeScript/issues/42',
            body: 'repro steps',
          },
        ],
        total_count: 1,
      },
      status: 200,
    });

    const result = await getMultipleGitHubHistoryItems({
      queries: [
        {
          operation: 'issue',
          owner: 'microsoft',
          repo: 'TypeScript',
          number: 42,
          content: { body: true },
        },
      ],
    } as never);

    const text = JSON.stringify(result.structuredContent ?? result);
    expect(text).not.toContain('"readIssue"');
    expect(text).not.toContain('"searchRepositoryCode"');
    expect(text).not.toContain('"searchCode"');
  });

  it('passes issueNumber for detail mode', async () => {
    fetchIssues.mockResolvedValue({
      data: {
        type: 'issues',
        owner: 'microsoft',
        repo: 'TypeScript',
        issues: [
          {
            number: 42,
            title: 'Crash on startup',
            state: 'open',
            author: 'someone',
            labels: [],
            created_at: '2026-01-01T00:00:00Z',
            updated_at: '2026-02-01T00:00:00Z',
            url: 'https://github.com/microsoft/TypeScript/issues/42',
            body: 'repro steps',
          },
        ],
        total_count: 1,
      },
      status: 200,
    });

    await getMultipleGitHubHistoryItems({
      queries: [
        {
          operation: 'issue',
          owner: 'microsoft',
          repo: 'TypeScript',
          number: 42,
          content: { body: true, comments: { discussion: true } },
        },
      ],
    } as never);

    expect(fetchIssues).toHaveBeenCalledWith(
      expect.objectContaining({
        issueNumber: 42,
        content: expect.objectContaining({
          body: true,
          comments: expect.objectContaining({ discussion: true }),
        }),
      }),
      undefined
    );
  });

  it('requires owner and repo', async () => {
    const result = await searchMultipleGitHubHistory({
      queries: [{ operation: 'issues' }],
    } as never);
    expect(fetchIssues).not.toHaveBeenCalled();
    const text = JSON.stringify(result.structuredContent ?? result);
    expect(text).toContain('Invalid input');
  });

  it('the public item schema accepts operation:"issue" and number', async () => {
    const { GitHubGetHistoryItemQueryLocalSchema } =
      await import('@octocodeai/octocode-core/schema');
    const parsed = GitHubGetHistoryItemQueryLocalSchema.safeParse({
      operation: 'issue',
      owner: 'o',
      repo: 'r',
      number: 7,
    });
    expect(parsed.success).toBe(true);
  });
});
