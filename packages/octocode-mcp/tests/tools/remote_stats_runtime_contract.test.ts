import { beforeEach, describe, expect, it, vi } from 'vitest';
import { incrementToolCharSavings } from 'octocode-shared';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';

const mockGetProvider = vi.hoisted(() => vi.fn());
const mockCheckNpmAvailability = vi.hoisted(() => vi.fn());
const mockCheckNpmRegistryReachable = vi.hoisted(() => vi.fn());
const mockSearchPackage = vi.hoisted(() => vi.fn());
const mockCheckNpmDeprecation = vi.hoisted(() => vi.fn());
const mockCreateLazyProviderContext = vi.hoisted(() => vi.fn());
const mockProviderSupports = vi.hoisted(() => vi.fn());
const mockCloneRepo = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getGitHubToken: vi.fn(async () => 'test-token'),
  isLoggingEnabled: vi.fn(() => false),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  isLocalEnabled: vi.fn(() => true),
  isCloneEnabled: vi.fn(() => true),
  getServerConfig: vi.fn(() => ({
    tools: {},
  })),
}));

vi.mock(
  '../../../octocode-tools-core/src/utils/exec/npm.js',
  async importOriginal => {
    const actual =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/utils/exec/npm.js')
      >();
    return {
      ...actual,
      checkNpmAvailability: mockCheckNpmAvailability,
    };
  }
);

vi.mock(
  '../../../octocode-tools-core/src/utils/package/npm.js',
  async importOriginal => {
    const actual =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/utils/package/npm.js')
      >();
    return {
      ...actual,
      checkNpmRegistryReachable: mockCheckNpmRegistryReachable,
    };
  }
);

vi.mock(
  '../../../octocode-tools-core/src/utils/package/common.js',
  async importOriginal => {
    const actual =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/utils/package/common.js')
      >();
    return {
      ...actual,
      searchPackage: mockSearchPackage,
      checkNpmDeprecation: mockCheckNpmDeprecation,
    };
  }
);

vi.mock(
  '../../../octocode-tools-core/src/tools/providerExecution.js',
  async importOriginal => {
    const actual =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/tools/providerExecution.js')
      >();
    return {
      ...actual,
      createLazyProviderContext: mockCreateLazyProviderContext,
      providerSupports: mockProviderSupports,
    };
  }
);

vi.mock(
  '../../../octocode-tools-core/src/tools/github_clone_repo/cloneRepo.js',
  () => ({
    cloneRepo: mockCloneRepo,
  })
);

import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { registerGitHubCloneRepoTool } from '../../src/tools/github_clone_repo/github_clone_repo.js';
import { registerNpmSearchTool } from '../../src/tools/package_search/package_search.js';

describe('remote tool stats runtime contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    const provider = {
      searchCode: vi.fn(async () => ({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
        rawResponseChars: 10_000,
      })),
      getFileContent: vi.fn(async () => ({
        data: {
          path: 'README.md',
          content: 'hello',
          encoding: 'utf-8',
          size: 5,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
        rawResponseChars: 9_000,
      })),
      getRepoStructure: vi.fn(async () => ({
        data: {
          projectPath: 'owner/repo',
          branch: 'main',
          path: '/',
          structure: {
            '.': { files: ['README.md'], folders: [] },
          },
          summary: { totalFiles: 1, totalFolders: 0, truncated: false },
          hints: [],
        },
        status: 200,
        provider: 'github',
        rawResponseChars: 8_000,
      })),
      searchRepos: vi.fn(async () => ({
        data: {
          repositories: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
        rawResponseChars: 7_000,
      })),
      searchPullRequests: vi.fn(async () => ({
        data: {
          items: [],
          totalCount: 0,
          pagination: {
            currentPage: 1,
            totalPages: 0,
            hasMore: false,
            totalMatches: 0,
            entriesPerPage: 10,
          },
        },
        status: 200,
        provider: 'github',
        rawResponseChars: 6_000,
      })),
    };
    mockGetProvider.mockReturnValue(provider);

    mockCheckNpmAvailability.mockResolvedValue(true);
    mockCheckNpmRegistryReachable.mockResolvedValue(true);
    mockSearchPackage.mockResolvedValue({
      packages: [
        {
          name: 'lodash',
          version: '4.17.21',
          description: 'utility library',
          repository: 'https://github.com/lodash/lodash',
        },
      ],
      totalFound: 1,
      rawResponseChars: 5_000,
    });
    mockCheckNpmDeprecation.mockResolvedValue({ deprecated: false });

    mockCreateLazyProviderContext.mockReturnValue(() => ({
      providerType: 'github',
      token: 'test-token',
      provider,
      capabilities: { cloneRepo: true },
    }));
    mockProviderSupports.mockReturnValue(true);
    mockCloneRepo.mockResolvedValue({
      localPath: '/tmp/octocode-test-clone',
      branch: 'main',
      cached: true,
    });
  });

  it('records charsSavedByTool for every GitHub tool and npmSearch when the tool runs', async () => {
    const mockServer = createMockMcpServer();

    registerGitHubSearchCodeTool(mockServer.server);
    registerFetchGitHubFileContentTool(mockServer.server);
    registerViewGitHubRepoStructureTool(mockServer.server);
    registerSearchGitHubReposTool(mockServer.server);
    registerSearchGitHubPullRequestsTool(mockServer.server);
    registerGitHubCloneRepoTool(mockServer.server);
    await registerNpmSearchTool(mockServer.server);

    await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
      queries: [
        {
          id: 'code',
          mainResearchGoal: 'stats telemetry',
          researchGoal: 'exercise ghSearchCode stats',
          reasoning: 'prove runtime char savings emission',
          owner: 'owner',
          repo: 'repo',
          keywords: ['foo'],
        },
      ],
    });
    await mockServer.callTool(TOOL_NAMES.GITHUB_FETCH_CONTENT, {
      queries: [
        {
          id: 'content',
          mainResearchGoal: 'stats telemetry',
          researchGoal: 'exercise ghGetFileContent stats',
          reasoning: 'prove runtime char savings emission',
          owner: 'owner',
          repo: 'repo',
          path: 'README.md',
        },
      ],
    });
    await mockServer.callTool(TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE, {
      queries: [
        {
          id: 'structure',
          mainResearchGoal: 'stats telemetry',
          researchGoal: 'exercise ghViewRepoStructure stats',
          reasoning: 'prove runtime char savings emission',
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
        },
      ],
    });
    await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
      queries: [
        {
          id: 'repos',
          mainResearchGoal: 'stats telemetry',
          researchGoal: 'exercise ghSearchRepos stats',
          reasoning: 'prove runtime char savings emission',
          keywords: ['repo'],
          owner: 'owner',
        },
      ],
    });
    await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
      queries: [
        {
          id: 'prs',
          mainResearchGoal: 'stats telemetry',
          researchGoal: 'exercise ghHistoryResearch stats',
          reasoning: 'prove runtime char savings emission',
          owner: 'owner',
          repo: 'repo',
          keywords: ['fix'],
        },
      ],
    });
    await mockServer.callTool(TOOL_NAMES.GITHUB_CLONE_REPO, {
      queries: [
        {
          id: 'clone',
          mainResearchGoal: 'stats telemetry',
          researchGoal: 'exercise ghCloneRepo stats',
          reasoning: 'prove runtime char savings emission',
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
        },
      ],
    });
    await mockServer.callTool(TOOL_NAMES.PACKAGE_SEARCH, {
      queries: [
        {
          id: 'pkg',
          mainResearchGoal: 'stats telemetry',
          researchGoal: 'exercise npmSearch stats',
          reasoning: 'prove runtime char savings emission',
          packageName: 'lodash',
        },
      ],
    });

    const expectedRawCharsByTool = new Map<string, number>([
      [TOOL_NAMES.GITHUB_SEARCH_CODE, 10_000],
      [TOOL_NAMES.GITHUB_FETCH_CONTENT, 9_000],
      [TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE, 8_000],
      [TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, 7_000],
      [TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, 6_000],
      [TOOL_NAMES.GITHUB_CLONE_REPO, 0],
      [TOOL_NAMES.PACKAGE_SEARCH, 5_000],
    ]);
    const expectedToolNames = [...expectedRawCharsByTool.keys()];
    const statsCalls = vi.mocked(incrementToolCharSavings).mock.calls;
    const recordedToolNames = statsCalls.map(([toolName]) => toolName);

    expect(recordedToolNames).toEqual(expectedToolNames);

    for (const [toolName, expectedRawChars] of expectedRawCharsByTool) {
      const call = statsCalls.find(
        ([recordedName]) => recordedName === toolName
      );
      expect(
        call?.[1],
        `${toolName} should record exact upstream raw chars`
      ).toBe(expectedRawChars);
      expect(
        call?.[2],
        `${toolName} should record response chars`
      ).toBeGreaterThan(0);
    }
  });
});
