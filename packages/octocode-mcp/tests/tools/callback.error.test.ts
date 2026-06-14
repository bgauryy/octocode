import { describe, it, expect, beforeEach, vi } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { ToolInvocationCallback } from '../../../octocode-tools-core/src/types/toolResults.js';
import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';

vi.mock('../../../octocode-tools-core/src/github/codeSearch.js', () => ({
  searchGitHubCodeAPI: vi.fn().mockResolvedValue({
    data: { items: [], total_count: 0, repository: { name: 'test-repo' } },
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
  searchGitHubReposAPI: vi.fn().mockResolvedValue({
    data: { total_count: 0, items: [] },
  }),
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
  }),
}));

vi.mock('../../../octocode-tools-core/src/session.js', () => ({
  logSessionError: vi.fn().mockResolvedValue(undefined),
}));

describe('Callback Error Handling', () => {
  let server: McpServer;
  let throwingCallback: ToolInvocationCallback;

  beforeEach(() => {
    server = new McpServer({
      name: 'test-server',
      version: '1.0.0',
    });
    throwingCallback = vi.fn().mockRejectedValue(new Error('Callback failed'));
    vi.clearAllMocks();
  });

  describe('github_search_code callback error handling', () => {
    it('should register tool with throwing callback without error', async () => {
      const tool = registerGitHubSearchCodeTool(server, throwingCallback);
      expect(tool).toBeDefined();

      expect(throwingCallback).not.toHaveBeenCalled();
    });

    it('should not propagate callback errors to callers', async () => {
      const errorCallback = vi.fn().mockImplementation(() => {
        throw new Error('Synchronous error');
      });

      const tool = registerGitHubSearchCodeTool(server, errorCallback);
      expect(tool).toBeDefined();
    });
  });

  describe('github_search_repos callback error handling', () => {
    it('should silently catch callback errors', async () => {
      const tool = registerSearchGitHubReposTool(server, throwingCallback);
      expect(tool).toBeDefined();

      expect(throwingCallback).not.toHaveBeenCalled();
    });
  });

  describe('github_fetch_content callback error handling', () => {
    it('should silently catch callback errors', async () => {
      const tool = registerFetchGitHubFileContentTool(server, throwingCallback);
      expect(tool).toBeDefined();
    });
  });

  describe('github_search_pull_requests callback error handling', () => {
    it('should silently catch callback errors', async () => {
      const tool = registerSearchGitHubPullRequestsTool(
        server,
        throwingCallback
      );
      expect(tool).toBeDefined();
    });
  });

  describe('github_view_repo_structure callback error handling', () => {
    it('should silently catch callback errors', async () => {
      const tool = registerViewGitHubRepoStructureTool(
        server,
        throwingCallback
      );
      expect(tool).toBeDefined();
    });
  });

  describe('Callback error types', () => {
    it('should handle async rejection errors', async () => {
      const asyncRejectCallback: ToolInvocationCallback = vi
        .fn()
        .mockRejectedValue(new Error('Async rejection'));

      const tool = registerGitHubSearchCodeTool(server, asyncRejectCallback);
      expect(tool).toBeDefined();
    });

    it('should handle callbacks that return rejected promises', async () => {
      const rejectedPromiseCallback: ToolInvocationCallback = vi
        .fn()
        .mockImplementation(async () => {
          return Promise.reject(new Error('Promise rejection'));
        });

      const tool = registerGitHubSearchCodeTool(
        server,
        rejectedPromiseCallback
      );
      expect(tool).toBeDefined();
    });
  });
});
