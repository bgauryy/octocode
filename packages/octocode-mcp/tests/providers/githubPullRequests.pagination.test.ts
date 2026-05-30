import { describe, it, expect } from 'vitest';
import { transformPullRequestResult } from '../../src/providers/github/githubPullRequests.js';

type Args = Parameters<typeof transformPullRequestResult>;

describe('transformPullRequestResult — pagination page size', () => {
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
