import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  clearAllCache,
  getCacheStats,
} from '../../../octocode-tools-core/src/utils/http/cache.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client';
import { searchGitHubCodeAPI } from '../../../octocode-tools-core/src/github/codeSearch';
import { searchGitHubReposAPI } from '../../../octocode-tools-core/src/github/repoSearch';
import { searchGitHubPullRequestsAPI } from '../../../octocode-tools-core/src/github/pullRequestSearch';
import { fetchGitHubFileContentAPI } from '../../../octocode-tools-core/src/github/fileContent.js';
import { viewGitHubRepositoryStructureAPI } from '../../../octocode-tools-core/src/github/repoStructure.js';

vi.mock('../../../octocode-tools-core/src/github/client');

const mockOctokit = {
  rest: {
    search: {
      code: vi.fn(),
      repos: vi.fn(),
      issuesAndPullRequests: vi.fn(),
    },
    repos: {
      getContent: vi.fn(),
      get: vi.fn(),
      listCommits: vi.fn(),
    },
    pulls: {
      get: vi.fn(),
      listCommits: vi.fn(),
      listFiles: vi.fn(),
      listReviewComments: vi.fn(),
      listReviews: vi.fn(),
    },
  },
};

describe('GitHub API Caching', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    vi.mocked(getOctokit).mockResolvedValue(
      mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
    );
  });

  describe('ghSearchCode caching', () => {
    beforeEach(() => {
      mockOctokit.rest.search.code.mockResolvedValue({
        data: {
          total_count: 1,
          incomplete_results: false,
          items: [
            {
              name: 'test.ts',
              path: 'src/test.ts',
              sha: 'abc123',
              html_url: 'https://github.com/owner/repo/blob/main/src/test.ts',
              repository: {
                full_name: 'owner/repo',
                owner: { login: 'owner' },
                name: 'repo',
                html_url: 'https://github.com/owner/repo',
              },
              text_matches: [],
            },
          ],
        },
      });
    });

    it('should cache code search results and return cached on second call', async () => {
      const params = {
        keywords: ['useState'],
        owner: 'facebook',
        repo: 'react',
        mainResearchGoal: 'Find hooks',
        researchGoal: 'Find useState',
        reasoning: 'Testing',
      };

      await searchGitHubCodeAPI(params);
      const statsBefore = getCacheStats();

      await searchGitHubCodeAPI(params);
      const statsAfter = getCacheStats();

      expect(statsAfter.hits).toBe(statsBefore.hits + 1);
      expect(mockOctokit.rest.search.code).toHaveBeenCalledTimes(1);
    });

    it('should hit cache when only context params differ', async () => {
      const params1 = {
        keywords: ['useState'],
        owner: 'facebook',
        repo: 'react',
        mainResearchGoal: 'Goal 1',
        researchGoal: 'Research 1',
        reasoning: 'Reason 1',
      };

      const params2 = {
        keywords: ['useState'],
        owner: 'facebook',
        repo: 'react',
        mainResearchGoal: 'DIFFERENT GOAL',
        researchGoal: 'DIFFERENT RESEARCH',
        reasoning: 'DIFFERENT REASON',
      };

      await searchGitHubCodeAPI(params1);
      await searchGitHubCodeAPI(params2);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledTimes(1);
    });

    it('should miss cache when API params differ', async () => {
      const params1 = {
        keywords: ['useState'],
        owner: 'facebook',
        repo: 'react',
        mainResearchGoal: 'Goal',
        researchGoal: 'Research',
        reasoning: 'Reason',
      };

      const params2 = {
        keywords: ['useEffect'],
        owner: 'facebook',
        repo: 'react',
        mainResearchGoal: 'Goal',
        researchGoal: 'Research',
        reasoning: 'Reason',
      };

      await searchGitHubCodeAPI(params1);
      await searchGitHubCodeAPI(params2);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledTimes(2);
    });
  });

  describe('ghSearchRepos caching', () => {
    beforeEach(() => {
      mockOctokit.rest.search.repos.mockResolvedValue({
        data: {
          total_count: 1,
          incomplete_results: false,
          items: [
            {
              id: 1,
              name: 'react',
              full_name: 'facebook/react',
              owner: { login: 'facebook' },
              html_url: 'https://github.com/facebook/react',
              description: 'React library',
              stargazers_count: 200000,
              forks_count: 40000,
              language: 'JavaScript',
              topics: ['javascript', 'react'],
              updated_at: '2024-01-01T00:00:00Z',
              pushed_at: '2024-01-01T00:00:00Z',
              created_at: '2013-01-01T00:00:00Z',
              default_branch: 'main',
              archived: false,
              license: { spdx_id: 'MIT' },
            },
          ],
        },
      });
    });

    it('should cache repo search results and return cached on second call', async () => {
      const params = {
        keywords: ['react'],
        mainResearchGoal: 'Find repos',
        researchGoal: 'Find react',
        reasoning: 'Testing',
      };

      await searchGitHubReposAPI(params);
      await searchGitHubReposAPI(params);

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledTimes(1);
    });

    it('should hit cache when only context params differ', async () => {
      const params1 = {
        keywords: ['react'],
        stars: '>1000',
        mainResearchGoal: 'Goal 1',
        researchGoal: 'Research 1',
        reasoning: 'Reason 1',
      };

      const params2 = {
        keywords: ['react'],
        stars: '>1000',
        mainResearchGoal: 'DIFFERENT',
        researchGoal: 'DIFFERENT',
        reasoning: 'DIFFERENT',
      };

      await searchGitHubReposAPI(params1);
      await searchGitHubReposAPI(params2);

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledTimes(1);
    });
  });

  describe('ghHistoryResearch caching', () => {
    beforeEach(() => {
      mockOctokit.rest.search.issuesAndPullRequests.mockResolvedValue({
        data: {
          total_count: 1,
          incomplete_results: false,
          items: [
            {
              number: 1,
              title: 'Test PR',
              state: 'open',
              user: { login: 'user' },
              html_url: 'https://github.com/owner/repo/pull/1',
              created_at: '2024-01-01T00:00:00Z',
              updated_at: '2024-01-01T00:00:00Z',
              labels: [],
              pull_request: {
                url: 'https://api.github.com/repos/owner/repo/pulls/1',
              },
            },
          ],
        },
      });
    });

    it('should cache PR search results and return cached on second call', async () => {
      const params = {
        query: 'fix bug',
        owner: 'facebook',
        repo: 'react',
        mainResearchGoal: 'Find PRs',
        researchGoal: 'Find bug fixes',
        reasoning: 'Testing',
      };

      await searchGitHubPullRequestsAPI(params);
      await searchGitHubPullRequestsAPI(params);

      expect(
        mockOctokit.rest.search.issuesAndPullRequests
      ).toHaveBeenCalledTimes(1);
    });

    it('should hit cache when only context params differ', async () => {
      const params1 = {
        query: 'fix',
        owner: 'facebook',
        repo: 'react',
        state: 'closed' as const,
        mainResearchGoal: 'Goal 1',
        researchGoal: 'Research 1',
        reasoning: 'Reason 1',
      };

      const params2 = {
        query: 'fix',
        owner: 'facebook',
        repo: 'react',
        state: 'closed' as const,
        mainResearchGoal: 'DIFFERENT',
        researchGoal: 'DIFFERENT',
        reasoning: 'DIFFERENT',
      };

      await searchGitHubPullRequestsAPI(params1);
      await searchGitHubPullRequestsAPI(params2);

      expect(
        mockOctokit.rest.search.issuesAndPullRequests
      ).toHaveBeenCalledTimes(1);
    });
  });

  describe('ghGetFileContent caching', () => {
    const fileContent = 'line 1\nline 2\nline 3\nline 4\nline 5';

    beforeEach(() => {
      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from(fileContent).toString('base64'),
          size: fileContent.length,
          sha: 'abc123',
          name: 'test.ts',
          path: 'src/test.ts',
        },
      });
      mockOctokit.rest.repos.listCommits.mockResolvedValue({ data: [] });
    });

    it('should cache file content and return cached on second call', async () => {
      const params = {
        owner: 'facebook',
        repo: 'react',
        path: 'src/index.ts',
        mainResearchGoal: 'Read file',
        researchGoal: 'Get content',
        reasoning: 'Testing',
      };

      await fetchGitHubFileContentAPI(params);
      await fetchGitHubFileContentAPI(params);

      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(1);
    });

    it('should hit cache when only line range params differ', async () => {
      const params1 = {
        owner: 'facebook',
        repo: 'react',
        path: 'src/index.ts',
        startLine: 1,
        endLine: 2,
        mainResearchGoal: 'Read file',
        researchGoal: 'Get content',
        reasoning: 'Testing',
      };

      const params2 = {
        owner: 'facebook',
        repo: 'react',
        path: 'src/index.ts',
        startLine: 3,
        endLine: 5,
        mainResearchGoal: 'Read file',
        researchGoal: 'Get content',
        reasoning: 'Testing',
      };

      await fetchGitHubFileContentAPI(params1);
      await fetchGitHubFileContentAPI(params2);

      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(1);
    });

    it('should hit cache when only matchString params differ', async () => {
      const params1 = {
        owner: 'facebook',
        repo: 'react',
        path: 'src/index.ts',
        matchString: 'line 1',
        mainResearchGoal: 'Read file',
        researchGoal: 'Get content',
        reasoning: 'Testing',
      };

      const params2 = {
        owner: 'facebook',
        repo: 'react',
        path: 'src/index.ts',
        matchString: 'line 3',
        mainResearchGoal: 'Read file',
        researchGoal: 'Get content',
        reasoning: 'Testing',
      };

      await fetchGitHubFileContentAPI(params1);
      await fetchGitHubFileContentAPI(params2);

      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(1);
    });

    it('should miss cache when file path differs', async () => {
      const params1 = {
        owner: 'facebook',
        repo: 'react',
        path: 'src/index.ts',
        mainResearchGoal: 'Read file',
        researchGoal: 'Get content',
        reasoning: 'Testing',
      };

      const params2 = {
        owner: 'facebook',
        repo: 'react',
        path: 'src/other.ts',
        mainResearchGoal: 'Read file',
        researchGoal: 'Get content',
        reasoning: 'Testing',
      };

      await fetchGitHubFileContentAPI(params1);
      await fetchGitHubFileContentAPI(params2);

      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(2);
    });
  });

  describe('ghViewRepoStructure caching', () => {
    beforeEach(() => {
      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: [
          { type: 'file', name: 'index.ts', path: 'src/index.ts', sha: 'a1' },
          { type: 'file', name: 'utils.ts', path: 'src/utils.ts', sha: 'a2' },
          {
            type: 'dir',
            name: 'components',
            path: 'src/components',
            sha: 'a3',
          },
        ],
      });
      mockOctokit.rest.repos.get.mockResolvedValue({
        data: { default_branch: 'main' },
      });
    });

    it('should cache repo structure and return cached on second call', async () => {
      const params = {
        owner: 'facebook',
        repo: 'react',
        branch: 'main',
        path: 'src',
        maxDepth: 1,
        mainResearchGoal: 'View structure',
        researchGoal: 'Get files',
        reasoning: 'Testing',
      };

      await viewGitHubRepositoryStructureAPI(params);
      await viewGitHubRepositoryStructureAPI(params);

      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(1);
    });

    it('should hit cache when only pagination params differ', async () => {
      const params1 = {
        owner: 'facebook',
        repo: 'react',
        branch: 'main',
        path: 'src',
        maxDepth: 1,
        itemsPerPage: 2,
        page: 1,
        mainResearchGoal: 'View structure',
        researchGoal: 'Get files',
        reasoning: 'Testing',
      };

      const params2 = {
        owner: 'facebook',
        repo: 'react',
        branch: 'main',
        path: 'src',
        maxDepth: 1,
        itemsPerPage: 2,
        page: 2,
        mainResearchGoal: 'View structure',
        researchGoal: 'Get files',
        reasoning: 'Testing',
      };

      await viewGitHubRepositoryStructureAPI(params1);
      await viewGitHubRepositoryStructureAPI(params2);

      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(1);
    });

    it('returns the correct page per page from one cached tree (not a stale fixed page)', async () => {
      const base = {
        owner: 'facebook',
        repo: 'react',
        branch: 'main',
        path: 'src',
        maxDepth: 1,
        itemsPerPage: 2,
        mainResearchGoal: 'View structure',
        researchGoal: 'Get files',
        reasoning: 'Testing',
      };

      const r1 = (await viewGitHubRepositoryStructureAPI({
        ...base,
        page: 1,
      })) as {
        pagination?: { currentPage?: number; totalEntries?: number };
        structure?: unknown;
      };
      const r2 = (await viewGitHubRepositoryStructureAPI({
        ...base,
        page: 2,
      })) as {
        pagination?: { currentPage?: number; totalEntries?: number };
        structure?: unknown;
      };

      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(1);
      expect(r1.pagination?.currentPage).toBe(1);
      expect(r2.pagination?.currentPage).toBe(2);
      expect(r1.pagination?.totalEntries).toBe(3);
      expect(r2.pagination?.totalEntries).toBe(3);
      expect(JSON.stringify(r1.structure)).not.toBe(
        JSON.stringify(r2.structure)
      );
    });

    it('should miss cache when path differs', async () => {
      const params1 = {
        owner: 'facebook',
        repo: 'react',
        branch: 'main',
        path: 'src',
        maxDepth: 1,
        mainResearchGoal: 'View structure',
        researchGoal: 'Get files',
        reasoning: 'Testing',
      };

      const params2 = {
        owner: 'facebook',
        repo: 'react',
        branch: 'main',
        path: 'packages',
        maxDepth: 1,
        mainResearchGoal: 'View structure',
        researchGoal: 'Get files',
        reasoning: 'Testing',
      };

      await viewGitHubRepositoryStructureAPI(params1);
      await viewGitHubRepositoryStructureAPI(params2);

      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(2);
    });
  });

  describe('Cache isolation between different APIs', () => {
    it('should not share cache between different API types', async () => {
      mockOctokit.rest.search.code.mockResolvedValue({
        data: { total_count: 0, incomplete_results: false, items: [] },
      });
      mockOctokit.rest.search.repos.mockResolvedValue({
        data: { total_count: 0, incomplete_results: false, items: [] },
      });

      await searchGitHubCodeAPI({
        keywords: ['test'],
        owner: 'owner',
        repo: 'repo',
        mainResearchGoal: 'Goal',
        researchGoal: 'Research',
        reasoning: 'Reason',
      });

      await searchGitHubReposAPI({
        keywords: ['test'],
        owner: 'owner',
        mainResearchGoal: 'Goal',
        researchGoal: 'Research',
        reasoning: 'Reason',
      });

      expect(mockOctokit.rest.search.code).toHaveBeenCalledTimes(1);
      expect(mockOctokit.rest.search.repos).toHaveBeenCalledTimes(1);
    });
  });
});
