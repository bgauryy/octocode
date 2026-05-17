import { describe, it, expect, vi, beforeEach } from 'vitest';
import { GitLabProvider } from '../../../src/providers/gitlab/GitLabProvider.js';
import { searchGitLabCodeAPI } from '../../../src/gitlab/codeSearch.js';
import { fetchGitLabFileContentAPI } from '../../../src/gitlab/fileContent.js';
import { searchGitLabProjectsAPI } from '../../../src/gitlab/projectsSearch.js';
import {
  searchGitLabMergeRequestsAPI,
  getGitLabMRNotes,
} from '../../../src/gitlab/mergeRequests.js';
import { viewGitLabRepositoryStructureAPI } from '../../../src/gitlab/repoStructure.js';
import type {
  CodeSearchQuery,
  FileContentQuery,
  RepoSearchQuery,
  PullRequestQuery,
  RepoStructureQuery,
} from '../../../src/providers/types.js';
import type {
  GitLabAPIResponse,
  GitLabCodeSearchResult,
} from '../../../src/gitlab/types.js';
import type { GitLabProjectsSearchResult } from '../../../src/gitlab/projectsSearch.js';
import type { GitLabMRSearchResult } from '../../../src/gitlab/mergeRequests.js';

// Type alias for simpler mock response casting
type MockProjectsResponse = GitLabAPIResponse<GitLabProjectsSearchResult>;
type MockMRResponse = GitLabAPIResponse<GitLabMRSearchResult>;

// Mock all GitLab API functions
vi.mock('../../../src/gitlab/codeSearch.js');
vi.mock('../../../src/gitlab/fileContent.js');
vi.mock('../../../src/gitlab/projectsSearch.js');
vi.mock('../../../src/gitlab/mergeRequests.js');
vi.mock('../../../src/gitlab/repoStructure.js');

const mockGetGitlab = vi.hoisted(() => vi.fn());
vi.mock('../../../src/gitlab/client.js', () => ({
  getGitlab: mockGetGitlab,
}));

const mockLogRateLimit = vi.hoisted(() => vi.fn());
vi.mock('../../../src/session.js', () => ({
  logRateLimit: mockLogRateLimit,
}));

describe('GitLabProvider', () => {
  let provider: GitLabProvider;

  beforeEach(() => {
    vi.clearAllMocks();
    provider = new GitLabProvider();
  });

  describe('constructor', () => {
    it('should create provider with gitlab type', () => {
      expect(provider.type).toBe('gitlab');
    });

    it('should create provider without config', () => {
      const p = new GitLabProvider();
      expect(p.type).toBe('gitlab');
    });

    it('should create provider with config', () => {
      const p = new GitLabProvider({
        type: 'gitlab',
        baseUrl: 'https://gitlab.example.com',
      });
      expect(p.type).toBe('gitlab');
    });
  });

  describe('searchCode', () => {
    describe('success cases', () => {
      it('should search code successfully with numeric project ID', async () => {
        const mockApiResponse = {
          data: {
            items: [
              {
                path: 'src/index.ts',
                data: 'export function test() {}',
                project_id: 12345,
                startline: 1,
              },
              {
                path: 'src/utils.ts',
                data: 'export const helper = () => {}',
                project_id: 12345,
                startline: 10,
              },
            ],
          },
          status: 200,
        } as GitLabAPIResponse<GitLabCodeSearchResult>;

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

        const query: CodeSearchQuery = {
          keywords: ['function', 'export'],
          projectId: '12345',
        };

        const result = await provider.searchCode(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('gitlab');
        expect(result.data).toBeDefined();
        expect(result.data!.items).toHaveLength(2);
        expect(result.data!.items[0]!.path).toBe('src/index.ts');
        expect(result.data!.items[0]!.matches[0]!.context).toBe(
          'export function test() {}'
        );
      });

      it('should search code with string path project ID', async () => {
        const mockApiResponse = {
          data: {
            items: [
              {
                path: 'README.md',
                data: '# Project',
                project_id: 123,
                startline: 1,
              },
            ],
          },
          status: 200,
        } as GitLabAPIResponse<GitLabCodeSearchResult>;

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

        const query: CodeSearchQuery = {
          keywords: ['readme'],
          projectId: 'my-group/my-project',
        };

        const result = await provider.searchCode(query);

        expect(searchGitLabCodeAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            search: 'readme',
            projectId: 'my-group%2Fmy-project', // URL encoded
          })
        );
        expect(result.status).toBe(200);
        expect(result.data!.items).toHaveLength(1);
      });

      it('should pass all query parameters to API', async () => {
        const mockApiResponse = {
          data: { items: [] },
          status: 200,
        };

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

        const query: CodeSearchQuery = {
          keywords: ['test', 'function'],
          projectId: '12345',
          path: 'src/',
          filename: 'index',
          extension: 'ts',
          ref: 'develop',
          limit: 50,
          page: 2,
        };

        await provider.searchCode(query);

        expect(searchGitLabCodeAPI).toHaveBeenCalledWith({
          search: 'test function',
          projectId: 12345,
          path: 'src/',
          filename: 'index',
          extension: 'ts',
          ref: 'develop',
          perPage: 50,
          page: 2,
        });
      });

      it('should handle pagination correctly', async () => {
        const mockApiResponse = {
          data: {
            items: Array(20).fill({
              path: 'test.ts',
              data: 'code',
              project_id: 123,
              startline: 1,
            }),
          },
          status: 200,
        };

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: '123',
          limit: 20,
          page: 1,
        };

        const result = await provider.searchCode(query);

        expect(result.data!.pagination.currentPage).toBe(1);
        expect(result.data!.pagination.hasMore).toBe(true);
      });

      it('should indicate no more pages when results less than limit', async () => {
        const mockApiResponse = {
          data: {
            items: Array(5).fill({
              path: 'test.ts',
              data: 'code',
              project_id: 123,
              startline: 1,
            }),
          },
          status: 200,
        };

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: '123',
          limit: 20,
        };

        const result = await provider.searchCode(query);

        expect(result.data!.pagination.hasMore).toBe(false);
      });
    });

    describe('error cases', () => {
      it('should return error when API returns error', async () => {
        const mockApiResponse = {
          error: 'Search query is required',
          status: 400,
          type: 'http' as const,
        };

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

        const query: CodeSearchQuery = {
          keywords: [],
          projectId: '123',
        };

        const result = await provider.searchCode(query);

        expect(result.error).toBe('Search query is required');
        expect(result.status).toBe(400);
        expect(result.provider).toBe('gitlab');
      });

      it('should default status to 500 when not provided in error response', async () => {
        const mockApiResponse = {
          error: 'Server error',
          // No status field
        };

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: '123',
        };

        const result = await provider.searchCode(query);

        expect(result.error).toBe('Server error');
        expect(result.status).toBe(500);
      });

      it('should handle error response without hints', async () => {
        const mockApiResponse = {
          error: 'Forbidden',
          status: 403,
          type: 'http' as const,
          // No hints field
        };

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: '123',
        };

        const result = await provider.searchCode(query);

        expect(result.error).toBe('Forbidden');
        expect(result.status).toBe(403);
        expect(result.hints).toBeUndefined();
      });

      it('should return error with hints when available', async () => {
        const mockApiResponse = {
          error: 'Search query is required',
          status: 400,
          type: 'http' as const,
          hints: ['Global code search requires GitLab Premium tier.'],
        };

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

        const query: CodeSearchQuery = {
          keywords: [''],
          projectId: '123',
        };

        const result = await provider.searchCode(query);

        expect(result.error).toBe('Search query is required');
        expect(result.hints).toContain(
          'Global code search requires GitLab Premium tier.'
        );
      });

      it('should return error when no data returned', async () => {
        const mockApiResponse = {
          status: 200,
        };

        vi.mocked(searchGitLabCodeAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: '123',
        };

        const result = await provider.searchCode(query);

        expect(result.error).toBe('No data returned from GitLab API');
        expect(result.status).toBe(500);
      });

      it('should handle thrown exceptions', async () => {
        vi.mocked(searchGitLabCodeAPI).mockRejectedValue(
          new Error('Network error')
        );

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: '123',
        };

        const result = await provider.searchCode(query);

        expect(result.error).toBe('Network error');
        expect(result.status).toBe(500);
        expect(result.provider).toBe('gitlab');
      });

      it('should handle non-Error thrown exceptions', async () => {
        vi.mocked(searchGitLabCodeAPI).mockRejectedValue('String error');

        const query: CodeSearchQuery = {
          keywords: ['test'],
          projectId: '123',
        };

        const result = await provider.searchCode(query);

        expect(result.error).toBe('An unknown error occurred');
        expect(result.status).toBe(500);
      });

      it('should throw error when projectId is missing', async () => {
        const query: CodeSearchQuery = {
          keywords: ['test'],
        };

        const result = await provider.searchCode(query);

        expect(result.error).toBe('Project ID is required');
        expect(result.status).toBe(500);
      });
    });
  });

  describe('getFileContent', () => {
    describe('success cases', () => {
      it('should get file content successfully with ref', async () => {
        const mockApiResponse = {
          data: {
            file_name: 'index.ts',
            file_path: 'src/index.ts',
            size: 1234,
            encoding: 'utf-8',
            content: 'export const test = true;',
            content_sha256: 'abc123',
            ref: 'main',
            blob_id: 'blob123',
            commit_id: 'commit123',
            last_commit_id: 'lastcommit123',
            execute_filemode: false,
          },
          status: 200,
        };

        vi.mocked(fetchGitLabFileContentAPI).mockResolvedValue(mockApiResponse);

        const query: FileContentQuery = {
          projectId: '12345',
          path: 'src/index.ts',
          ref: 'main',
        };

        const result = await provider.getFileContent(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('gitlab');
        expect(result.data).toBeDefined();
        expect(result.data!.path).toBe('src/index.ts');
        expect(result.data!.content).toBe('export const test = true;');
        expect(result.data!.encoding).toBe('utf-8');
        expect(result.data!.size).toBe(1234);
        expect(result.data!.ref).toBe('main');
        expect(result.data!.lastCommitSha).toBe('lastcommit123');
      });

      it('should use HEAD when ref is not provided', async () => {
        const mockApiResponse = {
          data: {
            file_name: 'test.ts',
            file_path: 'test.ts',
            size: 100,
            encoding: 'utf-8',
            content: 'test',
            content_sha256: 'sha',
            ref: 'HEAD',
            blob_id: 'blob',
            commit_id: 'commit',
            last_commit_id: 'last',
            execute_filemode: false,
          },
          status: 200,
        };

        vi.mocked(fetchGitLabFileContentAPI).mockResolvedValue(mockApiResponse);

        const query: FileContentQuery = {
          projectId: '123',
          path: 'test.ts',
        };

        await provider.getFileContent(query);

        expect(fetchGitLabFileContentAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            ref: 'HEAD',
          })
        );
      });

      it('should pass line range parameters', async () => {
        const mockApiResponse = {
          data: {
            file_name: 'test.ts',
            file_path: 'test.ts',
            size: 100,
            encoding: 'utf-8',
            content: 'partial content',
            content_sha256: 'sha',
            ref: 'main',
            blob_id: 'blob',
            commit_id: 'commit',
            last_commit_id: 'last',
            execute_filemode: false,
          },
          status: 200,
        };

        vi.mocked(fetchGitLabFileContentAPI).mockResolvedValue(mockApiResponse);

        const query: FileContentQuery = {
          projectId: '123',
          path: 'test.ts',
          ref: 'main',
          startLine: 10,
          endLine: 20,
        };

        await provider.getFileContent(query);

        expect(fetchGitLabFileContentAPI).toHaveBeenCalledWith({
          projectId: 123,
          path: 'test.ts',
          ref: 'main',
          startLine: 10,
          endLine: 20,
        });
      });

      it('should handle URL-encoded project path', async () => {
        const mockApiResponse = {
          data: {
            file_name: 'test.ts',
            file_path: 'test.ts',
            size: 100,
            encoding: 'utf-8',
            content: 'content',
            content_sha256: 'sha',
            ref: 'main',
            blob_id: 'blob',
            commit_id: 'commit',
            last_commit_id: 'last',
            execute_filemode: false,
          },
          status: 200,
        };

        vi.mocked(fetchGitLabFileContentAPI).mockResolvedValue(mockApiResponse);

        const query: FileContentQuery = {
          projectId: 'group/subgroup/project',
          path: 'test.ts',
          ref: 'main',
        };

        await provider.getFileContent(query);

        expect(fetchGitLabFileContentAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            projectId: 'group%2Fsubgroup%2Fproject',
          })
        );
      });
    });

    describe('error cases', () => {
      it('should return error when API returns error', async () => {
        const mockApiResponse = {
          error: 'File not found',
          status: 404,
          type: 'http' as const,
          hints: ['Check the file path and ref'],
        };

        vi.mocked(fetchGitLabFileContentAPI).mockResolvedValue(mockApiResponse);

        const query: FileContentQuery = {
          projectId: '123',
          path: 'nonexistent.ts',
          ref: 'main',
        };

        const result = await provider.getFileContent(query);

        expect(result.error).toBe('File not found');
        expect(result.status).toBe(404);
        expect(result.hints).toContain('Check the file path and ref');
      });

      it('should default status to 500 when not provided in error response', async () => {
        const mockApiResponse = {
          error: 'Server error',
          // No status field
        };

        vi.mocked(fetchGitLabFileContentAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const query: FileContentQuery = {
          projectId: '123',
          path: 'test.ts',
          ref: 'main',
        };

        const result = await provider.getFileContent(query);

        expect(result.error).toBe('Server error');
        expect(result.status).toBe(500);
      });

      it('should handle error response without hints', async () => {
        const mockApiResponse = {
          error: 'Forbidden',
          status: 403,
          type: 'http' as const,
          // No hints field
        };

        vi.mocked(fetchGitLabFileContentAPI).mockResolvedValue(mockApiResponse);

        const query: FileContentQuery = {
          projectId: '123',
          path: 'test.ts',
          ref: 'main',
        };

        const result = await provider.getFileContent(query);

        expect(result.error).toBe('Forbidden');
        expect(result.status).toBe(403);
        expect(result.hints).toBeUndefined();
      });

      it('should return error when no data returned', async () => {
        const mockApiResponse = {
          status: 200,
        };

        vi.mocked(fetchGitLabFileContentAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const query: FileContentQuery = {
          projectId: '123',
          path: 'test.ts',
          ref: 'main',
        };

        const result = await provider.getFileContent(query);

        expect(result.error).toBe('No data returned from GitLab API');
        expect(result.status).toBe(500);
      });

      it('should handle thrown exceptions', async () => {
        vi.mocked(fetchGitLabFileContentAPI).mockRejectedValue(
          new Error('Connection timeout')
        );

        const query: FileContentQuery = {
          projectId: '123',
          path: 'test.ts',
          ref: 'main',
        };

        const result = await provider.getFileContent(query);

        expect(result.error).toBe('Connection timeout');
        expect(result.status).toBe(500);
      });

      it('should throw error when projectId is missing', async () => {
        const query = {
          path: 'test.ts',
          ref: 'main',
        } as FileContentQuery;

        const result = await provider.getFileContent(query);

        expect(result.error).toBe('Project ID is required');
        expect(result.status).toBe(500);
      });
    });
  });

  describe('searchRepos', () => {
    describe('success cases', () => {
      it('should search repositories successfully', async () => {
        const mockApiResponse = {
          data: {
            projects: [
              {
                id: 123,
                name: 'my-project',
                path_with_namespace: 'my-group/my-project',
                description: 'A test project',
                web_url: 'https://gitlab.com/my-group/my-project',
                http_url_to_repo: 'https://gitlab.com/my-group/my-project.git',
                default_branch: 'main',
                star_count: 100,
                forks_count: 20,
                visibility: 'public',
                topics: ['typescript', 'nodejs'],
                created_at: '2023-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
                last_activity_at: '2024-01-15T00:00:00Z',
                open_issues_count: 5,
                archived: false,
              },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 20,
              totalMatches: 1,
              hasMore: false,
            },
          },
          status: 200,
        } as GitLabAPIResponse<GitLabProjectsSearchResult>;

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        const query: RepoSearchQuery = {
          keywords: ['typescript'],
        };

        const result = await provider.searchRepos(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('gitlab');
        expect(result.data).toBeDefined();
        expect(result.data!.repositories).toHaveLength(1);

        const repo = result.data!.repositories[0]!;
        expect(repo.id).toBe('123');
        expect(repo.name).toBe('my-project');
        expect(repo.fullPath).toBe('my-group/my-project');
        expect(repo.description).toBe('A test project');
        expect(repo.stars).toBe(100);
        expect(repo.forks).toBe(20);
        expect(repo.topics).toEqual(['typescript', 'nodejs']);
      });

      it('should pass query parameters to API', async () => {
        const mockApiResponse = {
          data: {
            projects: [],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 50,
              hasMore: false,
            },
          },
          status: 200,
        };

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        const query: RepoSearchQuery = {
          keywords: ['react', 'typescript'],
          topics: ['frontend'],
          visibility: 'public',
          minStars: 100,
          sort: 'stars',
          order: 'desc',
          limit: 50,
          page: 2,
        };

        await provider.searchRepos(query);

        expect(searchGitLabProjectsAPI).toHaveBeenCalledWith({
          search: 'react typescript',
          topic: 'frontend', // GitLab only supports single topic
          visibility: 'public',
          minStars: 100,
          orderBy: 'star_count',
          sort: 'desc',
          perPage: 50,
          page: 2,
        });
      });

      it('should handle projects with tag_list instead of topics', async () => {
        const mockApiResponse = {
          data: {
            projects: [
              {
                id: 456,
                name: 'old-project',
                path_with_namespace: 'group/old-project',
                description: null,
                web_url: 'https://gitlab.com/group/old-project',
                http_url_to_repo: 'https://gitlab.com/group/old-project.git',
                default_branch: 'master',
                star_count: 50,
                forks_count: 10,
                visibility: 'private',
                tag_list: ['legacy', 'deprecated'],
                created_at: '2020-01-01T00:00:00Z',
                updated_at: '2023-01-01T00:00:00Z',
                last_activity_at: '2023-06-01T00:00:00Z',
              },
            ],
            pagination: { currentPage: 1, hasMore: false },
          },
          status: 200,
        } as MockProjectsResponse;

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        const result = await provider.searchRepos({ keywords: ['legacy'] });

        expect(result.data!.repositories[0]!.topics).toEqual([
          'legacy',
          'deprecated',
        ]);
      });

      it('should handle projects with neither topics nor tag_list', async () => {
        const mockApiResponse = {
          data: {
            projects: [
              {
                id: 789,
                name: 'minimal-project',
                path_with_namespace: 'group/minimal-project',
                description: 'Minimal project with no topics',
                web_url: 'https://gitlab.com/group/minimal-project',
                http_url_to_repo:
                  'https://gitlab.com/group/minimal-project.git',
                default_branch: 'main',
                star_count: 5,
                forks_count: 0,
                visibility: 'public',
                // No topics or tag_list fields
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-10T00:00:00Z',
                last_activity_at: '2024-01-10T00:00:00Z',
              },
            ],
            pagination: { currentPage: 1, hasMore: false },
          },
          status: 200,
        } as MockProjectsResponse;

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        const result = await provider.searchRepos({ keywords: ['minimal'] });

        expect(result.data!.repositories[0]!.topics).toEqual([]);
      });

      it('should handle empty pagination', async () => {
        const mockApiResponse = {
          data: {
            projects: [],
            pagination: null,
          },
          status: 200,
        };

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const result = await provider.searchRepos({
          keywords: ['nonexistent'],
        });

        expect(result.data!.pagination.currentPage).toBe(1);
        expect(result.data!.pagination.totalPages).toBe(1);
        expect(result.data!.pagination.hasMore).toBe(false);
      });
    });

    describe('sort field mapping', () => {
      it.each([
        ['stars', 'star_count'],
        ['updated', 'updated_at'],
        ['created', 'created_at'],
      ])('should map sort field %s to %s', async (input, expected) => {
        const mockApiResponse = {
          data: { projects: [], pagination: { hasMore: false } },
          status: 200,
        } as unknown as MockProjectsResponse;

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        await provider.searchRepos({ sort: input as any });

        expect(searchGitLabProjectsAPI).toHaveBeenCalledWith(
          expect.objectContaining({ orderBy: expected })
        );
      });

      it('should pass undefined for unknown sort field', async () => {
        const mockApiResponse = {
          data: { projects: [], pagination: { hasMore: false } },
          status: 200,
        } as unknown as MockProjectsResponse;

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        await provider.searchRepos({ sort: 'best-match' });

        expect(searchGitLabProjectsAPI).toHaveBeenCalledWith(
          expect.objectContaining({ orderBy: undefined })
        );
      });

      it('should handle undefined sort', async () => {
        const mockApiResponse = {
          data: { projects: [], pagination: { hasMore: false } },
          status: 200,
        } as unknown as MockProjectsResponse;

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        await provider.searchRepos({});

        expect(searchGitLabProjectsAPI).toHaveBeenCalledWith(
          expect.objectContaining({ orderBy: undefined })
        );
      });
    });

    describe('error cases', () => {
      it('should return error when API returns error', async () => {
        const mockApiResponse = {
          error: 'Unauthorized',
          status: 401,
          type: 'http' as const,
        };

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        const result = await provider.searchRepos({ keywords: ['test'] });

        expect(result.error).toBe('Unauthorized');
        expect(result.status).toBe(401);
      });

      it('should default status to 500 when not provided in error response', async () => {
        const mockApiResponse = {
          error: 'Server error',
          // No status field
        };

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const result = await provider.searchRepos({ keywords: ['test'] });

        expect(result.error).toBe('Server error');
        expect(result.status).toBe(500);
      });

      it('should handle error response without hints', async () => {
        const mockApiResponse = {
          error: 'Forbidden',
          status: 403,
          type: 'http' as const,
          // No hints field
        };

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        const result = await provider.searchRepos({ keywords: ['test'] });

        expect(result.error).toBe('Forbidden');
        expect(result.status).toBe(403);
        expect(result.hints).toBeUndefined();
      });

      it('should handle error response with hints', async () => {
        const mockApiResponse = {
          error: 'Rate limited',
          status: 429,
          type: 'http' as const,
          hints: ['Wait 60 seconds before retrying'],
        };

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(mockApiResponse);

        const result = await provider.searchRepos({ keywords: ['test'] });

        expect(result.error).toBe('Rate limited');
        expect(result.status).toBe(429);
        expect(result.hints).toContain('Wait 60 seconds before retrying');
      });

      it('should return error when no data returned', async () => {
        const mockApiResponse = {
          status: 200,
        };

        vi.mocked(searchGitLabProjectsAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const result = await provider.searchRepos({ keywords: ['test'] });

        expect(result.error).toBe('No data returned from GitLab API');
        expect(result.status).toBe(500);
      });

      it('should handle thrown exceptions', async () => {
        vi.mocked(searchGitLabProjectsAPI).mockRejectedValue(
          new Error('Rate limited')
        );

        const result = await provider.searchRepos({ keywords: ['test'] });

        expect(result.error).toBe('Rate limited');
        expect(result.status).toBe(500);
      });
    });
  });

  describe('searchPullRequests', () => {
    describe('success cases', () => {
      it('should search merge requests successfully', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [
              {
                iid: 42,
                title: 'Add new feature',
                description: 'This adds a great feature',
                web_url: 'https://gitlab.com/group/project/-/merge_requests/42',
                state: 'opened',
                draft: false,
                work_in_progress: false,
                author: { username: 'developer' },
                assignees: [
                  { username: 'reviewer1' },
                  { username: 'reviewer2' },
                ],
                labels: ['enhancement', 'priority:high'],
                source_branch: 'feature/new-feature',
                target_branch: 'main',
                diff_refs: { head_sha: 'abc123', base_sha: 'def456' },
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-15T00:00:00Z',
                closed_at: null,
                merged_at: null,
                user_notes_count: 5,
              },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 20,
              totalMatches: 1,
              hasMore: false,
            },
          },
          status: 200,
        } as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const query: PullRequestQuery = {
          projectId: '123',
          state: 'open',
        };

        const result = await provider.searchPullRequests(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('gitlab');
        expect(result.data).toBeDefined();
        expect(result.data!.items).toHaveLength(1);

        const mr = result.data!.items[0]!;
        expect(mr.number).toBe(42);
        expect(mr.title).toBe('Add new feature');
        expect(mr.body).toBe('This adds a great feature');
        expect(mr.state).toBe('open');
        expect(mr.draft).toBe(false);
        expect(mr.author).toBe('developer');
        expect(mr.assignees).toEqual(['reviewer1', 'reviewer2']);
        expect(mr.labels).toEqual(['enhancement', 'priority:high']);
        expect(mr.sourceBranch).toBe('feature/new-feature');
        expect(mr.targetBranch).toBe('main');
        expect(mr.sourceSha).toBe('abc123');
        expect(mr.targetSha).toBe('def456');
        expect(mr.commentsCount).toBe(5);
      });

      it('should pass all query parameters to API', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [],
            pagination: { hasMore: false },
          },
          status: 200,
        } as unknown as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const query: PullRequestQuery = {
          projectId: '123',
          number: 42,
          state: 'open',
          author: 'dev1',
          assignee: 'reviewer',
          labels: ['bug', 'urgent'],
          baseBranch: 'main',
          headBranch: 'fix/bug',
          created: '2024-01-01',
          updated: '2024-01-15',
          sort: 'created',
          order: 'desc',
          limit: 50,
          page: 2,
        };

        await provider.searchPullRequests(query);

        expect(searchGitLabMergeRequestsAPI).toHaveBeenCalledWith({
          projectId: 123,
          iid: 42,
          state: 'opened',
          authorUsername: 'dev1',
          assigneeUsername: 'reviewer',
          labels: ['bug', 'urgent'],
          targetBranch: 'main',
          sourceBranch: 'fix/bug',
          createdAfter: '2024-01-01',
          updatedAfter: '2024-01-15',
          orderBy: 'created_at',
          sort: 'desc',
          perPage: 50,
          page: 2,
        });
      });

      it('should map updated sort field to updated_at', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [],
            pagination: { hasMore: false },
          },
          status: 200,
        } as unknown as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        await provider.searchPullRequests({ sort: 'updated' });

        expect(searchGitLabMergeRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({ orderBy: 'updated_at' })
        );
      });

      it('should pass undefined for unsupported sort fields', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [],
            pagination: { hasMore: false },
          },
          status: 200,
        } as unknown as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        await provider.searchPullRequests({ sort: 'best-match' });

        expect(searchGitLabMergeRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({ orderBy: undefined })
        );
      });

      it('should search without projectId for global search', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [],
            pagination: { hasMore: false },
          },
          status: 200,
        } as unknown as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const query: PullRequestQuery = {
          author: 'developer',
          state: 'open',
        };

        await provider.searchPullRequests(query);

        expect(searchGitLabMergeRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            projectId: undefined,
          })
        );
      });

      it('should fetch comments when withComments is true', async () => {
        const mockMRResponse = {
          data: {
            mergeRequests: [
              {
                iid: 1,
                title: 'MR with comments',
                description: 'Test',
                web_url: 'https://gitlab.com/mr/1',
                state: 'opened',
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        const mockNotesResponse = {
          data: [
            {
              id: 101,
              author: { username: 'reviewer' },
              body: 'Great work!',
              created_at: '2024-01-02T00:00:00Z',
              updated_at: '2024-01-02T00:00:00Z',
            },
            {
              id: 102,
              author: { username: 'developer' },
              body: 'Thanks!',
              created_at: '2024-01-03T00:00:00Z',
              updated_at: '2024-01-03T00:00:00Z',
            },
          ],
          status: 200,
        } as any;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockMRResponse
        );
        vi.mocked(getGitLabMRNotes).mockResolvedValue(mockNotesResponse);

        const query: PullRequestQuery = {
          projectId: '123',
          withComments: true,
        };

        const result = await provider.searchPullRequests(query);

        expect(getGitLabMRNotes).toHaveBeenCalledWith(123, 1);
        expect(result.data!.items[0]!.comments).toHaveLength(2);
        expect(result.data!.items[0]!.comments![0]!.body).toBe('Great work!');
      });

      it('should handle notes with missing author gracefully', async () => {
        const mockMRResponse = {
          data: {
            mergeRequests: [
              {
                iid: 1,
                title: 'MR with notes',
                description: 'Test',
                web_url: 'https://gitlab.com/mr/1',
                state: 'opened',
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        const mockNotesResponse = {
          data: [
            {
              id: 201,
              author: null, // Missing author
              body: 'System note without author',
              created_at: '2024-01-02T00:00:00Z',
              updated_at: '2024-01-02T00:00:00Z',
            },
            {
              id: 202,
              // author undefined
              body: 'Note with undefined author',
              created_at: '2024-01-03T00:00:00Z',
              updated_at: '2024-01-03T00:00:00Z',
            },
          ],
          status: 200,
        } as any;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockMRResponse
        );
        vi.mocked(getGitLabMRNotes).mockResolvedValue(mockNotesResponse);

        const query: PullRequestQuery = {
          projectId: '123',
          withComments: true,
        };

        const result = await provider.searchPullRequests(query);

        expect(result.data!.items[0]!.comments).toHaveLength(2);
        expect(result.data!.items[0]!.comments![0]!.author).toBe('');
        expect(result.data!.items[0]!.comments![1]!.author).toBe('');
      });

      it('should ignore errors when fetching comments', async () => {
        const mockMRResponse = {
          data: {
            mergeRequests: [
              {
                iid: 1,
                title: 'MR',
                description: 'Test',
                web_url: 'https://gitlab.com/mr/1',
                state: 'opened',
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockMRResponse
        );
        vi.mocked(getGitLabMRNotes).mockRejectedValue(
          new Error('Notes unavailable')
        );

        const query: PullRequestQuery = {
          projectId: '123',
          withComments: true,
        };

        const result = await provider.searchPullRequests(query);

        expect(result.status).toBe(200);
        expect(result.data!.items).toHaveLength(1);
        expect(result.data!.items[0]!.comments).toBeUndefined();
      });

      it('should log rate limits from optional comments enrichment', async () => {
        const mockMRResponse = {
          data: {
            mergeRequests: [
              {
                iid: 1,
                title: 'MR',
                description: 'Test',
                web_url: 'https://gitlab.com/mr/1',
                state: 'opened',
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockMRResponse
        );
        vi.mocked(getGitLabMRNotes).mockResolvedValue({
          error: 'Rate limited',
          status: 429,
          type: 'http',
          rateLimitRemaining: 0,
          rateLimitReset: 1700000000,
          retryAfter: 30,
        } as any);

        const result = await provider.searchPullRequests({
          projectId: '123',
          withComments: true,
        });

        expect(result.status).toBe(200);
        expect(result.data!.items[0]!.comments).toBeUndefined();
        expect(mockLogRateLimit).toHaveBeenCalledWith(
          expect.objectContaining({
            provider: 'gitlab',
            retry_after_seconds: 30,
            rate_limit_remaining: 0,
          })
        );
      });

      it('should handle notes response without data', async () => {
        const mockMRResponse = {
          data: {
            mergeRequests: [
              {
                iid: 1,
                title: 'MR without notes data',
                description: 'Test',
                web_url: 'https://gitlab.com/mr/1',
                state: 'opened',
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        const mockNotesResponse = {
          error: 'Notes unavailable',
          status: 500,
          // No data field
        };

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockMRResponse
        );
        vi.mocked(getGitLabMRNotes).mockResolvedValue(mockNotesResponse as any);

        const query: PullRequestQuery = {
          projectId: '123',
          withComments: true,
        };

        const result = await provider.searchPullRequests(query);

        expect(result.status).toBe(200);
        expect(result.data!.items).toHaveLength(1);
        expect(result.data!.items[0]!.comments).toBeUndefined();
      });
    });

    describe('state mapping', () => {
      it.each([
        ['open', 'opened'],
        ['closed', 'closed'],
        ['merged', 'merged'],
        ['all', 'all'],
      ])('should map state %s to %s', async (input, expected) => {
        const mockApiResponse = {
          data: { mergeRequests: [], pagination: { hasMore: false } },
          status: 200,
        } as unknown as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        await provider.searchPullRequests({ state: input as any });

        expect(searchGitLabMergeRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({ state: expected })
        );
      });

      it('should handle undefined state', async () => {
        const mockApiResponse = {
          data: { mergeRequests: [], pagination: { hasMore: false } },
          status: 200,
        } as unknown as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        await provider.searchPullRequests({});

        expect(searchGitLabMergeRequestsAPI).toHaveBeenCalledWith(
          expect.objectContaining({ state: undefined })
        );
      });
    });

    describe('transformPullRequestResult - state mapping', () => {
      it('should map opened state to open', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [
              {
                iid: 1,
                title: 'Open MR',
                description: '',
                web_url: 'url',
                state: 'opened',
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({});

        expect(result.data!.items[0]!.state).toBe('open');
      });

      it('should map closed state to closed', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [
              {
                iid: 2,
                title: 'Closed MR',
                description: '',
                web_url: 'url',
                state: 'closed',
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
                closed_at: '2024-01-05T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({});

        expect(result.data!.items[0]!.state).toBe('closed');
      });

      it('should map merged state to merged', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [
              {
                iid: 3,
                title: 'Merged MR',
                description: '',
                web_url: 'url',
                state: 'merged',
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-10T00:00:00Z',
                merged_at: '2024-01-10T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({});

        expect(result.data!.items[0]!.state).toBe('merged');
      });

      it('should default unknown states to open', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [
              {
                iid: 4,
                title: 'Unknown state MR',
                description: '',
                web_url: 'url',
                state: 'unknown_state',
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as unknown as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({});

        expect(result.data!.items[0]!.state).toBe('open');
      });

      it('should handle draft MRs via draft flag', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [
              {
                iid: 5,
                title: 'Draft: WIP feature',
                description: '',
                web_url: 'url',
                state: 'opened',
                draft: true,
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({});

        expect(result.data!.items[0]!.draft).toBe(true);
      });

      it('should handle draft MRs via work_in_progress flag', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [
              {
                iid: 6,
                title: 'WIP: Old style draft',
                description: '',
                web_url: 'url',
                state: 'opened',
                draft: false,
                work_in_progress: true,
                author: { username: 'dev' },
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({});

        expect(result.data!.items[0]!.draft).toBe(true);
      });

      it('should handle missing author gracefully', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [
              {
                iid: 7,
                title: 'MR without author',
                description: '',
                web_url: 'url',
                state: 'opened',
                author: null,
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as unknown as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({});

        expect(result.data!.items[0]!.author).toBe('');
      });

      it('should handle missing assignees gracefully', async () => {
        const mockApiResponse = {
          data: {
            mergeRequests: [
              {
                iid: 8,
                title: 'MR without assignees',
                description: '',
                web_url: 'url',
                state: 'opened',
                author: { username: 'dev' },
                assignees: null,
                source_branch: 'feature',
                target_branch: 'main',
                created_at: '2024-01-01T00:00:00Z',
                updated_at: '2024-01-01T00:00:00Z',
              },
            ],
            pagination: { hasMore: false },
          },
          status: 200,
        } as unknown as MockMRResponse;

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({});

        expect(result.data!.items[0]!.assignees).toEqual([]);
      });
    });

    describe('error cases', () => {
      it('should return error when API returns error', async () => {
        const mockApiResponse = {
          error: 'Project not found',
          status: 404,
          type: 'http' as const,
          hints: ['Check the project ID'],
        };

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({ projectId: '999' });

        expect(result.error).toBe('Project not found');
        expect(result.status).toBe(404);
        expect(result.hints).toContain('Check the project ID');
      });

      it('should default status to 500 when not provided in error response', async () => {
        const mockApiResponse = {
          error: 'Server error',
          // No status field
        };

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const result = await provider.searchPullRequests({});

        expect(result.error).toBe('Server error');
        expect(result.status).toBe(500);
      });

      it('should handle error response without hints', async () => {
        const mockApiResponse = {
          error: 'Forbidden',
          status: 403,
          type: 'http' as const,
          // No hints field
        };

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.searchPullRequests({});

        expect(result.error).toBe('Forbidden');
        expect(result.status).toBe(403);
        expect(result.hints).toBeUndefined();
      });

      it('should return error when no data returned', async () => {
        const mockApiResponse = {
          status: 200,
        };

        vi.mocked(searchGitLabMergeRequestsAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const result = await provider.searchPullRequests({});

        expect(result.error).toBe('No data returned from GitLab API');
        expect(result.status).toBe(500);
      });

      it('should handle thrown exceptions', async () => {
        vi.mocked(searchGitLabMergeRequestsAPI).mockRejectedValue(
          new Error('API unavailable')
        );

        const result = await provider.searchPullRequests({});

        expect(result.error).toBe('API unavailable');
        expect(result.status).toBe(500);
      });
    });
  });

  describe('getRepoStructure', () => {
    describe('success cases', () => {
      it('should get repository structure successfully', async () => {
        const mockApiResponse = {
          data: {
            projectId: 123,
            projectPath: 'group/project',
            branch: 'main',
            path: '/',
            summary: {
              totalFiles: 10,
              totalFolders: 3,
              truncated: false,
              filtered: true,
              originalCount: 13,
            },
            structure: {
              '.': {
                files: ['README.md', 'package.json'],
                folders: ['src', 'tests'],
              },
              src: { files: ['index.ts', 'utils.ts'], folders: [] },
            },
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
              entriesPerPage: 20,
              totalEntries: 13,
            },
            hints: ['Project: group/project'],
          },
          status: 200,
        };

        vi.mocked(viewGitLabRepositoryStructureAPI).mockResolvedValue(
          mockApiResponse
        );

        const query: RepoStructureQuery = {
          projectId: '123',
        };

        const result = await provider.getRepoStructure(query);

        expect(result.status).toBe(200);
        expect(result.provider).toBe('gitlab');
        expect(result.data).toBeDefined();
        expect(result.data!.projectPath).toBe('group/project');
        expect(result.data!.branch).toBe('main');
        expect(result.data!.structure['.']).toBeDefined();
        expect(result.data!.summary.totalFiles).toBe(10);
      });

      it('should pass all query parameters to API', async () => {
        const mockApiResponse = {
          data: {
            projectId: 123,
            projectPath: 'group/project',
            branch: 'develop',
            path: 'src/',
            summary: {
              totalFiles: 5,
              totalFolders: 2,
              truncated: false,
              filtered: true,
              originalCount: 7,
            },
            structure: {},
            pagination: {
              currentPage: 2,
              totalPages: 3,
              hasMore: true,
              entriesPerPage: 10,
              totalEntries: 25,
            },
            hints: [],
          },
          status: 200,
        };

        vi.mocked(viewGitLabRepositoryStructureAPI).mockResolvedValue(
          mockApiResponse
        );

        const query: RepoStructureQuery = {
          projectId: '123',
          ref: 'develop',
          path: 'src/',
          recursive: true,
          entriesPerPage: 10,
          entryPageNumber: 2,
        };

        await provider.getRepoStructure(query);

        expect(viewGitLabRepositoryStructureAPI).toHaveBeenCalledWith({
          projectId: 123,
          ref: 'develop',
          path: 'src/',
          recursive: true,
          perPage: 10,
          page: 2,
        });
      });

      it('should handle URL-encoded project path', async () => {
        const mockApiResponse = {
          data: {
            projectId: 'group%2Fsubgroup%2Fproject',
            projectPath: 'group/subgroup/project',
            branch: 'main',
            path: '/',
            summary: {
              totalFiles: 1,
              totalFolders: 0,
              truncated: false,
              filtered: true,
              originalCount: 1,
            },
            structure: {},
            hints: [],
          },
          status: 200,
        };

        vi.mocked(viewGitLabRepositoryStructureAPI).mockResolvedValue(
          mockApiResponse
        );

        const query: RepoStructureQuery = {
          projectId: 'group/subgroup/project',
        };

        await provider.getRepoStructure(query);

        expect(viewGitLabRepositoryStructureAPI).toHaveBeenCalledWith(
          expect.objectContaining({
            projectId: 'group%2Fsubgroup%2Fproject',
          })
        );
      });
    });

    describe('error cases', () => {
      it('should return error when API returns error', async () => {
        const mockApiResponse = {
          error: 'Repository not found',
          status: 404,
          type: 'http' as const,
          hints: ['Check project ID and permissions'],
        };

        vi.mocked(viewGitLabRepositoryStructureAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.getRepoStructure({ projectId: '999' });

        expect(result.error).toBe('Repository not found');
        expect(result.status).toBe(404);
        expect(result.hints).toContain('Check project ID and permissions');
      });

      it('should default status to 500 when not provided in error response', async () => {
        const mockApiResponse = {
          error: 'Unknown server error',
          // No status field
        };

        vi.mocked(viewGitLabRepositoryStructureAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const result = await provider.getRepoStructure({ projectId: '123' });

        expect(result.error).toBe('Unknown server error');
        expect(result.status).toBe(500);
      });

      it('should handle error response without hints', async () => {
        const mockApiResponse = {
          error: 'Access denied',
          status: 403,
          type: 'http' as const,
          // No hints field
        };

        vi.mocked(viewGitLabRepositoryStructureAPI).mockResolvedValue(
          mockApiResponse
        );

        const result = await provider.getRepoStructure({ projectId: '123' });

        expect(result.error).toBe('Access denied');
        expect(result.status).toBe(403);
        expect(result.hints).toBeUndefined();
      });

      it('should return error when no data returned', async () => {
        const mockApiResponse = {
          status: 200,
        };

        vi.mocked(viewGitLabRepositoryStructureAPI).mockResolvedValue(
          mockApiResponse as any
        );

        const result = await provider.getRepoStructure({ projectId: '123' });

        expect(result.error).toBe('No data returned from GitLab API');
        expect(result.status).toBe(500);
      });

      it('should handle thrown exceptions', async () => {
        vi.mocked(viewGitLabRepositoryStructureAPI).mockRejectedValue(
          new Error('Timeout')
        );

        const result = await provider.getRepoStructure({ projectId: '123' });

        expect(result.error).toBe('Timeout');
        expect(result.status).toBe(500);
      });

      it('should throw error when projectId is missing', async () => {
        const query = {} as RepoStructureQuery;

        const result = await provider.getRepoStructure(query);

        expect(result.error).toBe('Project ID is required');
        expect(result.status).toBe(500);
      });
    });
  });

  describe('parseProjectId helper', () => {
    it('should parse numeric project ID', async () => {
      const mockApiResponse = {
        data: { items: [] },
        status: 200,
      };

      vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

      await provider.searchCode({ keywords: ['test'], projectId: '12345' });

      expect(searchGitLabCodeAPI).toHaveBeenCalledWith(
        expect.objectContaining({ projectId: 12345 })
      );
    });

    it('should URL-encode string project path', async () => {
      const mockApiResponse = {
        data: { items: [] },
        status: 200,
      };

      vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

      await provider.searchCode({
        keywords: ['test'],
        projectId: 'group/project',
      });

      expect(searchGitLabCodeAPI).toHaveBeenCalledWith(
        expect.objectContaining({ projectId: 'group%2Fproject' })
      );
    });

    it('should URL-encode nested project path', async () => {
      const mockApiResponse = {
        data: { items: [] },
        status: 200,
      };

      vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

      await provider.searchCode({
        keywords: ['test'],
        projectId: 'org/group/subgroup/project',
      });

      expect(searchGitLabCodeAPI).toHaveBeenCalledWith(
        expect.objectContaining({
          projectId: 'org%2Fgroup%2Fsubgroup%2Fproject',
        })
      );
    });

    it('should handle numeric string that looks like ID but has leading zeros', async () => {
      const mockApiResponse = {
        data: { items: [] },
        status: 200,
      };

      vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

      // "00123" parses to 123, but String(123) !== "00123", so it's treated as path
      await provider.searchCode({ keywords: ['test'], projectId: '00123' });

      expect(searchGitLabCodeAPI).toHaveBeenCalledWith(
        expect.objectContaining({ projectId: '00123' })
      );
    });

    it('should throw error when projectId is undefined', async () => {
      const result = await provider.searchCode({ keywords: ['test'] });

      expect(result.error).toBe('Project ID is required');
    });

    it('should throw error when projectId is empty string', async () => {
      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: '',
      });

      expect(result.error).toBe('Project ID is required');
    });
  });

  describe('handleError helper', () => {
    it('should extract message from Error instance', async () => {
      vi.mocked(searchGitLabCodeAPI).mockRejectedValue(
        new Error('Specific error message')
      );

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: '123',
      });

      expect(result.error).toBe('Specific error message');
      expect(result.status).toBe(500);
      expect(result.provider).toBe('gitlab');
    });

    it('should handle non-Error thrown values', async () => {
      vi.mocked(searchGitLabCodeAPI).mockRejectedValue({ custom: 'error' });

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: '123',
      });

      expect(result.error).toBe('An unknown error occurred');
      expect(result.status).toBe(500);
    });

    it('should handle null thrown value', async () => {
      vi.mocked(searchGitLabCodeAPI).mockRejectedValue(null);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: '123',
      });

      expect(result.error).toBe('An unknown error occurred');
    });

    it('should handle undefined thrown value', async () => {
      vi.mocked(searchGitLabCodeAPI).mockRejectedValue(undefined);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: '123',
      });

      expect(result.error).toBe('An unknown error occurred');
    });
  });

  describe('edge cases', () => {
    it('should handle empty search results', async () => {
      const mockApiResponse = {
        data: { items: [] },
        status: 200,
      };

      vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

      const result = await provider.searchCode({
        keywords: ['nonexistent'],
        projectId: '123',
      });

      expect(result.data!.items).toHaveLength(0);
      expect(result.data!.totalCount).toBe(0);
    });

    it('should handle special characters in project path', async () => {
      const mockApiResponse = {
        data: { items: [] },
        status: 200,
      };

      vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

      await provider.searchCode({
        keywords: ['test'],
        projectId: 'group/project-with-dash_and_underscore',
      });

      expect(searchGitLabCodeAPI).toHaveBeenCalledWith(
        expect.objectContaining({
          projectId: 'group%2Fproject-with-dash_and_underscore',
        })
      );
    });

    it('should handle unicode in project path', async () => {
      const mockApiResponse = {
        data: { items: [] },
        status: 200,
      };

      vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

      await provider.searchCode({
        keywords: ['test'],
        projectId: 'group/projet-francais',
      });

      expect(searchGitLabCodeAPI).toHaveBeenCalledWith(
        expect.objectContaining({
          projectId: 'group%2Fprojet-francais',
        })
      );
    });

    it('should handle API response with status but no error field', async () => {
      const mockApiResponse = {
        status: 500,
      };

      vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse as any);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: '123',
      });

      expect(result.error).toBe('No data returned from GitLab API');
      expect(result.status).toBe(500);
    });

    it('should preserve hints from API response', async () => {
      const mockApiResponse = {
        error: 'Rate limited',
        status: 429,
        type: 'http' as const,
        hints: ['Wait 60 seconds', 'Consider using a token'],
      };

      vi.mocked(searchGitLabCodeAPI).mockResolvedValue(mockApiResponse);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: '123',
      });

      expect(result.hints).toEqual([
        'Wait 60 seconds',
        'Consider using a token',
      ]);
    });
  });

  describe('resolveDefaultBranch', () => {
    it('should resolve default branch from GitLab Projects.show', async () => {
      const mockProjectsShow = vi
        .fn()
        .mockResolvedValue({ default_branch: 'develop' });
      mockGetGitlab.mockResolvedValue({ Projects: { show: mockProjectsShow } });

      const result = await provider.resolveDefaultBranch('group/project');

      expect(result).toBe('develop');
      expect(mockProjectsShow).toHaveBeenCalledWith('group%2Fproject');
    });

    it('should resolve default branch for numeric projectId', async () => {
      const mockProjectsShow = vi
        .fn()
        .mockResolvedValue({ default_branch: 'main' });
      mockGetGitlab.mockResolvedValue({ Projects: { show: mockProjectsShow } });

      const result = await provider.resolveDefaultBranch('12345');

      expect(result).toBe('main');
      expect(mockProjectsShow).toHaveBeenCalledWith(12345);
    });

    it('should fallback to "main" when API call fails', async () => {
      mockGetGitlab.mockRejectedValue(new Error('Network error'));

      const result = await provider.resolveDefaultBranch('group/project');

      expect(result).toBe('main');
    });

    it('should fallback to "main" when default_branch is not set', async () => {
      const mockProjectsShow = vi.fn().mockResolvedValue({});
      mockGetGitlab.mockResolvedValue({ Projects: { show: mockProjectsShow } });

      const result = await provider.resolveDefaultBranch('group/project');

      expect(result).toBe('main');
    });

    it('should pass config to getGitlab when available', async () => {
      const configuredProvider = new GitLabProvider({
        type: 'gitlab',
        baseUrl: 'https://gitlab.corp.com',
        token: 'glpat-custom',
      });
      const mockProjectsShow = vi
        .fn()
        .mockResolvedValue({ default_branch: 'trunk' });
      mockGetGitlab.mockResolvedValue({ Projects: { show: mockProjectsShow } });

      const result =
        await configuredProvider.resolveDefaultBranch('group/project');

      expect(result).toBe('trunk');
      expect(mockGetGitlab).toHaveBeenCalledWith({
        host: 'https://gitlab.corp.com',
        token: 'glpat-custom',
      });
    });
  });

  describe('logRateLimit on error', () => {
    it('should call logRateLimit when error has rate limit info', async () => {
      const rateLimitError = Object.assign(new Error('rate limited'), {
        status: 429,
        response: {
          status: 429,
          headers: {
            'retry-after': '30',
            'ratelimit-remaining': '0',
            'ratelimit-reset': '1700000000',
          },
        },
      });
      vi.mocked(searchGitLabCodeAPI).mockRejectedValue(rateLimitError);

      await provider.searchCode({
        keywords: ['test'],
        projectId: '123',
      });

      expect(mockLogRateLimit).toHaveBeenCalledWith(
        expect.objectContaining({
          provider: 'gitlab',
        })
      );
    });

    it('should not call logRateLimit when error has no rate limit info', async () => {
      vi.mocked(searchGitLabCodeAPI).mockRejectedValue(new Error('Not found'));

      await provider.searchCode({
        keywords: ['test'],
        projectId: '123',
      });

      expect(mockLogRateLimit).not.toHaveBeenCalled();
    });
  });
});
