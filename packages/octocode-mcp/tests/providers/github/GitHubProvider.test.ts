import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { GitHubProvider } from '../../../../octocode-tools-core/src/providers/github/GitHubProvider.js';
import type {
  CodeSearchQuery,
  FileContentQuery,
  RepoSearchQuery,
  PullRequestQuery,
  RepoStructureQuery,
} from '../../../../octocode-tools-core/src/providers/types.js';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';

vi.mock('../../../../octocode-tools-core/src/github/codeSearch.js', () => ({
  searchGitHubCodeAPI: vi.fn(),
}));

vi.mock('../../../../octocode-tools-core/src/github/fileContent.js', () => ({
  fetchGitHubFileContentAPI: vi.fn(),
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

vi.mock('../../../../octocode-tools-core/src/github/repoStructure.js', () => ({
  viewGitHubRepositoryStructureAPI: vi.fn(),
}));

vi.mock('../../../../octocode-tools-core/src/github/client.js', () => ({
  resolveDefaultBranch: vi.fn(),
}));

import { searchGitHubCodeAPI } from '../../../../octocode-tools-core/src/github/codeSearch.js';
import { fetchGitHubFileContentAPI } from '../../../../octocode-tools-core/src/github/fileContent.js';
import { searchGitHubReposAPI } from '../../../../octocode-tools-core/src/github/repoSearch.js';
import { searchGitHubPullRequestsAPI } from '../../../../octocode-tools-core/src/github/pullRequestSearch.js';
import { viewGitHubRepositoryStructureAPI } from '../../../../octocode-tools-core/src/github/repoStructure.js';
import { resolveDefaultBranch as resolveGitHubDefaultBranch } from '../../../../octocode-tools-core/src/github/client.js';

const mockSearchGitHubCodeAPI = vi.mocked(searchGitHubCodeAPI);
const mockResolveDefaultBranch = vi.mocked(resolveGitHubDefaultBranch);
const mockFetchGitHubFileContentAPI = vi.mocked(fetchGitHubFileContentAPI);
const mockSearchGitHubReposAPI = vi.mocked(searchGitHubReposAPI);
const mockSearchGitHubPullRequestsAPI = vi.mocked(searchGitHubPullRequestsAPI);
const mockViewGitHubRepositoryStructureAPI = vi.mocked(
  viewGitHubRepositoryStructureAPI
);

describe('GitHubProvider', () => {
  let provider: GitHubProvider;

  beforeEach(() => {
    vi.clearAllMocks();
    provider = new GitHubProvider();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('constructor', () => {
    it('should create provider with default config (no config)', () => {
      const provider = new GitHubProvider();
      expect(provider.type).toBe('github');
    });

    it('should create provider with empty config', () => {
      const provider = new GitHubProvider({} as any);
      expect(provider.type).toBe('github');
    });

    it('should create provider with authInfo config', () => {
      const authInfo = {
        token: 'test-auth-token',
        clientId: 'client-id',
        scopes: [],
      };
      const provider = new GitHubProvider({ type: 'github', authInfo });
      expect(provider.type).toBe('github');
    });

    it('should create provider with token config (converts to authInfo)', () => {
      const provider = new GitHubProvider({
        type: 'github',
        token: 'direct-token',
      });
      expect(provider.type).toBe('github');
    });

    it('should prefer token over authInfo when both provided', () => {
      const authInfo = {
        token: 'auth-token',
        clientId: 'client-id',
        scopes: [],
      };
      const provider = new GitHubProvider({
        type: 'github',
        authInfo,
        token: 'direct-token',
      });
      expect(provider.type).toBe('github');
    });
  });

  describe('searchCode', () => {
    describe('success cases', () => {
      it('should search code successfully with minimal query', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          data: {
            total_count: 1,
            items: [
              {
                path: 'src/index.ts',
                matches: [{ context: 'test context', positions: [[0, 4]] }],
                url: 'https://github.com/owner/repo/blob/main/src/index.ts',
                repository: {
                  nameWithOwner: 'owner/repo',
                  url: 'https://github.com/owner/repo',
                },
              },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 10,
              totalMatches: 1,
              hasMore: false,
            },
          },
          status: 200,
        });

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: 'owner/repo',
        };

        const result = await provider.searchCode(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('github');
        expect(result.data).toBeDefined();
        expect(result.data?.items).toHaveLength(1);
        expect(result.data!.items[0]!.path).toBe('src/index.ts');
        expect(mockSearchGitHubCodeAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            keywords: ['test'],
            owner: 'owner',
            repo: 'repo',
          }),
          undefined
        );
      });

      it('should search code with all query parameters', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          data: {
            total_count: 5,
            items: [],
            pagination: {
              currentPage: 2,
              totalPages: 3,
              perPage: 10,
              totalMatches: 50,
              hasMore: true,
            },
            _researchContext: {
              foundFiles: [],
              repositoryContext: {
                owner: 'owner',
                repo: 'repo',
                branch: 'main',
              },
            },
          },
          status: 200,
        });

        const query: CodeSearchQuery = {
          keywords: ['function', 'class'],
          projectId: 'owner/repo',
          extension: 'ts',
          filename: 'index',
          path: 'src/',
          limit: 25,
          page: 2,
          mainResearchGoal: 'Find all TypeScript files',
          researchGoal: 'Locate functions',
          reasoning: 'Need to analyze code structure',
        };

        const result = await provider.searchCode(query);

        expect(result.status).toBe(200);
        expect(result.data?.pagination.currentPage).toBe(2);
        expect(result.data?.pagination.totalPages).toBe(3);
        expect(result.data?.pagination.hasMore).toBe(true);
        expect(mockSearchGitHubCodeAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            keywords: ['function', 'class'],
            owner: 'owner',
            repo: 'repo',
            extension: 'ts',
            filename: 'index',
            path: 'src/',
            limit: 25,
            page: 2,
            mainResearchGoal: 'Find all TypeScript files',
            researchGoal: 'Locate functions',
            reasoning: 'Need to analyze code structure',
          }),
          undefined
        );
      });

      it('should transform code search result with repository context', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          data: {
            total_count: 2,
            items: [
              {
                path: 'src/utils.ts',
                matches: [
                  { context: 'export function helper', positions: [[7, 15]] },
                  { context: 'function internal', positions: [[0, 8]] },
                ],
                url: 'https://github.com/test/project/blob/main/src/utils.ts',
                repository: {
                  nameWithOwner: 'test/project',
                  url: 'https://github.com/test/project',
                },
                lastModifiedAt: '2024-01-15T10:00:00Z',
              },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 10,
              totalMatches: 2,
              hasMore: false,
            },
            _researchContext: {
              foundFiles: ['src/utils.ts'],
              repositoryContext: {
                owner: 'test',
                repo: 'project',
                branch: 'main',
              },
            },
          },
          status: 200,
        });

        const query: CodeSearchQuery = {
          keywords: ['function'],
          projectId: 'test/project',
        };

        const result = await provider.searchCode(query);

        expect(result.data!.items[0]!.matches).toHaveLength(2);
        expect(result.data!.items[0]!.lastModifiedAt).toBe(
          '2024-01-15T10:00:00Z'
        );
        expect(result.data?.repositoryContext).toEqual({
          owner: 'test',
          repo: 'project',
          branch: 'main',
        });
      });

      it('should handle empty url in code search item', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          data: {
            total_count: 1,
            items: [
              {
                path: 'test.ts',
                matches: [],
                url: '',
                repository: {
                  nameWithOwner: 'owner/repo',
                  url: 'https://github.com/owner/repo',
                },
              },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 10,
              totalMatches: 1,
              hasMore: false,
            },
          },
          status: 200,
        });

        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'owner/repo',
        });

        expect(result.data!.items[0]!.url).toBe('');
      });

      it('should handle pagination defaults when not provided', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          data: {
            total_count: 10,
            items: [],
            pagination: undefined,
          },
          status: 200,
        });

        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'owner/repo',
        });

        expect(result.data?.pagination).toEqual({
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: undefined,
        });
      });
    });

    describe('error cases', () => {
      it('should handle API error with string error message', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          error: 'Rate limit exceeded',
          status: 403,
          type: 'http',
          hints: ['Wait 60 seconds before retrying'],
        });

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: 'owner/repo',
        };

        const result = await provider.searchCode(query);

        expect(result.error).toBe('Rate limit exceeded');
        expect(result.status).toBe(403);
        expect(result.provider).toBe('github');
        expect(result.hints).toContain('Wait 60 seconds before retrying');
      });

      it('should handle API error with object error (toString)', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          error: 'Error details',
          status: 500,
          type: 'http',
        });

        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('Error details');
        expect(result.status).toBe(500);
      });

      it('should handle API error with default status 500', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          error: 'Unknown error occurred',
        } as any);

        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'owner/repo',
        });

        expect(result.status).toBe(500);
      });

      it('should return error when no data returned', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          status: 200,
        } as any);

        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('No data returned from GitHub API');
        expect(result.status).toBe(500);
      });

      it('should return error when data is null', async () => {
        mockSearchGitHubCodeAPI.mockResolvedValue({
          data: null,
          status: 200,
        } as any);

        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('No data returned from GitHub API');
      });

      it('should handle thrown exceptions', async () => {
        mockSearchGitHubCodeAPI.mockRejectedValue(new Error('Network failure'));

        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('Network failure');
        expect(result.status).toBe(500);
        expect(result.provider).toBe('github');
      });

      it('should handle non-Error thrown exceptions', async () => {
        mockSearchGitHubCodeAPI.mockRejectedValue('String error');

        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('String error');
        expect(result.status).toBe(500);
      });

      it('should throw error for invalid projectId format', async () => {
        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'invalid-format-no-slash',
        });

        expect(result.error).toContain('Invalid GitHub projectId format');
        expect(result.error).toContain("Expected 'owner/repo'");
        expect(result.status).toBe(500);
      });

      it('should throw error for projectId with empty parts', async () => {
        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: '/repo',
        });

        expect(result.error).toContain('Invalid GitHub projectId format');
      });

      it('should throw error for projectId with trailing slash', async () => {
        const result = await provider.searchCode({
          keywords: ['test'],
          projectId: 'owner/',
        });

        expect(result.error).toContain('Invalid GitHub projectId format');
      });
    });
  });

  describe('getFileContent', () => {
    describe('success cases', () => {
      it('should fetch file content successfully', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          data: {
            path: 'src/index.ts',
            content: 'export default function() {}',
            branch: 'main',
            owner: 'owner',
            repo: 'repo',
          },
          status: 200,
        });

        const query: FileContentQuery = {
          projectId: 'owner/repo',
          path: 'src/index.ts',
        };

        const result = await provider.getFileContent(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('github');
        expect(result.data).toBeDefined();
        expect(result.data?.path).toBe('src/index.ts');
        expect(result.data?.content).toBe('export default function() {}');
        expect(result.data?.encoding).toBe('utf-8');
      });

      it('should fetch file content with all parameters', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          data: {
            path: 'src/utils.ts',
            content: 'function helper() {\n  return true;\n}',
            branch: 'feature-branch',
            owner: 'test',
            repo: 'project',
            startLine: 10,
            endLine: 20,
            isPartial: true,
            lastModified: '2024-01-15T10:00:00Z',
            lastModifiedBy: 'developer',
            pagination: {
              currentPage: 1,
              totalPages: 5,
              hasMore: true,
            },
          },
          status: 200,
        });

        const query: FileContentQuery = {
          projectId: 'test/project',
          path: 'src/utils.ts',
          ref: 'feature-branch',
          startLine: 10,
          endLine: 20,
          matchString: 'function',
          contextLines: 3,
          charOffset: 0,
          charLength: 1000,
          fullContent: false,
          mainResearchGoal: 'Analyze helper functions',
          researchGoal: 'Find implementation details',
          reasoning: 'Need to understand utility code',
        };

        const result = await provider.getFileContent(query);

        expect(result.data?.isPartial).toBe(true);
        expect(result.data?.startLine).toBe(10);
        expect(result.data?.endLine).toBe(20);
        expect(result.data?.lastModified).toBe('2024-01-15T10:00:00Z');
        expect(result.data?.lastModifiedBy).toBe('developer');
        expect(mockFetchGitHubFileContentAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            owner: 'test',
            repo: 'project',
            path: 'src/utils.ts',
            branch: 'feature-branch',
            startLine: 10,
            endLine: 20,
            matchString: 'function',
            contextLines: 3,
            charOffset: 0,
            charLength: 1000,
            fullContent: false,
          }),
          undefined
        );
      });

      it('should forward minify:"symbols" to fetchGitHubFileContentAPI', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          data: {
            path: 'src/index.ts',
            content: 'export function f(): void',
            branch: 'main',
            owner: 'owner',
            repo: 'repo',
          },
          status: 200,
        });

        await provider.getFileContent({
          projectId: 'owner/repo',
          path: 'src/index.ts',
          minify: 'symbols',
        });

        expect(mockFetchGitHubFileContentAPI).toHaveBeenCalledWith(
          expect.objectContaining({ minify: 'symbols' }),
          undefined
        );
      });

      it('should handle file with empty content', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          data: {
            path: 'empty.ts',
            content: '',
            branch: 'main',
          },
          status: 200,
        });

        const result = await provider.getFileContent({
          projectId: 'owner/repo',
          path: 'empty.ts',
        });

        expect(result.data?.content).toBe('');
        expect(result.data?.size).toBe(0);
      });

      it('should use query path when data path is missing', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          data: {
            content: 'content',
          },
          status: 200,
        });

        const result = await provider.getFileContent({
          projectId: 'owner/repo',
          path: 'fallback/path.ts',
        });

        expect(result.data?.path).toBe('fallback/path.ts');
      });

      it('should use query ref when data branch is missing', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          data: {
            path: 'test.ts',
            content: 'content',
          },
          status: 200,
        });

        const result = await provider.getFileContent({
          projectId: 'owner/repo',
          path: 'test.ts',
          ref: 'develop',
        });

        expect(result.data?.ref).toBe('develop');
      });
    });

    describe('error cases', () => {
      it('should return error when projectId is undefined', async () => {
        const result = await provider.getFileContent({
          path: 'test.ts',
        } as FileContentQuery);

        expect(result.error).toBe('Project ID is required for file content');
        expect(result.status).toBe(400);
        expect(result.provider).toBe('github');
      });

      it('should return error when projectId is empty string', async () => {
        const result = await provider.getFileContent({
          projectId: '',
          path: 'test.ts',
        } as FileContentQuery);

        expect(result.error).toBe('Project ID is required for file content');
        expect(result.status).toBe(400);
      });

      it('should return error when projectId is missing owner', async () => {
        const result = await provider.getFileContent({
          projectId: '/repo',
          path: 'test.ts',
        });

        expect(result.error).toContain('Invalid GitHub projectId format');
        expect(result.status).toBe(500);
      });

      it('should handle API error response', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          error: 'File not found',
          status: 404,
          type: 'http',
          hints: ['Check the file path'],
        });

        const result = await provider.getFileContent({
          projectId: 'owner/repo',
          path: 'nonexistent.ts',
        });

        expect(result.error).toBe('File not found');
        expect(result.status).toBe(404);
        expect(result.hints).toContain('Check the file path');
      });

      it('should handle API error with object error', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          error: 'Detailed error',
          status: 500,
          type: 'http',
        });

        const result = await provider.getFileContent({
          projectId: 'owner/repo',
          path: 'test.ts',
        });

        expect(result.error).toBe('Detailed error');
      });

      it('should default to status 500 when API error has no status', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          error: 'File content error without status',
          type: 'http',
        });

        const result = await provider.getFileContent({
          projectId: 'owner/repo',
          path: 'test.ts',
        });

        expect(result.error).toBe('File content error without status');
        expect(result.status).toBe(500);
      });

      it('should return error when no data returned', async () => {
        mockFetchGitHubFileContentAPI.mockResolvedValue({
          status: 200,
        } as any);

        const result = await provider.getFileContent({
          projectId: 'owner/repo',
          path: 'test.ts',
        });

        expect(result.error).toBe('No data returned from GitHub API');
      });

      it('should handle thrown exceptions', async () => {
        mockFetchGitHubFileContentAPI.mockRejectedValue(
          new Error('Connection timeout')
        );

        const result = await provider.getFileContent({
          projectId: 'owner/repo',
          path: 'test.ts',
        });

        expect(result.error).toBe('Request timeout');
        expect(result.status).toBe(500);
      });
    });
  });

  describe('searchRepos', () => {
    describe('success cases', () => {
      it('should search repositories successfully', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: {
            repositories: [
              {
                owner: 'facebook',
                repo: 'react',
                stars: 200000,
                description:
                  'A JavaScript library for building user interfaces',
                url: 'https://github.com/facebook/react',
                defaultBranch: 'main',
                visibility: 'public',
                topics: ['javascript', 'react', 'ui'],
                forksCount: 40000,
                openIssuesCount: 1000,
                createdAt: '2013-05-24T10:00:00Z',
                updatedAt: '2024-01-15T10:00:00Z',
                pushedAt: '2024-01-15T08:00:00Z',
              },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 10,
              perPage: 10,
              totalMatches: 100,
              hasMore: true,
            },
          },
          status: 200,
        });

        const query: RepoSearchQuery = {
          keywords: ['react', 'javascript'],
        };

        const result = await provider.searchRepos(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('github');
        expect(result.data?.repositories).toHaveLength(1);
        expect(result.data!.repositories[0]!.name).toBe('react');
        expect(result.data!.repositories[0]!.fullPath).toBe('facebook/react');
        expect(result.data!.repositories[0]!.stars).toBe(200000);
        expect(result.data!.repositories[0]!.forks).toBe(40000);
        expect(result.data!.repositories[0]!.visibility).toBe('public');
      });

      it('should search repos with all query parameters', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: {
            repositories: [],
            pagination: {
              currentPage: 2,
              totalPages: 5,
              perPage: 10,
              totalMatches: 50,
              hasMore: true,
            },
          },
          status: 200,
        });

        const query: RepoSearchQuery = {
          keywords: ['typescript'],
          topics: ['testing'],
          owner: 'microsoft',
          minStars: 1000,
          sort: 'stars',
          limit: 50,
          page: 2,
          mainResearchGoal: 'Find TypeScript testing frameworks',
          researchGoal: 'Identify popular repos',
          reasoning: 'Need to evaluate testing options',
        };

        await provider.searchRepos(query);

        expect(mockSearchGitHubReposAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            keywords: ['typescript'],
            topicsToSearch: ['testing'],
            owner: 'microsoft',
            stars: '>=1000',
            sort: 'stars',
            limit: 50,
            page: 2,
          }),
          undefined
        );
      });

      it('should handle best-match sort (undefined in API call)', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: {
            repositories: [],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 10,
              totalMatches: 0,
              hasMore: false,
            },
          },
          status: 200,
        });

        await provider.searchRepos({
          keywords: ['test'],
          sort: 'best-match',
        });

        expect(mockSearchGitHubReposAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            sort: undefined,
          }),
          undefined
        );
      });

      it('should handle repo without optional fields', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: {
            repositories: [
              {
                owner: 'test',
                repo: 'minimal',
                url: 'https://github.com/test/minimal',
                stars: 0,
                description: '',
                createdAt: '2024-01-01T00:00:00Z',
                updatedAt: '2024-01-01T00:00:00Z',
                pushedAt: '2024-01-01T00:00:00Z',
              } as GitHubRepositoryOutput,
            ],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 10,
              totalMatches: 1,
              hasMore: false,
            },
          },
          status: 200,
        });

        const result = await provider.searchRepos({
          keywords: ['minimal'],
        });

        const repo = result.data?.repositories[0];
        expect(repo?.description).toBeNull();
        expect(repo?.defaultBranch).toBe('main');
        expect(repo?.stars).toBe(0);
        expect(repo?.forks).toBe(0);
        expect(repo?.visibility).toBe('public');
        expect(repo?.topics).toEqual([]);
      });

      it('should handle pagination defaults when not provided', async () => {
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

        const result = await provider.searchRepos({
          keywords: ['test'],
        });

        expect(result.data?.pagination).toEqual({
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: undefined,
        });
        expect(result.data?.totalCount).toBe(1);
      });

      it('should handle minStars=0 (no stars filter)', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          data: {
            repositories: [],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 10,
              totalMatches: 0,
              hasMore: false,
            },
          },
          status: 200,
        });

        await provider.searchRepos({
          keywords: ['test'],
          minStars: 0,
        });

        expect(mockSearchGitHubReposAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            stars: undefined,
          }),
          undefined
        );
      });
    });

    describe('error cases', () => {
      it('should handle API error', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          error: 'Search failed',
          status: 422,
          type: 'http',
          hints: ['Try different keywords'],
        });

        const result = await provider.searchRepos({
          keywords: ['invalid query'],
        });

        expect(result.error).toBe('Search failed');
        expect(result.status).toBe(422);
        expect(result.hints).toContain('Try different keywords');
      });

      it('should handle API error with object error type', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          error: { toString: () => 'Object error message' } as any,
          status: 400,
          type: 'http',
        });

        const result = await provider.searchRepos({
          keywords: ['test'],
        });

        expect(result.error).toBe('Object error message');
        expect(result.status).toBe(400);
      });

      it('should default to status 500 when API error has no status', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          error: 'Error without status',
        } as any);

        const result = await provider.searchRepos({
          keywords: ['test'],
        });

        expect(result.error).toBe('Error without status');
        expect(result.status).toBe(500);
      });

      it('should return error when no data returned', async () => {
        mockSearchGitHubReposAPI.mockResolvedValue({
          status: 200,
        } as any);

        const result = await provider.searchRepos({
          keywords: ['test'],
        });

        expect(result.error).toBe('No data returned from GitHub API');
      });

      it('should handle thrown exceptions', async () => {
        mockSearchGitHubReposAPI.mockRejectedValue(
          new Error('API unavailable')
        );

        const result = await provider.searchRepos({
          keywords: ['test'],
        });

        expect(result.error).toBe('API unavailable');
        expect(result.status).toBe(500);
      });
    });
  });

  describe('searchPullRequests', () => {
    describe('success cases', () => {
      it('should search pull requests successfully', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [
            {
              number: 123,
              title: 'Add new feature',
              body: 'This PR adds a cool feature',
              url: 'https://api.github.com/repos/owner/repo/pulls/123',
              state: 'open',
              draft: false,
              merged: false,
              author: 'developer',
              assignees: ['reviewer'],
              labels: [{ id: 1, name: 'enhancement', color: 'blue' }],
              head_ref: 'feature-branch',
              base_ref: 'main',
              head_sha: 'abc123',
              base_sha: 'def456',
              created_at: '2024-01-10T10:00:00Z',
              updated_at: '2024-01-15T10:00:00Z',
              comments: 5,
              changed_files: 10,
              additions: 100,
              deletions: 20,
            },
          ],
          total_count: 1,
          pagination: {
            currentPage: 1,
            totalPages: 1,
            perPage: 10,
            totalMatches: 1,
            hasMore: false,
          },
        });

        const query: PullRequestQuery = {
          projectId: 'owner/repo',
          state: 'open',
        };

        const result = await provider.searchPullRequests(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('github');
        expect(result.data?.items).toHaveLength(1);
        expect(result.data!.items[0]!.number).toBe(123);
        expect(result.data!.items[0]!.title).toBe('Add new feature');
        expect(result.data!.items[0]!.state).toBe('open');
        expect(result.data!.items[0]!.assignees).toEqual(['reviewer']);
        expect(result.data!.items[0]!.labels).toEqual(['enhancement']);
        expect(result.data?.repositoryContext).toEqual({
          owner: 'owner',
          repo: 'repo',
        });
      });

      it('should handle merged state conversion', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [],
          total_count: 0,
        });

        await provider.searchPullRequests({
          projectId: 'owner/repo',
          state: 'merged',
        });

        expect(mockSearchGitHubPullRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            state: 'closed',
            merged: true,
          }),
          undefined
        );
      });

      it('should handle "all" state conversion', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [],
          total_count: 0,
        });

        await provider.searchPullRequests({
          projectId: 'owner/repo',
          state: 'all',
        });

        expect(mockSearchGitHubPullRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            state: undefined,
            merged: undefined,
          }),
          undefined
        );
      });

      it('should handle PR with merged state in response', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [
            {
              number: 100,
              title: 'Merged PR',
              url: 'https://api.github.com/repos/owner/repo/pulls/100',
              state: 'closed',
              draft: false,
              merged: true,
              merged_at: '2024-01-12T10:00:00Z',
              author: 'dev',
              head_ref: 'feature',
              base_ref: 'main',
              head_sha: 'abc123',
              base_sha: 'def456',
              created_at: '2024-01-10T10:00:00Z',
              updated_at: '2024-01-12T10:00:00Z',
            },
          ],
          total_count: 1,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.data!.items[0]!.state).toBe('merged');
        expect(result.data!.items[0]!.mergedAt).toBe('2024-01-12T10:00:00Z');
      });

      it('should search PRs with all query parameters', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [],
          total_count: 0,
        });

        const query: PullRequestQuery = {
          projectId: 'test/project',
          number: 42,
          state: 'open',
          author: 'contributor',
          assignee: 'maintainer',
          labels: ['bug', 'priority'],
          baseBranch: 'main',
          headBranch: 'fix-branch',
          created: '>2024-01-01',
          updated: '<2024-12-31',
          content: {
            comments: { discussion: true, reviewInline: true },
            commits: { list: true, includeFiles: true },
            changedFiles: true,
            patches: { mode: 'all' },
          },
          sort: 'updated',
          order: 'desc',
          limit: 25,
          page: 2,
        };

        await provider.searchPullRequests(query);

        expect(mockSearchGitHubPullRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            owner: 'test',
            repo: 'project',
            prNumber: 42,
            state: 'open',
            author: 'contributor',
            assignee: 'maintainer',
            label: ['bug', 'priority'],
            base: 'main',
            head: 'fix-branch',
            created: '>2024-01-01',
            updated: '<2024-12-31',
            content: {
              comments: { discussion: true, reviewInline: true },
              commits: { list: true, includeFiles: true },
              changedFiles: true,
              patches: { mode: 'all' },
            },
            sort: 'updated',
            order: 'desc',
            limit: 25,
            page: 2,
          }),
          undefined
        );
      });

      it('should handle PR without projectId (cross-repo search)', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [
            {
              number: 1,
              title: 'Cross-repo PR',
              url: 'https://api.github.com/repos/org/repo/pulls/1',
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

        const result = await provider.searchPullRequests({
          author: 'specific-user',
        });

        expect(result.data?.repositoryContext).toBeUndefined();
        expect(mockSearchGitHubPullRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            owner: undefined,
            repo: undefined,
          }),
          undefined
        );
      });

      it('should handle PR with comments and file changes', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [
            {
              number: 50,
              title: 'PR with details',
              url: 'https://api.github.com/repos/owner/repo/pulls/50',
              state: 'open',
              draft: false,
              merged: false,
              author: 'dev',
              head_ref: 'branch',
              base_ref: 'main',
              head_sha: 'abc123',
              base_sha: 'def456',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-01T10:00:00Z',
              comment_details: [
                {
                  id: 'comment-1',
                  user: 'reviewer',
                  body: 'LGTM',
                  created_at: '2024-01-02T10:00:00Z',
                  updated_at: '2024-01-02T10:00:00Z',
                },
              ],
              file_changes: [
                {
                  filename: 'src/index.ts',
                  status: 'modified',
                  additions: 10,
                  deletions: 5,
                  patch: '@@ -1,5 +1,10 @@',
                },
              ],
            },
          ],
          total_count: 1,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
          content: { comments: { discussion: true, reviewInline: true } },
        });

        expect(result.data!.items[0]!.comments).toHaveLength(1);
        expect(result.data!.items[0]!.comments?.[0]!.author).toBe('reviewer');
        expect(result.data!.items[0]!.fileChanges).toHaveLength(1);
        expect(result.data!.items[0]!.fileChanges?.[0]!.path).toBe(
          'src/index.ts'
        );
      });

      it('should handle empty assignees and labels', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [
            {
              number: 1,
              title: 'Minimal PR',
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
              assignees: undefined,
              labels: undefined,
            },
          ],
          total_count: 1,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.data!.items[0]!.assignees).toEqual([]);
        expect(result.data!.items[0]!.labels).toEqual([]);
      });

      it('should handle assignees and labels as strings', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [
            {
              number: 1,
              title: 'PR',
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
              assignees: ['user1', 'user2'],
              labels: [
                { id: 1, name: 'label1', color: 'red' },
                { id: 2, name: 'label2', color: 'blue' },
              ],
            },
          ],
          total_count: 1,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.data!.items[0]!.assignees).toEqual(['user1', 'user2']);
        expect(result.data!.items[0]!.labels).toEqual(['label1', 'label2']);
      });

      it('should use head_ref and base_ref for branch refs', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [
            {
              number: 1,
              title: 'PR',
              url: 'https://api.github.com/repos/owner/repo/pulls/1',
              state: 'open',
              draft: false,
              merged: false,
              author: 'user',
              head_ref: 'feature-branch',
              base_ref: 'develop',
              head_sha: 'abc123',
              base_sha: 'def456',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-01T10:00:00Z',
            },
          ],
          total_count: 1,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.data!.items[0]!.sourceBranch).toBe('feature-branch');
        expect(result.data!.items[0]!.targetBranch).toBe('develop');
      });

      it('should fall back to empty string when no branch refs provided', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [
            {
              number: 1,
              title: 'PR without refs',
              url: 'https://api.github.com/repos/owner/repo/pulls/1',
              state: 'open',
              draft: false,
              merged: false,
              author: 'user',
              head_ref: '',
              base_ref: '',
              head_sha: 'abc123',
              base_sha: 'def456',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-01T10:00:00Z',
            },
          ],
          total_count: 1,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.data!.items[0]!.sourceBranch).toBe('');
        expect(result.data!.items[0]!.targetBranch).toBe('');
      });

      it('should handle pagination defaults', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          pull_requests: [],
          total_count: 0,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.data?.pagination).toEqual({
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: undefined,
        });
      });
    });

    describe('error cases', () => {
      it('should handle API error', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          error: 'Search failed',
          hints: ['Check parameters'],
          pull_requests: [],
          total_count: 0,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('Search failed');
        expect(result.status).toBe(500);
        expect(result.hints).toContain('Check parameters');
      });

      it('should handle API error with object error type', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          error: { toString: () => 'Object PR error' } as any,
          hints: [],
          pull_requests: [],
          total_count: 0,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('Object PR error');
        expect(result.status).toBe(500);
      });

      it('should handle response with undefined pull_requests array', async () => {
        mockSearchGitHubPullRequestsAPI.mockResolvedValue({
          total_count: 0,
        });

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.data?.items).toEqual([]);
        expect(result.data?.totalCount).toBe(0);
      });

      it('should handle thrown exceptions', async () => {
        mockSearchGitHubPullRequestsAPI.mockRejectedValue(
          new Error('Network error')
        );

        const result = await provider.searchPullRequests({
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('Network error');
        expect(result.status).toBe(500);
      });

      it('should handle invalid projectId format', async () => {
        const result = await provider.searchPullRequests({
          projectId: 'invalid',
        });

        expect(result.error).toContain('Invalid GitHub projectId format');
      });
    });
  });

  describe('getRepoStructure', () => {
    describe('success cases', () => {
      it('should get repository structure successfully', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          owner: 'test',
          repo: 'project',
          branch: 'main',
          apiSource: true,
          path: '/',
          structure: {
            '.': {
              files: ['README.md', 'package.json'],
              folders: ['src', 'tests'],
            },
            src: {
              files: ['index.ts', 'utils.ts'],
              folders: [],
            },
          },
          summary: {
            totalFiles: 4,
            totalFolders: 2,
            truncated: false,
            filtered: false,
            originalCount: 6,
          },
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
          },
          hints: ['Use depth parameter for deeper traversal'],
        });

        const query: RepoStructureQuery = {
          projectId: 'test/project',
        };

        const result = await provider.getRepoStructure(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('github');
        expect(result.data?.projectPath).toBe('test/project');
        expect(result.data?.branch).toBe('main');
        expect(result.data?.structure['.']).toBeDefined();
        expect(result.data?.summary.totalFiles).toBe(4);
        expect(result.data?.hints).toContain(
          'Use depth parameter for deeper traversal'
        );
      });

      it('should get repo structure with all parameters', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          owner: 'test',
          repo: 'project',
          branch: 'develop',
          apiSource: true,
          path: 'src/components',
          structure: {},
          summary: {
            totalFiles: 0,
            totalFolders: 0,
            truncated: false,
            filtered: false,
            originalCount: 0,
          },
        });

        const query: RepoStructureQuery = {
          projectId: 'test/project',
          ref: 'develop',
          path: 'src/components',
          depth: 3,
          itemsPerPage: 50,
          page: 2,
          mainResearchGoal: 'Understand component structure',
          researchGoal: 'Find React components',
          reasoning: 'Need to map UI architecture',
        };

        await provider.getRepoStructure(query);

        expect(mockViewGitHubRepositoryStructureAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            owner: 'test',
            repo: 'project',
            branch: 'develop',
            path: 'src/components',
            maxDepth: 3,
            itemsPerPage: 50,
            page: 2,
            mainResearchGoal: 'Understand component structure',
            researchGoal: 'Find React components',
            reasoning: 'Need to map UI architecture',
          }),
          undefined
        );
      });

      it('should use HEAD as default branch when ref not provided', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          owner: 'test',
          repo: 'project',
          branch: 'HEAD',
          apiSource: true,
          path: '/',
          structure: {},
          summary: {
            totalFiles: 0,
            totalFolders: 0,
            truncated: false,
            filtered: false,
            originalCount: 0,
          },
        });

        await provider.getRepoStructure({
          projectId: 'test/project',
        });

        expect(mockViewGitHubRepositoryStructureAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            branch: 'HEAD',
          }),
          undefined
        );
      });

      it('should handle empty structure', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          owner: 'test',
          repo: 'empty-repo',
          branch: 'main',
          apiSource: true,
          path: '/',
          structure: {},
          summary: {
            totalFiles: 0,
            totalFolders: 0,
            truncated: false,
            filtered: false,
            originalCount: 0,
          },
        });

        const result = await provider.getRepoStructure({
          projectId: 'test/empty-repo',
        });

        expect(result.data?.structure).toEqual({});
        expect(result.data?.summary.totalFiles).toBe(0);
      });

      it('should handle missing summary fields', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          owner: 'test',
          repo: 'project',
          branch: 'main',
          apiSource: true,
          path: '/',
          structure: {},
          summary: {
            totalFiles: 0,
            totalFolders: 0,
            truncated: false,
            filtered: false,
            originalCount: 0,
          },
        });

        const result = await provider.getRepoStructure({
          projectId: 'test/project',
        });

        expect(result.data?.summary).toEqual({
          totalFiles: 0,
          totalFolders: 0,
          truncated: false,
        });
      });

      it('should handle missing branch in response', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          owner: 'test',
          repo: 'project',
          branch: '',
          path: '/',
          apiSource: true,
          structure: {},
          summary: {
            totalFiles: 0,
            totalFolders: 0,
            truncated: false,
            filtered: false,
            originalCount: 0,
          },
        });

        const result = await provider.getRepoStructure({
          projectId: 'test/project',
        });

        expect(result.data?.branch).toBe('');
      });

      it('should handle missing path in response', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          owner: 'test',
          repo: 'project',
          branch: 'main',
          path: '/',
          apiSource: true,
          structure: {},
          summary: {
            totalFiles: 0,
            totalFolders: 0,
            truncated: false,
            filtered: false,
            originalCount: 0,
          },
        });

        const result = await provider.getRepoStructure({
          projectId: 'test/project',
        });

        expect(result.data?.path).toBe('/');
      });

      it('should handle undefined structure in response', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          owner: 'test',
          repo: 'project',
          branch: 'main',
          apiSource: true,
          path: '/',
          structure: {},
          summary: {
            totalFiles: 0,
            totalFolders: 0,
            truncated: false,
            filtered: false,
            originalCount: 0,
          },
        });

        const result = await provider.getRepoStructure({
          projectId: 'test/project',
        });

        expect(result.data?.structure).toEqual({});
      });
    });

    describe('error cases', () => {
      it('should return error when projectId is missing', async () => {
        const result = await provider.getRepoStructure({
          projectId: '',
        } as any);

        expect(result.error).toBe(
          'Project ID is required for repository structure'
        );
        expect(result.status).toBe(400);
      });

      it('should return error for invalid projectId format', async () => {
        const result = await provider.getRepoStructure({
          projectId: 'invalid-no-slash',
        });

        expect(result.error).toContain('Invalid GitHub projectId format');
        expect(result.status).toBe(500);
      });

      it('should handle API error response', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          error: 'Repository not found',
          status: 404,
          hints: ['Check repository name'],
        });

        const result = await provider.getRepoStructure({
          projectId: 'nonexistent/repo',
        });

        expect(result.error).toBe('Repository not found');
        expect(result.status).toBe(404);
        expect(result.hints).toContain('Check repository name');
      });

      it('should handle API error with object error', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          error: { toString: () => 'Complex error message' } as any,
          status: 500,
        });

        const result = await provider.getRepoStructure({
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('Complex error message');
      });

      it('should default to status 500 when API error has no status', async () => {
        mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
          error: 'Error without status',
        });

        const result = await provider.getRepoStructure({
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('Error without status');
        expect(result.status).toBe(500);
      });

      it('should handle thrown exceptions', async () => {
        mockViewGitHubRepositoryStructureAPI.mockRejectedValue(
          new Error('Timeout')
        );

        const result = await provider.getRepoStructure({
          projectId: 'owner/repo',
        });

        expect(result.error).toBe('Timeout');
        expect(result.status).toBe(500);
      });
    });
  });

  describe('parseProjectId helper', () => {
    it('should parse valid projectId', async () => {
      mockSearchGitHubCodeAPI.mockResolvedValue({
        data: {
          total_count: 0,
          items: [],
          pagination: {
            currentPage: 1,
            totalPages: 1,
            perPage: 10,
            totalMatches: 0,
            hasMore: false,
          },
        },
        status: 200,
      });

      await provider.searchCode({
        keywords: ['test'],
        projectId: 'valid-owner/valid-repo',
      });

      expect(mockSearchGitHubCodeAPI).toHaveBeenCalledWith(
        expect.objectContaining({
          owner: 'valid-owner',
          repo: 'valid-repo',
        }),
        undefined
      );
    });

    it('should handle undefined projectId', async () => {
      mockSearchGitHubCodeAPI.mockResolvedValue({
        data: {
          total_count: 0,
          items: [],
          pagination: {
            currentPage: 1,
            totalPages: 1,
            perPage: 10,
            totalMatches: 0,
            hasMore: false,
          },
        },
        status: 200,
      });

      await provider.searchCode({
        keywords: ['test'],
      });

      expect(mockSearchGitHubCodeAPI).toHaveBeenCalledWith(
        expect.objectContaining({
          owner: undefined,
          repo: undefined,
        }),
        undefined
      );
    });

    it('should throw error for projectId with too many slashes', async () => {
      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'org/owner/repo',
      });

      expect(result.error).toContain('Invalid GitHub projectId format');
    });

    it('should throw error for empty string projectId parts', async () => {
      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: '/',
      });

      expect(result.error).toContain('Invalid GitHub projectId format');
    });
  });

  describe('error handling (handleError)', () => {
    it('should convert Error instances to ProviderResponse', async () => {
      const customError = new Error('Custom error message');
      mockSearchGitHubCodeAPI.mockRejectedValue(customError);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      expect(result).toEqual({
        error: 'Custom error message',
        status: 500,
        provider: 'github',
      });
    });

    it('should handle non-Error objects', async () => {
      mockSearchGitHubCodeAPI.mockRejectedValue({ custom: 'object' });

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      expect(result.error).toBe('Unknown error occurred');
    });

    it('should handle null thrown value', async () => {
      mockSearchGitHubCodeAPI.mockRejectedValue(null);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      expect(result.error).toBe('Unknown error occurred');
    });

    it('should handle undefined thrown value', async () => {
      mockSearchGitHubCodeAPI.mockRejectedValue(undefined);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      expect(result.error).toBe('Unknown error occurred');
    });
  });

  describe('provider type', () => {
    it('should have readonly type property set to github', () => {
      expect(provider.type).toBe('github');
    });
  });

  describe('auth info propagation', () => {
    it('should pass authInfo to searchGitHubCodeAPI', async () => {
      const authInfo = { token: 'test-token', clientId: 'client', scopes: [] };
      const authenticatedProvider = new GitHubProvider({
        type: 'github',
        authInfo,
      });

      mockSearchGitHubCodeAPI.mockResolvedValue({
        data: {
          total_count: 0,
          items: [],
          pagination: {
            currentPage: 1,
            totalPages: 1,
            perPage: 10,
            totalMatches: 0,
            hasMore: false,
          },
        },
        status: 200,
      });

      await authenticatedProvider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      expect(mockSearchGitHubCodeAPI).toHaveBeenCalledWith(
        expect.any(Object),
        authInfo
      );
    });

    it('should pass token-based authInfo to fetchGitHubFileContentAPI', async () => {
      const authenticatedProvider = new GitHubProvider({
        type: 'github',
        token: 'direct-token',
      });

      mockFetchGitHubFileContentAPI.mockResolvedValue({
        data: { path: 'test.ts', content: '' },
        status: 200,
      });

      await authenticatedProvider.getFileContent({
        projectId: 'owner/repo',
        path: 'test.ts',
      });

      expect(mockFetchGitHubFileContentAPI).toHaveBeenCalledWith(
        expect.any(Object),
        expect.objectContaining({ token: 'direct-token' })
      );
    });

    it('should pass authInfo to searchGitHubReposAPI', async () => {
      const authInfo = { token: 'repo-token', clientId: 'client', scopes: [] };
      const authenticatedProvider = new GitHubProvider({
        type: 'github',
        authInfo,
      });

      mockSearchGitHubReposAPI.mockResolvedValue({
        data: {
          repositories: [],
          pagination: {
            currentPage: 1,
            totalPages: 1,
            perPage: 10,
            totalMatches: 0,
            hasMore: false,
          },
        },
        status: 200,
      });

      await authenticatedProvider.searchRepos({
        keywords: ['test'],
      });

      expect(mockSearchGitHubReposAPI).toHaveBeenCalledWith(
        expect.any(Object),
        authInfo
      );
    });

    it('should pass authInfo to searchGitHubPullRequestsAPI', async () => {
      const authInfo = { token: 'pr-token', clientId: 'client', scopes: [] };
      const authenticatedProvider = new GitHubProvider({
        type: 'github',
        authInfo,
      });

      mockSearchGitHubPullRequestsAPI.mockResolvedValue({
        pull_requests: [],
        total_count: 0,
      });

      await authenticatedProvider.searchPullRequests({
        projectId: 'owner/repo',
      });

      expect(mockSearchGitHubPullRequestsAPI).toHaveBeenCalledWith(
        expect.any(Object),
        authInfo
      );
    });

    it('should pass authInfo to viewGitHubRepositoryStructureAPI', async () => {
      const authInfo = {
        token: 'structure-token',
        clientId: 'client',
        scopes: [],
      };
      const authenticatedProvider = new GitHubProvider({
        type: 'github',
        authInfo,
      });

      mockViewGitHubRepositoryStructureAPI.mockResolvedValue({
        owner: 'test',
        repo: 'project',
        branch: 'main',
        apiSource: true,
        path: '/',
        structure: {},
        summary: {
          totalFiles: 0,
          totalFolders: 0,
          truncated: false,
          filtered: false,
          originalCount: 0,
        },
      });

      await authenticatedProvider.getRepoStructure({
        projectId: 'test/project',
      });

      expect(mockViewGitHubRepositoryStructureAPI).toHaveBeenCalledWith(
        expect.any(Object),
        authInfo
      );
    });
  });

  describe('resolveDefaultBranch', () => {
    it('should resolve default branch for a valid projectId', async () => {
      mockResolveDefaultBranch.mockResolvedValue('main');

      const result = await provider.resolveDefaultBranch('owner/repo');

      expect(result).toBe('main');
      expect(mockResolveDefaultBranch).toHaveBeenCalledWith(
        'owner',
        'repo',
        undefined
      );
    });

    it('should pass authInfo when resolving default branch', async () => {
      const authInfo = { token: 'test-token' };
      const authenticatedProvider = new GitHubProvider({
        type: 'github',
        authInfo: authInfo as any,
      });
      mockResolveDefaultBranch.mockResolvedValue('develop');

      const result =
        await authenticatedProvider.resolveDefaultBranch('org/project');

      expect(result).toBe('develop');
      expect(mockResolveDefaultBranch).toHaveBeenCalledWith(
        'org',
        'project',
        authInfo
      );
    });

    it('should throw for invalid projectId format', async () => {
      await expect(provider.resolveDefaultBranch('invalid')).rejects.toThrow(
        "Invalid GitHub projectId format: 'invalid'. Expected 'owner/repo'."
      );
    });

    it('should throw for empty projectId', async () => {
      await expect(provider.resolveDefaultBranch('')).rejects.toThrow(
        "Cannot resolve default branch: invalid projectId ''."
      );
    });

    it('should propagate errors from underlying API', async () => {
      mockResolveDefaultBranch.mockRejectedValue(
        new Error('Could not determine default branch')
      );

      await expect(provider.resolveDefaultBranch('owner/repo')).rejects.toThrow(
        'Could not determine default branch'
      );
    });
  });
});
