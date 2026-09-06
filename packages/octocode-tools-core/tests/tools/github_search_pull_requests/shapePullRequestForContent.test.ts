import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

import { shapePullRequestForContent } from '../../../src/tools/github_search_pull_requests/contentResponse.js';
import { normalizePullRequestContentRequest } from '../../../src/tools/github_search_pull_requests/contentRequest.js';
import { GitHubGetHistoryItemQueryLocalSchema } from '../../../src/tools/github_search_pull_requests/historySchemes.js';

const PR = {
  number: 42,
  title: 'Fix the thing',
  state: 'open',
  author: 'someone',
  targetBranch: 'main',
  createdAt: '2026-01-01T00:00:00Z',
  mergedAt: null,
};

describe('pull-request exact-item content shaping', () => {
  it('emits a per-row next drill-down even for a lean list-mode row (regression: list mode used to dead-end)', () => {
    // Mirrors the lean shape pullRequestsMode builds for a plain list query
    // with no explicit content selection.
    const leanRequest = normalizePullRequestContentRequest({} as never);
    const shaped = shapePullRequestForContent(
      PR,
      { owner: 'octo', repo: 'engine' },
      leanRequest,
      false,
      true // pullRequestsMode now always passes showContentMap:true
    );

    expect(shaped.next).toBeDefined();
    const next = shaped.next as Record<
      string,
      { tool: string; query: { operation?: string; number?: number } }
    >;
    expect(next.getBody).toMatchObject({
      tool: 'ghGetHistoryItem',
      query: { number: 42, operation: 'pullRequest' },
    });
    expect(next.target).toBeUndefined();
  });

  it('omits next when showContentMap is explicitly false', () => {
    const leanRequest = normalizePullRequestContentRequest({} as never);
    const shaped = shapePullRequestForContent(
      PR,
      { owner: 'octo', repo: 'engine' },
      leanRequest,
      false,
      false
    );
    expect(shaped.next).toBeUndefined();
  });

  it('builds only canonical contentPagination without transient top-level aliases', () => {
    const request = normalizePullRequestContentRequest({
      content: {
        body: true,
        changedFiles: true,
        comments: { discussion: true },
        commits: { includeFiles: true },
      },
    } as never);
    const shaped = shapePullRequestForContent(
      {
        ...PR,
        body: 'body text',
        fileChanges: [
          { path: 'a.ts', status: 'modified' },
          { path: 'b.ts', status: 'added' },
          { path: 'c.ts', status: 'modified' },
        ],
        comments: [
          { id: 1, commentType: 'discussion', body: 'first' },
          { id: 2, commentType: 'discussion', body: 'second' },
          { id: 3, commentType: 'discussion', body: 'third' },
        ],
        commits: [
          { sha: 'a', message: 'first' },
          { sha: 'b', message: 'second' },
          { sha: 'c', message: 'third' },
        ],
      },
      {
        owner: 'octo',
        repo: 'engine',
        prNumber: 42,
        goal: 'internal transport metadata',
        reasoning: 'must not leak',
        limit: 1,
        includeDiff: false,
        page: 1,
        pageSize: 1,
        filePage: 1,
        commentPage: 1,
        commitPage: 1,
        content: {
          body: true,
          changedFiles: true,
          comments: {
            discussion: true,
            reviewInline: false,
            includeBots: false,
          },
          commits: { includeFiles: true },
        },
      } as never,
      request
    );

    expect(shaped.contentPagination).toMatchObject({
      body: expect.any(Object),
      changedFiles: expect.any(Object),
      comments: expect.any(Object),
      commits: expect.any(Object),
    });
    const pagination = shaped.contentPagination as Record<
      string,
      { nextQuery?: Record<string, unknown> }
    >;
    expect(pagination.changedFiles?.nextQuery).toMatchObject({
      operation: 'pullRequest',
      owner: 'octo',
      repo: 'engine',
      number: 42,
      filePage: 2,
      commentPage: 1,
      commitPage: 1,
      content: {
        body: true,
        changedFiles: true,
        comments: { discussion: true },
        commits: { includeFiles: true },
      },
    });
    expect(pagination.comments?.nextQuery).toMatchObject({
      operation: 'pullRequest',
      number: 42,
      filePage: 1,
      commentPage: 2,
      commitPage: 1,
    });
    expect(pagination.commits?.nextQuery).toMatchObject({
      operation: 'pullRequest',
      number: 42,
      filePage: 1,
      commentPage: 1,
      commitPage: 2,
    });
    expect(JSON.stringify(shaped.contentPagination)).not.toContain('prNumber');
    const emittedNextQueries = Object.values(pagination)
      .map(entry => entry.nextQuery)
      .filter((query): query is Record<string, unknown> => query !== undefined);
    for (const nextQuery of emittedNextQueries) {
      expect(
        GitHubGetHistoryItemQueryLocalSchema.safeParse(nextQuery).success
      ).toBe(true);
    }
    expect(JSON.stringify(emittedNextQueries)).not.toMatch(
      /ghSearchPullRequests|ghSearchIssues|ghSearchCommits|prNumber|issueNumber|"(?:goal|reasoning|type|limit|itemsPerPage|page)"|"(?:includeBots|reviewInline)":false/
    );
    for (const retired of [
      'bodyPagination',
      'filePagination',
      'commentPagination',
      'commitPagination',
      'filePathsPagination',
    ]) {
      expect(shaped).not.toHaveProperty(retired);
    }

    const source = readFileSync(
      new URL(
        '../../../src/tools/github_search_pull_requests/contentResponse.ts',
        import.meta.url
      ),
      'utf8'
    );
    expect(source).not.toContain('removeLegacyPaginationFields');
    expect(source).not.toMatch(
      /shaped\.(?:bodyPagination|filePagination|commentPagination|commitPagination|filePathsPagination)/
    );
  });

  it('turns every history-detail page axis into an explicit terminal limit at schema page 1000', () => {
    const rows = Array.from({ length: 1001 }, (_, index) => ({
      path: `src/${index}.ts`,
      status: 'modified',
      id: index,
      commentType: 'discussion',
      body: `comment ${index}`,
      sha: String(index),
      message: `commit ${index}`,
    }));
    const request = normalizePullRequestContentRequest({
      content: {
        changedFiles: true,
        comments: { discussion: true },
        commits: {},
      },
    } as never);
    const shaped = shapePullRequestForContent(
      { ...PR, fileChanges: rows, comments: rows, commits: rows },
      {
        owner: 'octo',
        repo: 'engine',
        prNumber: 42,
        pageSize: 1,
        filePage: 1000,
        commentPage: 1000,
        commitPage: 1000,
      },
      request
    );
    const pagination = shaped.contentPagination as Record<
      string,
      Record<string, unknown>
    >;

    for (const axis of ['changedFiles', 'comments', 'commits']) {
      expect(pagination[axis]).toMatchObject({
        currentPage: 1000,
        hasMore: true,
        terminalLimit: true,
        continuationUnavailable: {
          reason: 'schemaPageLimit',
          maxPage: 1000,
        },
      });
      expect(pagination[axis]).not.toHaveProperty('nextPage');
      expect(pagination[axis]).not.toHaveProperty('nextQuery');
    }
  });
});
