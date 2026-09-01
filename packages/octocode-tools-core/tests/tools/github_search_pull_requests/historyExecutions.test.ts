import { beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';

const mocks = vi.hoisted(() => ({
  pullRequests: vi.fn(),
  issues: vi.fn(),
  commits: vi.fn(),
  fetchCommit: vi.fn(),
}));

vi.mock(
  '../../../src/tools/github_search_pull_requests/execution/pullRequestsMode.js',
  () => ({ handlePullRequestsMode: mocks.pullRequests })
);
vi.mock(
  '../../../src/tools/github_search_pull_requests/execution/issuesMode.js',
  () => ({ handleIssuesMode: mocks.issues })
);
vi.mock(
  '../../../src/tools/github_search_pull_requests/execution/commitsMode.js',
  () => ({ handleCommitsMode: mocks.commits })
);
vi.mock('../../../src/github/commit.js', () => ({
  fetchCommit: mocks.fetchCommit,
}));

import {
  getMultipleGitHubHistoryItems,
  searchMultipleGitHubHistory,
} from '../../../src/tools/github_search_pull_requests/historyExecutions.js';
import {
  GitHubGetHistoryItemQueryLocalSchema,
  GitHubSearchHistoryQueryLocalSchema,
} from '../../../src/tools/github_search_pull_requests/historySchemes.js';

function args(queries: Array<Record<string, unknown>>) {
  return { queries } as never;
}

function resultRows(
  result: Awaited<ReturnType<typeof searchMultipleGitHubHistory>>
) {
  return (
    result.structuredContent as {
      results: Array<{
        index: number;
        data: Record<string, any>;
        meta: { diagnostics?: { codes?: string[] } };
      }>;
    }
  ).results;
}

describe('GitHub history public adapters', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.pullRequests.mockResolvedValue({ kind: 'pullRequests' });
    mocks.issues.mockResolvedValue({ kind: 'issues' });
    mocks.commits.mockImplementation(async query => ({
      kind: (query as { base?: string }).base ? 'compare' : 'commits',
    }));
    mocks.fetchCommit.mockResolvedValue({
      status: 200,
      data: {
        type: 'commit',
        owner: 'o',
        repo: 'r',
        ref: 'abc',
        sha: 'abc',
        message: 'message',
        messageHeadline: 'message',
        author: { name: 'A', email: '' },
        parents: [],
        changedFiles: 0,
      },
    });
  });

  it('preserves mixed search batch order and zero-based indexes', async () => {
    const result = await searchMultipleGitHubHistory(
      args([
        { operation: 'commits', owner: 'o', repo: 'r' },
        { operation: 'pullRequests', keywords: ['schema'] },
        { operation: 'issues', owner: 'o', repo: 'r' },
      ])
    );

    expect(resultRows(result).map(row => row.index)).toEqual([0, 1, 2]);
    expect(resultRows(result).map(row => row.data.kind)).toEqual([
      'commits',
      'pullRequests',
      'issues',
    ]);
  });

  it('preserves mixed exact-item order across all specialized executors', async () => {
    const result = await getMultipleGitHubHistoryItems(
      args([
        { operation: 'issue', owner: 'o', repo: 'r', number: 1 },
        { operation: 'commit', owner: 'o', repo: 'r', ref: 'abc' },
        { operation: 'pullRequest', owner: 'o', repo: 'r', number: 2 },
        {
          operation: 'compare',
          owner: 'o',
          repo: 'r',
          base: 'main',
          head: 'next',
        },
      ])
    );

    expect(resultRows(result).map(row => row.index)).toEqual([0, 1, 2, 3]);
    expect(
      resultRows(result).map(row => row.data.kind ?? row.data.type)
    ).toEqual(['issues', 'commit', 'pullRequests', 'compare']);
    expect(mocks.fetchCommit).toHaveBeenCalledWith(
      expect.objectContaining({ owner: 'o', repo: 'r', ref: 'abc' }),
      undefined
    );
  });

  it('emits sanitized executable next-page calls for every search branch', async () => {
    mocks.pullRequests.mockResolvedValue({
      status: 'success',
      kind: 'pullRequests',
      pagination: { hasMore: true, nextPage: 2 },
    });
    mocks.issues.mockResolvedValue({
      status: 'empty',
      kind: 'issues',
      issues: [],
      pagination: { hasMore: true, nextPage: 3 },
    });
    mocks.commits.mockResolvedValue({
      status: 'success',
      kind: 'commits',
      pagination: { hasMore: true, nextPage: 4 },
    });

    const result = await searchMultipleGitHubHistory(
      args([
        {
          operation: 'pullRequests',
          keywords: ['schema'],
          state: 'open',
          pageSize: 5,
        },
        {
          operation: 'issues',
          owner: 'o',
          repo: 'r',
          keywords: ['bug'],
          label: ['regression'],
          pageSize: 6,
        },
        {
          operation: 'commits',
          owner: 'o',
          repo: 'r',
          branch: 'main',
          path: 'src/',
          pageSize: 7,
        },
      ])
    );
    const rows = resultRows(result);
    const continuations = rows.map(
      row =>
        (
          row.data.next as {
            nextPage: { tool: string; query: Record<string, unknown> };
          }
        ).nextPage
    );

    expect(continuations.map(next => next.tool)).toEqual([
      'ghSearchHistory',
      'ghSearchHistory',
      'ghSearchHistory',
    ]);
    expect(continuations.map(next => next.query)).toEqual([
      {
        operation: 'pullRequests',
        keywords: ['schema'],
        state: 'open',
        pageSize: 5,
        page: 2,
      },
      {
        operation: 'issues',
        owner: 'o',
        repo: 'r',
        keywords: ['bug'],
        label: ['regression'],
        pageSize: 6,
        page: 3,
      },
      {
        operation: 'commits',
        owner: 'o',
        repo: 'r',
        branch: 'main',
        path: 'src/',
        pageSize: 7,
        page: 4,
      },
    ]);
    for (const continuation of continuations) {
      expect(
        GitHubSearchHistoryQueryLocalSchema.safeParse(continuation.query)
          .success
      ).toBe(true);
    }
    expect(JSON.stringify(result)).not.toMatch(
      /ghSearchPullRequests|ghSearchIssues|ghSearchCommits|prNumber|issueNumber/
    );
    expect(JSON.stringify(continuations)).not.toMatch(
      /"(?:goal|reasoning|type|limit|itemsPerPage)"/
    );
  });

  it.each(['pullRequests', 'issues', 'commits'] as const)(
    'marks the %s schema page ceiling as terminal without an invalid continuation',
    async operation => {
      mocks[operation].mockResolvedValue({
        kind: operation,
        pagination: { hasMore: true, nextPage: 1001 },
      });
      const result = await searchMultipleGitHubHistory(
        args([
          {
            operation,
            owner: 'o',
            repo: 'r',
            page: 1000,
            ...(operation === 'commits' ? {} : { keywords: ['schema'] }),
          },
        ])
      );
      const row = resultRows(result)[0]!;
      expect(row.data).toMatchObject({
        terminalLimit: true,
        pagination: {
          hasMore: true,
          continuationUnavailable: {
            reason: 'schemaPageLimit',
            maxPage: 1000,
          },
        },
      });
      expect(row.data.pagination.nextPage).toBeUndefined();
      expect(row.data.next?.nextPage).toBeUndefined();
      expect(row.meta.diagnostics?.codes).toContain('terminalLimitReached');
      expect(row.meta.diagnostics?.codes).not.toContain('continuationMissing');
    }
  );

  it.each(['pullRequests', 'issues', 'commits'] as const)(
    'marks the %s GitHub result-window cap and suppresses a doomed continuation',
    async operation => {
      mocks[operation].mockResolvedValue({
        kind: operation,
        pagination: {
          currentPage: 500,
          perPage: 2,
          totalMatchesCapped: true,
          hasMore: true,
          nextPage: 501,
        },
      });
      const result = await searchMultipleGitHubHistory(
        args([
          {
            operation,
            owner: 'o',
            repo: 'r',
            page: 500,
            pageSize: 2,
            ...(operation === 'commits' ? {} : { keywords: ['schema'] }),
          },
        ])
      );
      const row = resultRows(result)[0]!;
      expect(row.data).toMatchObject({
        isPartial: true,
        terminalLimit: true,
        partialReasons: ['providerResultCap'],
        pagination: {
          continuationUnavailable: {
            reason: 'providerResultCap',
            maxResults: 1000,
          },
        },
      });
      expect(row.data.pagination.nextPage).toBeUndefined();
      expect(row.data.next?.nextPage).toBeUndefined();
      expect(row.meta.diagnostics?.codes).toContain('terminalLimitReached');
    }
  );

  it.each(['pullRequests', 'issues', 'commits'] as const)(
    'marks the %s provider cursor loss as terminal without an unusable continuation',
    async operation => {
      mocks[operation].mockResolvedValue({
        kind: operation,
        pagination: { hasMore: true },
      });
      const result = await searchMultipleGitHubHistory(
        args([
          {
            operation,
            owner: 'o',
            repo: 'r',
            ...(operation === 'commits' ? {} : { keywords: ['schema'] }),
          },
        ])
      );
      const row = resultRows(result)[0]!;

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
    }
  );

  it.each(['pullRequests', 'issues'] as const)(
    'marks provider-incomplete %s results and emits a same-page retry',
    async operation => {
      mocks[operation].mockResolvedValue({
        kind: operation,
        incompleteResults: true,
        pagination: { hasMore: false },
      });
      const result = await searchMultipleGitHubHistory(
        args([
          {
            operation,
            owner: 'o',
            repo: 'r',
            keywords: ['schema'],
            page: 4,
          },
        ])
      );
      const row = resultRows(result)[0]!;
      expect(row.data).toMatchObject({
        isPartial: true,
        partialReasons: ['providerIncompleteResults'],
      });
      expect(row.data.next.retry).toMatchObject({
        tool: 'ghSearchHistory',
        query: { operation, page: 4 },
      });
      expect(row.meta.diagnostics?.partial).toBe(true);
      expect(row.meta.diagnostics?.codes ?? []).not.toContain(
        'continuationMissing'
      );
    }
  );

  it('sanitizes exact-commit diff continuations through the public get schema', async () => {
    mocks.fetchCommit.mockResolvedValue({
      status: 200,
      data: {
        type: 'commit',
        owner: 'o',
        repo: 'r',
        ref: 'abc',
        sha: 'abc',
        message: 'message',
        messageHeadline: 'message',
        author: { name: 'A', email: '' },
        parents: [],
        changedFiles: 2,
        filesPagination: { hasMore: true, nextFilePage: 2 },
        files: [
          {
            path: 'src/a.ts',
            patchPagination: { nextCharOffset: 10 },
          },
        ],
      },
    });

    const result = await getMultipleGitHubHistoryItems(
      args([
        {
          operation: 'commit',
          owner: 'o',
          repo: 'r',
          ref: 'abc',
          includeDiff: true,
          path: 'src/',
          filePage: 1,
          pageSize: 1,
          charLength: 10,
          goal: 'internal transport metadata',
          reasoning: 'must not leak',
        },
      ])
    );
    const row = resultRows(result)[0]!;
    const next = row.data.next as Record<
      string,
      { tool: string; query: Record<string, unknown> }
    >;

    expect(Object.keys(next).sort()).toEqual(['continuePatch', 'nextFilePage']);
    for (const continuation of Object.values(next)) {
      expect(continuation.tool).toBe('ghGetHistoryItem');
      expect(
        GitHubGetHistoryItemQueryLocalSchema.safeParse(continuation.query)
          .success
      ).toBe(true);
    }
    expect(next.nextFilePage?.query).toEqual({
      operation: 'commit',
      owner: 'o',
      repo: 'r',
      ref: 'abc',
      includeDiff: true,
      path: 'src/',
      filePage: 2,
      pageSize: 1,
      charLength: 10,
    });
    expect(next.continuePatch?.query).toEqual({
      operation: 'commit',
      owner: 'o',
      repo: 'r',
      ref: 'abc',
      includeDiff: true,
      path: 'src/',
      filePage: 1,
      pageSize: 1,
      charOffset: 10,
      charLength: 10,
    });
    expect(JSON.stringify(result)).not.toMatch(
      /ghSearchPullRequests|ghSearchIssues|ghSearchCommits|prNumber|issueNumber/
    );
    expect(JSON.stringify(next)).not.toMatch(
      /"(?:goal|reasoning|type|limit|itemsPerPage)"/
    );
  });

  it.each([
    {
      operation: 'pullRequest' as const,
      executor: 'pullRequests' as const,
      resultKey: 'pullRequests',
      pagination: {
        body: {
          hasMore: true,
          nextQuery: {
            operation: 'pullRequest',
            owner: 'o',
            repo: 'r',
            number: 7,
            content: { body: true },
            charOffset: 10,
          },
        },
        changedFiles: {
          hasMore: true,
          nextQuery: {
            operation: 'pullRequest',
            owner: 'o',
            repo: 'r',
            number: 7,
            content: { changedFiles: true },
            filePage: 2,
          },
        },
        comments: {
          hasMore: true,
          nextQuery: {
            operation: 'pullRequest',
            owner: 'o',
            repo: 'r',
            number: 7,
            content: { comments: { discussion: true } },
            commentPage: 2,
          },
        },
        commentBody: {
          hasMore: true,
          nextQuery: {
            operation: 'pullRequest',
            owner: 'o',
            repo: 'r',
            number: 7,
            content: { comments: { discussion: true } },
            commentBodyOffset: 10,
          },
        },
        commits: {
          hasMore: true,
          nextQuery: {
            operation: 'pullRequest',
            owner: 'o',
            repo: 'r',
            number: 7,
            content: { commits: {} },
            commitPage: 2,
          },
        },
        patches: {
          hasMore: true,
          nextQuery: {
            operation: 'pullRequest',
            owner: 'o',
            repo: 'r',
            number: 7,
            content: { patches: { mode: 'all' } },
            charOffset: 10,
          },
        },
        filePaths: {
          hasMore: true,
          nextQuery: {
            operation: 'pullRequest',
            owner: 'o',
            repo: 'r',
            number: 7,
            content: { changedFiles: true },
            filePage: 2,
          },
        },
      },
      expectedNames: [
        'continueBody',
        'continueCommentBody',
        'continuePatch',
        'nextChangedFilesPage',
        'nextCommentsPage',
        'nextCommitsPage',
        'nextFilePathsPage',
      ],
    },
    {
      operation: 'issue' as const,
      executor: 'issues' as const,
      resultKey: 'issues',
      pagination: {
        body: {
          hasMore: true,
          nextQuery: {
            operation: 'issue',
            owner: 'o',
            repo: 'r',
            number: 7,
            content: { body: true },
            charOffset: 10,
          },
        },
        comments: {
          hasMore: true,
          nextQuery: {
            operation: 'issue',
            owner: 'o',
            repo: 'r',
            number: 7,
            content: { comments: { discussion: true } },
            commentPage: 2,
          },
        },
      },
      expectedNames: ['continueBody', 'nextCommentsPage'],
    },
  ])(
    'promotes every partial $operation content axis to a named executable continuation',
    async ({ operation, executor, resultKey, pagination, expectedNames }) => {
      mocks[executor].mockResolvedValue({
        [resultKey]: [{ number: 7, contentPagination: pagination }],
      });

      const result = await getMultipleGitHubHistoryItems(
        args([{ operation, owner: 'o', repo: 'r', number: 7 }])
      );
      const row = resultRows(result)[0]!;
      const next = row.data.next as Record<
        string,
        { tool: string; query: Record<string, unknown> }
      >;

      expect(Object.keys(next).sort()).toEqual(expectedNames);
      for (const continuation of Object.values(next)) {
        expect(continuation.tool).toBe('ghGetHistoryItem');
        expect(
          GitHubGetHistoryItemQueryLocalSchema.safeParse(continuation.query)
            .success
        ).toBe(true);
      }
      expect(row.meta.diagnostics?.partial).toBe(true);
      expect(row.meta.diagnostics?.codes ?? []).not.toContain(
        'continuationMissing'
      );
      expect(JSON.stringify(row.data)).not.toContain('nextQuery');
    }
  );

  it('marks an exact-item content axis terminal instead of emitting a schema-invalid continuation at a cap', async () => {
    mocks.issues.mockResolvedValue({
      issues: [
        {
          number: 7,
          contentPagination: {
            comments: {
              hasMore: true,
              nextQuery: {
                operation: 'issue',
                owner: 'o',
                repo: 'r',
                number: 7,
                content: { comments: { discussion: true } },
                commentPage: 1001,
              },
            },
          },
        },
      ],
    });

    const result = await getMultipleGitHubHistoryItems(
      args([{ operation: 'issue', owner: 'o', repo: 'r', number: 7 }])
    );
    const row = resultRows(result)[0]!;

    expect(row.data).toMatchObject({
      terminalLimit: true,
      partialReasons: ['contentPagination'],
      issues: [
        {
          contentPagination: {
            comments: {
              hasMore: true,
              continuationUnavailable: { reason: 'schemaLimit' },
            },
          },
        },
      ],
    });
    expect(row.data.next?.nextCommentsPage).toBeUndefined();
    expect(row.meta.diagnostics?.partial).toBe(true);
    expect(row.meta.diagnostics?.codes).toContain('terminalLimitReached');
    expect(row.meta.diagnostics?.codes).not.toContain('continuationMissing');
  });

  it('promotes a truncated commit-list message to an exact readCommit continuation', async () => {
    mocks.commits.mockResolvedValue({
      commits: [
        {
          sha: 'deadbeef',
          messageHeadline: 'Long explanation',
          message: `Long explanation\n${'x'.repeat(500)}…`,
          messageTruncated: true,
        },
      ],
      pagination: { hasMore: false },
      next: {
        prDetail: {
          tool: 'ghGetHistoryItem',
          query: {
            operation: 'pullRequest',
            owner: 'o',
            repo: 'r',
            number: 12,
          },
        },
      },
    });

    const result = await searchMultipleGitHubHistory(
      args([{ operation: 'commits', owner: 'o', repo: 'r' }])
    );
    const row = resultRows(result)[0]!;

    expect(row.data.next).toMatchObject({
      prDetail: expect.any(Object),
      readCommit: {
        tool: 'ghGetHistoryItem',
        query: {
          operation: 'commit',
          owner: 'o',
          repo: 'r',
          ref: 'deadbeef',
        },
      },
    });
    expect(
      GitHubGetHistoryItemQueryLocalSchema.safeParse(
        (row.data.next as Record<string, any>).readCommit.query
      ).success
    ).toBe(true);
    expect(row.data).toMatchObject({
      isPartial: true,
      partialReasons: ['commitMessageTruncated'],
    });
    expect(row.meta.diagnostics?.partial).toBe(true);
  });

  it('uses only the public number vocabulary in the PR list drill-down hint', () => {
    const source = readFileSync(
      new URL(
        '../../../src/tools/github_search_pull_requests/execution/pullRequestsMode.ts',
        import.meta.url
      ),
      'utf8'
    );
    expect(source).toContain('swap number');
    expect(source).not.toContain('swap prNumber');
  });
});
