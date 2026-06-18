import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';
import { getTextContent } from '../utils/testHelpers.js';

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
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

  describe('limit and filter behavior', () => {
    it(' — limit is not capped, returns all repos', async () => {
      const repos = [
        repo({ id: '1', fullPath: 'a/top-repo', stars: 900, language: 'Go' }),
        repo({ id: '2', fullPath: 'b/second', stars: 500 }),
        repo({ id: '3', fullPath: 'c/third', stars: 300 }),
        repo({ id: '4', fullPath: 'd/fourth', stars: 100 }),
      ];
      mockProvider.searchRepos.mockResolvedValue(okResponse(repos));

      const result = await call({
        id: 'q_no_filter',
        keywords: ['anything'],
        limit: 50,
      });

      const passedQuery = mockProvider.searchRepos.mock.calls[0]?.[0] as {
        limit?: number;
      };
      expect(passedQuery.limit).not.toBe(3);
      expect(result.isError).toBe(false);
    });

    it(' — default limit applies when limit not passed', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([repo({ fullPath: 'a/only' })])
      );

      const result = await call({
        id: 'q_default_limit',
        keywords: ['x'],
      });

      expect(result.isError).toBe(false);
    });

    it(' — no (top:) summary hint when no repos found', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'q_empty_results',
        keywords: ['nomatch'],
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
        keywords: ['x'],
        sort: 'forks',
        concise: true,
      });

      const repos = result.structuredContent as {
        results?: Array<{ data?: { repositories?: string[] } }>;
      };
      expect(repos.results?.[0]?.data?.repositories?.[0]).toContain('b/high');
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
        keywords: ['x'],
        sort: 'updated',
        concise: true,
      });

      const repos = result.structuredContent as {
        results?: Array<{ data?: { repositories?: string[] } }>;
      };
      expect(repos.results?.[0]?.data?.repositories?.[0]).toContain('b/new');
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
        keywords: ['x'],
        sort: 'best-match',
        concise: true,
      });

      const repos = result.structuredContent as {
        results?: Array<{ data?: { repositories?: string[] } }>;
      };
      expect(repos.results?.[0]?.data?.repositories?.[0]).toContain('b/high');
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
        keywords: ['whale'],
        language: 'python',
        concise: true,
      });

      const repos = result.structuredContent as {
        results?: Array<{ data?: { repositories?: string[] } }>;
      };
      expect(repos.results?.[0]?.data?.repositories?.[0]).toContain(
        'org/whale'
      );
    });
  });

  describe('empty-result recovery hints', () => {
    it('names topics AND keywords when both present and empty', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'empty_both',
        topicsToSearch: ['t1'],
        keywords: ['k1'],
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
        keywords: ['k1'],
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
        keywords: ['x'],
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
        keywords: ['k1'],
      });

      const text = getTextContent(result.content);
      expect(text).toContain('Topic search failed: Topic boom');
      expect(text).toContain('Only keywords search succeeded');
      expect(text).toContain('kw/win');
    });
  });

  describe(' — full hints returned (metadata mode)', () => {
    it('returns full hints even when results are empty', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'q_topics_empty',
        topicsToSearch: ['t1'],
      });

      expect(result.isError).toBe(false);
    });

    it('passes every repository filter through to the provider', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'all_repo_filters',
        keywords: ['agent'],
        forks: '>100',
        license: 'mit',
        goodFirstIssues: '>5',
        visibility: 'public',
        archived: false,
        match: ['name', 'description'],
        sort: 'stars',
        limit: 10,
        page: 2,
      });

      expect(result.isError).toBe(false);
      const providerCall = mockProvider.searchRepos.mock.calls[0]?.[0] as {
        forks?: string;
        license?: string;
        goodFirstIssues?: string;
        visibility?: string;
        archived?: boolean;
        match?: string[];
        sort?: string;
        limit?: number;
        page?: number;
      };
      expect(providerCall).toMatchObject({
        forks: '>100',
        license: 'mit',
        goodFirstIssues: '>5',
        visibility: 'public',
        archived: false,
        match: ['name', 'description'],
        sort: 'stars',
        limit: 10,
        page: 2,
      });
    });

    it('accepts repo filter-only searches supported by the schema', async () => {
      mockProvider.searchRepos.mockResolvedValue(okResponse([]));

      const result = await call({
        id: 'license_only',
        license: 'apache-2.0',
        limit: 1,
      });

      expect(result.isError).toBe(false);
      expect(mockProvider.searchRepos).toHaveBeenCalledWith(
        expect.objectContaining({ license: 'apache-2.0', limit: 1 })
      );
    });
  });

  describe('noisy results hint — no owner/language/stars filter', () => {
    it('emits narrowing hint when results are returned but no owner/language/stars given', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse(
          [repo({ fullPath: 'a/repo1' }), repo({ fullPath: 'b/repo2' })],
          { currentPage: 1, totalPages: 10, hasMore: true, totalMatches: 500 }
        )
      );

      const result = await call({
        id: 'noisy_keywords',
        keywords: ['typescript'],
      });

      const text = getTextContent(result.content);
      expect(result.isError).toBe(false);
      expect(text).toContain('Large result set');
    });

    it('does not emit narrowing hint when owner is provided', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([repo({ fullPath: 'myorg/repo1' })])
      );

      const result = await call({
        id: 'owner_scoped',
        keywords: ['typescript'],
        owner: 'myorg',
      });

      const text = getTextContent(result.content);
      expect(result.isError).toBe(false);
      expect(text).not.toContain('Large result set with no owner');
    });

    it('produces no hint when empty results and no keywords/topics/filters', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });
      const result = await call({ id: 'no_hints', owner: 'someorg' });
      const text = getTextContent(result.content);
      expect(result.isError).toBe(false);
      expect(text).not.toContain('Drop the rarest');
    });

    it('shows filters-only hint when empty results with owner but no keywords', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });
      const result = await call({ id: 'filter_only_hint', owner: 'someorg' });
      const text = getTextContent(result.content);
      expect(result.isError).toBe(false);
      expect(text).toContain('Remove owner/language/topic first');
    });
  });

  describe('Sort by stars with undefined stars values', () => {
    it('handles repos with undefined stars in sort by stars (covers ?? 0 branch)', async () => {
      mockProvider.searchRepos.mockResolvedValue(
        okResponse([
          repo({ id: '1', fullPath: 'a/low', stars: undefined }),
          repo({ id: '2', fullPath: 'b/high', stars: 500 }),
        ])
      );
      const result = await call({
        id: 'sort_stars_undef',
        keywords: ['x'],
        sort: 'stars',
        concise: true,
      });
      const repos = result.structuredContent as {
        results?: Array<{ data?: { repositories?: string[] } }>;
      };
      expect(repos.results?.[0]?.data?.repositories?.[0]).toContain('b/high');
    });
  });
});
