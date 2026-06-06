import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { searchGitHubPullRequestsAPI } from '../../src/github/pullRequestSearch';
import * as client from '../../src/github/client';
import * as queryBuilders from '../../src/github/queryBuilders';

vi.mock('../../src/github/client');
vi.mock('../../src/session');
vi.mock('../../src/utils/http/cache.js', () => ({
  withDataCache: vi.fn((_key, fn) => fn()),
  generateCacheKey: vi.fn(() => 'cache-key'),
}));

describe('pullRequestSearch coverage', () => {
  const mockOctokit = {
    rest: {
      search: {
        issuesAndPullRequests: vi.fn(),
      },
      pulls: {
        list: vi.fn(),
        get: vi.fn(),
        listFiles: vi.fn(),
      },
      issues: {
        listComments: vi.fn(),
      },
    },
  };

  beforeEach(() => {
    vi.spyOn(client, 'getOctokit').mockResolvedValue(
      mockOctokit as unknown as Awaited<ReturnType<typeof client.getOctokit>>
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should handle sort parameter correctly', async () => {
    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: { items: [], total_count: 0 },
    });

    await searchGitHubPullRequestsAPI({
      query: 'test',
      sort: 'updated',
      order: 'asc',
    });

    expect(mockOctokit.rest.search.issuesAndPullRequests).toHaveBeenCalledWith(
      expect.objectContaining({
        sort: 'updated',
        order: 'asc',
      })
    );
  });

  it('should fallback sort to undefined if invalid', async () => {
    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: { items: [], total_count: 0 },
    });

    await searchGitHubPullRequestsAPI({
      query: 'test',
      sort: 'invalid-sort' as 'created',
    });

    expect(mockOctokit.rest.search.issuesAndPullRequests).toHaveBeenCalledWith(
      expect.objectContaining({
        sort: 'invalid-sort',
      })
    );
  });

  it('should handle REST API search error', async () => {
    mockOctokit.rest.pulls.list.mockRejectedValue(new Error('API Error'));

    vi.spyOn(queryBuilders, 'shouldUseSearchForPRs').mockReturnValue(false);

    const result = await searchGitHubPullRequestsAPI({
      owner: 'owner',
      repo: 'repo',
    });

    expect(result.error).toBeDefined();
    expect(result.hints).toBeDefined();
  });

  it('should handle error when fetching PR comments', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test query'
    );

    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: {
        items: [
          {
            number: 1,
            title: 'Test PR',
            html_url: 'http://github.com/owner/repo/pull/1',
            state: 'open',
            user: { login: 'user' },
            pull_request: {},
          },
        ],
        total_count: 1,
      },
    });

    mockOctokit.rest.issues.listComments.mockRejectedValue(
      new Error('Comment Error')
    );

    const result = await searchGitHubPullRequestsAPI({
      query: 'test',
      owner: 'owner',
      repo: 'repo',
      withComments: true,
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests).toHaveLength(1);
    expect(result.pull_requests?.[0]?.comments).toBe(0);
  });

  it('should handle error when fetching file changes', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test query'
    );

    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: {
        items: [
          {
            number: 1,
            title: 'Test PR',
            html_url: 'http://github.com/owner/repo/pull/1',
            state: 'open',
            user: { login: 'user' },
            pull_request: {},
          },
        ],
        total_count: 1,
      },
    });

    mockOctokit.rest.pulls.listFiles.mockRejectedValue(new Error('File Error'));

    const result = await searchGitHubPullRequestsAPI({
      query: 'test',
      owner: 'owner',
      repo: 'repo',
      type: 'fullContent',
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests).toHaveLength(1);
    expect(result.pull_requests?.[0]?.file_changes).toBeUndefined();
  });

  it('should use REST API when specific conditions met', async () => {
    vi.spyOn(queryBuilders, 'shouldUseSearchForPRs').mockReturnValue(false);
    mockOctokit.rest.pulls.list.mockResolvedValue({ data: [] });

    await searchGitHubPullRequestsAPI({
      owner: 'owner',
      repo: 'repo',
      state: 'closed',
    });

    expect(mockOctokit.rest.pulls.list).toHaveBeenCalledWith(
      expect.objectContaining({
        state: 'closed',
      })
    );
  });

  it('should return error if search query build fails', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue('');

    const result = await searchGitHubPullRequestsAPI({});

    expect(result.error).toBeDefined();
  });

  it('should handle REST API with head and base filters', async () => {
    vi.spyOn(queryBuilders, 'shouldUseSearchForPRs').mockReturnValue(false);
    mockOctokit.rest.pulls.list.mockResolvedValue({ data: [] });

    await searchGitHubPullRequestsAPI({
      owner: 'owner',
      repo: 'repo',
      head: 'feature-branch',
      base: 'main',
    });

    expect(mockOctokit.rest.pulls.list).toHaveBeenCalledWith(
      expect.objectContaining({
        head: 'feature-branch',
        base: 'main',
      })
    );
  });

  it('should handle search with sort parameter variations', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test'
    );
    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: { items: [], total_count: 0 },
    });

    await searchGitHubPullRequestsAPI({
      query: 'test',
      sort: 'updated',
    });

    expect(mockOctokit.rest.search.issuesAndPullRequests).toHaveBeenCalledWith(
      expect.objectContaining({
        sort: 'updated',
      })
    );
  });

  it('should handle merged PR with merged_at date', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test'
    );

    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: {
        items: [
          {
            number: 1,
            title: 'Test PR',
            html_url: 'http://github.com/owner/repo/pull/1',
            state: 'closed',
            user: { login: 'user' },
            pull_request: {},
            merged_at: '2024-01-01T00:00:00Z',
          },
        ],
        total_count: 1,
      },
    });

    mockOctokit.rest.pulls.get.mockResolvedValue({
      data: {
        number: 1,
        title: 'Test PR',
        state: 'closed',
        user: { login: 'user' },
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
        merged_at: '2024-01-01T00:00:00Z',
      },
    });

    const result = await searchGitHubPullRequestsAPI({
      query: 'test',
      owner: 'owner',
      repo: 'repo',
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests).toHaveLength(1);
    expect(result.pull_requests?.[0]?.merged).toBe(true);
    expect(result.pull_requests?.[0]?.merged_at).toBeDefined();
  });

  it('should handle PR with file_changes that have patches', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test'
    );

    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: {
        items: [
          {
            number: 1,
            title: 'Test PR',
            html_url: 'http://github.com/owner/repo/pull/1',
            state: 'open',
            user: { login: 'user' },
            pull_request: {},
          },
        ],
        total_count: 1,
      },
    });

    mockOctokit.rest.pulls.get.mockResolvedValue({
      data: {
        number: 1,
        title: 'Test PR',
        state: 'open',
        user: { login: 'user' },
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
      },
    });

    mockOctokit.rest.pulls.listFiles.mockResolvedValue({
      data: [
        {
          filename: 'test.ts',
          status: 'modified',
          additions: 10,
          deletions: 5,
          changes: 15,
          patch: '@@ -1,3 +1,4 @@\n+new line',
        },
      ],
    });

    const result = await searchGitHubPullRequestsAPI({
      query: 'test',
      owner: 'owner',
      repo: 'repo',
      type: 'fullContent',
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests).toHaveLength(1);
    expect(result.pull_requests?.[0]?.file_changes).toBeDefined();
    expect(result.pull_requests?.[0]?.file_changes?.[0]?.patch).toBeDefined();
  });

  it('should handle draft PR flag correctly', async () => {
    vi.spyOn(queryBuilders, 'shouldUseSearchForPRs').mockReturnValue(false);

    mockOctokit.rest.pulls.list.mockResolvedValue({
      data: [
        {
          number: 1,
          title: 'Draft PR',
          html_url: 'http://github.com/owner/repo/pull/1',
          state: 'open',
          user: { login: 'user' },
          head: { ref: 'feature', sha: 'abc123' },
          base: { ref: 'main', sha: 'def456' },
          draft: true,
        },
      ],
    });

    const result = await searchGitHubPullRequestsAPI({
      owner: 'owner',
      repo: 'repo',
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests).toHaveLength(1);
    expect(result.pull_requests?.[0]?.draft).toBe(true);
  });

  it('should calculate additions and deletions from file_changes', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test'
    );

    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: {
        items: [
          {
            number: 1,
            title: 'Test PR',
            html_url: 'http://github.com/owner/repo/pull/1',
            state: 'open',
            user: { login: 'user' },
            pull_request: {},
          },
        ],
        total_count: 1,
      },
    });

    mockOctokit.rest.pulls.get.mockResolvedValue({
      data: {
        number: 1,
        title: 'Test PR',
        state: 'open',
        user: { login: 'user' },
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
      },
    });

    mockOctokit.rest.pulls.listFiles.mockResolvedValue({
      data: [
        {
          filename: 'file1.ts',
          status: 'modified',
          additions: 10,
          deletions: 5,
          changes: 15,
        },
        {
          filename: 'file2.ts',
          status: 'added',
          additions: 20,
          deletions: 2,
          changes: 22,
        },
      ],
    });

    const result = await searchGitHubPullRequestsAPI({
      query: 'test',
      owner: 'owner',
      repo: 'repo',
      type: 'fullContent',
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests?.[0]?.additions).toBe(30);
    expect(result.pull_requests?.[0]?.deletions).toBe(7);
    expect(result.pull_requests?.[0]?.changed_files).toBe(2);
  });

  it('should include sanitization warnings when present', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test'
    );

    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: {
        items: [
          {
            number: 1,
            title: 'Test PR',
            html_url: 'http://github.com/owner/repo/pull/1',
            state: 'open',
            user: {
              login: 'user',
              html_url: 'mailto:user@example.com',
            },
            pull_request: {},
          },
        ],
        total_count: 1,
      },
    });

    mockOctokit.rest.pulls.get.mockResolvedValue({
      data: {
        number: 1,
        title: 'Test PR',
        state: 'open',
        user: {
          login: 'user',
          html_url: 'mailto:user@example.com',
        },
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
      },
    });

    const result = await searchGitHubPullRequestsAPI({
      query: 'test',
      owner: 'owner',
      repo: 'repo',
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests).toHaveLength(1);
  });

  it('should process PR content with secrets through sanitization (coverage)', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test'
    );

    const fakeGitHubToken =
      'ghp_abcdefghijklmnopqrstuvwxyz12345678901234567890';

    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: {
        items: [
          {
            number: 1,
            title: `Fix auth issue with token ${fakeGitHubToken}`,
            html_url: 'http://github.com/owner/repo/pull/1',
            state: 'open',
            user: { login: 'user' },
            pull_request: {},
          },
        ],
        total_count: 1,
      },
    });

    mockOctokit.rest.pulls.get.mockResolvedValue({
      data: {
        number: 1,
        title: `Fix auth issue with token ${fakeGitHubToken}`,
        state: 'open',
        user: { login: 'user' },
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
      },
    });

    const result = await searchGitHubPullRequestsAPI({
      query: 'test',
      owner: 'owner',
      repo: 'repo',
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests).toHaveLength(1);
    expect(result.pull_requests?.[0]?.title).toContain('[REDACTED-');
  });

  it('should process PR body with AWS keys through sanitization (coverage)', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test'
    );

    const fakeAwsKey = 'AKIAIOSFODNN7EXAMPLE';

    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: {
        items: [
          {
            number: 1,
            title: 'Update config',
            body: `Added AWS config with key ${fakeAwsKey}`,
            html_url: 'http://github.com/owner/repo/pull/1',
            state: 'open',
            user: { login: 'user' },
            pull_request: {},
          },
        ],
        total_count: 1,
      },
    });

    mockOctokit.rest.pulls.get.mockResolvedValue({
      data: {
        number: 1,
        title: 'Update config',
        body: `Added AWS config with key ${fakeAwsKey}`,
        state: 'open',
        user: { login: 'user' },
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
      },
    });

    const result = await searchGitHubPullRequestsAPI({
      query: 'test',
      owner: 'owner',
      repo: 'repo',
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests).toHaveLength(1);
    expect(result.pull_requests?.[0]?.body).toContain(
      '[REDACTED-AWSACCESSKEYID]'
    );
  });

  it('should handle clean PR content without sanitization warnings (coverage)', async () => {
    vi.spyOn(queryBuilders, 'buildPullRequestSearchQuery').mockReturnValue(
      'test'
    );

    mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
      data: {
        items: [
          {
            number: 1,
            title: 'Add new feature',
            body: 'This PR adds a new exciting feature to the codebase.',
            html_url: 'http://github.com/owner/repo/pull/1',
            state: 'open',
            user: { login: 'user' },
            pull_request: {},
          },
        ],
        total_count: 1,
      },
    });

    mockOctokit.rest.pulls.get.mockResolvedValue({
      data: {
        number: 1,
        title: 'Add new feature',
        body: 'This PR adds a new exciting feature to the codebase.',
        state: 'open',
        user: { login: 'user' },
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
      },
    });

    const result = await searchGitHubPullRequestsAPI({
      query: 'test',
      owner: 'owner',
      repo: 'repo',
    });

    expect(result.pull_requests).toBeDefined();
    expect(result.pull_requests).toHaveLength(1);
    expect(result.pull_requests?.[0]?.title).not.toContain('[REDACTED-');
    expect(result.pull_requests?.[0]?.body).not.toContain('[REDACTED-');
  });
});
