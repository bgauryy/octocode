import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
}));

vi.mock('octocode-lsp/manager', () => ({
  LSP_UNAVAILABLE_HINT: 'LSP unavailable test',
  acquirePooledClient: vi.fn(),
}));

vi.mock('../../../octocode-tools-core/src/hints/index.js', () => ({
  getHints: vi.fn(() => []),
}));

vi.mock('../../../octocode-tools-core/src/utils/response/bulk.js', () => ({
  executeBulkOperation: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: '' }],
    isError: false,
  }),
}));

vi.mock('octocode-security/withSecurityValidation', () => ({
  withSecurityValidation: vi.fn((_toolName, handler) => handler),
  withBasicSecurityValidation: vi.fn(handler => handler),
}));

vi.mock(
  '../../../octocode-tools-core/src/tools/utils.js',
  async importOriginal => ({
    ...(await importOriginal<object>()),
    invokeCallbackSafely: vi.fn().mockResolvedValue(undefined),
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/github_search_code/execution.js',
  () => ({
    searchMultipleGitHubCode: vi.fn().mockResolvedValue({
      content: [{ type: 'text', text: '' }],
      isError: false,
    }),
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/github_search_repos/execution.js',
  () => ({
    searchMultipleGitHubRepos: vi.fn().mockResolvedValue({
      content: [{ type: 'text', text: '' }],
      isError: false,
    }),
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/github_view_repo_structure/execution.js',
  () => ({
    exploreMultipleRepositoryStructures: vi.fn().mockResolvedValue({
      content: [{ type: 'text', text: '' }],
      isError: false,
    }),
  })
);

import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { searchMultipleGitHubCode } from '../../../octocode-tools-core/src/tools/github_search_code/execution.js';
import { searchMultipleGitHubRepos } from '../../../octocode-tools-core/src/tools/github_search_repos/execution.js';
import { exploreMultipleRepositoryStructures } from '../../../octocode-tools-core/src/tools/github_view_repo_structure/execution.js';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';

describe('Tool Execution Branch Coverage Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('github_search_code/github_search_code.ts - registerGitHubSearchCodeTool', () => {
    it('should handle falsy queries (line 38)', async () => {
      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      } as unknown as McpServer;

      registerGitHubSearchCodeTool(mockServer);

      expect(mockServer.registerTool).toHaveBeenCalled();

      const registerCall = vi.mocked(mockServer.registerTool).mock.calls[0]!;
      const handler = registerCall[2] as any;

      const mockAuthInfo = {};
      const mockSessionId = 'test-session';

      await handler({ queries: undefined }, mockAuthInfo, mockSessionId);

      expect(searchMultipleGitHubCode).toHaveBeenCalledWith(
        expect.objectContaining({ queries: [] })
      );
    });
  });

  describe('github_search_repos/github_search_repos.ts - registerSearchGitHubReposTool', () => {
    it('should handle falsy queries (line 38)', async () => {
      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      } as unknown as McpServer;

      registerSearchGitHubReposTool(mockServer);

      expect(mockServer.registerTool).toHaveBeenCalled();

      const registerCall = vi.mocked(mockServer.registerTool).mock.calls[0]!;
      const handler = registerCall[2] as any;

      const mockAuthInfo = {};
      const mockSessionId = 'test-session';

      await handler({ queries: undefined }, mockAuthInfo, mockSessionId);

      expect(searchMultipleGitHubRepos).toHaveBeenCalledWith(
        expect.objectContaining({ queries: [] })
      );
    });
  });

  describe('github_view_repo_structure/github_view_repo_structure.ts - registerViewGitHubRepoStructureTool', () => {
    it('should handle falsy queries (line 39)', async () => {
      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      } as unknown as McpServer;

      registerViewGitHubRepoStructureTool(mockServer);

      expect(mockServer.registerTool).toHaveBeenCalled();

      const registerCall = vi.mocked(mockServer.registerTool).mock.calls[0]!;
      const handler = registerCall[2] as any;

      const mockAuthInfo = {};
      const mockSessionId = 'test-session';

      await handler({ queries: undefined }, mockAuthInfo, mockSessionId);

      expect(exploreMultipleRepositoryStructures).toHaveBeenCalledWith(
        expect.objectContaining({ queries: [] })
      );
    });
  });
});
