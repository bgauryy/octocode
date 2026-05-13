import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  GitHubFetchContentDataSchema,
  GitHubFetchContentOutputSchema,
  GitHubSearchCodeDataSchema,
  GitHubSearchCodeOutputSchema,
} from '@octocodeai/octocode-core';
import { registerTools } from '../../src/tools/toolsManager.js';
import type { ToolConfig } from '../../src/tools/toolConfig.js';
import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import {
  createMockMcpServer,
  type MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { expectHasResultsData } from './assertions.js';
import { FLOW_CATALOG } from './catalog.js';

const mockGetProvider = vi.hoisted(() => vi.fn());
const mockGetServerConfig = vi.hoisted(() => vi.fn());
const mockIsToolInMetadata = vi.hoisted(() => vi.fn());
const mockGetActiveProvider = vi.hoisted(() => vi.fn());
const mockGetActiveProviderConfig = vi.hoisted(() => vi.fn());
const mockLogSessionError = vi.hoisted(() => vi.fn());
const mockLogToolCall = vi.hoisted(() => vi.fn());

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../src/session.js', () => ({
  logSessionError: mockLogSessionError,
  logToolCall: mockLogToolCall,
}));

vi.mock('../../src/serverConfig.js', () => ({
  getServerConfig: mockGetServerConfig,
  getActiveProviderConfig: mockGetActiveProviderConfig,
  getActiveProvider: mockGetActiveProvider,
  isLocalEnabled: vi.fn(() => false),
  isCloneEnabled: vi.fn(() => false),
  isLoggingEnabled: vi.fn(() => false),
}));

const remoteFlowToolLoader = (): ToolConfig[] => [
  {
    name: 'githubSearchCode',
    description: 'Flow test description',
    isDefault: true,
    isLocal: false,
    type: 'search',
    fn: registerGitHubSearchCodeTool,
  },
  {
    name: 'githubGetFileContent',
    description: 'Flow test description',
    isDefault: true,
    isLocal: false,
    type: 'content',
    fn: registerFetchGitHubFileContentTool,
  },
];

async function registerRemoteFlowTools(server: MockMcpServer['server']) {
  return registerTools(server, undefined, {
    toolLoader: remoteFlowToolLoader,
    metadataGateway: { hasTool: mockIsToolInMetadata },
  });
}

vi.mock('../../src/tools/toolMetadata/proxies.js', async () => {
  const actual = await vi.importActual<
    typeof import('../../src/tools/toolMetadata/proxies.js')
  >('../../src/tools/toolMetadata/proxies.js');
  const { STATIC_TOOL_NAMES } = await import('../../src/tools/toolNames.js');

  return {
    ...actual,
    isToolInMetadata: mockIsToolInMetadata,
    TOOL_NAMES: STATIC_TOOL_NAMES as typeof actual.TOOL_NAMES,
    DESCRIPTIONS: new Proxy(
      {},
      {
        get: () => 'Flow test description',
      }
    ),
  };
});

describe(FLOW_CATALOG.remoteSearchToFetchContent.id, () => {
  const providerFlows = [
    {
      provider: 'github' as const,
      baseUrl: undefined as string | undefined,
      token: 'github-token',
      owner: 'octocat',
      repo: 'octokit',
      urlPrefix: 'https://github.com',
    },
    {
      provider: 'gitlab' as const,
      baseUrl: 'https://gitlab.example.com',
      token: 'gitlab-token',
      owner: 'group',
      repo: 'project',
      urlPrefix: 'https://gitlab.example.com',
    },
    {
      provider: 'bitbucket' as const,
      baseUrl: 'https://api.bitbucket.org',
      token: 'bitbucket-token',
      owner: 'workspace',
      repo: 'repo',
      urlPrefix: 'https://bitbucket.org',
    },
  ];

  let mockServer: MockMcpServer;
  let mockProvider: {
    capabilities: {
      cloneRepo: boolean;
      fetchDirectoryToDisk: boolean;
      requiresScopedCodeSearch: boolean;
      supportsMergedState: boolean;
      supportsMultiTopicSearch: boolean;
    };
    searchCode: ReturnType<typeof vi.fn>;
    getFileContent: ReturnType<typeof vi.fn>;
    searchRepos: ReturnType<typeof vi.fn>;
    searchPullRequests: ReturnType<typeof vi.fn>;
    getRepoStructure: ReturnType<typeof vi.fn>;
    resolveDefaultBranch: ReturnType<typeof vi.fn>;
  };

  function setupActiveProvider(provider: (typeof providerFlows)[number]) {
    mockGetActiveProvider.mockReturnValue(provider.provider);
    mockGetActiveProviderConfig.mockReturnValue({
      provider: provider.provider,
      baseUrl: provider.baseUrl,
      token: provider.token,
    });
  }

  beforeEach(async () => {
    vi.clearAllMocks();
    mockGetServerConfig.mockReturnValue({
      toolsToRun: ['githubSearchCode', 'githubGetFileContent'],
      enableTools: [],
      disableTools: [],
    });
    mockIsToolInMetadata.mockReturnValue(true);
    mockServer = createMockMcpServer();
    mockProvider = {
      capabilities: {
        cloneRepo: false,
        fetchDirectoryToDisk: false,
        requiresScopedCodeSearch: false,
        supportsMergedState: true,
        supportsMultiTopicSearch: true,
      },
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
      resolveDefaultBranch: vi.fn().mockResolvedValue('main'),
    };
    mockGetProvider.mockReturnValue(mockProvider);
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  it.each(providerFlows)(
    'chains remote search->fetch for %s provider',
    async providerCase => {
      setupActiveProvider(providerCase);
      const result = await registerRemoteFlowTools(mockServer.server);
      expect(result.successCount).toBe(2);
      expect(result.failedTools).toEqual([]);

      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/score.ts',
              matches: [
                {
                  context:
                    'export function computeScore(input: ScoreInput): number {',
                  positions: [[16, 28]],
                },
              ],
              url: `${providerCase.urlPrefix}/${providerCase.owner}/${providerCase.repo}/-/blob/main/src/score.ts`,
              repository: {
                id: '42',
                name: `${providerCase.owner}/${providerCase.repo}`,
                url: `${providerCase.urlPrefix}/${providerCase.owner}/${providerCase.repo}`,
              },
              lastModifiedAt: '2026-03-13T10:00:00.000Z',
            },
          ],
          totalCount: 1,
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
            entriesPerPage: 10,
            totalMatches: 1,
          },
          repositoryContext: {
            owner: providerCase.owner,
            repo: providerCase.repo,
            branch: 'main',
          },
        },
        status: 200,
        provider: providerCase.provider,
      });

      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'src/score.ts',
          content:
            'export function computeScore(input: ScoreInput): number {\n  return input.value + input.bonus;\n}\n',
          encoding: 'utf-8',
          size: 96,
          ref: 'main',
          lastModified: '2026-03-13T10:00:00.000Z',
        },
        status: 200,
        provider: providerCase.provider,
      });

      const searchResponse = await mockServer.callTool('githubSearchCode', {
        queries: [
          {
            id: `remote_search_score_${providerCase.provider}`,
            owner: providerCase.owner,
            repo: providerCase.repo,
            keywordsToSearch: ['computeScore'],
            path: 'src',
            match: 'file',
            researchGoal: `Find the computeScore implementation in ${providerCase.provider}`,
            reasoning: 'Need a remote file path before fetching content',
          },
        ],
      });

      const searchData = expectHasResultsData(
        GitHubSearchCodeOutputSchema,
        GitHubSearchCodeDataSchema,
        searchResponse
      );
      const matchedFile = searchData.files?.[0];

      expect(matchedFile).toBeDefined();
      expect(matchedFile?.path).toBe('src/score.ts');
      expect(matchedFile?.owner).toBe(providerCase.owner);
      expect(matchedFile?.repo).toBe(providerCase.repo);
      expect(mockGetProvider).toHaveBeenCalledWith(
        providerCase.provider,
        expect.objectContaining({
          type: providerCase.provider,
          baseUrl: providerCase.baseUrl,
          token: providerCase.token,
        })
      );
      expect(mockProvider.searchCode).toHaveBeenCalledWith(
        expect.objectContaining({
          keywords: ['computeScore'],
          projectId: `${providerCase.owner}/${providerCase.repo}`,
          path: 'src',
        })
      );

      const fetchResponse = await mockServer.callTool('githubGetFileContent', {
        queries: [
          {
            id: `remote_fetch_score_${providerCase.provider}`,
            owner: matchedFile!.owner,
            repo: matchedFile!.repo,
            path: matchedFile!.path,
            branch: searchData.repositoryContext?.branch,
            matchString: 'export function computeScore',
            researchGoal: 'Read the matched remote file',
            reasoning: 'Use the path handoff from remote code search',
          },
        ],
      });

      const fetchData = expectHasResultsData(
        GitHubFetchContentOutputSchema,
        GitHubFetchContentDataSchema,
        fetchResponse
      );

      expect(fetchData.content).toContain('computeScore');
      expect(fetchData.lastModified).toBe('2026-03-13T10:00:00.000Z');
      expect(mockProvider.getFileContent).toHaveBeenCalledWith(
        expect.objectContaining({
          projectId: `${providerCase.owner}/${providerCase.repo}`,
          path: 'src/score.ts',
          ref: searchData.repositoryContext?.branch,
        })
      );
    }
  );

  it.each(providerFlows)(
    'rejects dangerous payload keys before provider execution for %s provider',
    async providerCase => {
      setupActiveProvider(providerCase);
      const result = await registerRemoteFlowTools(mockServer.server);
      expect(result.successCount).toBe(2);
      expect(result.failedTools).toEqual([]);

      const maliciousPayload = JSON.parse(`{
        "queries": [
          {
            "id": "security_bad_payload_${providerCase.provider}",
            "owner": "${providerCase.owner}",
            "repo": "${providerCase.repo}",
            "keywordsToSearch": ["computeScore"],
            "path": "src",
            "match": "file",
            "researchGoal": "Attempt unsafe payload key injection",
            "reasoning": "Security flow coverage"
          }
        ],
        "__proto__": {
          "polluted": true
        }
      }`) as Record<string, unknown>;

      const response = await mockServer.callTool(
        'githubSearchCode',
        maliciousPayload
      );

      expect(response.isError).toBe(true);
      expect(mockProvider.searchCode).not.toHaveBeenCalled();
      const textContent = response.content.find(item => item.type === 'text');
      expect(textContent?.text).toContain('Security validation failed');
    }
  );

  it.each(providerFlows)(
    'reuses handed-off branch without default-branch lookups for %s provider',
    async providerCase => {
      setupActiveProvider(providerCase);
      const result = await registerRemoteFlowTools(mockServer.server);
      expect(result.successCount).toBe(2);
      expect(result.failedTools).toEqual([]);

      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/score.ts',
              matches: [
                {
                  context:
                    'export function computeScore(input: ScoreInput): number {',
                  positions: [[16, 28]],
                },
              ],
              url: `${providerCase.urlPrefix}/${providerCase.owner}/${providerCase.repo}/-/blob/main/src/score.ts`,
              repository: {
                id: '42',
                name: `${providerCase.owner}/${providerCase.repo}`,
                url: `${providerCase.urlPrefix}/${providerCase.owner}/${providerCase.repo}`,
              },
              lastModifiedAt: '2026-03-13T10:00:00.000Z',
            },
          ],
          totalCount: 1,
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
            entriesPerPage: 10,
            totalMatches: 1,
          },
          repositoryContext: {
            owner: providerCase.owner,
            repo: providerCase.repo,
            branch: 'main',
          },
        },
        status: 200,
        provider: providerCase.provider,
      });

      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'src/score.ts',
          content:
            'export function computeScore(input: ScoreInput): number {\n  return input.value + input.bonus;\n}\n',
          encoding: 'utf-8',
          size: 96,
          ref: 'main',
          lastModified: '2026-03-13T10:00:00.000Z',
        },
        status: 200,
        provider: providerCase.provider,
      });

      const searchResponse = await mockServer.callTool('githubSearchCode', {
        queries: [
          {
            id: `remote_branch_handoff_${providerCase.provider}`,
            owner: providerCase.owner,
            repo: providerCase.repo,
            keywordsToSearch: ['computeScore'],
            path: 'src',
            match: 'file',
            researchGoal: 'Find remote file with explicit branch handoff',
            reasoning:
              'Efficiency flow coverage for default branch lookup avoidance',
          },
        ],
      });

      const searchData = expectHasResultsData(
        GitHubSearchCodeOutputSchema,
        GitHubSearchCodeDataSchema,
        searchResponse
      );

      await mockServer.callTool('githubGetFileContent', {
        queries: [
          {
            id: `remote_branch_fetch_${providerCase.provider}`,
            owner: providerCase.owner,
            repo: providerCase.repo,
            path: searchData.files![0]!.path,
            branch: searchData.repositoryContext?.branch,
            researchGoal: 'Read fetched file from handed-off branch',
            reasoning: 'Branch is already known from search response',
          },
        ],
      });

      expect(mockProvider.getFileContent).toHaveBeenCalledTimes(1);
      expect(mockProvider.resolveDefaultBranch).not.toHaveBeenCalled();
    }
  );

  it.each(providerFlows)(
    'continues remote search->fetch across query-level output pagination for %s provider',
    async providerCase => {
      setupActiveProvider(providerCase);
      const result = await registerRemoteFlowTools(mockServer.server);
      expect(result.successCount).toBe(2);
      expect(result.failedTools).toEqual([]);

      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: Array.from({ length: 3 }, (_, index) => ({
            path: `src/score-${index + 1}.ts`,
            matches: [
              {
                context: `export function computeScore${index + 1}(input: ScoreInput): number {\n${'x'.repeat(2000)}\n}`,
                positions: [[16, 29]],
              },
            ],
            url: `${providerCase.urlPrefix}/${providerCase.owner}/${providerCase.repo}/-/blob/main/src/score-${index + 1}.ts`,
            repository: {
              id: '42',
              name: `${providerCase.owner}/${providerCase.repo}`,
              url: `${providerCase.urlPrefix}/${providerCase.owner}/${providerCase.repo}`,
            },
            lastModifiedAt: '2026-03-13T10:00:00.000Z',
          })),
          totalCount: 3,
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
            entriesPerPage: 10,
            totalMatches: 3,
          },
          repositoryContext: {
            owner: providerCase.owner,
            repo: providerCase.repo,
            branch: 'main',
          },
        },
        status: 200,
        provider: providerCase.provider,
      });

      mockProvider.getFileContent.mockImplementation(async query => ({
        data: {
          path: query.path,
          content: `export function ${query.path.replace(/[/.-]/g, '_')}() {\n  return 'paged';\n}\n`,
          encoding: 'utf-8',
          size: 96,
          ref: 'main',
          lastModified: '2026-03-13T10:00:00.000Z',
        },
        status: 200,
        provider: providerCase.provider,
      }));

      const firstSearchResponse = await mockServer.callTool(
        'githubSearchCode',
        {
          queries: [
            {
              id: `remote_paged_search_${providerCase.provider}`,
              owner: providerCase.owner,
              repo: providerCase.repo,
              keywordsToSearch: ['computeScore'],
              path: 'src',
              match: 'file',
              charLength: 600,
              researchGoal: `Find paginated computeScore results in ${providerCase.provider}`,
              reasoning:
                'Verify query-level output pagination still allows fetch handoff',
            },
          ],
        }
      );

      const firstSearchData = expectHasResultsData(
        GitHubSearchCodeOutputSchema,
        GitHubSearchCodeDataSchema,
        firstSearchResponse
      );

      expect(firstSearchData.outputPagination?.hasMore).toBe(true);
      expect(firstSearchData.files?.length).toBe(1);

      const nextCharOffset =
        (firstSearchData.outputPagination?.charOffset ?? 0) +
        (firstSearchData.outputPagination?.charLength ?? 0);

      const secondSearchResponse = await mockServer.callTool(
        'githubSearchCode',
        {
          queries: [
            {
              id: `remote_paged_search_${providerCase.provider}`,
              owner: providerCase.owner,
              repo: providerCase.repo,
              keywordsToSearch: ['computeScore'],
              path: 'src',
              match: 'file',
              charLength: 600,
              charOffset: nextCharOffset,
              researchGoal: `Continue paginated computeScore results in ${providerCase.provider}`,
              reasoning:
                'Use outputPagination metadata to resume the same search query',
            },
          ],
        }
      );

      const secondSearchData = expectHasResultsData(
        GitHubSearchCodeOutputSchema,
        GitHubSearchCodeDataSchema,
        secondSearchResponse
      );

      expect(
        secondSearchData.files?.[0]?.path !==
          firstSearchData.files?.[0]?.path ||
          secondSearchData.files?.[0]?.text_matches?.[0] !==
            firstSearchData.files?.[0]?.text_matches?.[0]
      ).toBe(true);

      const fetchResponse = await mockServer.callTool('githubGetFileContent', {
        queries: [
          {
            id: `remote_fetch_paged_${providerCase.provider}`,
            owner: providerCase.owner,
            repo: providerCase.repo,
            path: secondSearchData.files![0]!.path,
            branch: secondSearchData.repositoryContext?.branch,
            researchGoal: 'Read the file returned on the next output page',
            reasoning:
              'Verify paginated search results can still hand off to fetch content',
          },
        ],
      });

      const fetchData = expectHasResultsData(
        GitHubFetchContentOutputSchema,
        GitHubFetchContentDataSchema,
        fetchResponse
      );

      expect(fetchData.content).toContain(
        secondSearchData.files![0]!.path.replace(/[/.-]/g, '_')
      );
      expect(mockProvider.getFileContent).toHaveBeenLastCalledWith(
        expect.objectContaining({
          path: secondSearchData.files![0]!.path,
        })
      );
    }
  );

  it.each(providerFlows)(
    'continues remote search->fetch across bulk response pagination for %s provider',
    async providerCase => {
      setupActiveProvider(providerCase);
      const result = await registerRemoteFlowTools(mockServer.server);
      expect(result.successCount).toBe(2);
      expect(result.failedTools).toEqual([]);

      mockProvider.searchCode.mockImplementation(async query => {
        const keyword = query.keywords?.[0] ?? 'unknown';

        return {
          data: {
            items: [
              {
                path: `src/${keyword}.ts`,
                matches: [
                  {
                    context: `export function ${keyword}() {\n${'y'.repeat(400)}\n}`,
                    positions: [[16, 16 + keyword.length]],
                  },
                ],
                url: `${providerCase.urlPrefix}/${providerCase.owner}/${providerCase.repo}/-/blob/main/src/${keyword}.ts`,
                repository: {
                  id: '42',
                  name: `${providerCase.owner}/${providerCase.repo}`,
                  url: `${providerCase.urlPrefix}/${providerCase.owner}/${providerCase.repo}`,
                },
                lastModifiedAt: '2026-03-13T10:00:00.000Z',
              },
            ],
            totalCount: 1,
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
              entriesPerPage: 10,
              totalMatches: 1,
            },
            repositoryContext: {
              owner: providerCase.owner,
              repo: providerCase.repo,
              branch: 'main',
            },
          },
          status: 200,
          provider: providerCase.provider,
        };
      });

      mockProvider.getFileContent.mockImplementation(async query => ({
        data: {
          path: query.path,
          content: `export function ${query.path.replace(/[/.-]/g, '_')}() {\n  return 'bulk';\n}\n`,
          encoding: 'utf-8',
          size: 96,
          ref: 'main',
          lastModified: '2026-03-13T10:00:00.000Z',
        },
        status: 200,
        provider: providerCase.provider,
      }));

      const queries = [
        {
          id: `bulk_page_one_${providerCase.provider}`,
          owner: providerCase.owner,
          repo: providerCase.repo,
          keywordsToSearch: ['computeScore'],
          path: 'src',
          match: 'file',
          researchGoal: `Find computeScore in ${providerCase.provider}`,
          reasoning: 'First bulk query for pagination flow coverage',
        },
        {
          id: `bulk_page_two_${providerCase.provider}`,
          owner: providerCase.owner,
          repo: providerCase.repo,
          keywordsToSearch: ['buildSummary'],
          path: 'src',
          match: 'file',
          researchGoal: `Find buildSummary in ${providerCase.provider}`,
          reasoning: 'Second bulk query for pagination flow coverage',
        },
      ];

      const firstResponse = await mockServer.callTool('githubSearchCode', {
        queries,
        responseCharLength: 200,
      });

      const firstParsed = GitHubSearchCodeOutputSchema.parse(
        firstResponse.structuredContent
      );

      expect(firstParsed.responsePagination?.hasMore).toBe(true);
      expect(firstParsed.results).toHaveLength(1);
      expect(firstParsed.results[0]?.id).toBe(
        `bulk_page_one_${providerCase.provider}`
      );

      const nextResponse = await mockServer.callTool('githubSearchCode', {
        queries,
        responseCharLength: 200,
        responseCharOffset:
          (firstParsed.responsePagination?.charOffset ?? 0) +
          (firstParsed.responsePagination?.charLength ?? 0),
      });

      const nextParsed = GitHubSearchCodeOutputSchema.parse(
        nextResponse.structuredContent
      );
      const nextResult = nextParsed.results[0];

      expect(nextResult?.status).toBe('hasResults');

      const nextData = GitHubSearchCodeDataSchema.parse(nextResult?.data);

      const fetchResponse = await mockServer.callTool('githubGetFileContent', {
        queries: [
          {
            id: `remote_fetch_bulk_${providerCase.provider}`,
            owner: providerCase.owner,
            repo: providerCase.repo,
            path: nextData.files![0]!.path,
            branch: nextData.repositoryContext?.branch,
            researchGoal:
              'Read the file returned on the next bulk response page',
            reasoning:
              'Verify responsePagination still allows downstream fetch',
          },
        ],
      });

      const fetchData = expectHasResultsData(
        GitHubFetchContentOutputSchema,
        GitHubFetchContentDataSchema,
        fetchResponse
      );

      expect(fetchData.content?.length ?? 0).toBeGreaterThan(0);
      expect(mockProvider.getFileContent).toHaveBeenLastCalledWith(
        expect.objectContaining({
          path: nextData.files![0]!.path,
        })
      );
    }
  );
});
