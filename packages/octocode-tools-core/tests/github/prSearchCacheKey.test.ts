import { describe, expect, it } from 'vitest';
import { buildPullRequestSearchCacheKey } from '../../src/github/pullRequestSearch.js';
import type { GitHubPullRequestsSearchParams } from '../../src/github/githubAPI.js';

/**
 * Regression: the PR-search cache key must include every param that changes the
 * built search query. Fields like `review`, `checks`, `milestone`, `locked`,
 * etc. were previously omitted, so two searches differing only in one of them
 * collided on a single cache entry and served stale results.
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
    ['milestone', { milestone: 'v1' }, { milestone: 'v2' }],
    ['checks', { checks: 'success' }, { checks: 'failure' }],
    ['locked', { locked: true }, { locked: false }],
    ['visibility', { visibility: 'public' }, { visibility: 'private' }],
    ['language', { language: 'ts' }, { language: 'go' }],
    ['team-mentions', { 'team-mentions': 'a' }, { 'team-mentions': 'b' }],
    ['project', { project: 'p1' }, { project: 'p2' }],
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

  it('keeps the `no-*` cousins distinct from their positive filters', () => {
    expect(
      buildPullRequestSearchCacheKey({ ...base, 'no-milestone': true })
    ).not.toBe(buildPullRequestSearchCacheKey({ ...base, milestone: 'v1' }));
  });
});
