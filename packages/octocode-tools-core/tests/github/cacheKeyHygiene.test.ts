import { describe, expect, it } from 'vitest';
import {
  buildIssueSearchCacheKey,
  type FetchIssuesParams,
} from '../../src/github/issues.js';
import { buildPullRequestSearchCacheKey } from '../../src/github/pullRequestSearch.js';
import { generateCacheKey } from '../../src/utils/http/cache.js';
import type { GitHubPullRequestsSearchParams } from '../../src/github/githubAPI.js';

describe('GitHub cache key hygiene (Phase A)', () => {
  it('bumps cache VERSION to v2', () => {
    const key = generateCacheKey('gh-api-repos', { keywords: ['x'] });
    expect(key.startsWith('v2-')).toBe(true);
  });

  it('issues cache key differs when archived flips', () => {
    const base: FetchIssuesParams = {
      owner: 'microsoft',
      repo: 'TypeScript',
      state: 'open',
    };
    expect(
      buildIssueSearchCacheKey({ ...base, archived: true }, undefined, 'tokA')
    ).not.toBe(
      buildIssueSearchCacheKey({ ...base, archived: false }, undefined, 'tokA')
    );
  });

  it('issues cache key differs by auth fingerprint', () => {
    const base: FetchIssuesParams = {
      owner: 'microsoft',
      repo: 'TypeScript',
    };
    expect(buildIssueSearchCacheKey(base, undefined, 'tokA')).not.toBe(
      buildIssueSearchCacheKey(base, undefined, 'tokB')
    );
  });

  it('PR cache key differs by auth fingerprint', () => {
    const base: GitHubPullRequestsSearchParams = {
      owner: 'facebook',
      repo: 'react',
      state: 'open',
    };
    expect(buildPullRequestSearchCacheKey(base, undefined, 'tokA')).not.toBe(
      buildPullRequestSearchCacheKey(base, undefined, 'tokB')
    );
  });

  it('repo search key fields archived/license/forks/visibility/repo/goodFirstIssues change the hash', () => {
    const auth = 'tokA';
    const base = {
      keywords: ['mcp'],
      topicsToSearch: undefined,
      owner: undefined,
      repo: undefined,
      stars: undefined,
      size: undefined,
      created: undefined,
      updated: undefined,
      language: undefined,
      match: undefined,
      sort: undefined,
      limit: undefined,
      page: undefined,
      archived: undefined,
      visibility: undefined,
      forks: undefined,
      license: undefined,
      goodFirstIssues: undefined,
      auth,
    };
    const key = (extra: Record<string, unknown>) =>
      generateCacheKey('gh-api-repos', { ...base, ...extra });

    expect(key({ archived: true })).not.toBe(key({ archived: false }));
    expect(key({ visibility: 'public' })).not.toBe(
      key({ visibility: 'private' })
    );
    expect(key({ forks: '>10' })).not.toBe(key({ forks: '>100' }));
    expect(key({ license: 'mit' })).not.toBe(key({ license: 'apache-2.0' }));
    expect(key({ goodFirstIssues: '>1' })).not.toBe(
      key({ goodFirstIssues: '>5' })
    );
    expect(key({ repo: 'octocode' })).not.toBe(key({ repo: 'other' }));
  });
});
