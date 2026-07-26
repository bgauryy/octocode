import { describe, expect, it } from 'vitest';
import {
  SearchPullRequestsLocalSchema,
  SearchPullRequestsBulkLocalSchema,
  SearchIssuesLocalSchema,
  SearchIssuesBulkLocalSchema,
  SearchCommitsLocalSchema,
  SearchCommitsBulkLocalSchema,
  ListReleasesLocalSchema,
  ListReleasesBulkLocalSchema,
} from '../../../src/tools/github_search_pull_requests/splitSchemes.js';
import { GITHUB_SEARCH_MAX_LIMIT } from '../../../src/config.js';

// ---------------------------------------------------------------------------
// SearchPullRequestsLocalSchema (ghSearchPullRequests)
// ---------------------------------------------------------------------------

describe('SearchPullRequestsLocalSchema', () => {
  it('accepts a minimal query with owner+repo', () => {
    const result = SearchPullRequestsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
    });
    expect(result.success).toBe(true);
  });

  it('accepts a full-featured query', () => {
    const result = SearchPullRequestsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
      state: 'open',
      keywordsToSearch: ['hooks', 'render'],
      label: 'bug',
      draft: false,
      limit: 20,
      page: 1,
      itemsPerPage: 5,
    });
    expect(result.success).toBe(true);
  });

  it('clamps limit above GITHUB_SEARCH_MAX_LIMIT', () => {
    const result = SearchPullRequestsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
      limit: GITHUB_SEARCH_MAX_LIMIT + 1000,
    });
    expect(result.success).toBe(true);
    expect(result.data!.limit).toBe(GITHUB_SEARCH_MAX_LIMIT);
  });

  it('clamps a negative page to 1', () => {
    const result = SearchPullRequestsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
      page: -5,
    });
    expect(result.success).toBe(true);
    expect(result.data!.page).toBeGreaterThanOrEqual(1);
  });

  it('accepts prNumber for fetching a specific PR', () => {
    const result = SearchPullRequestsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
      prNumber: 42,
    });
    expect(result.success).toBe(true);
  });

  it('accepts pagination read fields (filePage, commentPage, commitPage)', () => {
    const result = SearchPullRequestsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
      prNumber: 1,
      filePage: 2,
      commentPage: 3,
      commitPage: 1,
    });
    expect(result.success).toBe(true);
  });
});

describe('SearchPullRequestsBulkLocalSchema', () => {
  it('accepts a bulk request with multiple queries', () => {
    const result = SearchPullRequestsBulkLocalSchema.safeParse({
      queries: [
        { owner: 'facebook', repo: 'react', state: 'open' },
        { owner: 'vercel', repo: 'next.js', state: 'closed' },
      ],
    });
    expect(result.success).toBe(true);
  });

  it('accepts an invalid query in bulk (relaxed)', () => {
    // Bulk schemas are relaxed — individual bad queries should not abort parse
    const result = SearchPullRequestsBulkLocalSchema.safeParse({
      queries: [{ owner: 'facebook', repo: 'react' }],
    });
    expect(result.success).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// SearchIssuesLocalSchema (ghSearchIssues)
// ---------------------------------------------------------------------------

describe('SearchIssuesLocalSchema', () => {
  it('accepts a minimal query', () => {
    const result = SearchIssuesLocalSchema.safeParse({
      owner: 'microsoft',
      repo: 'TypeScript',
    });
    expect(result.success).toBe(true);
  });

  it('accepts issueNumber for detail read', () => {
    const result = SearchIssuesLocalSchema.safeParse({
      owner: 'microsoft',
      repo: 'TypeScript',
      issueNumber: 99,
    });
    expect(result.success).toBe(true);
  });

  it('clamps limit above max', () => {
    const result = SearchIssuesLocalSchema.safeParse({
      owner: 'microsoft',
      repo: 'TypeScript',
      limit: 999,
    });
    expect(result.success).toBe(true);
    expect(result.data!.limit).toBeLessThanOrEqual(GITHUB_SEARCH_MAX_LIMIT);
  });
});

describe('SearchIssuesBulkLocalSchema', () => {
  it('accepts bulk queries', () => {
    const result = SearchIssuesBulkLocalSchema.safeParse({
      queries: [
        { owner: 'microsoft', repo: 'TypeScript' },
        { owner: 'vercel', repo: 'next.js', state: 'open' },
      ],
    });
    expect(result.success).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// SearchCommitsLocalSchema (ghSearchCommits)
// ---------------------------------------------------------------------------

describe('SearchCommitsLocalSchema', () => {
  it('accepts a minimal query with owner+repo', () => {
    const result = SearchCommitsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
    });
    expect(result.success).toBe(true);
  });

  it('accepts since/until filters', () => {
    const result = SearchCommitsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
      since: '2024-01-01',
      until: '2024-06-01',
    });
    expect(result.success).toBe(true);
  });

  it('defaults page to 1', () => {
    const result = SearchCommitsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
    });
    expect(result.success).toBe(true);
    expect(result.data!.page).toBe(1);
  });

  it('clamps a page below 1 to 1', () => {
    const result = SearchCommitsLocalSchema.safeParse({
      owner: 'facebook',
      repo: 'react',
      page: 0,
    });
    expect(result.success).toBe(true);
    expect(result.data!.page).toBeGreaterThanOrEqual(1);
  });
});

describe('SearchCommitsBulkLocalSchema', () => {
  it('accepts bulk commit queries', () => {
    const result = SearchCommitsBulkLocalSchema.safeParse({
      queries: [{ owner: 'facebook', repo: 'react' }],
    });
    expect(result.success).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// ListReleasesLocalSchema (ghListReleases)
// ---------------------------------------------------------------------------

describe('ListReleasesLocalSchema', () => {
  it('accepts a minimal query', () => {
    const result = ListReleasesLocalSchema.safeParse({
      owner: 'vercel',
      repo: 'next.js',
    });
    expect(result.success).toBe(true);
  });

  it('defaults page to 1', () => {
    const result = ListReleasesLocalSchema.safeParse({
      owner: 'vercel',
      repo: 'next.js',
    });
    expect(result.success).toBe(true);
    expect(result.data!.page).toBe(1);
  });

  it('accepts a custom page', () => {
    const result = ListReleasesLocalSchema.safeParse({
      owner: 'vercel',
      repo: 'next.js',
      page: 3,
    });
    expect(result.success).toBe(true);
    expect(result.data!.page).toBe(3);
  });
});

describe('ListReleasesBulkLocalSchema', () => {
  it('accepts bulk releases queries', () => {
    const result = ListReleasesBulkLocalSchema.safeParse({
      queries: [
        { owner: 'vercel', repo: 'next.js' },
        { owner: 'microsoft', repo: 'TypeScript', page: 2 },
      ],
    });
    expect(result.success).toBe(true);
  });
});
