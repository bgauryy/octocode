import { describe, expect, it, vi } from 'vitest';
import { shouldEnrichPullRequestFromSearch } from '../../src/github/prContentFetcher.js';
import type { GitHubPullRequestsSearchParams } from '../../src/github/githubAPI.js';

describe('shouldEnrichPullRequestFromSearch (Phase D lean)', () => {
  const base: GitHubPullRequestsSearchParams = {
    owner: 'facebook',
    repo: 'react',
    state: 'open',
  };

  it('is false for lean list search (no content / reviewMode)', () => {
    expect(shouldEnrichPullRequestFromSearch(base)).toBe(false);
  });

  it('is true when prNumber is set', () => {
    expect(
      shouldEnrichPullRequestFromSearch({ ...base, prNumber: 1 })
    ).toBe(true);
  });

  it('is true for reviewMode full', () => {
    expect(
      shouldEnrichPullRequestFromSearch({ ...base, reviewMode: 'full' })
    ).toBe(true);
  });

  it('is true when content requests changed files', () => {
    expect(
      shouldEnrichPullRequestFromSearch({
        ...base,
        content: { changedFiles: true },
      })
    ).toBe(true);
  });

  it('is true when content requests comments', () => {
    expect(
      shouldEnrichPullRequestFromSearch({
        ...base,
        content: { comments: { discussion: true } },
      })
    ).toBe(true);
  });
});

describe('transformPullRequestItemFromSearch lean path', () => {
  it('does not call pulls.get for lean search', async () => {
    const pullsGet = vi.fn();
    const octokit = {
      rest: {
        pulls: { get: pullsGet },
      },
    };

    const { transformPullRequestItemFromSearch } = await import(
      '../../src/github/prContentFetcher.js'
    );

    await transformPullRequestItemFromSearch(
      {
        number: 42,
        title: 'Lean PR',
        state: 'open',
        html_url: 'https://github.com/facebook/react/pull/42',
        user: { login: 'dev' },
        pull_request: { url: 'https://api.github.com/...' },
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-02T00:00:00Z',
        labels: [],
      } as never,
      {
        owner: 'facebook',
        repo: 'react',
        state: 'open',
      },
      octokit as never
    );

    expect(pullsGet).not.toHaveBeenCalled();
  });

  it('calls pulls.get when enrichment is required', async () => {
    const pullsGet = vi.fn().mockResolvedValue({
      data: {
        head: { ref: 'feature', sha: 'abc' },
        base: { ref: 'main', sha: 'def' },
        draft: false,
        additions: 1,
        deletions: 0,
        changed_files: 1,
      },
    });
    const octokit = {
      rest: {
        pulls: { get: pullsGet },
      },
    };

    const { transformPullRequestItemFromSearch } = await import(
      '../../src/github/prContentFetcher.js'
    );

    await transformPullRequestItemFromSearch(
      {
        number: 42,
        title: 'Detail PR',
        state: 'open',
        html_url: 'https://github.com/facebook/react/pull/42',
        user: { login: 'dev' },
        pull_request: { url: 'https://api.github.com/...' },
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-02T00:00:00Z',
        labels: [],
      } as never,
      {
        owner: 'facebook',
        repo: 'react',
        state: 'open',
        content: { changedFiles: true },
      },
      octokit as never
    );

    expect(pullsGet).toHaveBeenCalled();
  });
});
