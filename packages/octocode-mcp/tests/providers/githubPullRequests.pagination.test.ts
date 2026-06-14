import { describe, it, expect } from 'vitest';
import { transformPullRequestResult } from '../../../octocode-tools-core/src/providers/github/githubPullRequests.js';

type Args = Parameters<typeof transformPullRequestResult>;

describe('transformPullRequestResult — pagination page size', () => {
  it('maps commit_details and comment_details (with inline metadata) to provider types', () => {
    const data = {
      pull_requests: [
        {
          number: 77,
          title: 'Add feature',
          url: 'https://github.com/o/r/pull/77',
          state: 'open',
          draft: false,
          merged: false,
          created_at: '2024-01-01',
          updated_at: '2024-01-02',
          author: 'dev',
          head_ref: 'feat',
          base_ref: 'main',
          comment_details: [
            {
              id: 'c1',
              user: 'alice',
              body: 'nice',
              created_at: '2024-01-01',
              updated_at: '2024-01-01',
              commentType: 'review_inline' as const,
              path: 'src/foo.ts',
              line: 10,
            },
          ],
          commit_details: [
            {
              sha: 'abc',
              message: 'feat: do thing',
              author: 'dev',
              date: '2024-01-01',
              files: [],
            },
          ],
        },
      ],
      total_count: 1,
    } as unknown as Args[0];

    const result = transformPullRequestResult(data, {} as Args[1]);
    const pr = result.items[0]!;

    expect(pr.commits).toHaveLength(1);
    expect(pr.commits![0]!.sha).toBe('abc');
    expect(pr.comments).toHaveLength(1);
    expect(pr.comments![0]!.commentType).toBe('review_inline');
    expect(pr.comments![0]!.path).toBe('src/foo.ts');
    expect(pr.comments![0]!.line).toBe(10);
  });

  it('preserves body and comment pagination metadata', () => {
    const data = {
      pull_requests: [
        {
          number: 79,
          title: 'Paged PR',
          body: 'paged body',
          body_pagination: {
            charOffset: 20,
            charLength: 10,
            totalChars: 100,
            hasMore: true,
            nextCharOffset: 30,
          },
          url: 'https://github.com/o/r/pull/79',
          state: 'open',
          draft: false,
          merged: false,
          created_at: '2024-01-01',
          updated_at: '2024-01-02',
          author: 'dev',
          head_ref: 'feat',
          base_ref: 'main',
          comment_details: [
            {
              id: 'c3',
              user: 'bob',
              body: 'paged comment',
              body_pagination: {
                charOffset: 50,
                charLength: 20,
                totalChars: 200,
                hasMore: true,
                nextCharOffset: 70,
              },
              created_at: '2024-01-01',
              updated_at: '2024-01-01',
            },
          ],
        },
      ],
      total_count: 1,
    } as unknown as Args[0];

    const result = transformPullRequestResult(data, {} as Args[1]);
    const pr = result.items[0]!;

    expect(pr.bodyPagination?.nextCharOffset).toBe(30);
    expect(pr.comments?.[0]?.bodyPagination?.nextCharOffset).toBe(70);
  });

  it('handles comment_details without inline metadata', () => {
    const data = {
      pull_requests: [
        {
          number: 78,
          title: 'Discussion PR',
          url: 'https://github.com/o/r/pull/78',
          state: 'open',
          draft: false,
          merged: false,
          created_at: '2024-01-01',
          updated_at: '2024-01-02',
          author: 'dev',
          head_ref: 'feat',
          base_ref: 'main',
          comment_details: [
            {
              id: 'c2',
              user: 'bob',
              body: 'discussion comment',
              created_at: '2024-01-01',
              updated_at: '2024-01-01',
            },
          ],
        },
      ],
      total_count: 1,
    } as unknown as Args[0];

    const result = transformPullRequestResult(data, {} as Args[1]);
    const pr = result.items[0]!;

    expect(pr.comments).toHaveLength(1);
    expect(pr.comments![0]!.commentType).toBeUndefined();
    expect(pr.comments![0]!.path).toBeUndefined();
    expect(pr.comments![0]!.line).toBeUndefined();
    expect(pr.commits).toBeUndefined();
  });

  it('carries the API perPage into entriesPerPage (does not drop to 10)', () => {
    const data = {
      pull_requests: [],
      total_count: 15,
      pagination: {
        currentPage: 2,
        totalPages: 5,
        perPage: 3,
        totalMatches: 15,
        hasMore: true,
      },
    } as unknown as Args[0];

    const result = transformPullRequestResult(data, {} as Args[1]);

    expect(result.pagination?.entriesPerPage).toBe(3);
    expect(result.pagination?.currentPage).toBe(2);
    expect(result.pagination?.totalMatches).toBe(15);
    expect(result.pagination?.hasMore).toBe(true);
  });
});
