import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// Create hoisted mocks
const mockGetOctokit = vi.hoisted(() => vi.fn());
const mockHandleGitHubAPIError = vi.hoisted(() => vi.fn());
const mockBuildPullRequestSearchQuery = vi.hoisted(() => vi.fn());
const mockShouldUseSearchForPRs = vi.hoisted(() => vi.fn());
const mockGenerateCacheKey = vi.hoisted(() => vi.fn());
const mockWithDataCache = vi.hoisted(() => vi.fn());
const mockContentSanitizer = vi.hoisted(() => ({
  sanitizeContent: vi.fn(),
}));
const mockLogSessionError = vi.hoisted(() => vi.fn());

// Set up mocks
vi.mock('../../src/session.js', () => ({
  logSessionError: mockLogSessionError,
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: mockGetOctokit,
  OctokitWithThrottling: class MockOctokit {},
}));

vi.mock('../../src/github/errors.js', () => ({
  handleGitHubAPIError: mockHandleGitHubAPIError,
}));

vi.mock('../../src/github/queryBuilders.js', () => ({
  buildPullRequestSearchQuery: mockBuildPullRequestSearchQuery,
  shouldUseSearchForPRs: mockShouldUseSearchForPRs,
}));

vi.mock('../../src/utils/http/cache.js', () => ({
  generateCacheKey: mockGenerateCacheKey,
  withDataCache: mockWithDataCache,
}));

vi.mock('octocode-security-utils/contentSanitizer', () => ({
  ContentSanitizer: mockContentSanitizer,
}));

// Import after mocks are set up
import { searchGitHubPullRequestsAPI } from '../../src/github/pullRequestSearch.js';
import { fetchGitHubPullRequestByNumberAPI } from '../../src/github/prByNumber.js';
import { transformPullRequestItemFromREST } from '../../src/github/prContentFetcher.js';
import type { PullRequestSimple } from '../../src/github/githubAPI.js';
import { countSerializedChars } from '../../src/utils/response/charSavings.js';

// Type for mock PR items in tests - allows partial data
type MockPRItem = Partial<PullRequestSimple>;

describe('Pull Request Search', () => {
  let mockOctokit: {
    rest: {
      search: { issuesAndPullRequests: ReturnType<typeof vi.fn> };
      pulls: {
        list: ReturnType<typeof vi.fn>;
        get: ReturnType<typeof vi.fn>;
        listFiles: ReturnType<typeof vi.fn>;
        listCommits: ReturnType<typeof vi.fn>;
      };
      issues: { listComments: ReturnType<typeof vi.fn> };
      repos: { getCommit: ReturnType<typeof vi.fn> };
    };
  };

  beforeEach(() => {
    vi.clearAllMocks();

    // Setup default mock Octokit instance
    mockOctokit = {
      rest: {
        search: { issuesAndPullRequests: vi.fn() },
        pulls: {
          list: vi.fn(),
          get: vi.fn(),
          listFiles: vi.fn(),
          listCommits: vi.fn(),
        },
        issues: { listComments: vi.fn() },
        repos: { getCommit: vi.fn() },
      },
    };
    mockGetOctokit.mockResolvedValue(mockOctokit);

    // Setup default cache behavior - execute the operation directly
    mockGenerateCacheKey.mockReturnValue('test-cache-key');
    mockWithDataCache.mockImplementation(
      async (_cacheKey: string, operation: () => Promise<unknown>) => {
        return await operation();
      }
    );

    // Setup default error handler
    mockHandleGitHubAPIError.mockReturnValue({
      error: 'API Error',
      type: 'http',
    });

    // Setup default content sanitizer
    mockContentSanitizer.sanitizeContent.mockImplementation(
      (content: string) => ({
        content,
        warnings: [],
      })
    );

    // Setup default query builder
    mockBuildPullRequestSearchQuery.mockReturnValue(
      'repo:test/test is:pr state:open'
    );
    mockShouldUseSearchForPRs.mockReturnValue(false);
    mockLogSessionError.mockClear();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('searchGitHubPullRequestsAPI', () => {
    it('should fetch specific PR by number when prNumber is provided', async () => {
      const mockPR = {
        number: 123,
        title: 'Test PR',
        state: 'open',
        draft: false,
        user: { login: 'testuser' },
        labels: [],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/123',
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
        body: 'Test description',
      };

      mockOctokit.rest.pulls.get.mockResolvedValue({ data: mockPR });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        prNumber: 123,
      });

      expect(result.pull_requests).toHaveLength(1);
      expect(result.pull_requests?.[0]?.number).toBe(123);
      expect(result.total_count).toBe(1);
      expect(mockOctokit.rest.pulls.get).toHaveBeenCalledWith({
        owner: 'test',
        repo: 'repo',
        pull_number: 123,
      });
    });


    it('should include raw chars from REST list and fetched file changes', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(false);

      const mockPRs = [
        {
          number: 9,
          title: 'Raw PR',
          state: 'open',
          draft: false,
          user: { login: 'user1' },
          labels: [],
          created_at: '2023-01-01T00:00:00Z',
          updated_at: '2023-01-02T00:00:00Z',
          closed_at: null,
          html_url: 'https://github.com/test/repo/pull/9',
          head: { ref: 'feature', sha: 'abc' },
          base: { ref: 'main', sha: 'def' },
          body: 'Description',
        },
      ];
      const mockFiles = [
        {
          filename: 'src/index.ts',
          status: 'modified',
          additions: 2,
          deletions: 1,
          changes: 3,
          patch: '@@ raw patch',
        },
      ];

      mockOctokit.rest.pulls.list.mockResolvedValue({ data: mockPRs });
      mockOctokit.rest.pulls.listFiles.mockResolvedValue({ data: mockFiles });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        type: 'metadata',
      });

      expect(result.rawResponseChars).toBe(
        countSerializedChars(mockPRs) + countSerializedChars(mockFiles)
      );
    });

    it('should use REST API for simple repo searches', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(false);

      const mockPRs = [
        {
          number: 1,
          title: 'PR 1',
          state: 'open',
          draft: false,
          user: { login: 'user1' },
          labels: [],
          created_at: '2023-01-01T00:00:00Z',
          updated_at: '2023-01-02T00:00:00Z',
          closed_at: null,
          html_url: 'https://github.com/test/repo/pull/1',
          head: { ref: 'feature1', sha: 'abc1' },
          base: { ref: 'main', sha: 'def1' },
          body: 'Description 1',
        },
      ];

      mockOctokit.rest.pulls.list.mockResolvedValue({ data: mockPRs });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        state: 'open',
      });

      expect(result.pull_requests).toHaveLength(1);
      expect(mockOctokit.rest.pulls.list).toHaveBeenCalledWith({
        owner: 'test',
        repo: 'repo',
        state: 'open',
        per_page: 30,
        page: 1,
        sort: 'created',
        direction: 'desc',
      });
    });

    it('should use Search API for complex queries', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(true);
      mockBuildPullRequestSearchQuery.mockReturnValue(
        'repo:test/repo is:pr bug'
      );

      const mockSearchResult = {
        total_count: 1,
        incomplete_results: false,
        items: [
          {
            number: 2,
            title: 'Bug fix',
            state: 'open',
            draft: false,
            user: { login: 'user2' },
            labels: [{ name: 'bug' }],
            created_at: '2023-01-01T00:00:00Z',
            updated_at: '2023-01-02T00:00:00Z',
            closed_at: null,
            html_url: 'https://github.com/test/repo/pull/2',
            body: 'Bug fix description',
            pull_request: {},
          },
        ],
      };

      mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
        data: mockSearchResult,
      });

      mockOctokit.rest.pulls.get.mockResolvedValue({
        data: {
          number: 2,
          head: { ref: 'bugfix', sha: 'abc2' },
          base: { ref: 'main', sha: 'def2' },
          draft: false,
        },
      });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        query: 'bug',
      });

      expect(result.pull_requests).toHaveLength(1);
      expect(result.total_count).toBe(1);
      expect(mockOctokit.rest.search.issuesAndPullRequests).toHaveBeenCalled();
    });

    it('should fetch commits when withCommits is true (Search API)', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(true);
      mockBuildPullRequestSearchQuery.mockReturnValue('repo:test/repo is:pr');

      const mockSearchResult = {
        total_count: 1,
        incomplete_results: false,
        items: [
          {
            number: 123,
            title: 'PR with commits',
            state: 'open',
            user: { login: 'user1' },
            labels: [],
            created_at: '2023-01-01T00:00:00Z',
            updated_at: '2023-01-02T00:00:00Z',
            html_url: 'https://github.com/test/repo/pull/123',
            pull_request: {},
            body: 'body',
          },
        ],
      };

      mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
        data: mockSearchResult,
      });

      mockOctokit.rest.pulls.get.mockResolvedValue({
        data: { head: {}, base: {}, draft: false },
      });

      const mockCommits = [
        {
          sha: 'sha1',
          commit: {
            message: 'commit 1',
            author: { date: '2023-01-01T00:00:00Z', name: 'User' },
          },
        },
      ];

      mockOctokit.rest.pulls.listCommits = vi
        .fn()
        .mockResolvedValue({ data: mockCommits });
      mockOctokit.rest.repos.getCommit.mockResolvedValue({
        data: { files: [] },
      });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        withCommits: true,
      });

      expect(result.pull_requests?.[0]?.commit_details).toBeDefined();
      expect(result.pull_requests?.[0]?.commit_details).toHaveLength(1);
    });

    it('should handle commit fetch API error gracefully (Search API)', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(true);
      mockBuildPullRequestSearchQuery.mockReturnValue('repo:test/repo is:pr');

      const mockSearchResult = {
        total_count: 1,
        items: [
          {
            number: 123,
            title: 'PR commit error',
            state: 'open',
            user: { login: 'user1' },
            html_url: 'url',
            pull_request: {},
          },
        ],
      };

      mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
        data: mockSearchResult,
      });
      mockOctokit.rest.pulls.get.mockResolvedValue({ data: {} });

      // Simulate API error in listCommits
      mockOctokit.rest.pulls.listCommits = vi
        .fn()
        .mockRejectedValue(new Error('API Error'));

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        withCommits: true,
      });

      // Should succeed but without commit_details
      expect(result.pull_requests?.[0]?.commit_details).toBeUndefined();
      // Error should be logged now
      expect(mockLogSessionError).toHaveBeenCalled();
      // Warning should be present (using type assertion since _sanitization_warnings may not be in PullRequestInfo type)
      const prWithWarnings = result.pull_requests?.[0] as
        | { _sanitization_warnings?: string[] }
        | undefined;
      expect(prWithWarnings?._sanitization_warnings).toBeDefined();
    });

    it('should return error when no valid search parameters provided', async () => {
      mockBuildPullRequestSearchQuery.mockReturnValue(null);
      mockShouldUseSearchForPRs.mockReturnValue(true);

      const result = await searchGitHubPullRequestsAPI({});

      expect(result.error).toContain('No valid search parameters provided');
      expect(result.pull_requests).toHaveLength(0);
    });

    it('should handle API errors gracefully', async () => {
      mockOctokit.rest.pulls.list.mockRejectedValue(new Error('API Error'));
      mockHandleGitHubAPIError.mockReturnValue({
        error: 'Failed to fetch PRs',
        type: 'http',
      });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
      });

      expect(result.error).toContain('Pull request list failed');
      expect(result.pull_requests).toHaveLength(0);
    });

    it('should fetch file changes when type is fullContent', async () => {
      const mockPR = {
        number: 123,
        title: 'Test PR',
        state: 'open',
        draft: false,
        user: { login: 'testuser' },
        labels: [],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/123',
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
        body: 'Test description',
      };

      const mockFiles = [
        {
          filename: 'test.ts',
          status: 'modified',
          additions: 10,
          deletions: 5,
          changes: 15,
          patch: '@@ -1,5 +1,10 @@...',
        },
      ];

      mockOctokit.rest.pulls.get.mockResolvedValue({ data: mockPR });
      mockOctokit.rest.pulls.listFiles.mockResolvedValue({ data: mockFiles });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        prNumber: 123,
        type: 'fullContent',
      });

      expect(result.pull_requests).toHaveLength(1);
      expect(mockOctokit.rest.pulls.listFiles).toHaveBeenCalledWith({
        owner: 'test',
        repo: 'repo',
        pull_number: 123,
        per_page: 100,
        page: 1,
      });
    });

    it('should fetch comments when withComments is true', async () => {
      const mockPR = {
        number: 123,
        title: 'Test PR',
        state: 'open',
        draft: false,
        user: { login: 'testuser' },
        labels: [],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/123',
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
        body: 'Test description',
      };

      const mockComments = [
        {
          id: 1,
          user: { login: 'commenter1' },
          body: 'Great work!',
          created_at: '2023-01-03T00:00:00Z',
          updated_at: '2023-01-03T00:00:00Z',
        },
      ];

      mockOctokit.rest.pulls.get.mockResolvedValue({ data: mockPR });
      mockOctokit.rest.issues.listComments.mockResolvedValue({
        data: mockComments,
      });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        prNumber: 123,
        withComments: true,
      });

      expect(result.pull_requests).toHaveLength(1);
      expect(mockOctokit.rest.issues.listComments).toHaveBeenCalledWith({
        owner: 'test',
        repo: 'repo',
        issue_number: 123,
      });
    });

    it('should sanitize PR title and body', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(false);

      const mockPR = {
        number: 123,
        title: 'Test PR with secret',
        state: 'open',
        draft: false,
        user: { login: 'testuser' },
        labels: [],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/123',
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
        body: 'Description with token: ghp_secrettoken123',
      };

      mockOctokit.rest.pulls.list.mockResolvedValue({ data: [mockPR] });

      mockContentSanitizer.sanitizeContent.mockImplementation(
        (content: string) => ({
          content: content.replace('ghp_secrettoken123', '[REDACTED]'),
          warnings: ['Secret detected'],
        })
      );

      await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
      });

      expect(mockContentSanitizer.sanitizeContent).toHaveBeenCalledWith(
        'Test PR with secret'
      );
      expect(mockContentSanitizer.sanitizeContent).toHaveBeenCalledWith(
        'Description with token: ghp_secrettoken123'
      );
    });

    it('should handle REST API with head/base filters', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(false);

      mockOctokit.rest.pulls.list.mockResolvedValue({ data: [] });

      await searchGitHubPullRequestsAPI({
        owner: 'test',
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

    it('should use correct sort parameter for Search API', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(true);
      mockBuildPullRequestSearchQuery.mockReturnValue('repo:test/repo is:pr');

      mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
        data: {
          total_count: 0,
          incomplete_results: false,
          items: [],
        },
      });

      await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        sort: 'updated',
      });

      expect(
        mockOctokit.rest.search.issuesAndPullRequests
      ).toHaveBeenCalledWith(
        expect.objectContaining({
          sort: 'updated',
        })
      );
    });

    it('should correctly set merged status from PR details when using Search API', async () => {
      // This test verifies the fix for the bug where merged: true filter
      // returned PRs with merged: false because merged_at wasn't being
      // copied from PR details to the result
      mockShouldUseSearchForPRs.mockReturnValue(true);
      mockBuildPullRequestSearchQuery.mockReturnValue(
        'repo:test/repo is:pr is:merged'
      );

      const mockSearchResult = {
        total_count: 1,
        incomplete_results: false,
        items: [
          {
            number: 100,
            title: 'Merged PR',
            state: 'closed',
            user: { login: 'user1' },
            labels: [],
            created_at: '2023-01-01T00:00:00Z',
            updated_at: '2023-01-10T00:00:00Z',
            closed_at: '2023-01-10T00:00:00Z',
            html_url: 'https://github.com/test/repo/pull/100',
            body: 'This PR was merged',
            pull_request: {},
            // Note: Search API doesn't return merged_at
          },
        ],
      };

      mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
        data: mockSearchResult,
      });

      // PR details include merged_at - this is where we get the merged status
      mockOctokit.rest.pulls.get.mockResolvedValue({
        data: {
          number: 100,
          head: { ref: 'feature', sha: 'abc' },
          base: { ref: 'main', sha: 'def' },
          draft: false,
          merged_at: '2023-01-10T00:00:00Z', // This should be copied to result
        },
      });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        merged: true,
        state: 'closed',
      });

      expect(result.pull_requests).toHaveLength(1);
      expect(result.pull_requests?.[0]?.merged).toBe(true);
      expect(result.pull_requests?.[0]?.merged_at).toBe('2023-01-10T00:00:00Z');
    });

    it('should correctly set merged: false when PR is closed but not merged', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(true);
      mockBuildPullRequestSearchQuery.mockReturnValue(
        'repo:test/repo is:pr is:unmerged'
      );

      const mockSearchResult = {
        total_count: 1,
        incomplete_results: false,
        items: [
          {
            number: 101,
            title: 'Closed but not merged PR',
            state: 'closed',
            user: { login: 'user1' },
            labels: [],
            created_at: '2023-01-01T00:00:00Z',
            updated_at: '2023-01-10T00:00:00Z',
            closed_at: '2023-01-10T00:00:00Z',
            html_url: 'https://github.com/test/repo/pull/101',
            body: 'This PR was closed without merging',
            pull_request: {},
          },
        ],
      };

      mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
        data: mockSearchResult,
      });

      // PR details without merged_at - closed but not merged
      mockOctokit.rest.pulls.get.mockResolvedValue({
        data: {
          number: 101,
          head: { ref: 'feature', sha: 'abc' },
          base: { ref: 'main', sha: 'def' },
          draft: false,
          merged_at: null, // Not merged
        },
      });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        merged: false,
        state: 'closed',
      });

      expect(result.pull_requests).toHaveLength(1);
      expect(result.pull_requests?.[0]?.merged).toBe(false);
      expect(result.pull_requests?.[0]?.merged_at).toBeUndefined();
    });

    it('should filter out non-PR items from search results', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(true);
      mockBuildPullRequestSearchQuery.mockReturnValue('repo:test/repo is:pr');

      const mockSearchResult = {
        total_count: 2,
        incomplete_results: false,
        items: [
          {
            number: 1,
            title: 'Issue 1',
            state: 'open',
            user: { login: 'user1' },
            labels: [],
            created_at: '2023-01-01T00:00:00Z',
            updated_at: '2023-01-02T00:00:00Z',
            closed_at: null,
            html_url: 'https://github.com/test/repo/issues/1',
            body: 'Issue description',
            // No pull_request field - this is an issue, not a PR
          },
          {
            number: 2,
            title: 'PR 2',
            state: 'open',
            user: { login: 'user2' },
            labels: [],
            created_at: '2023-01-01T00:00:00Z',
            updated_at: '2023-01-02T00:00:00Z',
            closed_at: null,
            html_url: 'https://github.com/test/repo/pull/2',
            body: 'PR description',
            pull_request: {}, // This is a PR
          },
        ],
      };

      mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
        data: mockSearchResult,
      });

      mockOctokit.rest.pulls.get.mockResolvedValue({
        data: {
          number: 2,
          head: { ref: 'feature', sha: 'abc' },
          base: { ref: 'main', sha: 'def' },
          draft: false,
        },
      });

      const result = await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
      });

      // Should only include the PR, not the issue
      expect(result.pull_requests).toHaveLength(1);
      expect(result.pull_requests?.[0]?.number).toBe(2);
    });

    it('should handle limit parameter correctly', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(false);
      mockOctokit.rest.pulls.list.mockResolvedValue({ data: [] });

      await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        limit: 50,
      });

      expect(mockOctokit.rest.pulls.list).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 50,
        })
      );
    });

    it('should respect max limit of 100', async () => {
      mockShouldUseSearchForPRs.mockReturnValue(false);
      mockOctokit.rest.pulls.list.mockResolvedValue({ data: [] });

      await searchGitHubPullRequestsAPI({
        owner: 'test',
        repo: 'repo',
        limit: 500, // Should be capped at 100
      });

      expect(mockOctokit.rest.pulls.list).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 100, // Capped at 100
        })
      );
    });
  });

  describe('fetchGitHubPullRequestByNumberAPI', () => {
    it('should fetch specific PR with caching', async () => {
      const mockPR = {
        number: 456,
        title: 'Specific PR',
        state: 'closed',
        draft: false,
        merged_at: '2023-01-10T00:00:00Z',
        user: { login: 'testuser' },
        labels: [],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: '2023-01-10T00:00:00Z',
        html_url: 'https://github.com/test/repo/pull/456',
        head: { ref: 'feature', sha: 'abc123' },
        base: { ref: 'main', sha: 'def456' },
        body: 'PR description',
      };

      mockOctokit.rest.pulls.get.mockResolvedValue({ data: mockPR });

      const result = await fetchGitHubPullRequestByNumberAPI({
        owner: 'test',
        repo: 'repo',
        prNumber: 456,
      });

      expect(result.pull_requests).toHaveLength(1);
      expect(result.pull_requests?.[0]?.number).toBe(456);
      expect(result.pull_requests?.[0]?.merged).toBe(true);
      expect(mockOctokit.rest.pulls.get).toHaveBeenCalledWith({
        owner: 'test',
        repo: 'repo',
        pull_number: 456,
      });
    });

    it('should return error when PR not found', async () => {
      mockOctokit.rest.pulls.get.mockRejectedValue(new Error('Not Found'));
      mockHandleGitHubAPIError.mockReturnValue({
        error: 'Not Found',
        type: 'http',
      });

      const result = await fetchGitHubPullRequestByNumberAPI({
        owner: 'test',
        repo: 'repo',
        prNumber: 999,
      });

      expect(result.error).toContain('Failed to fetch pull request #999');
      expect(result.hints).toContain(
        'Verify that pull request #999 exists in test/repo'
      );
    });

    it('should return error when prNumber is missing', async () => {
      const result = await fetchGitHubPullRequestByNumberAPI({
        owner: 'test',
        repo: 'repo',
      });

      expect(result.error).toContain(
        'Owner, repo, and prNumber are required parameters'
      );
      expect(result.pull_requests).toHaveLength(0);
    });

    it('should return error when owner or repo are arrays', async () => {
      const result = await fetchGitHubPullRequestByNumberAPI({
        owner: ['test'],
        repo: 'repo',
        prNumber: 123,
      });

      expect(result.error).toContain('Owner and repo must be single values');
      expect(result.pull_requests).toHaveLength(0);
    });
    it('should configure caching correctly', async () => {
      const mockPR = { number: 456, state: 'open' };
      mockOctokit.rest.pulls.get.mockResolvedValue({ data: mockPR });

      await fetchGitHubPullRequestByNumberAPI({
        owner: 'test',
        repo: 'repo',
        prNumber: 456,
      });

      expect(mockWithDataCache).toHaveBeenCalledWith(
        expect.any(String),
        expect.any(Function),
        expect.objectContaining({
          shouldCache: expect.any(Function),
        })
      );

      // Verify the shouldCache predicate
      const cacheOptions = mockWithDataCache.mock.calls[0]?.[2] as
        | { shouldCache: (data: unknown) => boolean }
        | undefined;
      expect(cacheOptions?.shouldCache({ error: 'Some error' })).toBe(false);
      expect(cacheOptions?.shouldCache({ pull_requests: [] })).toBe(true);
    });
  });

  describe('transformPullRequestItemFromREST', () => {
    it('should transform REST API PR item correctly', async () => {
      const mockItem: MockPRItem = {
        number: 789,
        title: 'Transform Test',
        state: 'open',
        draft: true,
        user: { login: 'testuser' } as PullRequestSimple['user'],
        labels: [
          { name: 'bug' },
          { name: 'enhancement' },
        ] as PullRequestSimple['labels'],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/789',
        head: { ref: 'feature', sha: 'abc123' } as PullRequestSimple['head'],
        base: { ref: 'main', sha: 'def456' } as PullRequestSimple['base'],
        body: 'Test body',
      };

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo' },
        mockOctokit
      );

      expect(result.number).toBe(789);
      expect(result.title).toBe('Transform Test');
      expect(result.state).toBe('open');
      expect(result.draft).toBe(true);
      expect(result.author).toBe('testuser');
      expect(result.labels).toEqual(['bug', 'enhancement']);
    });

    it('should fetch file changes when type is fullContent', async () => {
      const mockItem: MockPRItem = {
        number: 790,
        title: 'With Content',
        state: 'open',
        draft: false,
        user: { login: 'testuser' } as PullRequestSimple['user'],
        labels: [] as PullRequestSimple['labels'],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/790',
        head: { ref: 'feature', sha: 'abc123' } as PullRequestSimple['head'],
        base: { ref: 'main', sha: 'def456' } as PullRequestSimple['base'],
        body: 'Test body',
      };

      const mockFiles = [
        {
          filename: 'src/test.ts',
          status: 'modified',
          additions: 20,
          deletions: 10,
          changes: 30,
          patch: '@@ -1,10 +1,20 @@...',
        },
      ];

      mockOctokit.rest.pulls.listFiles.mockResolvedValue({ data: mockFiles });

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo', type: 'fullContent' },
        mockOctokit
      );

      expect(result.file_changes).toBeDefined();
      expect(result.file_changes?.total_count).toBe(1);
      expect(mockOctokit.rest.pulls.listFiles).toHaveBeenCalled();
    });

    it('should fetch comments when withComments is true', async () => {
      const mockItem: MockPRItem = {
        number: 791,
        title: 'With Comments',
        state: 'open',
        draft: false,
        user: { login: 'testuser' } as PullRequestSimple['user'],
        labels: [] as PullRequestSimple['labels'],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/791',
        head: { ref: 'feature', sha: 'abc123' } as PullRequestSimple['head'],
        base: { ref: 'main', sha: 'def456' } as PullRequestSimple['base'],
        body: 'Test body',
      };

      const mockComments = [
        {
          id: 1,
          user: { login: 'reviewer1' },
          body: 'Looks good!',
          created_at: '2023-01-03T00:00:00Z',
          updated_at: '2023-01-03T00:00:00Z',
        },
      ];

      mockOctokit.rest.issues.listComments.mockResolvedValue({
        data: mockComments,
      });

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo', withComments: true },
        mockOctokit
      );

      expect(result.comments).toBeDefined();
      expect(result.comments).toHaveLength(1);
      expect(mockOctokit.rest.issues.listComments).toHaveBeenCalled();
    });

    it('should handle sanitization warnings', async () => {
      const mockItem: MockPRItem = {
        number: 792,
        title: 'Secret in title: ghp_token123',
        state: 'open',
        draft: false,
        user: { login: 'testuser' } as PullRequestSimple['user'],
        labels: [] as PullRequestSimple['labels'],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/792',
        head: { ref: 'feature', sha: 'abc123' } as PullRequestSimple['head'],
        base: { ref: 'main', sha: 'def456' } as PullRequestSimple['base'],
        body: 'Body with secret: sk-openai123',
      };

      mockContentSanitizer.sanitizeContent.mockImplementation(
        (content: string) => ({
          content: content.replace(/ghp_\w+|sk-\w+/g, '[REDACTED]'),
          warnings: ['Secret detected'],
        })
      );

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo' },
        mockOctokit
      );

      expect(result._sanitization_warnings).toBeDefined();
      expect(result._sanitization_warnings).toContain('Secret detected');
    });

    it('should handle failed file changes fetch gracefully', async () => {
      const mockItem: MockPRItem = {
        number: 793,
        title: 'Failed File Fetch',
        state: 'open',
        draft: false,
        user: { login: 'testuser' } as PullRequestSimple['user'],
        labels: [] as PullRequestSimple['labels'],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/793',
        head: { ref: 'feature', sha: 'abc123' } as PullRequestSimple['head'],
        base: { ref: 'main', sha: 'def456' } as PullRequestSimple['base'],
        body: 'Test body',
      };

      mockOctokit.rest.pulls.listFiles.mockRejectedValue(
        new Error('Failed to fetch files')
      );

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo', type: 'fullContent' },
        mockOctokit
      );

      // Should complete without file_changes
      expect(result.number).toBe(793);
      expect(result.file_changes).toBeUndefined();
    });

    it('should handle failed comments fetch gracefully', async () => {
      const mockItem: MockPRItem = {
        number: 794,
        title: 'Failed Comments Fetch',
        state: 'open',
        draft: false,
        user: { login: 'testuser' } as PullRequestSimple['user'],
        labels: [] as PullRequestSimple['labels'],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/794',
        head: { ref: 'feature', sha: 'abc123' } as PullRequestSimple['head'],
        base: { ref: 'main', sha: 'def456' } as PullRequestSimple['base'],
        body: 'Test body',
      };

      mockOctokit.rest.issues.listComments.mockRejectedValue(
        new Error('Failed to fetch comments')
      );

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo', withComments: true },
        mockOctokit
      );

      // Should complete with empty comments array
      expect(result.number).toBe(794);
      expect(result.comments).toEqual([]);
    });

    it('should fetch commits when withCommits is true', async () => {
      const mockItem: MockPRItem = {
        number: 795,
        title: 'With Commits',
        state: 'open',
        draft: false,
        user: { login: 'testuser' } as PullRequestSimple['user'],
        labels: [] as PullRequestSimple['labels'],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/795',
        head: { ref: 'feature', sha: 'abc123' } as PullRequestSimple['head'],
        base: { ref: 'main', sha: 'def456' } as PullRequestSimple['base'],
        body: 'Test body',
      };

      const mockCommits = [
        {
          sha: 'commit1',
          commit: {
            message: 'feat: add feature',
            author: { name: 'Author', date: '2023-01-01T00:00:00Z' },
          },
        },
      ];

      const mockCommitFiles = [
        {
          filename: 'src/feature.ts',
          status: 'added',
          additions: 50,
          deletions: 0,
          changes: 50,
        },
      ];

      mockOctokit.rest.pulls.listCommits = vi
        .fn()
        .mockResolvedValue({ data: mockCommits });
      mockOctokit.rest.repos.getCommit.mockResolvedValue({
        data: { files: mockCommitFiles },
      });

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo', withCommits: true },
        mockOctokit
      );

      expect(result.commits).toBeDefined();
      expect(result.commits).toHaveLength(1);
      expect(mockOctokit.rest.pulls.listCommits).toHaveBeenCalledWith({
        owner: 'test',
        repo: 'repo',
        pull_number: 795,
      });
      expect(mockOctokit.rest.repos.getCommit).toHaveBeenCalledWith({
        owner: 'test',
        repo: 'repo',
        ref: 'commit1',
      });
    });

    it('should NOT fetch commits when withCommits is false/undefined', async () => {
      const mockItem: MockPRItem = {
        number: 796,
        title: 'Without Commits',
        state: 'open',
        draft: false,
        user: { login: 'testuser' } as PullRequestSimple['user'],
        labels: [] as PullRequestSimple['labels'],
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-02T00:00:00Z',
        closed_at: null,
        html_url: 'https://github.com/test/repo/pull/796',
        head: { ref: 'feature', sha: 'abc123' } as PullRequestSimple['head'],
        base: { ref: 'main', sha: 'def456' } as PullRequestSimple['base'],
        body: 'Test body',
      };

      // Mock listCommits to throw if called (or spy on it)
      mockOctokit.rest.pulls.listCommits = vi.fn();

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo' }, // withCommits undefined
        mockOctokit
      );

      expect(result.commits).toBeUndefined();
      expect(mockOctokit.rest.pulls.listCommits).not.toHaveBeenCalled();
    });

    it('should handle error when fetching commits gracefully', async () => {
      const mockItem: MockPRItem = {
        number: 797,
        title: 'Commit Error',
        state: 'open',
        user: { login: 'testuser' } as PullRequestSimple['user'],
        html_url: 'url',
      };

      // Mock listCommits to return invalid data to cause a runtime error in processing
      // This will bypass the internal try/catch in fetchPRCommitsAPI (which only catches API errors)
      // and cause fetchPRCommitsWithFiles to throw when trying to sort/map
      mockOctokit.rest.pulls.listCommits = vi.fn().mockResolvedValue({
        data: 'not-an-array', // This will cause .sort() or iteration to fail
      });

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo', withCommits: true },
        mockOctokit
      );

      expect(result.commits).toBeUndefined();
      expect(mockLogSessionError).toHaveBeenCalledWith(
        expect.any(String),
        expect.stringContaining('TypeError')
      );
    });

    it('should sort commits by date descending, handling missing dates', async () => {
      const mockItem: MockPRItem = {
        number: 798,
        title: 'Sort Test',
        state: 'open',
      };

      const mockCommits = [
        {
          sha: 'sha1',
          commit: {
            message: 'old',
            author: { date: '2023-01-01T00:00:00Z', name: 'User' },
          },
        },
        {
          sha: 'sha2',
          commit: {
            message: 'new',
            author: { date: '2023-01-02T00:00:00Z', name: 'User' },
          },
        },
        {
          sha: 'sha3',
          commit: {
            message: 'nodate',
            author: null, // No date case
          },
        },
      ];

      // sha2 (new) -> sha1 (old) -> sha3 (no date = 0)

      mockOctokit.rest.pulls.listCommits = vi
        .fn()
        .mockResolvedValue({ data: mockCommits });
      mockOctokit.rest.repos.getCommit.mockResolvedValue({
        data: { files: [] },
      });

      const result = await transformPullRequestItemFromREST(
        mockItem as PullRequestSimple,
        { owner: 'test', repo: 'repo', withCommits: true },
        mockOctokit
      );

      expect(result.commits).toBeDefined();

      expect(result.commits![0]!.sha).toBe('sha2');

      expect(result.commits![1]!.sha).toBe('sha1');

      expect(result.commits![2]!.sha).toBe('sha3');
    });
  });
});
