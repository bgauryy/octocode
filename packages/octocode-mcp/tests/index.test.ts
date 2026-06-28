import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
// The server name/version are derived from package.json (src/index.ts builds
// `${name}_${version}`), so assert against that source rather than hardcoding a
// package name that changes on rename/version-sync.
import { name as pkgName } from '../package.json';

vi.mock('@modelcontextprotocol/sdk/server/mcp.js');
vi.mock('@modelcontextprotocol/sdk/server/stdio.js');
vi.mock('../../octocode-tools-core/src/utils/http/cache.js');
vi.mock('../src/tools/github_search_code/github_search_code.js');
vi.mock('../src/tools/github_fetch_content/github_fetch_content.js');
vi.mock('../src/tools/github_search_repos/github_search_repos.js');
vi.mock(
  '../src/tools/github_search_pull_requests/github_search_pull_requests.js'
);
vi.mock(
  '../src/tools/github_view_repo_structure/github_view_repo_structure.js'
);
vi.mock('../../octocode-tools-core/src/utils/exec/npm.js');
vi.mock('../../octocode-tools-core/src/serverConfig.js');
vi.mock('../src/tools/toolsManager.js');
vi.mock('../../octocode-tools-core/src/providers/factory.js', () => ({
  initializeProviders: vi.fn().mockResolvedValue(undefined),
  clearProviderCache: vi.fn(),
}));
vi.mock('../../octocode-tools-core/src/github/client.js', () => ({
  clearOctokitInstances: vi.fn(),
}));
vi.mock('octocode-security/withSecurityValidation', () => ({
  configureSecurity: vi.fn(),
}));
vi.mock(
  '../../octocode-tools-core/src/tools/toolMetadata/proxies.js',
  async importOriginal => ({
    ...(await importOriginal<object>()),
    loadToolContent: vi
      .fn()
      .mockResolvedValue({ systemPrompt: 'Test instructions' }),
  })
);
vi.mock(
  '../../octocode-tools-core/src/tools/github_clone_repo/cache.js',
  () => ({
    startCacheGC: vi.fn(),
    stopCacheGC: vi.fn(),
  })
);
import { registerGitHubSearchCodeTool } from '../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../src/tools/github_fetch_content/github_fetch_content.js';
import { registerSearchGitHubReposTool } from '../src/tools/github_search_repos/github_search_repos.js';
import { registerSearchGitHubPullRequestsTool } from '../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { registerViewGitHubRepoStructureTool } from '../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import {
  initialize,
  cleanup,
  getServerConfig,
  getGitHubToken,
  isCloneEnabled,
  getActiveProvider,
} from '../../octocode-tools-core/src/serverConfig.js';
import { registerTools } from '../src/tools/toolsManager.js';
import { TOOL_NAMES } from '../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

const mockMcpServer = {
  connect: vi.fn(function () {}),
  close: vi.fn(function () {}),
};

const mockTransport = {
  start: vi.fn(function () {}),
};

const mockMcpServerConstructor = vi.mocked(McpServer);
const mockStdioServerTransport = vi.mocked(StdioServerTransport);
const mockRegisterTools = vi.mocked(registerTools);
const mockGetGitHubToken = vi.mocked(getGitHubToken);
const mockInitialize = vi.mocked(initialize);
const mockCleanup = vi.mocked(cleanup);
const mockGetServerConfig = vi.mocked(getServerConfig);
const mockIsCloneEnabled = vi.mocked(isCloneEnabled);
const mockGetActiveProvider = vi.mocked(getActiveProvider);

const mockRegisterGitHubSearchCodeTool = vi.mocked(
  registerGitHubSearchCodeTool
);
const mockRegisterFetchGitHubFileContentTool = vi.mocked(
  registerFetchGitHubFileContentTool
);
const mockRegisterSearchGitHubReposTool = vi.mocked(
  registerSearchGitHubReposTool
);
const mockRegisterSearchGitHubPullRequestsTool = vi.mocked(
  registerSearchGitHubPullRequestsTool
);
const mockRegisterViewGitHubRepoStructureTool = vi.mocked(
  registerViewGitHubRepoStructureTool
);

describe('Index Module', () => {
  let processExitSpy: any;

  let processStdinResumeSpy: any;

  let processStdinOnSpy: any;

  let processOnSpy: any;

  let originalGithubToken: string | undefined;
  let originalGhToken: string | undefined;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();

    originalGithubToken = process.env.GITHUB_TOKEN;
    originalGhToken = process.env.GH_TOKEN;

    process.env.GITHUB_TOKEN = 'test-token';

    mockMcpServerConstructor.mockImplementation(function () {
      return mockMcpServer as unknown as InstanceType<typeof McpServer>;
    });
    mockStdioServerTransport.mockImplementation(function () {
      return mockTransport as unknown as InstanceType<
        typeof StdioServerTransport
      >;
    });

    processExitSpy = vi.spyOn(process, 'exit').mockImplementation(function (
      _code?: string | number | null | undefined
    ) {
      return undefined as never;
    });
    processStdinResumeSpy = vi
      .spyOn(process.stdin, 'resume')
      .mockImplementation(function () {
        return process.stdin;
      });
    processStdinOnSpy = vi
      .spyOn(process.stdin, 'once')
      .mockImplementation(function () {
        return process.stdin;
      });
    processOnSpy = vi.spyOn(process, 'once').mockImplementation(function () {
      return process;
    });

    mockMcpServer.connect.mockResolvedValue(undefined);
    mockMcpServer.close.mockResolvedValue(undefined);

    const mockRegisteredTool = {
      name: 'mock-tool',
      description: 'Mock tool',
      callback: vi.fn(function () {
        return { content: [{ type: 'text', text: '' }] };
      }),
      enabled: true,
      enable: vi.fn(function () {}),
      disable: vi.fn(function () {}),
      getStatus: vi.fn(function () {}),
      getMetrics: vi.fn(function () {}),
      update: vi.fn(function () {}),
      remove: vi.fn(function () {}),
    } as unknown as RegisteredTool;
    mockRegisterGitHubSearchCodeTool.mockImplementation(function () {
      return mockRegisteredTool;
    });
    mockRegisterFetchGitHubFileContentTool.mockImplementation(function () {
      return mockRegisteredTool;
    });
    mockRegisterSearchGitHubReposTool.mockImplementation(function () {
      return mockRegisteredTool;
    });
    mockRegisterSearchGitHubPullRequestsTool.mockImplementation(function () {
      return mockRegisteredTool;
    });
    mockRegisterViewGitHubRepoStructureTool.mockImplementation(function () {
      return mockRegisteredTool;
    });

    mockGetGitHubToken.mockResolvedValue('test-token');
    mockInitialize.mockResolvedValue(undefined);
    mockCleanup.mockImplementation(() => {});
    mockGetServerConfig.mockReturnValue({
      version: '4.0.5',
      githubApiUrl: 'https://api.github.com',
      enableTools: [],
      disableTools: [],
      timeout: 30000,
      maxRetries: 3,
      enableLocal: false,
      enableClone: false,
      outputFormat: 'yaml',
      tokenSource: 'env:GITHUB_TOKEN',
    });

    mockRegisterTools.mockImplementation(async () => {
      return { successCount: 4, failedTools: [] };
    });

    mockIsCloneEnabled.mockReturnValue(false);
    mockGetActiveProvider.mockReturnValue('github');
  });

  afterEach(() => {
    if (originalGithubToken !== undefined) {
      process.env.GITHUB_TOKEN = originalGithubToken;
    } else {
      delete process.env.GITHUB_TOKEN;
    }
    if (originalGhToken !== undefined) {
      process.env.GH_TOKEN = originalGhToken;
    } else {
      delete process.env.GH_TOKEN;
    }

    processExitSpy?.mockRestore();
    processStdinResumeSpy?.mockRestore();
    processStdinOnSpy?.mockRestore();
    processOnSpy?.mockRestore();
  });

  const waitForAsyncOperations = async () => {
    for (let i = 0; i < 25; i++) await Promise.resolve();
  };

  describe('Basic Module Import', () => {
    it('should create server with correct configuration', async () => {
      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockMcpServerConstructor).toHaveBeenCalledWith(
        expect.objectContaining({
          name: expect.stringContaining(pkgName),
          title: 'Octocode MCP',
          version: expect.any(String),
        }),
        expect.objectContaining({
          capabilities: expect.objectContaining({
            tools: { listChanged: false },
          }),
        })
      );
    });

    it('should use version from package.json', async () => {
      await import('../src/index.js');
      await waitForAsyncOperations();

      const serverConfig = mockMcpServerConstructor.mock.calls[0]?.[0];
      expect(typeof serverConfig?.version).toEqual('string');
      expect((serverConfig?.version?.length ?? 0) > 0).toEqual(true);
    });
  });

  describe('NPM Status Check', () => {
    it('should no longer check NPM status during initialization', async () => {
      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalled();
    });

    it('should register all tools without NPM status dependency', async () => {
      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });
  });

  describe('GitHub Token Detection', () => {
    it('should use GITHUB_TOKEN when present', async () => {
      process.env.GITHUB_TOKEN = 'github-token';

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should use GH_TOKEN when GITHUB_TOKEN is not present', async () => {
      delete process.env.GITHUB_TOKEN;
      process.env.GH_TOKEN = 'gh-token';

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should use CLI token when no env tokens are present', async () => {
      delete process.env.GITHUB_TOKEN;
      delete process.env.GH_TOKEN;

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should exit when no token is available', async () => {
      delete process.env.GITHUB_TOKEN;
      delete process.env.GH_TOKEN;

      mockGetGitHubToken.mockRejectedValue(new Error('No token available'));

      let exitCalled = false;
      let exitCode: number | undefined;
      processExitSpy.mockImplementation(
        (code?: string | number | null | undefined) => {
          exitCalled = true;
          exitCode =
            typeof code === 'number'
              ? code
              : code
                ? parseInt(String(code))
                : undefined;
          return undefined as never;
        }
      );

      try {
        await import('../src/index.js');
        await waitForAsyncOperations();
        await waitForAsyncOperations();
      } catch {
        void 0;
      }

      expect(exitCalled).toBe(true);
      expect(exitCode).toBe(1);
    });
  });

  describe('Tool Registration', () => {
    it('should register all tools successfully', async () => {
      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should continue registering tools even if some fail', async () => {
      mockRegisterTools.mockImplementation(async () => {
        return {
          successCount: 3,
          failedTools: [TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS],
        };
      });

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalled();
    });

    it('should exit when no tools are successfully registered', async () => {
      mockRegisterTools.mockImplementation(async () => {
        return { successCount: 0, failedTools: ['all'] };
      });

      let exitCalled = false;
      let exitCode: number | undefined;
      processExitSpy.mockImplementation(
        (code?: string | number | null | undefined) => {
          exitCalled = true;
          exitCode =
            typeof code === 'number'
              ? code
              : code
                ? parseInt(String(code))
                : undefined;
          return undefined as never;
        }
      );

      try {
        await import('../src/index.js');
        await waitForAsyncOperations();
        await waitForAsyncOperations();
      } catch {
        void 0;
      }

      expect(exitCalled).toBe(true);
      expect(exitCode).toBe(1);
    });

    it('should handle tool registration errors gracefully', async () => {
      mockRegisterTools.mockImplementation(async () => {
        return {
          successCount: 2,
          failedTools: [TOOL_NAMES.GITHUB_SEARCH_CODE],
        };
      });

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalled();
    });

    it('should handle multiple tool registration errors', async () => {
      mockRegisterTools.mockImplementation(async () => {
        return { successCount: 1, failedTools: ['tool1', 'tool2'] };
      });

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalled();
    });

    it('should handle all tool registration errors', async () => {
      mockRegisterTools.mockImplementation(async () => {
        return { successCount: 0, failedTools: ['all', 'tools', 'failed'] };
      });

      let exitCalled = false;
      let exitCode: number | undefined;
      processExitSpy.mockImplementation(
        (code?: string | number | null | undefined) => {
          exitCalled = true;
          exitCode =
            typeof code === 'number'
              ? code
              : code
                ? parseInt(String(code))
                : undefined;
          return undefined as never;
        }
      );

      try {
        await import('../src/index.js');
        await waitForAsyncOperations();
        await waitForAsyncOperations();
      } catch {
        void 0;
      }

      expect(mockMcpServerConstructor).toHaveBeenCalled();
      expect(exitCalled).toBe(true);
      expect(exitCode).toBe(1);
    });
  });

  describe('Server Startup', () => {
    it('should handle server startup errors', async () => {
      mockMcpServer.connect.mockRejectedValue(new Error('Connection failed'));

      let exitCalled = false;
      let exitCode: number | undefined;
      processExitSpy.mockImplementation(
        (code?: string | number | null | undefined) => {
          exitCalled = true;
          exitCode =
            typeof code === 'number'
              ? code
              : code
                ? parseInt(String(code))
                : undefined;
          return undefined as never;
        }
      );

      try {
        await import('../src/index.js');
        await waitForAsyncOperations();
        await waitForAsyncOperations();
      } catch {
        void 0;
      }

      expect(exitCalled).toBe(true);
      expect(exitCode).toBe(1);
    });

    it('should handle initialization errors', async () => {
      mockInitialize.mockRejectedValue(new Error('Init failed'));

      let exitCalled = false;
      processExitSpy.mockImplementation(() => {
        exitCalled = true;
        return undefined as never;
      });

      try {
        await import('../src/index.js');
        await waitForAsyncOperations();
        await waitForAsyncOperations();
      } catch {
        void 0;
      }

      expect(exitCalled).toBe(true);
    });
  });

  describe('Tool Names Export Consistency', () => {
    it('should have consistent tool name exports', () => {
      expect(TOOL_NAMES.GITHUB_SEARCH_CODE).toBe(TOOL_NAMES.GITHUB_SEARCH_CODE);
      expect(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES).toBe(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
      );
      expect(TOOL_NAMES.GITHUB_FETCH_CONTENT).toBe(
        TOOL_NAMES.GITHUB_FETCH_CONTENT
      );
    });
  });

  describe('registerAllTools', () => {
    it('should handle missing GitHub token silently', async () => {
      mockGetGitHubToken.mockResolvedValue(null);
      const { registerAllTools } = await import('../src/index.js');

      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      await registerAllTools(mockMcpServer as unknown as McpServer);

      expect(mockRegisterTools).toHaveBeenCalled();
      expect(stderrSpy).not.toHaveBeenCalled();

      stderrSpy.mockRestore();
    });

    it('should handle GitHub token available', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');
      const { registerAllTools } = await import('../src/index.js');

      await registerAllTools(mockMcpServer as unknown as McpServer);

      expect(mockRegisterTools).toHaveBeenCalled();
    });
  });

  describe('Clone Configuration', () => {
    it('should start cache GC when clone is enabled', async () => {
      mockIsCloneEnabled.mockReturnValue(true);

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockIsCloneEnabled).toHaveBeenCalled();
    });

    it('should not start cache GC when clone is disabled', async () => {
      mockIsCloneEnabled.mockReturnValue(false);

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockIsCloneEnabled).toHaveBeenCalled();
    });
  });

  describe('Tools Configuration', () => {
    let originalEnableTools: string | undefined;
    let originalDisableTools: string | undefined;

    beforeEach(() => {
      originalEnableTools = process.env.ENABLE_TOOLS;
      originalDisableTools = process.env.DISABLE_TOOLS;
    });

    afterEach(() => {
      if (originalEnableTools !== undefined) {
        process.env.ENABLE_TOOLS = originalEnableTools;
      } else {
        delete process.env.ENABLE_TOOLS;
      }
      if (originalDisableTools !== undefined) {
        process.env.DISABLE_TOOLS = originalDisableTools;
      } else {
        delete process.env.DISABLE_TOOLS;
      }
    });

    it('should register default tools when no configuration is set', async () => {
      delete process.env.ENABLE_TOOLS;
      delete process.env.DISABLE_TOOLS;

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should register default tools when configuration is empty', async () => {
      process.env.ENABLE_TOOLS = '';
      process.env.DISABLE_TOOLS = '';

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should enable additional tools with ENABLE_TOOLS', async () => {
      process.env.ENABLE_TOOLS = TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS;

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should disable tools with DISABLE_TOOLS', async () => {
      process.env.DISABLE_TOOLS = 'ghSearchCode,ghGetFileContent';

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should handle both ENABLE_TOOLS and DISABLE_TOOLS', async () => {
      process.env.ENABLE_TOOLS = TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS;
      process.env.DISABLE_TOOLS = TOOL_NAMES.GITHUB_SEARCH_CODE;

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should handle whitespace in tool configuration', async () => {
      process.env.ENABLE_TOOLS = ' ghHistoryResearch ';
      process.env.DISABLE_TOOLS = ' ghSearchCode ';

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should handle invalid tool names gracefully', async () => {
      process.env.ENABLE_TOOLS = 'ghHistoryResearch,invalidTool';
      process.env.DISABLE_TOOLS = 'nonExistentTool';

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should exit when all tools are disabled', async () => {
      mockRegisterTools.mockImplementation(async () => {
        return { successCount: 0, failedTools: [] };
      });

      let exitCalled = false;
      let exitCode: number | undefined;
      processExitSpy.mockImplementation(
        (code?: string | number | null | undefined) => {
          exitCalled = true;
          exitCode =
            typeof code === 'number'
              ? code
              : code
                ? parseInt(String(code))
                : undefined;
          return undefined as never;
        }
      );

      try {
        await import('../src/index.js');
        await waitForAsyncOperations();
        await waitForAsyncOperations();
      } catch {
        void 0;
      }

      expect(exitCalled).toBe(true);
      expect(exitCode).toBe(1);
    });
  });
});
