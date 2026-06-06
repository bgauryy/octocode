import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import { getTextContent } from '../utils/testHelpers.js';

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../src/serverConfig.js', () => ({
  isLoggingEnabled: vi.fn(() => false),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  getGitHubToken: vi.fn(() => Promise.resolve('mock-token')),
}));

interface ProviderRepo {
  id: string;
  name: string;
  fullPath: string;
  description?: string;
  url: string;
  stars?: number;
  forks?: number;
  language?: string;
  topics?: string[];
  createdAt?: string;
  updatedAt?: string;
  pushedAt?: string;
  defaultBranch?: string;
  isPrivate?: boolean;
}

function repo(
  overrides: Partial<ProviderRepo> & { fullPath: string }
): ProviderRepo {
  const name = overrides.fullPath.split('/').pop() ?? 'repo';
  return {
    id: overrides.id ?? '1',
    name,
    description: 'desc',
    url: `https://github.com/${overrides.fullPath}`,
    stars: 100,
    forks: 10,
    language: 'TypeScript',
    topics: [],
    createdAt: '01/01/2020',
    updatedAt: '01/01/2024',
    pushedAt: '01/01/2024',
    defaultBranch: 'main',
    isPrivate: false,
    ...overrides,
  };
}

function okResponse(
  repositories: ProviderRepo[],
  pagination: {
    currentPage: number;
    totalPages: number;
    hasMore: boolean;
    entriesPerPage?: number;
    totalMatches?: number;
  } = { currentPage: 1, totalPages: 1, hasMore: false }
) {
  return {
    data: {
      repositories,
      totalCount: repositories.length,
      pagination,
    },
    status: 200,
    provider: 'github',
  };
}

describe('GitHub Search Repositories Coverage', () => {
  let mockProvider: {
    searchCode: ReturnType<typeof vi.fn>;
    getFileContent: ReturnType<typeof vi.fn>;
    searchRepos: ReturnType<typeof vi.fn>;
    searchPullRequests: ReturnType<typeof vi.fn>;
    getRepoStructure: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);
  });

  function call(query: Record<string, unknown>) {
    const mockServer = createMockMcpServer();
    registerSearchGitHubReposTool(mockServer.server);
    return mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
      queries: [query],
    });
  }

  describe('verbose flag — pass-through contract', () => {
    it('verbose:false — limit is not capped, returns all repos', async () => {
      const repos = [
        repo({ id: '1', fullPath: 'a/top-repo', stars: 900, language: 'Go' }),
        repo({ id: '2', fullPath: 'b/second', stars: 500 }),
        repo({ id: '3', fullPath: 'c/third', stars: 300 }),
        repo({ id: '4', fullPath: 'd/fourth', stars: 100 }),
      ];
      mockProvider.searchRepos.mockResolvedValue(okResponse(repos));

      const result = await call({
        id: 'verbose_false',
        verbose: false,
        keywordsToSearch: ['anything'],
        limit: 50,
      });

      const passedQuery = mockProvider.searchRepos.mock.calls[0]?.[0] as {
        limit?: number;
      };
      expect(passedQuery.limit).not.toBe(3);
      expect(result.isError).toBe(false);
    });

    it('verbose:false — default limit applies when limit not passed', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([repo({ fullPath: 'a/only' })])
      );

      const result = await call({
        id: 'verbose_false_no_limit',
        verbose: false,
        keywordsToSearch: ['x'],
      });

      expect(result.isError).toBe(false);
    });

    it('verbose:false — no (top:) summary hint when no repos found', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'verbose_false_empty',
        verbose: false,
        keywordsToSearch: ['nomatch'],
      });

      expect(result.isError).toBe(false);
      const text = getTextContent(result.content);
      expect(text).not.toContain('(top:');
    });
  });

  describe('sort branches', () => {
    it('sorts by forks when sort=forks', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([
          repo({ id: '1', fullPath: 'a/low', forks: 2, stars: 999 }),
          repo({ id: '2', fullPath: 'b/high', forks: 900, stars: 1 }),
        ])
      );

      const result = await call({
        id: 'sort_forks',
        keywordsToSearch: ['x'],
        sort: 'forks',
      });

      const structured = result.structuredContent as {
        results?: Array<{
          data?: { repositories?: Array<{ owner: string; repo: string }> };
        }>;
      };
      expect(structured.results?.[0]?.data?.repositories?.[0]).toMatchObject({
        owner: 'b',
        repo: 'high',
      });
    });

    it('sorts by updated date when sort=updated', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([
          repo({ id: '1', fullPath: 'a/old', updatedAt: '2020-01-01' }),
          repo({ id: '2', fullPath: 'b/new', updatedAt: '2024-06-01' }),
        ])
      );

      const result = await call({
        id: 'sort_updated',
        keywordsToSearch: ['x'],
        sort: 'updated',
      });

      const structured = result.structuredContent as {
        results?: Array<{
          data?: { repositories?: Array<{ owner: string; repo: string }> };
        }>;
      };
      expect(structured.results?.[0]?.data?.repositories?.[0]).toMatchObject({
        owner: 'b',
        repo: 'new',
      });
    });

    it('sorts by created date when sort=created, handling missing dates', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([
          repo({ id: '1', fullPath: 'a/nodate', createdAt: undefined }),
          repo({ id: '2', fullPath: 'b/dated', createdAt: '2024-06-01' }),
        ])
      );

      const result = await call({
        id: 'sort_created',
        keywordsToSearch: ['x'],
        sort: 'created',
      });

      const structured = result.structuredContent as {
        results?: Array<{
          data?: { repositories?: Array<{ owner: string; repo: string }> };
        }>;
      };
      expect(structured.results?.[0]?.data?.repositories?.[0]).toMatchObject({
        owner: 'b',
        repo: 'dated',
      });
    });

    it('falls back to relevance/stars when sort=best-match', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([
          repo({ id: '1', fullPath: 'a/low', stars: 5 }),
          repo({ id: '2', fullPath: 'b/high', stars: 5000 }),
        ])
      );

      const result = await call({
        id: 'sort_best_match',
        keywordsToSearch: ['x'],
        sort: 'best-match',
      });

      const structured = result.structuredContent as {
        results?: Array<{
          data?: { repositories?: Array<{ owner: string; repo: string }> };
        }>;
      };
      expect(structured.results?.[0]?.data?.repositories?.[0]).toMatchObject({
        owner: 'b',
        repo: 'high',
      });
    });
  });

  describe('relevance scoring', () => {
    it('boosts repos matching the requested language and term in topics/description', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([
          repo({
            id: '1',
            fullPath: 'org/unrelated',
            stars: 10000,
            language: 'Go',
            topics: ['misc'],
            description: 'nothing here',
          }),
          repo({
            id: '2',
            fullPath: 'org/whale',
            stars: 5,
            language: 'Python',
            topics: ['whale'],
            description: 'a whale tracker',
          }),
        ])
      );

      const result = await call({
        id: 'relevance_language',
        keywordsToSearch: ['whale'],
        language: 'python',
      });

      const structured = result.structuredContent as {
        results?: Array<{
          data?: { repositories?: Array<{ owner: string; repo: string }> };
        }>;
      };
      expect(structured.results?.[0]?.data?.repositories?.[0]).toMatchObject({
        owner: 'org',
        repo: 'whale',
      });
    });
  });

  describe('empty-result recovery hints', () => {
    it('names topics AND keywords when both present and empty', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'empty_both',
        topicsToSearch: ['t1'],
        keywordsToSearch: ['k1'],
        stars: '>1000',
        created: '>2020-01-01',
        updated: '>2023-01-01',
      });

      const text = getTextContent(result.content);
      expect(text).toContain('Drop topics, then keywords');
      expect(text).toContain('Filters (');
      expect(text).toContain('stars=');
      expect(text).toContain('created=');
      expect(text).toContain('updated=');
    });

    it('names topics-only recovery when only topics empty', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'empty_topics',
        topicsToSearch: ['t1'],
      });

      const text = getTextContent(result.content);
      expect(text).toContain('No topic match');
    });

    it('names keywords-only recovery when only keywords empty', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'empty_keywords',
        keywordsToSearch: ['k1'],
      });

      const text = getTextContent(result.content);
      expect(text).toContain('No keyword match');
    });
  });

  describe('merge & pagination', () => {
    it('preserves pagination for a single successful variant', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([repo({ fullPath: 'a/repo' })], {
          currentPage: 1,
          totalPages: 3,
          hasMore: true,
          entriesPerPage: 10,
          totalMatches: 25,
        })
      );

      const result = await call({
        id: 'single_pagination',
        keywordsToSearch: ['x'],
      });

      const text = getTextContent(result.content);
      expect(text).toContain('pagination:');
    });

    it('handles a topic search failing while the keyword search succeeds (labels Topic search failed)', async () => {
      mockProvider.searchRepos
        .mockResolvedValueOnce({
          error: 'Topic boom',
          status: 500,
          provider: 'github',
        })
        .mockResolvedValueOnce(okResponse([repo({ fullPath: 'kw/win' })]));

      const result = await call({
        id: 'topic_fail',
        topicsToSearch: ['t1'],
        keywordsToSearch: ['k1'],
      });

      const text = getTextContent(result.content);
      expect(text).toContain('Topic search failed: Topic boom');
      expect(text).toContain('Only keywords search succeeded');
      expect(text).toContain('kw/win');
    });
  });

  describe('verbose:true — full hints returned (metadata mode)', () => {
    it('returns full hints even when results are empty', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'verbose_true_empty',
        verbose: true,
        topicsToSearch: ['t1'],
      });

      expect(result.isError).toBe(false);
    });
  });

  describe('noisy results hint — no owner/language/stars filter', () => {
    it('emits narrowing hint when results are returned but no owner/language/stars given', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([
          repo({ fullPath: 'a/repo1' }),
          repo({ fullPath: 'b/repo2' }),
        ])
      );

      const result = await call({
        id: 'noisy_keywords',
        keywordsToSearch: ['typescript'],
      });

      const text = getTextContent(result.content);
      expect(result.isError).toBe(false);
      expect(text).toContain('owner');
    });

    it('does not emit narrowing hint when owner is provided', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([repo({ fullPath: 'myorg/repo1' })])
      );

      const result = await call({
        id: 'owner_scoped',
        keywordsToSearch: ['typescript'],
        owner: 'myorg',
      });

      const text = getTextContent(result.content);
      expect(result.isError).toBe(false);
      expect(text).not.toContain('Large result set with no owner');
    });
  });
});
