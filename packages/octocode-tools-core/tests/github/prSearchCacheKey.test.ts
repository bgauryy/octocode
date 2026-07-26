import { describe, expect, it } from 'vitest';
import { buildPullRequestSearchCacheKey } from '../../src/github/pullRequestSearch.js';
import type { GitHubPullRequestsSearchParams } from '../../src/github/githubAPI.js';

/**
 * Regression: the PR-search cache key must include every param that changes the
 * built search query, so two searches differing only in one of them do not
 * collide on a single cache entry and serve stale results. (milestone/locked/
 * visibility/language/team-mentions/project were removed from the PR surface.)
 */
describe('buildPullRequestSearchCacheKey', () => {
  const base: GitHubPullRequestsSearchParams = {
    owner: 'facebook',
    repo: 'react',
    state: 'open',
  };

  it('differs when only `review` differs', () => {
    const withNone = buildPullRequestSearchCacheKey({
      ...base,
      review: 'none',
    });
    const withApproved = buildPullRequestSearchCacheKey({
      ...base,
      review: 'approved',
    });
    expect(withNone).not.toBe(withApproved);
  });

  it.each([
    ['checks', { checks: 'success' }, { checks: 'failure' }],
    ['archived', { archived: true }, { archived: false }],
  ] as [string, Partial<GitHubPullRequestsSearchParams>, Partial<GitHubPullRequestsSearchParams>][])(
    'differs when only `%s` differs',
    (_field, left, right) => {
      expect(buildPullRequestSearchCacheKey({ ...base, ...left })).not.toBe(
        buildPullRequestSearchCacheKey({ ...base, ...right })
      );
    }
  );

  it('is stable for identical params', () => {
    expect(buildPullRequestSearchCacheKey({ ...base, review: 'none' })).toBe(
      buildPullRequestSearchCacheKey({ ...base, review: 'none' })
    );
  });
});
