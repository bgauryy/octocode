import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { registerTools } from '../../src/tools/toolsManager.js';

// Don't mock toolConfig - use real DEFAULT_TOOLS
vi.mock('../../src/serverConfig.js', () => ({
  getServerConfig: vi.fn(),
  isLocalEnabled: vi.fn(),
  isCloneEnabled: vi.fn(),
}));

vi.mock('../../src/tools/toolMetadata/proxies.js', async () => {
  const actual = await vi.importActual<
    typeof import('../../src/tools/toolMetadata/proxies.js')
  >('../../src/tools/toolMetadata/proxies.js');
  return {
    ...actual,
    isToolInMetadata: vi.fn().mockReturnValue(true),
  };
});

vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(),
}));

// Mock tool registration functions to just return successfully
vi.mock('../../src/tools/github_search_code/github_search_code.js', () => ({
  registerGitHubSearchCodeTool: vi.fn().mockReturnValue({}),
}));
vi.mock('../../src/tools/github_fetch_content/github_fetch_content.js', () => ({
  registerFetchGitHubFileContentTool: vi.fn().mockReturnValue({}),
}));
vi.mock('../../src/tools/github_search_repos/github_search_repos.js', () => ({
  registerSearchGitHubReposTool: vi.fn().mockReturnValue({}),
}));
vi.mock(
  '../../src/tools/github_search_pull_requests/github_search_pull_requests.js',
  () => ({
    registerSearchGitHubPullRequestsTool: vi.fn().mockReturnValue({}),
  })
);
vi.mock(
  '../../src/tools/github_view_repo_structure/github_view_repo_structure.js',
  () => ({
    registerViewGitHubRepoStructureTool: vi.fn().mockReturnValue({}),
  })
);
vi.mock('../../src/tools/package_search/package_search.js', () => ({
  registerPackageSearchTool: vi.fn().mockReturnValue({}),
}));
vi.mock('../../src/tools/github_clone_repo/register.js', () => ({
  registerGitHubCloneRepoTool: vi.fn().mockReturnValue({}),
}));

// Mock local tools - export registration functions used by toolConfig
vi.mock('../../src/tools/local_ripgrep/register.js', () => ({
  registerLocalRipgrepTool: vi.fn().mockReturnValue({}),
}));
vi.mock('../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep: vi.fn().mockResolvedValue({ status: 'hasResults' }),
}));
vi.mock('../../src/tools/local_view_structure/register.js', () => ({
  registerLocalViewStructureTool: vi.fn().mockReturnValue({}),
}));
vi.mock('../../src/tools/local_view_structure/local_view_structure.js', () => ({
  viewStructure: vi.fn().mockResolvedValue({ status: 'hasResults' }),
}));
vi.mock('../../src/tools/local_find_files/register.js', () => ({
  registerLocalFindFilesTool: vi.fn().mockReturnValue({}),
}));
vi.mock('../../src/tools/local_find_files/findFiles.js', () => ({
  findFiles: vi.fn().mockResolvedValue({ status: 'hasResults' }),
}));
vi.mock('../../src/tools/local_fetch_content/register.js', () => ({
  registerLocalFetchContentTool: vi.fn().mockReturnValue({}),
}));
vi.mock('../../src/tools/local_fetch_content/fetchContent.js', () => ({
  fetchContent: vi.fn().mockResolvedValue({ status: 'hasResults' }),
}));
vi.mock('../../src/utils/bulkOperations.js', () => ({
  executeBulkOperation: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'test' }],
  }),
}));

import {
  getServerConfig,
  isLocalEnabled,
  isCloneEnabled,
} from '../../src/serverConfig.js';

describe('Tool Count Verification', () => {
  const originalStderr = process.stderr.write;

  beforeEach(() => {
    vi.clearAllMocks();
    process.stderr.write = vi.fn();
  });

  afterEach(() => {
    process.stderr.write = originalStderr;
  });

  it('should register exactly 6 tools WITHOUT local enabled', async () => {
    vi.mocked(getServerConfig).mockReturnValue({
      version: '1.0.0',
      githubApiUrl: 'https://api.github.com',
      timeout: 30000,
      maxRetries: 3,
      loggingEnabled: true,
      enableLocal: false,
      enableClone: false,
      outputFormat: 'yaml',
      tokenSource: 'env:GITHUB_TOKEN',
    });
    vi.mocked(isLocalEnabled).mockReturnValue(false);
    vi.mocked(isCloneEnabled).mockReturnValue(false);

    const mockServer = {} as McpServer;
    const result = await registerTools(mockServer);

    expect(result.successCount).toBe(6);
    expect(result.failedTools).toHaveLength(0);
  });

  it('should register exactly 13 tools WITH local enabled but WITHOUT clone', async () => {
    vi.mocked(getServerConfig).mockReturnValue({
      version: '1.0.0',
      githubApiUrl: 'https://api.github.com',
      timeout: 30000,
      maxRetries: 3,
      loggingEnabled: true,
      enableLocal: true,
      enableClone: false,
      outputFormat: 'yaml',
      tokenSource: 'env:GITHUB_TOKEN',
    });
    vi.mocked(isLocalEnabled).mockReturnValue(true);
    vi.mocked(isCloneEnabled).mockReturnValue(false);

    const mockServer = {
      registerTool: vi.fn(),
    } as unknown as McpServer;

    const result = await registerTools(mockServer);

    // 6 GitHub + 4 Local + 3 LSP = 13 tools (no clone)
    expect(result.successCount).toBe(13);
    expect(result.failedTools).toHaveLength(0);
  });

  it('should register exactly 14 tools WITH local AND clone enabled', async () => {
    vi.mocked(getServerConfig).mockReturnValue({
      version: '1.0.0',
      githubApiUrl: 'https://api.github.com',
      timeout: 30000,
      maxRetries: 3,
      loggingEnabled: true,
      enableLocal: true,
      enableClone: true,
      outputFormat: 'yaml',
      tokenSource: 'env:GITHUB_TOKEN',
    });
    vi.mocked(isLocalEnabled).mockReturnValue(true);
    vi.mocked(isCloneEnabled).mockReturnValue(true);

    const mockServer = {
      registerTool: vi.fn(),
    } as unknown as McpServer;

    const result = await registerTools(mockServer);

    // 6 GitHub + 1 Clone + 4 Local + 3 LSP = 14 tools
    expect(result.successCount).toBe(14);
    expect(result.failedTools).toHaveLength(0);
  });
});
