import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createMockMcpServer,
  type MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { getTextContent } from '../utils/testHelpers.js';
import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { registerGitHubCloneRepoTool } from '../../src/tools/github_clone_repo/github_clone_repo.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  isLoggingEnabled: vi.fn(() => false),
  isCloneEnabled: vi.fn(() => true),
  getActiveProvider: vi.fn(() => 'github'),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  getGitHubToken: vi.fn(() => Promise.resolve('mock-token')),
  getServerConfig: vi.fn(() => ({
    version: '1.0.0',
    timeout: 30000,
    maxRetries: 3,
    loggingEnabled: false,
    enableLocal: true,
    enableClone: true,
  })),
}));

describe('provider initialization errors across provider-backed tools', () => {
  let mockServer: MockMcpServer;

  beforeEach(() => {
    mockServer = createMockMcpServer();
    mockGetProvider.mockImplementation(() => {
      throw new Error('provider boot failed');
    });

    registerGitHubSearchCodeTool(mockServer.server);
    registerFetchGitHubFileContentTool(mockServer.server);
    registerSearchGitHubPullRequestsTool(mockServer.server);
    registerViewGitHubRepoStructureTool(mockServer.server);
    registerSearchGitHubReposTool(mockServer.server);
    registerGitHubCloneRepoTool(mockServer.server);
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.clearAllMocks();
  });

  it('returns an error result for ghSearchCode', async () => {
    const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
      queries: [{ keywords: ['test'] }],
    });

    expect(getTextContent(result.content)).toContain(
      'Failed to initialize github provider: provider boot failed'
    );
  });

  it('returns an error result for ghGetFileContent', async () => {
    const result = await mockServer.callTool(TOOL_NAMES.GITHUB_FETCH_CONTENT, {
      queries: [{ owner: 'owner', repo: 'repo', path: 'README.md' }],
    });

    expect(getTextContent(result.content)).toContain(
      'Failed to initialize github provider: provider boot failed'
    );
  });

  it('returns an error result for ghHistoryResearch', async () => {
    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      {
        queries: [{ owner: 'owner', repo: 'repo', prNumber: 1 }],
      }
    );

    expect(getTextContent(result.content)).toContain(
      'Failed to initialize github provider: provider boot failed'
    );
  });

  it('returns an error result for ghViewRepoStructure', async () => {
    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [{ owner: 'owner', repo: 'repo' }],
      }
    );

    expect(getTextContent(result.content)).toContain(
      'Failed to initialize github provider: provider boot failed'
    );
  });

  it('returns an error result for ghSearchRepos', async () => {
    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      {
        queries: [{ keywords: ['test'] }],
      }
    );

    expect(getTextContent(result.content)).toContain(
      'Failed to initialize github provider: provider boot failed'
    );
  });

  it('returns an error result for ghCloneRepo', async () => {
    const result = await mockServer.callTool(TOOL_NAMES.GITHUB_CLONE_REPO, {
      queries: [{ owner: 'owner', repo: 'repo' }],
    });

    expect(getTextContent(result.content)).toContain(
      'Failed to initialize github provider: provider boot failed'
    );
  });
});
