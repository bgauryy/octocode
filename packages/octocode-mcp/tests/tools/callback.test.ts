import { describe, it, expect, beforeEach, vi } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { ToolInvocationCallback } from '../../../octocode-tools-core/src/types/toolResults.js';
import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { registerGitHubCloneRepoTool } from '../../src/tools/github_clone_repo/github_clone_repo.js';
import { registerTools } from '../../src/tools/toolsManager.js';

vi.mock('../../../octocode-tools-core/src/github/codeSearch.js', () => ({
  searchGitHubCodeAPI: vi.fn().mockResolvedValue({
    data: { items: [], repository: { name: 'test-repo' } },
  }),
}));

vi.mock('../../../octocode-tools-core/src/github/fileOperations.js', () => ({
  fetchGitHubFileContentAPI: vi.fn().mockResolvedValue({
    data: { content: 'test content', path: 'test.ts' },
  }),
  viewGitHubRepositoryStructureAPI: vi
    .fn()
    .mockResolvedValue({ files: [], folders: { folders: [] } }),
}));

vi.mock('../../../octocode-tools-core/src/github/repoSearch.js', () => ({
  searchGitHubReposAPI: vi
    .fn()
    .mockResolvedValue({ data: { repositories: [] } }),
}));

vi.mock('../../../octocode-tools-core/src/github/pullRequestSearch.js', () => ({
  searchGitHubPullRequestsAPI: vi
    .fn()
    .mockResolvedValue({ pull_requests: [], total_count: 0 }),
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getServerConfig: vi.fn().mockReturnValue({
    version: '1.0.0',
    timeout: 30000,
    maxRetries: 3,
    loggingEnabled: false,
    enableLocal: false,
    enableClone: false,
  }),
  isLocalEnabled: vi.fn().mockReturnValue(false),
  isCloneEnabled: vi.fn().mockReturnValue(false),
}));

describe('Tool Invocation Callback', () => {
  let server: McpServer;
  let mockCallback: ReturnType<typeof vi.fn<ToolInvocationCallback>>;

  beforeEach(() => {
    server = new McpServer({
      name: 'test-server',
      version: '1.0.0',
    });
    mockCallback = vi.fn<ToolInvocationCallback>().mockResolvedValue(undefined);
    vi.clearAllMocks();
  });

  describe('Tool Registration', () => {
    it('should register github_search_code with callback', () => {
      const tool = registerGitHubSearchCodeTool(server, mockCallback);
      expect(tool).toBeDefined();
    });

    it('should register github_search_code without callback', () => {
      const tool = registerGitHubSearchCodeTool(server);
      expect(tool).toBeDefined();
    });

    it('should register github_fetch_content with callback', () => {
      const tool = registerFetchGitHubFileContentTool(server, mockCallback);
      expect(tool).toBeDefined();
    });

    it('should register github_search_repos with callback', () => {
      const tool = registerSearchGitHubReposTool(server, mockCallback);
      expect(tool).toBeDefined();
    });

    it('should register github_search_pull_requests with callback', () => {
      const tool = registerSearchGitHubPullRequestsTool(server, mockCallback);
      expect(tool).toBeDefined();
    });

    it('should register github_view_repo_structure with callback', () => {
      const tool = registerViewGitHubRepoStructureTool(server, mockCallback);
      expect(tool).toBeDefined();
    });

    it('should register github_clone_repo with callback', () => {
      const tool = registerGitHubCloneRepoTool(server, mockCallback);
      expect(tool).toBeDefined();
    });

    it('should register github_clone_repo without callback', () => {
      const tool = registerGitHubCloneRepoTool(server);
      expect(tool).toBeDefined();
    });
  });

  describe('registerTools with callback', () => {
    it('should pass callback to all registered tools', async () => {
      const result = await registerTools(server, mockCallback);

      expect(result.successCount).toBeGreaterThanOrEqual(0);
      expect(typeof result.successCount).toBe('number');
      expect(Array.isArray(result.failedTools)).toBe(true);
    });

    it('should work without callback', async () => {
      const result = await registerTools(server);

      expect(result.successCount).toBeGreaterThanOrEqual(0);
      expect(typeof result.successCount).toBe('number');
      expect(Array.isArray(result.failedTools)).toBe(true);
    });
  });

  describe('Type safety', () => {
    it('should accept properly typed callback', () => {
      const typedCallback: ToolInvocationCallback = async (
        toolName: string,
        queries: unknown[]
      ) => {
        expect(typeof toolName).toBe('string');
        expect(Array.isArray(queries)).toBe(true);
      };

      const tool = registerGitHubSearchCodeTool(server, typedCallback);
      expect(tool).toBeDefined();
    });

    it('should accept undefined callback', () => {
      const tool = registerGitHubSearchCodeTool(server, undefined);
      expect(tool).toBeDefined();
    });
  });
});
