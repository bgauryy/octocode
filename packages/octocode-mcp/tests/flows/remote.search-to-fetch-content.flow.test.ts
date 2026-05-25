import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';

type CodeSearchFlatResponse = {
  results: Array<{
    id: string;
    owner: string;
    repo: string;
    matches: Array<{ path: string; value?: string }>;
  }>;
  pagination?: {
    currentPage: number;
    totalPages: number;
    perPage: number;
    totalMatches: number;
    hasMore: boolean;
  };
};

function parseCodeSearchFlatResponse(
  result: CallToolResult
): CodeSearchFlatResponse {
  return result.structuredContent as CodeSearchFlatResponse;
}
import { registerTools } from '../../src/tools/toolsManager.js';
import type { ToolConfig } from '../../src/tools/toolConfig.js';
import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import {
  createMockMcpServer,
  type MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
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

      const searchData = parseCodeSearchFlatResponse(searchResponse);
      const matchedGroup = searchData.results[0];
      const matchedMatch = matchedGroup?.matches[0];

      expect(matchedGroup).toBeDefined();
      expect(matchedMatch?.path).toBe('src/score.ts');
      expect(matchedGroup?.owner).toBe(providerCase.owner);
      expect(matchedGroup?.repo).toBe(providerCase.repo);
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
            owner: matchedGroup!.owner,
            repo: matchedGroup!.repo,
            path: matchedMatch!.path,
            matchString: 'export function computeScore',
            researchGoal: 'Read the matched remote file',
            reasoning: 'Use the path handoff from remote code search',
          },
        ],
      });

      const fetchStructured = fetchResponse.structuredContent as {
        results: Array<{
          owner: string;
          repo: string;
          files?: Array<{
            path: string;
            content?: string;
            lastModified?: string;
          }>;
        }>;
      };
      const fetchedFile = fetchStructured.results[0]?.files?.[0];

      expect(fetchedFile?.content).toContain('computeScore');
      expect(fetchedFile?.lastModified).toBe('2026-03-13T10:00:00.000Z');
      expect(mockProvider.getFileContent).toHaveBeenCalledWith(
        expect.objectContaining({
          projectId: `${providerCase.owner}/${providerCase.repo}`,
          path: 'src/score.ts',
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

      const searchData = parseCodeSearchFlatResponse(searchResponse);
      const firstMatch = searchData.results[0]?.matches[0];

      expect(firstMatch?.path).toBe('src/score.ts');

      await mockServer.callTool('githubGetFileContent', {
        queries: [
          {
            id: `remote_branch_fetch_${providerCase.provider}`,
            owner: providerCase.owner,
            repo: providerCase.repo,
            path: firstMatch!.path,
            researchGoal: 'Read fetched file from handed-off path',
            reasoning: 'Path is already known from search response',
          },
        ],
      });

      expect(mockProvider.getFileContent).toHaveBeenCalledTimes(1);
    }
  );
});
