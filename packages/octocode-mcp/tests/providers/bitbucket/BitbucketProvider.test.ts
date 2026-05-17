import { describe, it, expect, vi, beforeEach } from 'vitest';
import { BitbucketProvider } from '../../../src/providers/bitbucket/BitbucketProvider.js';
import {
  transformCodeSearchResult,
  transformRepoSearchResult,
} from '../../../src/providers/bitbucket/bitbucketSearch.js';
import { parseBitbucketProjectId } from '../../../src/providers/bitbucket/utils.js';
import { transformFileContentResult } from '../../../src/providers/bitbucket/bitbucketContent.js';
import {
  mapPRState,
  transformPullRequestResult,
} from '../../../src/providers/bitbucket/bitbucketPullRequests.js';

import { searchBitbucketCodeAPI } from '../../../src/bitbucket/codeSearch.js';
import {
  fetchBitbucketFileContentAPI,
  getBitbucketDefaultBranch,
} from '../../../src/bitbucket/fileContent.js';
import { searchBitbucketReposAPI } from '../../../src/bitbucket/repoSearch.js';
import { searchBitbucketPRsAPI } from '../../../src/bitbucket/pullRequestSearch.js';
import { viewBitbucketRepoStructureAPI } from '../../../src/bitbucket/repoStructure.js';

vi.mock('../../../src/bitbucket/codeSearch.js');
vi.mock('../../../src/bitbucket/repoSearch.js');
vi.mock('../../../src/bitbucket/pullRequestSearch.js');
vi.mock('../../../src/bitbucket/repoStructure.js');
vi.mock('../../../src/bitbucket/fileContent.js', () => ({
  fetchBitbucketFileContentAPI: vi.fn(),
  getBitbucketDefaultBranch: vi.fn().mockResolvedValue('main'),
}));

const mockLogRateLimit = vi.hoisted(() => vi.fn());
vi.mock('../../../src/session.js', () => ({
  logRateLimit: mockLogRateLimit,
}));

describe('BitbucketProvider', () => {
  let provider: BitbucketProvider;

  beforeEach(() => {
    vi.clearAllMocks();
    provider = new BitbucketProvider();
  });

  describe('constructor', () => {
    it('should create provider with bitbucket type', () => {
      expect(provider.type).toBe('bitbucket');
    });

    it('should accept config parameter', () => {
      const p = new BitbucketProvider({
        type: 'bitbucket',
        baseUrl: 'https://api.bitbucket.org/2.0',
      });
      expect(p.type).toBe('bitbucket');
    });
  });

  describe('searchCode', () => {
    it('should return 400 when no workspace (no projectId)', async () => {
      const result = await provider.searchCode({ keywords: ['test'] });
      expect(result.error).toBeDefined();
      expect(result.status).toBe(400);
      expect(result.provider).toBe('bitbucket');
    });

    it('should call API and return transformed results on success', async () => {
      vi.mocked(searchBitbucketCodeAPI).mockResolvedValue({
        data: {
          items: [
            {
              type: 'code_search_result',
              content_matches: [
                {
                  lines: [
                    { line: 1, segments: [{ text: 'hello', match: true }] },
                  ],
                },
              ],
              file: {
                path: 'src/index.ts',
                type: 'commit_file',
                links: {
                  self: {
                    href: 'https://bb.org/ws/repo/src/main/src/index.ts',
                  },
                },
              },
            },
          ],
          totalCount: 1,
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
            totalMatches: 1,
          },
        },
        status: 200,
      });

      const result = await provider.searchCode({
        keywords: ['hello'],
        projectId: 'ws/repo',
      });
      expect(result.status).toBe(200);
      expect(result.data?.items).toHaveLength(1);
      expect(result.data?.items[0]!.path).toBe('src/index.ts');
    });

    it('should forward API errors', async () => {
      vi.mocked(searchBitbucketCodeAPI).mockResolvedValue({
        error: 'rate limited',
        status: 429,
        type: 'http',
        hints: ['wait'],
      });

      const result = await provider.searchCode({
        keywords: ['x'],
        projectId: 'ws/repo',
      });
      expect(result.error).toBe('rate limited');
      expect(result.status).toBe(429);
    });

    it('should handle null data response', async () => {
      vi.mocked(searchBitbucketCodeAPI).mockResolvedValue({
        data: undefined as never,
        status: 200,
      });

      const result = await provider.searchCode({
        keywords: ['x'],
        projectId: 'ws/repo',
      });
      expect(result.error).toContain('No data');
    });

    it('should handle thrown errors via handleError', async () => {
      vi.mocked(searchBitbucketCodeAPI).mockRejectedValue(
        Object.assign(new Error('timeout'), { status: 504 })
      );

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'ws/repo',
      });
      expect(result.error).toBeDefined();
      expect(result.provider).toBe('bitbucket');
    });
  });

  describe('getFileContent', () => {
    it('should resolve default branch when ref not provided', async () => {
      vi.mocked(getBitbucketDefaultBranch).mockResolvedValue('develop');
      vi.mocked(fetchBitbucketFileContentAPI).mockResolvedValue({
        data: {
          content: 'code',
          path: 'a.ts',
          size: 4,
          ref: 'develop',
          encoding: 'utf-8',
        },
        status: 200,
      });

      const result = await provider.getFileContent({
        projectId: 'ws/repo',
        path: 'a.ts',
      });
      expect(result.data?.ref).toBe('develop');
    });

    it('should use provided ref', async () => {
      vi.mocked(fetchBitbucketFileContentAPI).mockResolvedValue({
        data: {
          content: 'code',
          path: 'a.ts',
          size: 4,
          ref: 'v2',
          encoding: 'utf-8',
        },
        status: 200,
      });

      const result = await provider.getFileContent({
        projectId: 'ws/repo',
        path: 'a.ts',
        ref: 'v2',
      });
      expect(result.data?.ref).toBe('v2');
    });

    it('should forward API errors', async () => {
      vi.mocked(getBitbucketDefaultBranch).mockResolvedValue('main');
      vi.mocked(fetchBitbucketFileContentAPI).mockResolvedValue({
        error: 'Not found',
        status: 404,
        type: 'http',
        hints: ['check path'],
      });

      const result = await provider.getFileContent({
        projectId: 'ws/repo',
        path: 'missing.ts',
      });
      expect(result.error).toBe('Not found');
      expect(result.status).toBe(404);
    });

    it('should handle null data', async () => {
      vi.mocked(getBitbucketDefaultBranch).mockResolvedValue('main');
      vi.mocked(fetchBitbucketFileContentAPI).mockResolvedValue({
        data: undefined as never,
        status: 200,
      });

      const result = await provider.getFileContent({
        projectId: 'ws/repo',
        path: 'a.ts',
      });
      expect(result.error).toContain('No data');
    });

    it('should handle thrown errors via handleError', async () => {
      vi.mocked(fetchBitbucketFileContentAPI).mockRejectedValue(
        Object.assign(new Error('not found'), { status: 404 })
      );

      const result = await provider.getFileContent({
        projectId: 'ws/repo',
        path: 'a.ts',
      });
      expect(result.error).toBeDefined();
      expect(result.status).toBe(404);
    });
  });

  describe('searchRepos', () => {
    it('should return 400 when no owner/workspace', async () => {
      const result = await provider.searchRepos({ keywords: ['test'] });
      expect(result.error).toBeDefined();
      expect(result.status).toBe(400);
    });

    it('should call API and transform results', async () => {
      vi.mocked(searchBitbucketReposAPI).mockResolvedValue({
        data: {
          repositories: [
            {
              uuid: '{u1}',
              name: 'my-repo',
              full_name: 'ws/my-repo',
              slug: 'my-repo',
              description: 'Desc',
              is_private: false,
              language: 'TypeScript',
              mainbranch: { name: 'main', type: 'branch' },
              updated_on: '2024-01-01T00:00:00Z',
              created_on: '2023-01-01T00:00:00Z',
              links: {
                html: { href: 'https://bb.org/ws/my-repo' },
                clone: [
                  { href: 'https://bb.org/ws/my-repo.git', name: 'https' },
                ],
              },
            },
          ],
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
            totalMatches: 1,
          },
        },
        status: 200,
      });

      const result = await provider.searchRepos({
        owner: 'ws',
        keywords: ['my-repo'],
      });
      expect(result.status).toBe(200);
      expect(result.data?.repositories).toHaveLength(1);
      expect(result.data?.repositories[0]!.name).toBe('my-repo');
      expect(result.data?.repositories[0]!.cloneUrl).toContain('https');
    });

    it('should forward API errors', async () => {
      vi.mocked(searchBitbucketReposAPI).mockResolvedValue({
        error: 'Unauthorized',
        status: 401,
        type: 'http',
      });

      const result = await provider.searchRepos({ owner: 'ws' });
      expect(result.error).toBe('Unauthorized');
    });

    it('should handle null data', async () => {
      vi.mocked(searchBitbucketReposAPI).mockResolvedValue({
        data: undefined as never,
        status: 200,
      });

      const result = await provider.searchRepos({ owner: 'ws' });
      expect(result.error).toContain('No data');
    });

    it('should handle thrown errors via handleError', async () => {
      vi.mocked(searchBitbucketReposAPI).mockRejectedValue(
        new Error('connection failed')
      );

      const result = await provider.searchRepos({ owner: 'ws' });
      expect(result.error).toBeDefined();
      expect(result.provider).toBe('bitbucket');
    });
  });

  describe('searchPullRequests', () => {
    it('should return 400 when no projectId', async () => {
      const result = await provider.searchPullRequests({ state: 'open' });
      expect(result.error).toBeDefined();
      expect(result.status).toBe(400);
    });

    it('should call API and transform results', async () => {
      vi.mocked(searchBitbucketPRsAPI).mockResolvedValue({
        data: {
          pullRequests: [
            {
              id: 1,
              title: 'PR',
              description: 'desc',
              state: 'MERGED' as const,
              author: { display_name: 'john', uuid: '{u}' },
              source: { branch: { name: 'feat' }, commit: { hash: 'abc' } },
              destination: {
                branch: { name: 'main' },
                commit: { hash: 'def' },
              },
              created_on: '2024-01-01T00:00:00Z',
              updated_on: '2024-01-02T00:00:00Z',
              comment_count: 3,
            },
          ],
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
            totalMatches: 1,
          },
        },
        status: 200,
      });

      const result = await provider.searchPullRequests({
        projectId: 'ws/repo',
        state: 'merged',
      });

      expect(result.status).toBe(200);
      expect(result.data?.items).toHaveLength(1);
      expect(result.data?.items[0]!.state).toBe('merged');
      expect(result.data?.items[0]!.mergedAt).toBeDefined();
    });

    it('should forward API errors', async () => {
      vi.mocked(searchBitbucketPRsAPI).mockResolvedValue({
        error: 'Forbidden',
        status: 403,
        type: 'http',
      });

      const result = await provider.searchPullRequests({
        projectId: 'ws/repo',
      });
      expect(result.error).toBe('Forbidden');
    });

    it('should handle null data', async () => {
      vi.mocked(searchBitbucketPRsAPI).mockResolvedValue({
        data: undefined as never,
        status: 200,
      });

      const result = await provider.searchPullRequests({
        projectId: 'ws/repo',
      });
      expect(result.error).toContain('No data');
    });

    it('should pass withDiff for fullContent type', async () => {
      vi.mocked(searchBitbucketPRsAPI).mockResolvedValue({
        data: {
          pullRequests: [],
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
      });

      await provider.searchPullRequests({
        projectId: 'ws/repo',
        type: 'fullContent' as never,
      });

      expect(searchBitbucketPRsAPI).toHaveBeenCalledWith(
        expect.objectContaining({ withDiff: true })
      );
    });

    it('should pass withDiffstat for partialContent type', async () => {
      vi.mocked(searchBitbucketPRsAPI).mockResolvedValue({
        data: {
          pullRequests: [],
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
      });

      await provider.searchPullRequests({
        projectId: 'ws/repo',
        type: 'partialContent' as never,
      });

      expect(searchBitbucketPRsAPI).toHaveBeenCalledWith(
        expect.objectContaining({ withDiffstat: true })
      );
    });

    it('should handle thrown errors via handleError', async () => {
      vi.mocked(searchBitbucketPRsAPI).mockRejectedValue(
        Object.assign(new Error('unauthorized'), { status: 401 })
      );

      const result = await provider.searchPullRequests({
        projectId: 'ws/repo',
      });
      expect(result.error).toBeDefined();
      expect(result.status).toBe(401);
    });
  });

  describe('getRepoStructure', () => {
    it('should call API and build structure from entries', async () => {
      vi.mocked(viewBitbucketRepoStructureAPI).mockResolvedValue({
        data: {
          entries: [
            { type: 'commit_directory', path: 'src' },
            { type: 'commit_file', path: 'src/index.ts' },
            { type: 'commit_file', path: 'README.md' },
          ],
          branch: 'main',
          path: '',
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
            totalMatches: 3,
          },
        },
        status: 200,
      });

      const result = await provider.getRepoStructure({ projectId: 'ws/repo' });
      expect(result.status).toBe(200);
      expect(result.data?.summary.totalFiles).toBe(2);
      expect(result.data?.summary.totalFolders).toBe(1);
      expect(result.data?.structure).toBeDefined();
    });

    it('should forward API errors', async () => {
      vi.mocked(viewBitbucketRepoStructureAPI).mockResolvedValue({
        error: 'Not found',
        status: 404,
        type: 'http',
      });

      const result = await provider.getRepoStructure({ projectId: 'ws/repo' });
      expect(result.error).toBe('Not found');
    });

    it('should handle null data', async () => {
      vi.mocked(viewBitbucketRepoStructureAPI).mockResolvedValue({
        data: undefined as never,
        status: 200,
      });

      const result = await provider.getRepoStructure({ projectId: 'ws/repo' });
      expect(result.error).toContain('No data');
    });

    it('should build correct structure for nested files', async () => {
      vi.mocked(viewBitbucketRepoStructureAPI).mockResolvedValue({
        data: {
          entries: [
            { type: 'commit_directory', path: 'src' },
            { type: 'commit_directory', path: 'src/utils' },
            { type: 'commit_file', path: 'src/utils/helper.ts' },
            { type: 'commit_file', path: 'src/index.ts' },
          ],
          branch: 'main',
          path: '',
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
            totalMatches: 4,
          },
        },
        status: 200,
      });

      const result = await provider.getRepoStructure({ projectId: 'ws/repo' });
      expect(result.data?.structure['.']).toBeDefined();
      expect(result.data?.structure['.']!.folders).toContain('src');
    });

    it('should handle thrown errors via handleError', async () => {
      vi.mocked(viewBitbucketRepoStructureAPI).mockRejectedValue(
        Object.assign(new Error('forbidden'), { status: 403 })
      );

      const result = await provider.getRepoStructure({ projectId: 'ws/repo' });
      expect(result.error).toBeDefined();
      expect(result.status).toBe(403);
    });
  });

  describe('resolveDefaultBranch', () => {
    it('should resolve default branch from API', async () => {
      vi.mocked(getBitbucketDefaultBranch).mockResolvedValue('develop');

      const branch = await provider.resolveDefaultBranch('ws/repo');
      expect(branch).toBe('develop');
    });

    it('should fall back to "main" on API error', async () => {
      vi.mocked(getBitbucketDefaultBranch).mockRejectedValue(new Error('fail'));

      const branch = await provider.resolveDefaultBranch('ws/repo');
      expect(branch).toBe('main');
    });

    it('should fall back to "main" on invalid projectId', async () => {
      const branch = await provider.resolveDefaultBranch('invalid');
      expect(branch).toBe('main');
    });
  });

  describe('handleError', () => {
    it('should handle 429 errors without rate limit fields', async () => {
      vi.mocked(searchBitbucketCodeAPI).mockRejectedValue(
        Object.assign(new Error('rate limited'), { status: 429 })
      );

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'ws/repo',
      });

      expect(result.error).toBeDefined();
      expect(result.provider).toBe('bitbucket');
      expect(result.status).toBe(429);
      expect(result.rateLimit).toBeUndefined();
    });

    it('should log rate limit and return rateLimit info when 429 with retry-after header', async () => {
      const rateLimitError = Object.assign(new Error('Too Many Requests'), {
        status: 429,
        response: {
          headers: {
            get: (key: string) => (key === 'retry-after' ? '30' : null),
          },
        },
      });

      vi.mocked(searchBitbucketCodeAPI).mockRejectedValue(rateLimitError);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'ws/repo',
      });

      expect(result.error).toBeDefined();
      expect(result.status).toBe(429);
      expect(mockLogRateLimit).toHaveBeenCalledWith(
        expect.objectContaining({ limit_type: 'primary' })
      );
    });

    it('should not include rateLimit for non-rate-limit errors', async () => {
      vi.mocked(searchBitbucketCodeAPI).mockRejectedValue(
        new Error('generic error')
      );

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'ws/repo',
      });

      expect(result.error).toBeDefined();
      expect(result.rateLimit).toBeUndefined();
      expect(mockLogRateLimit).not.toHaveBeenCalled();
    });

    it('should handle TypeError (network errors)', async () => {
      vi.mocked(viewBitbucketRepoStructureAPI).mockRejectedValue(
        new TypeError('fetch failed')
      );

      const result = await provider.getRepoStructure({ projectId: 'ws/repo' });
      expect(result.error).toBeDefined();
      expect(result.provider).toBe('bitbucket');
    });

    it('should handle non-Error objects', async () => {
      vi.mocked(searchBitbucketCodeAPI).mockRejectedValue('string error');

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'ws/repo',
      });
      expect(result.error).toBeDefined();
      expect(result.status).toBe(500);
    });
  });
});

describe('transformCodeSearchResult', () => {
  it('should transform items with content matches', () => {
    const result = transformCodeSearchResult(
      [
        {
          type: 'code_search_result',
          content_matches: [
            {
              lines: [
                {
                  line: 1,
                  segments: [
                    { text: 'hello ', match: false },
                    { text: 'world', match: true },
                  ],
                },
              ],
            },
          ],
          file: {
            path: 'a.ts',
            type: 'commit_file',
            links: { self: { href: 'https://url' } },
          },
        },
      ],
      { keywords: ['world'], page: 2, limit: 5 }
    );

    expect(result.items).toHaveLength(1);
    expect(result.items[0]!.matches[0]!.context).toBe('hello world');
    expect(result.pagination.currentPage).toBe(2);
  });

  it('should handle empty content_matches', () => {
    const result = transformCodeSearchResult(
      [
        {
          type: 'code_search_result',
          content_matches: [],
          file: { path: 'b.ts', type: 'commit_file' },
        },
      ],
      { keywords: ['x'] }
    );

    expect(result.items[0]!.matches[0]!.context).toBe('');
  });

  it('should preserve repository identity from projectId', () => {
    const result = transformCodeSearchResult(
      [
        {
          type: 'code_search_result',
          content_matches: [],
          file: {
            path: 'packages/app/src/index.ts',
            type: 'commit_file',
            links: {
              self: {
                href: 'https://bitbucket.org/ws/repo/src/main/packages/app/src/index.ts',
              },
            },
          },
        },
      ],
      { keywords: ['index'], projectId: 'ws/repo' }
    );

    expect(result.items[0]!.repository.name).toBe('ws/repo');
    expect(result.repositoryContext).toEqual({ owner: 'ws', repo: 'repo' });
  });

  it('should parse repository identity from Bitbucket API self links', () => {
    const result = transformCodeSearchResult(
      [
        {
          type: 'code_search_result',
          content_matches: [],
          file: {
            path: 'packages/app/src/index.ts',
            type: 'commit_file',
            links: {
              self: {
                href: 'https://api.bitbucket.org/2.0/repositories/ws/repo/src/main/packages/app/src/index.ts',
              },
            },
          },
        },
      ],
      { keywords: ['index'] }
    );

    expect(result.items[0]!.repository.name).toBe('ws/repo');
  });
});

describe('transformRepoSearchResult', () => {
  it('should handle repos without optional fields', () => {
    const result = transformRepoSearchResult([
      {
        uuid: '{u}',
        name: 'repo',
        full_name: 'ws/repo',
        slug: 'repo',
        description: '',
        is_private: true,
        language: '',
        updated_on: '',
        created_on: '',
      },
    ]);

    expect(result.repositories[0]!.visibility).toBe('private');
    expect(result.repositories[0]!.defaultBranch).toBe('main');
    expect(result.repositories[0]!.forks).toBe(0);
  });

  it('should use pagination when provided', () => {
    const result = transformRepoSearchResult([], {
      currentPage: 3,
      totalPages: 5,
      hasMore: true,
      totalMatches: 50,
    });

    expect(result.pagination.currentPage).toBe(3);
    expect(result.totalCount).toBe(50);
  });
});

describe('transformFileContentResult', () => {
  it('should slice content by startLine and endLine', () => {
    const result = transformFileContentResult(
      {
        content: 'line1\nline2\nline3\nline4',
        path: 'f.ts',
        size: 20,
        ref: 'main',
        encoding: 'utf-8',
      },
      { projectId: 'ws/repo', path: 'f.ts', startLine: 2, endLine: 3 }
    );

    expect(result.content).toBe('line2\nline3');
    expect(result.isPartial).toBe(true);
    expect(result.startLine).toBe(2);
    expect(result.endLine).toBe(3);
  });

  it('should handle startLine only', () => {
    const result = transformFileContentResult(
      {
        content: 'a\nb\nc',
        path: 'f.ts',
        size: 5,
        ref: 'main',
        encoding: 'utf-8',
      },
      { projectId: 'ws/repo', path: 'f.ts', startLine: 2 }
    );

    expect(result.content).toBe('b\nc');
  });

  it('should return full content when no line params', () => {
    const result = transformFileContentResult(
      {
        content: 'full content',
        path: 'f.ts',
        size: 12,
        ref: 'main',
        encoding: 'utf-8',
      },
      { projectId: 'ws/repo', path: 'f.ts' }
    );

    expect(result.content).toBe('full content');
    expect(result.isPartial).toBe(false);
  });

  it('should prioritize matchString extraction over explicit line ranges', () => {
    const result = transformFileContentResult(
      {
        content: ['alpha', 'beta', 'target line', 'delta', 'epsilon'].join(
          '\n'
        ),
        path: 'f.ts',
        size: 32,
        ref: 'main',
        encoding: 'utf-8',
      },
      {
        projectId: 'ws/repo',
        path: 'f.ts',
        startLine: 1,
        endLine: 2,
        matchString: 'TARGET',
        matchStringContextLines: 1,
      }
    );

    expect(result.content).toBe('beta\ntarget line\ndelta');
    expect(result.startLine).toBe(2);
    expect(result.endLine).toBe(4);
  });

  it('should apply char pagination after content extraction', () => {
    const result = transformFileContentResult(
      {
        content: '1234567890abcdefghij',
        path: 'f.ts',
        size: 20,
        ref: 'main',
        encoding: 'utf-8',
      },
      {
        projectId: 'ws/repo',
        path: 'f.ts',
        charOffset: 5,
        charLength: 5,
      }
    );

    expect(result.content).toBe('67890');
    expect(result.pagination).toMatchObject({
      charOffset: 5,
      charLength: 5,
      totalChars: 20,
      hasMore: true,
    });
  });
});

describe('transformPullRequestResult', () => {
  it('should map DECLINED state to "closed"', () => {
    const result = transformPullRequestResult(
      [
        {
          id: 1,
          title: 'PR',
          description: 'desc',
          state: 'DECLINED' as const,
          author: { display_name: 'john', uuid: '{u}' },
          source: { branch: { name: 'feat' } },
          destination: { branch: { name: 'main' } },
          created_on: '2024-01-01T00:00:00Z',
          updated_on: '2024-01-01T00:00:00Z',
        },
      ],
      { currentPage: 1, totalPages: 1, hasMore: false, totalMatches: 1 }
    );

    expect(result.items[0]!.state).toBe('closed');
  });

  it('should map SUPERSEDED state to "closed"', () => {
    const result = transformPullRequestResult(
      [
        {
          id: 1,
          title: 'PR',
          description: 'desc',
          state: 'SUPERSEDED' as 'OPEN',
          author: { display_name: 'john', uuid: '{u}' },
          source: { branch: { name: 'feat' } },
          destination: { branch: { name: 'main' } },
          created_on: '2024-01-01T00:00:00Z',
          updated_on: '2024-01-01T00:00:00Z',
        },
      ],
      { currentPage: 1, totalPages: 1, hasMore: false }
    );

    expect(result.items[0]!.state).toBe('closed');
  });

  it('should include comments when provided', () => {
    const result = transformPullRequestResult(
      [
        {
          id: 1,
          title: 'PR',
          description: '',
          state: 'OPEN' as const,
          author: { display_name: 'john', uuid: '{u}' },
          source: { branch: { name: 'feat' } },
          destination: { branch: { name: 'main' } },
          created_on: '',
          updated_on: '',
        },
      ],
      { currentPage: 1, totalPages: 1, hasMore: false },
      [
        {
          id: 100,
          content: { raw: 'Nice!' },
          user: { display_name: 'reviewer' },
          created_on: '2024-01-01T00:00:00Z',
          updated_on: '2024-01-01T00:00:00Z',
        },
      ]
    );

    expect(result.items[0]!.comments).toHaveLength(1);
    expect(result.items[0]!.comments![0]!.body).toBe('Nice!');
  });

  it('should handle PR with null description', () => {
    const result = transformPullRequestResult(
      [
        {
          id: 1,
          title: 'PR',
          description: null as unknown as string,
          state: 'OPEN' as const,
          author: { display_name: 'john', uuid: '{u}' },
          source: { branch: { name: 'feat' } },
          destination: { branch: { name: 'main' } },
          created_on: '',
          updated_on: '',
        },
      ],
      { currentPage: 1, totalPages: 1, hasMore: false }
    );

    expect(result.items[0]!.body).toBeNull();
  });

  it('should use username when available for author', () => {
    const result = transformPullRequestResult(
      [
        {
          id: 1,
          title: 'PR',
          description: '',
          state: 'OPEN' as const,
          author: {
            display_name: 'John Doe',
            username: 'johnd',
            uuid: '{u}',
          },
          source: { branch: { name: 'feat' } },
          destination: { branch: { name: 'main' } },
          created_on: '',
          updated_on: '',
        },
      ],
      { currentPage: 1, totalPages: 1, hasMore: false }
    );

    expect(result.items[0]!.author).toBe('johnd');
  });

  it('should include file change summaries from diffstat for metadata mode', () => {
    const result = transformPullRequestResult(
      [
        {
          id: 1,
          title: 'PR',
          description: '',
          state: 'OPEN' as const,
          author: { display_name: 'john', uuid: '{u}' },
          source: { branch: { name: 'feat' } },
          destination: { branch: { name: 'main' } },
          created_on: '',
          updated_on: '',
        },
      ],
      { currentPage: 1, totalPages: 1, hasMore: false },
      undefined,
      [
        {
          type: 'diffstat',
          status: 'modified',
          new: { path: 'src/app.ts' },
          lines_added: 4,
          lines_removed: 1,
        },
      ],
      undefined,
      { projectId: 'ws/repo', type: 'metadata' }
    );

    expect(result.items[0]!.changedFilesCount).toBe(1);
    expect(result.items[0]!.additions).toBe(4);
    expect(result.items[0]!.deletions).toBe(1);
    expect(result.items[0]!.fileChanges).toEqual([
      {
        path: 'src/app.ts',
        status: 'modified',
        additions: 4,
        deletions: 1,
        patch: undefined,
      },
    ]);
  });

  it('should attach parsed patches for fullContent mode', () => {
    const diff = [
      'diff --git a/src/app.ts b/src/app.ts',
      '--- a/src/app.ts',
      '+++ b/src/app.ts',
      '@@ -1 +1 @@',
      '-old',
      '+new',
    ].join('\n');

    const result = transformPullRequestResult(
      [
        {
          id: 1,
          title: 'PR',
          description: '',
          state: 'OPEN' as const,
          author: { display_name: 'john', uuid: '{u}' },
          source: { branch: { name: 'feat' } },
          destination: { branch: { name: 'main' } },
          created_on: '',
          updated_on: '',
        },
      ],
      { currentPage: 1, totalPages: 1, hasMore: false },
      undefined,
      [
        {
          type: 'diffstat',
          status: 'modified',
          new: { path: 'src/app.ts' },
          lines_added: 1,
          lines_removed: 1,
        },
      ],
      diff,
      { projectId: 'ws/repo', type: 'fullContent' }
    );

    expect(result.items[0]!.fileChanges?.[0]!.patch).toContain('+new');
  });
});

describe('mapPRState', () => {
  it('should map "open" to "OPEN"', () => {
    expect(mapPRState('open')).toBe('OPEN');
  });

  it('should map "closed" to "DECLINED"', () => {
    expect(mapPRState('closed')).toBe('DECLINED');
  });

  it('should map "merged" to "MERGED"', () => {
    expect(mapPRState('merged')).toBe('MERGED');
  });

  it('should return undefined for no state', () => {
    expect(mapPRState()).toBeUndefined();
  });

  it('should throw for unknown state', () => {
    expect(() => mapPRState('draft')).toThrow(/Invalid Bitbucket PR state/);
  });

  it('should throw for other unknown states', () => {
    expect(() => mapPRState('foo')).toThrow(/Invalid Bitbucket PR state/);
    expect(() => mapPRState('pending')).toThrow(/Invalid Bitbucket PR state/);
  });

  it('should map "opened" to "OPEN"', () => {
    expect(mapPRState('opened')).toBe('OPEN');
  });

  it('should map "declined" to "DECLINED"', () => {
    expect(mapPRState('declined')).toBe('DECLINED');
  });

  it('should map "superseded" to "SUPERSEDED"', () => {
    expect(mapPRState('superseded')).toBe('SUPERSEDED');
  });

  it('should return undefined for "all"', () => {
    expect(mapPRState('all')).toBeUndefined();
  });

  it('should normalize case and whitespace', () => {
    expect(mapPRState('  OPEN  ')).toBe('OPEN');
    expect(mapPRState('Merged')).toBe('MERGED');
  });

  it('should throw for empty-after-trim state', () => {
    expect(() => mapPRState('  ')).toThrow(/Invalid Bitbucket PR state/);
  });
});

describe('transformFileContentResult — fallback branches', () => {
  it('uses query.path when data.path is undefined', () => {
    const result = transformFileContentResult(
      {
        content: 'some content',
        path: undefined as unknown as string,
        size: 12,
        ref: 'main',
        encoding: 'utf-8',
      },
      { projectId: 'ws/repo', path: 'fallback/path.ts' }
    );

    expect(result.path).toBe('fallback/path.ts');
  });

  it('uses 0 when data.size is undefined', () => {
    const result = transformFileContentResult(
      {
        content: 'some content',
        path: 'file.ts',
        size: undefined as unknown as number,
        ref: 'main',
        encoding: 'utf-8',
      },
      { projectId: 'ws/repo', path: 'file.ts' }
    );

    expect(result.size).toBe(0);
  });

  it('uses query.ref when data.ref is undefined', () => {
    const result = transformFileContentResult(
      {
        content: 'some content',
        path: 'file.ts',
        size: 12,
        ref: undefined as unknown as string,
        encoding: 'utf-8',
      },
      { projectId: 'ws/repo', path: 'file.ts', ref: 'develop' }
    );

    expect(result.ref).toBe('develop');
  });

  it('uses empty string when both data.ref and query.ref are absent', () => {
    const result = transformFileContentResult(
      {
        content: 'some content',
        path: 'file.ts',
        size: 12,
        ref: undefined as unknown as string,
        encoding: 'utf-8',
      },
      { projectId: 'ws/repo', path: 'file.ts' }
    );

    expect(result.ref).toBe('');
  });
});

describe('buildStructureFromEntries — basePath branches', () => {
  it('strips the basePath prefix from entry paths', async () => {
    vi.mocked(viewBitbucketRepoStructureAPI).mockResolvedValue({
      data: {
        entries: [
          { type: 'commit_file', path: 'src/index.ts' },
          { type: 'commit_directory', path: 'src/utils' },
          { type: 'commit_file', path: 'src/utils/helper.ts' },
        ],
        branch: 'main',
        path: 'src',
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: 3,
        },
      },
      status: 200,
    });

    const provider = new BitbucketProvider();
    const result = await provider.getRepoStructure({
      projectId: 'ws/repo',
      path: 'src',
    });

    expect(result.status).toBe(200);
    expect(result.data?.structure).toBeDefined();
  });

  it('handles path with leading slash after stripping basePath', async () => {
    vi.mocked(viewBitbucketRepoStructureAPI).mockResolvedValue({
      data: {
        entries: [
          { type: 'commit_file', path: 'src/index.ts' },
          { type: 'commit_file', path: 'src/other.ts' },
        ],
        branch: 'main',
        path: 'src/',
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: 2,
        },
      },
      status: 200,
    });

    const provider = new BitbucketProvider();
    const result = await provider.getRepoStructure({
      projectId: 'ws/repo',
      path: 'src/',
    });

    expect(result.status).toBe(200);
    expect(result.data?.structure).toBeDefined();
  });
});

describe('parseBitbucketProjectId', () => {
  it('should parse valid projectId', () => {
    const result = parseBitbucketProjectId('workspace/repo');
    expect(result.workspace).toBe('workspace');
    expect(result.repoSlug).toBe('repo');
  });

  it('should throw for undefined projectId', () => {
    expect(() => parseBitbucketProjectId()).toThrow('Project ID is required');
  });

  it('should throw for invalid format (no slash)', () => {
    expect(() => parseBitbucketProjectId('noslash')).toThrow(
      'Invalid Bitbucket projectId'
    );
  });

  it('should throw for too many parts', () => {
    expect(() => parseBitbucketProjectId('a/b/c')).toThrow(
      'Invalid Bitbucket projectId'
    );
  });

  it('should throw for empty parts', () => {
    expect(() => parseBitbucketProjectId('/repo')).toThrow(
      'Invalid Bitbucket projectId'
    );
    expect(() => parseBitbucketProjectId('ws/')).toThrow(
      'Invalid Bitbucket projectId'
    );
  });
});
