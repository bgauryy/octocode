import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  parseGitHubProjectId,
  transformFileContentResult,
  getFileContent,
} from '../../../../octocode-tools-core/src/providers/github/githubContent.js';
import {
  parseGitHubProjectId as parseGitHubProjectIdStructure,
  transformRepoStructureResult,
  getRepoStructure,
} from '../../../../octocode-tools-core/src/providers/github/githubStructure.js';
import {
  parseGitHubProjectId as parseGitHubProjectIdSearch,
  transformCodeSearchResult,
  transformRepoSearchResult,
  searchCode,
  searchRepos,
} from '../../../../octocode-tools-core/src/providers/github/githubSearch.js';
import {
  parseGitHubProjectId as parseGitHubProjectIdPR,
  transformPullRequestResult,
  searchPullRequests,
} from '../../../../octocode-tools-core/src/providers/github/githubPullRequests.js';
import type {
  FileContentQuery,
  RepoStructureQuery,
  CodeSearchQuery,
  RepoSearchQuery,
  PullRequestQuery,
} from '../../../../octocode-tools-core/src/providers/types.js';

vi.mock('../../../../octocode-tools-core/src/github/fileContent.js', () => ({
  fetchGitHubFileContentAPI: vi.fn(),
}));

vi.mock('../../../../octocode-tools-core/src/github/repoStructure.js', () => ({
  viewGitHubRepositoryStructureAPI: vi.fn(),
}));

vi.mock('../../../../octocode-tools-core/src/github/codeSearch.js', () => ({
  searchGitHubCodeAPI: vi.fn(),
}));

vi.mock('../../../../octocode-tools-core/src/github/repoSearch.js', () => ({
  searchGitHubReposAPI: vi.fn(),
}));

vi.mock(
  '../../../../octocode-tools-core/src/github/pullRequestSearch.js',
  () => ({
    searchGitHubPullRequestsAPI: vi.fn(),
  })
);

import { fetchGitHubFileContentAPI } from '../../../../octocode-tools-core/src/github/fileContent.js';
import { viewGitHubRepositoryStructureAPI } from '../../../../octocode-tools-core/src/github/repoStructure.js';
import { searchGitHubCodeAPI } from '../../../../octocode-tools-core/src/github/codeSearch.js';
import { searchGitHubReposAPI } from '../../../../octocode-tools-core/src/github/repoSearch.js';
import { searchGitHubPullRequestsAPI } from '../../../../octocode-tools-core/src/github/pullRequestSearch.js';

const mockFetchGitHubFileContentAPI = vi.mocked(fetchGitHubFileContentAPI);
const mockViewGitHubRepositoryStructureAPI = vi.mocked(
  viewGitHubRepositoryStructureAPI
);
const mockSearchGitHubCodeAPI = vi.mocked(searchGitHubCodeAPI);
const mockSearchGitHubReposAPI = vi.mocked(searchGitHubReposAPI);
const mockSearchGitHubPullRequestsAPI = vi.mocked(searchGitHubPullRequestsAPI);

describe('GitHub Provider Delegates', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('githubContent', () => {
    describe('parseGitHubProjectId', () => {
      it('should return undefined owner and repo when projectId is undefined', () => {
        const result = parseGitHubProjectId(undefined);
        expect(result).toEqual({ owner: undefined, repo: undefined });
      });

      it('should return undefined owner and repo when projectId is empty string', () => {
        const result = parseGitHubProjectId('');
        expect(result).toEqual({ owner: undefined, repo: undefined });
      });

      it('should parse valid owner/repo format', () => {
        const result = parseGitHubProjectId('owner/repo');
        expect(result).toEqual({ owner: 'owner', repo: 'repo' });
      });

      it('should throw error for projectId without slash', () => {
        expect(() => parseGitHubProjectId('no-slash')).toThrow(
          "Invalid GitHub projectId format: 'no-slash'. Expected 'owner/repo'."
        );
      });

      it('should throw error for projectId with empty owner', () => {
        expect(() => parseGitHubProjectId('/repo')).toThrow(
          "Invalid GitHub projectId format: '/repo'. Expected 'owner/repo'."
        );
      });

      it('should throw error for projectId with empty repo', () => {
        expect(() => parseGitHubProjectId('owner/')).toThrow(
          "Invalid GitHub projectId format: 'owner/'. Expected 'owner/repo'."
        );
      });

      it('should throw error for projectId with too many slashes', () => {
        expect(() => parseGitHubProjectId('org/owner/repo')).toThrow(
          "Invalid GitHub projectId format: 'org/owner/repo'. Expected 'owner/repo'."
        );
      });
    });

    describe('transformFileContentResult', () => {
      it('should transform result with missing path field', () => {
        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'fallback/path.ts',
        };
        const data = {
          content: 'test content',
        };
        const result = transformFileContentResult(data as any, query);
        expect(result.path).toBe('fallback/path.ts');
      });

      it('should transform result with missing content field', () => {
        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'test.ts',
        };
        const data = {
          path: 'test.ts',
        };
        const result = transformFileContentResult(data as any, query);
        expect(result.content).toBe('');
      });

      it('should transform result with missing branch field', () => {
        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'test.ts',
          ref: 'main',
        };
        const data = {
          path: 'test.ts',
          content: 'content',
        };
        const result = transformFileContentResult(data as any, query);
        expect(result.ref).toBe('main');
      });

      it('should transform result with missing ref in query', () => {
        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'test.ts',
        };
        const data = {
          path: 'test.ts',
          content: 'content',
        };
        const result = transformFileContentResult(data as any, query);
        expect(result.ref).toBe('');
      });

      it('surfaces a noMatches warning on a matchString miss instead of silent empty (F2)', () => {
        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'test.ts',
          matchString: 'NO_SUCH_ANCHOR_ZZ',
        };
        const data = {
          path: 'test.ts',
          content: '',
          totalLines: 42,
          matchNotFound: true,
          searchedFor: 'NO_SUCH_ANCHOR_ZZ',
        };
        const result = transformFileContentResult(data as any, query);
        expect(result.content).toBe('');
        expect(
          result.warnings?.some(w =>
            /no matches for "NO_SUCH_ANCHOR_ZZ".*42 lines scanned/i.test(w)
          )
        ).toBe(true);
      });
    });

    describe('getFileContent', () => {
      it('should return 400 error when projectId is undefined', async () => {
        const query: FileContentQuery = {
          path: 'test.ts',
        } as any;
        const result = await getFileContent(query);
        expect(result.error).toBe('Project ID is required for file content');
        expect(result.status).toBe(400);
        expect(result.provider).toBe('github');
      });

      it('should return 400 error when owner is missing', async () => {
        const customParse = vi.fn(() => ({ owner: undefined, repo: 'repo' }));
        const query: FileContentQuery = {
          projectId: 'invalid',
          path: 'test.ts',
        };
        const result = await getFileContent(query, undefined, customParse);
        expect(result.error).toBe('Project ID is required for file content');
        expect(result.status).toBe(400);
      });

      it('should return 400 error when repo is missing', async () => {
        const customParse = vi.fn(() => ({ owner: 'owner', repo: undefined }));
        const query: FileContentQuery = {
          projectId: 'invalid',
          path: 'test.ts',
        };
        const result = await getFileContent(query, undefined, customParse);
        expect(result.error).toBe('Project ID is required for file content');
        expect(result.status).toBe(400);
      });

      it('should return error when API returns error', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          error: 'File not found',
          status: 404,
          type: 'http',
          hints: ['Check the path'],
        });

        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'nonexistent.ts',
        };
        const result = await getFileContent(query);

        expect(result.error).toBe('File not found');
        expect(result.status).toBe(404);
        expect(result.hints).toEqual(['Check the path']);
      });

      it('should return error with default status 500 when API error has no status', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          error: 'Unknown error',
          type: 'http',
        });

        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'test.ts',
        };
        const result = await getFileContent(query);

        expect(result.error).toBe('Unknown error');
        expect(result.status).toBe(500);
      });

      it('should return 500 error when API returns no data', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          status: 200,
        } as any);

        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'test.ts',
        };
        const result = await getFileContent(query);

        expect(result.error).toBe('No data returned from GitHub API');
        expect(result.status).toBe(500);
      });

      it('should return success when API returns data', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          data: {
            path: 'test.ts',
            content: 'export const test = 1;',
            branch: 'main',
          },
          status: 200,
        });

        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'test.ts',
        };
        const result = await getFileContent(query);

        expect(result.status).toBe(200);
        expect(result.data).toBeDefined();
        expect(result.data?.path).toBe('test.ts');
        expect(result.data?.content).toBe('export const test = 1;');
      });
    });
  });

  describe('githubStructure', () => {
    describe('parseGitHubProjectId', () => {
      it('should return undefined owner and repo when projectId is undefined', () => {
        const result = parseGitHubProjectIdStructure(undefined);
        expect(result).toEqual({ owner: undefined, repo: undefined });
      });

      it('should throw error for invalid format', () => {
        expect(() => parseGitHubProjectIdStructure('invalid')).toThrow(
          "Invalid GitHub projectId format: 'invalid'. Expected 'owner/repo'."
        );
      });
    });

    describe('transformRepoStructureResult', () => {
      it('should transform result with missing summary fields', () => {
        const data = {
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
          path: '/',
          structure: {},
        };
        const result = transformRepoStructureResult(data as any);
        expect(result.summary).toEqual({
          totalFiles: 0,
          totalFolders: 0,
          truncated: false,
        });
      });

      it('should transform result with partial summary', () => {
        const data = {
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
          path: '/',
          structure: {},
          summary: {
            totalFiles: 10,
          },
        };
        const result = transformRepoStructureResult(data as any);
        expect(result.summary).toEqual({
          totalFiles: 10,
          totalFolders: 0,
          truncated: false,
        });
      });

      it('should use empty string for missing branch', () => {
        const data = {
          owner: 'owner',
          repo: 'repo',
          path: '/',
          structure: {},
        };
        const result = transformRepoStructureResult(data as any);
        expect(result.branch).toBe('');
      });

      it('should use default path when missing', () => {
        const data = {
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
          structure: {},
        };
        const result = transformRepoStructureResult(data as any);
        expect(result.path).toBe('/');
      });
    });

    describe('getRepoStructure', () => {
      it('should return 400 error when projectId is missing', async () => {
        const query: RepoStructureQuery = {} as any;
        const result = await getRepoStructure(query);
        expect(result.error).toBe(
          'Project ID is required for repository structure'
        );
        expect(result.status).toBe(400);
      });

      it('should return error when API returns error', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          error: 'Repository not found',
          status: 404,
        });

        const query: RepoStructureQuery = {
          projectId: 'nonexistent/repo',
        };
        const result = await getRepoStructure(query);

        expect(result.error).toBe('Repository not found');
        expect(result.status).toBe(404);
      });

      it('should handle error with object toString', async () => {
        const errorObj = {
          toString: () => 'Object error',
        };
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          error: errorObj as any,
          status: 500,
        });

        const query: RepoStructureQuery = {
          projectId: 'owner/repo',
        };
        const result = await getRepoStructure(query);

        expect(result.error).toBe('Object error');
        expect(result.status).toBe(500);
      });

      it('should return success when API returns data', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
          path: '/',
          structure: {},
          summary: {
            totalFiles: 0,
            totalFolders: 0,
            truncated: false,
          } as any,
        } as any);

        const query: RepoStructureQuery = {
          projectId: 'owner/repo',
        };
        const result = await getRepoStructure(query);

        expect(result.status).toBe(200);
        expect(result.data).toBeDefined();
      });
    });
  });

  describe('githubSearch', () => {
    describe('parseGitHubProjectId', () => {
      it('should return undefined owner and repo when projectId is undefined', () => {
        const result = parseGitHubProjectIdSearch(undefined);
        expect(result).toEqual({ owner: undefined, repo: undefined });
      });
    });

    describe('transformCodeSearchResult', () => {
      it('should transform result with empty items array', () => {
        const data = {
          total_count: 0,
          items: [],
        };
        const result = transformCodeSearchResult(data as any);
        expect(result.items).toEqual([]);
        expect(result.totalCount).toBe(0);
      });

      it('should handle missing pagination fields', () => {
        const data = {
          total_count: 5,
          items: [],
        };
        const result = transformCodeSearchResult(data as any);
        expect(result.pagination).toEqual({
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: undefined,
        });
      });
    });

    describe('transformRepoSearchResult', () => {
      it('should transform result with empty repositories array', () => {
        const data = {
          repositories: [],
        };
        const result = transformRepoSearchResult(data as any);
        expect(result.repositories).toEqual([]);
        expect(result.totalCount).toBe(0);
      });

      it('should handle missing pagination', () => {
        const data = {
          repositories: [
            {
              owner: 'test',
              repo: 'repo',
              url: 'https://github.com/test/repo',
              stars: 0,
              description: '',
              createdAt: '2024-01-01T00:00:00Z',
              updatedAt: '2024-01-01T00:00:00Z',
              pushedAt: '2024-01-01T00:00:00Z',
            },
          ],
        };
        const result = transformRepoSearchResult(data as any);
        expect(result.pagination).toEqual({
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: undefined,
        });
      });
    });

    describe('searchCode', () => {
      it('should return error when API returns error', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          error: 'Search failed',
          status: 422,
          type: 'http',
        });

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: 'owner/repo',
        };
        const result = await searchCode(query);

        expect(result.error).toBe('Search failed');
        expect(result.status).toBe(422);
      });

      it('should return 500 error when API returns no data', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          status: 200,
        } as any);

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: 'owner/repo',
        };
        const result = await searchCode(query);

        expect(result.error).toBe('No data returned from GitHub API');
        expect(result.status).toBe(500);
      });

      it('should return success when API returns data', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          data: {
            total_count: 1,
            items: [
              {
                path: 'test.ts',
                matches: [{ context: 'test', positions: [[0, 4]] }],
                url: 'https://github.com/owner/repo/blob/main/test.ts',
                repository: {
                  nameWithOwner: 'owner/repo',
                  url: 'https://github.com/owner/repo',
                },
              },
            ],
          },
          status: 200,
        });

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: 'owner/repo',
        };
        const result = await searchCode(query);

        expect(result.status).toBe(200);
        expect(result.data).toBeDefined();
      });
    });

    describe('searchRepos', () => {
      it('should return error when API returns error', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          error: 'Search failed',
          status: 422,
        } as any);

        const query: RepoSearchQuery = {
          keywords: ['test'],
        };
        const result = await searchRepos(query);

        expect(result.error).toBe('Search failed');
        expect(result.status).toBe(422);
      });

      it('should handle error with object toString', async () => {
        const errorObj = {
          toString: () => 'Object repo error',
        };
        mockSearchGitHubReposAPI.mockResolvedValue({
          error: errorObj as any,
          status: 500,
        } as any);

        const query: RepoSearchQuery = {
          keywords: ['test'],
        };
        const result = await searchRepos(query);

        expect(result.error).toBe('Object repo error');
        expect(result.status).toBe(500);
      });

      it('should return 500 error when no data in result', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          status: 200,
        } as any);

        const query: RepoSearchQuery = {
          keywords: ['test'],
        };
        const result = await searchRepos(query);

        expect(result.error).toBe('No data returned from GitHub API');
        expect(result.status).toBe(500);
      });

      it('should return 500 error when data is null', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: null,
          status: 200,
        } as any);

        const query: RepoSearchQuery = {
          keywords: ['test'],
        };
        const result = await searchRepos(query);

        expect(result.error).toBe('No data returned from GitHub API');
        expect(result.status).toBe(500);
      });

      it('should return success when API returns data', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: {
            repositories: [
              {
                owner: 'test',
                repo: 'repo',
                url: 'https://github.com/test/repo',
                stars: 0,
                description: '',
                createdAt: '2024-01-01T00:00:00Z',
                updatedAt: '2024-01-01T00:00:00Z',
                pushedAt: '2024-01-01T00:00:00Z',
                defaultBranch: 'main',
                visibility: 'public' as const,
                topics: [],
                forksCount: 0,
              },
            ],
          },
          status: 200,
        });

        const query: RepoSearchQuery = {
          keywords: ['test'],
        };
        const result = await searchRepos(query);

        expect(result.status).toBe(200);
        expect(result.data).toBeDefined();
      });

      it('should pass stars range string through to GitHub API (TC-20)', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: {
            repositories: [],
          },
          status: 200,
        });

        const query: RepoSearchQuery = {
          keywords: ['react'],
          stars: '100..500',
        };
        await searchRepos(query);

        expect(mockSearchGitHubReposAPI).toHaveBeenCalled();
        const apiCall = mockSearchGitHubReposAPI.mock.calls[0]![0];
        expect(apiCall.stars).toBe('100..500');
      });

      it('should pass >=1000 stars string through to GitHub API (TC-22)', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: {
            repositories: [],
          },
          status: 200,
        });

        const query: RepoSearchQuery = {
          keywords: ['react'],
          stars: '>=1000',
        };
        await searchRepos(query);

        expect(mockSearchGitHubReposAPI).toHaveBeenCalled();
        const apiCall = mockSearchGitHubReposAPI.mock.calls[0]![0];
        expect(apiCall.stars).toBe('>=1000');
      });

      it('should NOT convert minStars to >=N format, losing range info', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: {
            repositories: [],
          },
          status: 200,
        });

        const query: RepoSearchQuery = {
          keywords: ['react'],
          stars: '50..200',
        };
        await searchRepos(query);

        const apiCall = mockSearchGitHubReposAPI.mock.calls[0]![0];
        expect(apiCall.stars).not.toBe('>=50');
        expect(apiCall.stars).toBe('50..200');
      });
    });
  });

  describe('githubPullRequests', () => {
    describe('parseGitHubProjectId', () => {
      it('should return undefined owner and repo when projectId is undefined', () => {
        const result = parseGitHubProjectIdPR(undefined);
        expect(result).toEqual({ owner: undefined, repo: undefined });
      });
    });

    describe('transformPullRequestResult', () => {
      it('should transform result with empty pull_requests array', () => {
        const data = {
          pull_requests: [],
          total_count: 0,
        };
        const query: PullRequestQuery = {
          projectId: 'owner/repo',
        };
        const result = transformPullRequestResult(data as any, query);
        expect(result.items).toEqual([]);
        expect(result.totalCount).toBe(0);
      });

      it('should handle missing pagination fields', () => {
        const data = {
          pull_requests: [],
          total_count: 0,
        };
        const query: PullRequestQuery = {
          projectId: 'owner/repo',
        };
        const result = transformPullRequestResult(data as any, query);
        expect(result.pagination).toEqual({
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: undefined,
        });
      });

      it('should handle undefined projectId in query', () => {
        const data = {
          pull_requests: [],
          total_count: 0,
        };
        const query: PullRequestQuery = {};
        const result = transformPullRequestResult(data as any, query);
        expect(result.repositoryContext).toBeUndefined();
      });
    });

    describe('searchPullRequests', () => {
      it('should return error when API returns error', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          error: 'PR search failed',
          hints: ['Check parameters'],
        });

        const query: PullRequestQuery = {
          projectId: 'owner/repo',
        };
        const result = await searchPullRequests(query);

        expect(result.error).toBe('PR search failed');
        expect(result.status).toBe(500);
        expect(result.hints).toEqual(['Check parameters']);
      });

      it('should handle error with object toString', async () => {
        const errorObj = {
          toString: () => 'Object PR error',
        };
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          error: errorObj as any,
        });

        const query: PullRequestQuery = {
          projectId: 'owner/repo',
        };
        const result = await searchPullRequests(query);

        expect(result.error).toBe('Object PR error');
        expect(result.status).toBe(500);
      });

      it('should forward free-text query to the GitHub PR search API', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [],
          total_count: 0,
        });

        const query: PullRequestQuery = {
          projectId: 'vercel/next.js',
          query: 'hydration',
          state: 'closed',
        };
        await searchPullRequests(query);

        expect(mockSearchGitHubPullRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({ query: 'hydration' }),
          undefined
        );
      });

      it('should return success when API returns data', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [
            {
              number: 1,
              title: 'Test PR',
              url: 'https://api.github.com/repos/owner/repo/pulls/1',
              state: 'open',
              draft: false,
              merged: false,
              author: 'user',
              head_ref: 'branch',
              base_ref: 'main',
              head_sha: 'abc123',
              base_sha: 'def456',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-01T10:00:00Z',
            },
          ],
          total_count: 1,
        });

        const query: PullRequestQuery = {
          projectId: 'owner/repo',
        };
        const result = await searchPullRequests(query);

        expect(result.status).toBe(200);
        expect(result.data).toBeDefined();
        expect(result.data?.items).toHaveLength(1);
      });
    });
  });
});
