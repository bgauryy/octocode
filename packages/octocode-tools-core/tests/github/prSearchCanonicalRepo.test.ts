import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  reposGet: vi.fn(),
  searchIssuesAndPullRequests: vi.fn(),
  pullsGet: vi.fn(),
}));

vi.mock('../../src/github/client.js', () => ({
  OctokitWithThrottling: class {},
  getOctokit: vi.fn(async () => ({
    rest: {
      repos: { get: mocks.reposGet },
      search: { issuesAndPullRequests: mocks.searchIssuesAndPullRequests },
      pulls: { get: mocks.pullsGet },
    },
  })),
}));

const { searchGitHubPullRequestsAPI } =
  await import('../../src/github/pullRequestSearch.js');

describe('PR search canonical repository resolution', () => {
  beforeEach(() => {
    mocks.reposGet.mockReset();
    mocks.searchIssuesAndPullRequests.mockReset();
    mocks.pullsGet.mockReset();

    mocks.reposGet.mockResolvedValue({
      data: { full_name: 'react/react' },
    });
    mocks.searchIssuesAndPullRequests.mockResolvedValue({
      data: {
        total_count: 1,
        incomplete_results: false,
        items: [
          {
            number: 36809,
            title: 'Merged PR',
            html_url: 'https://github.com/react/react/pull/36809',
            state: 'closed',
            user: { login: 'Boshen' },
            labels: [],
            created_at: '2026-06-17T09:09:55Z',
            updated_at: '2026-06-17T23:07:24Z',
            closed_at: '2026-06-17T23:07:24Z',
            pull_request: {},
          },
        ],
      },
    });
    mocks.pullsGet.mockResolvedValue({
      data: {
        head: { ref: 'compiler', sha: 'head-sha' },
        base: { ref: 'main', sha: 'base-sha' },
        draft: false,
        merged_at: '2026-06-17T23:07:24Z',
        additions: 164,
        deletions: 146,
        changed_files: 6,
      },
    });
  });

  it('uses the canonical repo name for merged PR search queries', async () => {
    const result = await searchGitHubPullRequestsAPI({
      owner: 'facebook',
      repo: 'react',
      state: 'closed',
      merged: true,
      limit: 1,
    });

    expect(mocks.reposGet).toHaveBeenCalledWith({
      owner: 'facebook',
      repo: 'react',
    });
    expect(mocks.searchIssuesAndPullRequests).toHaveBeenCalledWith(
      expect.objectContaining({
        q: expect.stringContaining('repo:react/react'),
      })
    );
    expect(mocks.searchIssuesAndPullRequests).toHaveBeenCalledWith(
      expect.objectContaining({
        q: expect.not.stringContaining('repo:facebook/react'),
      })
    );
    expect(result.pull_requests[0]).toMatchObject({
      number: 36809,
      state: 'closed',
      merged: true,
      merged_at: '2026-06-17T23:07:24Z',
    });
  });
});
