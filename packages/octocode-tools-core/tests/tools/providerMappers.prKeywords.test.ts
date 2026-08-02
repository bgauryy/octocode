import { describe, expect, it } from 'vitest';

import { mapPullRequestToolQuery } from '../../src/tools/providerMappers/pullRequests.js';
import { buildGitHubPullRequestsSearchParams } from '../../src/providers/github/githubPullRequests.js';
import {
  buildPullRequestSearchQuery,
  shouldUseSearchForPRs,
} from '../../src/github/queryBuilders.js';

type PRArg = Parameters<typeof mapPullRequestToolQuery>[0];

/**
 * Regression for the benchmark-found P0: `keywordsToSearch` was silently
 * dropped between the tool mapper (which folds it into a provider-level
 * `query` string) and the GitHub provider adapter (whose param mapping never
 * copied `query`, because PullRequestQuery didn't declare the field). The
 * result: `shouldUseSearchForPRs` saw no query, fell back to a plain
 * `pulls.list` listing, and recent PRs were presented as keyword matches —
 * byte-identical output for a nonsense keyword vs a real one, no warning.
 */
describe('ghSearchPullRequests keywords survive the full mapping chain', () => {
  const toolQuery = {
    owner: 'facebook',
    repo: 'react',
    keywordsToSearch: ['useTransition'],
  } as PRArg;

  it('the provider params carry the keyword query string', () => {
    const providerQuery = mapPullRequestToolQuery(toolQuery);
    const params = buildGitHubPullRequestsSearchParams(
      providerQuery,
      'facebook',
      'react'
    );

    expect(params.query).toBeDefined();
    expect(params.query).toContain('useTransition');
  });

  it('keywords force the SEARCH path, never the plain listing', () => {
    const providerQuery = mapPullRequestToolQuery(toolQuery);
    const params = buildGitHubPullRequestsSearchParams(
      providerQuery,
      'facebook',
      'react'
    );

    expect(shouldUseSearchForPRs(params)).toBe(true);
  });

  it('the built search query string includes the keyword and repo scope', () => {
    const providerQuery = mapPullRequestToolQuery(toolQuery);
    const params = buildGitHubPullRequestsSearchParams(
      providerQuery,
      'facebook',
      'react'
    );

    const q = buildPullRequestSearchQuery(params);
    expect(q).toContain('useTransition');
    expect(q).toContain('is:pr');
    expect(q).toContain('repo:facebook/react');
  });

  it('without keywords or other search filters, the listing path is still allowed', () => {
    const providerQuery = mapPullRequestToolQuery({
      owner: 'facebook',
      repo: 'react',
    } as PRArg);
    const params = buildGitHubPullRequestsSearchParams(
      providerQuery,
      'facebook',
      'react'
    );

    expect(params.query).toBeUndefined();
    expect(shouldUseSearchForPRs(params)).toBe(false);
  });
});
