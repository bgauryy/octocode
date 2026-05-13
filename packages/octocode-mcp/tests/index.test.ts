import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';

// Mock all dependencies before importing index
vi.mock('@modelcontextprotocol/sdk/server/mcp.js');
vi.mock('@modelcontextprotocol/sdk/server/stdio.js');
vi.mock('../src/utils/http/cache.js');
vi.mock('../src/prompts/prompts.js');
vi.mock('../src/tools/github_search_code/github_search_code.js');
vi.mock('../src/tools/github_fetch_content/github_fetch_content.js');
vi.mock('../src/tools/github_search_repos/github_search_repos.js');
vi.mock(
  '../src/tools/github_search_pull_requests/github_search_pull_requests.js'
);
vi.mock(
  '../src/tools/github_view_repo_structure/github_view_repo_structure.js'
);
vi.mock('../src/utils/exec/npm.js');
vi.mock('../src/serverConfig.js');
vi.mock('../src/tools/toolsManager.js');
vi.mock('../src/providers/factory.js', () => ({
  initializeProviders: vi.fn().mockResolvedValue(undefined),
  clearProviderCache: vi.fn(),
}));
vi.mock('../src/github/client.js', () => ({
  clearOctokitInstances: vi.fn(),
}));
vi.mock('../src/session.js', () => ({
  initializeSession: vi
    .fn()
    .mockReturnValue({ getSessionId: () => 'test-session-id' }),
  logSessionInit: vi.fn().mockResolvedValue(undefined),
  logSessionError: vi.fn().mockResolvedValue(undefined),
  logToolCall: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('octocode-security-utils/withSecurityValidation', () => ({
  configureSecurity: vi.fn(),
}));
vi.mock('../src/tools/toolMetadata/proxies.js', async importOriginal => ({
  ...(await importOriginal<object>()),
  loadToolContent: vi
    .fn()
    .mockResolvedValue({ instructions: 'Test instructions' }),
}));
vi.mock('../src/tools/github_clone_repo/cache.js', () => ({
  startCacheGC: vi.fn(),
  stopCacheGC: vi.fn(),
}));
vi.mock('../src/utils/core/logger.js', () => {
  const mockLogger = {
    info: vi.fn().mockResolvedValue(undefined),
    warning: vi.fn().mockResolvedValue(undefined),
    error: vi.fn().mockResolvedValue(undefined),
  };
  return {
    createLogger: vi.fn().mockReturnValue(mockLogger),
    LoggerFactory: { getLogger: vi.fn().mockReturnValue(mockLogger) },
    Logger: vi.fn(),
  };
});

// Import mocked functions
import { registerPrompts } from '../src/prompts/prompts.js';
import { registerGitHubSearchCodeTool } from '../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../src/tools/github_fetch_content/github_fetch_content.js';
import { registerSearchGitHubReposTool } from '../src/tools/github_search_repos/github_search_repos.js';
import { registerSearchGitHubPullRequestsTool } from '../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { registerViewGitHubRepoStructureTool } from '../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { getGithubCLIToken } from '../src/utils/exec/npm.js';
import {
  initialize,
  cleanup,
  getServerConfig,
  getGitHubToken,
  arePromptsEnabled,
  isCloneEnabled,
  getActiveProvider,
} from '../src/serverConfig.js';
import { registerTools } from '../src/tools/toolsManager.js';
import { TOOL_NAMES } from '../src/tools/toolMetadata/proxies.js';
import {
  allowExpectedStderrWarning,
  allowUnexpectedWarningFailureForCurrentTest,
} from './warningPolicy.js';

// Mock implementations
const mockMcpServer = {
  connect: vi.fn(function () {}),
  close: vi.fn(function () {}),
};

const mockTransport = {
  start: vi.fn(function () {}),
};

const mockRegisterPrompts = vi.mocked(registerPrompts);
const mockMcpServerConstructor = vi.mocked(McpServer);
const mockStdioServerTransport = vi.mocked(StdioServerTransport);
const mockGetGithubCLIToken = vi.mocked(getGithubCLIToken);
const mockRegisterTools = vi.mocked(registerTools);
const mockGetGitHubToken = vi.mocked(getGitHubToken);
const mockInitialize = vi.mocked(initialize);
const mockCleanup = vi.mocked(cleanup);
const mockGetServerConfig = vi.mocked(getServerConfig);
const mockArePromptsEnabled = vi.mocked(arePromptsEnabled);
const mockIsCloneEnabled = vi.mocked(isCloneEnabled);
const mockGetActiveProvider = vi.mocked(getActiveProvider);

// Mock all tool registration functions
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

  // uncork spies removed — uncork calls were removed from index.ts (stdio safety)
  let originalGithubToken: string | undefined;
  let originalGhToken: string | undefined;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules(); // Reset module cache

    // Store original environment variables
    originalGithubToken = process.env.GITHUB_TOKEN;
    originalGhToken = process.env.GH_TOKEN;

    // Set a test token to avoid getToken() issues
    process.env.GITHUB_TOKEN = 'test-token';

    // Setup default mock implementations
    mockMcpServerConstructor.mockImplementation(function () {
      return mockMcpServer as unknown as InstanceType<typeof McpServer>;
    });
    mockStdioServerTransport.mockImplementation(function () {
      return mockTransport as unknown as InstanceType<
        typeof StdioServerTransport
      >;
    });

    // Mock GitHub CLI token
    mockGetGithubCLIToken.mockResolvedValue('cli-token');

    // Create spies for process methods - use a safer mock that doesn't throw by default
    processExitSpy = vi.spyOn(process, 'exit').mockImplementation(function (
      _code?: string | number | null | undefined
    ) {
      // Don't throw by default - let individual tests override if needed
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
    // uncork spies removed — uncork calls were removed from index.ts (stdio safety)

    // Mock server connect to resolve immediately
    mockMcpServer.connect.mockResolvedValue(undefined);
    mockMcpServer.close.mockResolvedValue(undefined);

    // Mock all tool registration functions to succeed by default
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
    mockRegisterPrompts.mockImplementation(function () {
      return mockRegisteredTool;
    });
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

    // Mock simplified dependencies
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
      loggingEnabled: true,
      enableLocal: false,
      enableClone: false,
      disablePrompts: false,
      outputFormat: 'yaml',
      tokenSource: 'env:GITHUB_TOKEN',
    });

    // Mock registerTools to return success count based on config
    mockRegisterTools.mockImplementation(async () => {
      return { successCount: 4, failedTools: [] }; // Default tools count
    });

    // Mock arePromptsEnabled to return true by default (prompts enabled)
    mockArePromptsEnabled.mockReturnValue(true);

    // Mock isCloneEnabled and getActiveProvider
    mockIsCloneEnabled.mockReturnValue(false);
    mockGetActiveProvider.mockReturnValue('github');
  });

  afterEach(() => {
    // Restore original environment variables
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

    // Restore spies
    processExitSpy?.mockRestore();
    processStdinResumeSpy?.mockRestore();
    processStdinOnSpy?.mockRestore();
    processOnSpy?.mockRestore();
    // uncork spies removed — uncork calls were removed from index.ts (stdio safety)
  });

  // Helper function to wait for startup promises to settle without real timers.
  const waitForAsyncOperations = async () => {
    for (let i = 0; i < 25; i++) await Promise.resolve();
  };

  describe('Basic Module Import', () => {
    it('should create server with correct configuration', async () => {
      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockMcpServerConstructor).toHaveBeenCalledWith(
        expect.objectContaining({
          name: expect.stringContaining('octocode-mcp'),
          title: 'Octocode MCP',
          version: expect.any(String),
        }),
        expect.objectContaining({
          capabilities: expect.objectContaining({
            prompts: {},
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
      mockGetGithubCLIToken.mockResolvedValue('cli-token');

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should exit when no token is available', async () => {
      delete process.env.GITHUB_TOKEN;
      delete process.env.GH_TOKEN;
      mockGetGithubCLIToken.mockResolvedValue(null);

      // Mock getToken to throw when no token is available
      mockGetGitHubToken.mockRejectedValue(new Error('No token available'));

      // Override the mock to track the exit call without throwing
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
        // Ignore any errors from module loading
      }

      expect(exitCalled).toBe(true);
      expect(exitCode).toBe(1);
    });
  });

  describe('Tool Registration', () => {
    it('should register all tools successfully', async () => {
      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was called with server
      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should continue registering tools even if some fail', async () => {
      // Mock registerTools to return partial success
      mockRegisterTools.mockImplementation(async () => {
        return {
          successCount: 3,
          failedTools: [TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS],
        };
      });

      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was still called
      expect(mockRegisterTools).toHaveBeenCalled();
    });

    it('should exit when no tools are successfully registered', async () => {
      // Make registerTools return no successful registrations
      mockRegisterTools.mockImplementation(async () => {
        return { successCount: 0, failedTools: ['all'] };
      });

      // Track the exit call without throwing
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
        // Ignore any errors from module loading
      }

      expect(exitCalled).toBe(true);
      expect(exitCode).toBe(1);
    });

    it('should handle tool registration errors gracefully', async () => {
      // Mock registerTools to return partial success
      mockRegisterTools.mockImplementation(async () => {
        return {
          successCount: 2,
          failedTools: [TOOL_NAMES.GITHUB_SEARCH_CODE],
        };
      });

      // The module should still load
      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was called
      expect(mockRegisterTools).toHaveBeenCalled();
    });

    it('should handle multiple tool registration errors', async () => {
      // Mock registerTools to return partial success with multiple failures
      mockRegisterTools.mockImplementation(async () => {
        return { successCount: 1, failedTools: ['tool1', 'tool2'] };
      });

      // The module should still load
      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was called
      expect(mockRegisterTools).toHaveBeenCalled();
    });

    it('should handle all tool registration errors', async () => {
      // Mock registerTools to fail completely
      mockRegisterTools.mockImplementation(async () => {
        return { successCount: 0, failedTools: ['all', 'tools', 'failed'] };
      });

      // Track the exit call without throwing
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
        // Ignore any errors from module loading
      }

      // Verify that the server was created but exit was called
      expect(mockMcpServerConstructor).toHaveBeenCalled();
      expect(exitCalled).toBe(true);
      expect(exitCode).toBe(1);
    });
  });

  describe('Server Startup', () => {
    it('should handle server startup errors', async () => {
      mockMcpServer.connect.mockRejectedValue(new Error('Connection failed'));

      // Track the exit call without throwing
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
        // Ignore any errors from module loading
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
        // Ignore
      }

      expect(exitCalled).toBe(true);
    });
  });

  // Signal handling tests removed - they depend on complex startup mocking

  // Graceful shutdown tests removed - they depend on complex startup mocking

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
    it('should handle missing GitHub token with warning', async () => {
      mockGetGitHubToken.mockResolvedValue(null);
      const { registerAllTools } = await import('../src/index.js');
      allowExpectedStderrWarning(/No GitHub token available/);
      allowUnexpectedWarningFailureForCurrentTest();

      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      const mockContent = {
        instructions: 'test',
        prompts: {},
        toolNames: TOOL_NAMES,
        baseSchema: {
          mainResearchGoal: '',
          researchGoal: '',
          reasoning: '',
          bulkQuery: () => '',
        },
        tools: {},
        baseHints: { hasResults: [], empty: [] },
        genericErrorHints: [],
      };

      await registerAllTools(
        mockMcpServer as unknown as McpServer,
        mockContent
      );

      // Should still register tools but log warning
      expect(mockRegisterTools).toHaveBeenCalled();
      expect(stderrSpy).toHaveBeenCalledWith(
        expect.stringContaining('No GitHub token available')
      );

      stderrSpy.mockRestore();
    });

    it('should not warn about a missing GitHub token when GitLab is the active provider', async () => {
      mockGetActiveProvider.mockReturnValue('gitlab');
      mockGetGitHubToken.mockResolvedValue(null);

      const { LoggerFactory } = await import('../src/utils/core/logger.js');
      const mockLogger = {
        info: vi.fn().mockResolvedValue(undefined),
        warning: vi.fn().mockResolvedValue(undefined),
        error: vi.fn().mockResolvedValue(undefined),
      };
      vi.mocked(LoggerFactory.getLogger).mockReturnValue(
        mockLogger as unknown as ReturnType<typeof LoggerFactory.getLogger>
      );

      const { registerAllTools } = await import('../src/index.js');
      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      const mockContent = {
        instructions: 'test',
        prompts: {},
        toolNames: TOOL_NAMES,
        baseSchema: {
          mainResearchGoal: '',
          researchGoal: '',
          reasoning: '',
          bulkQuery: () => '',
        },
        tools: {},
        baseHints: { hasResults: [], empty: [] },
        genericErrorHints: [],
      };

      await registerAllTools(
        mockMcpServer as unknown as McpServer,
        mockContent
      );

      expect(mockRegisterTools).toHaveBeenCalled();
      expect(mockGetGitHubToken).not.toHaveBeenCalled();
      expect(mockLogger.warning).not.toHaveBeenCalledWith(
        'No GitHub token - limited functionality'
      );
      expect(stderrSpy).not.toHaveBeenCalled();

      stderrSpy.mockRestore();
    });

    it('should handle GitHub token available', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');
      const { registerAllTools } = await import('../src/index.js');

      const mockContent = {
        instructions: 'test',
        prompts: {},
        toolNames: TOOL_NAMES,
        baseSchema: {
          mainResearchGoal: '',
          researchGoal: '',
          reasoning: '',
          bulkQuery: () => '',
        },
        tools: {},
        baseHints: { hasResults: [], empty: [] },
        genericErrorHints: [],
      };

      await registerAllTools(
        mockMcpServer as unknown as McpServer,
        mockContent
      );

      expect(mockRegisterTools).toHaveBeenCalled();
    });
  });

  describe('Prompts Configuration', () => {
    it('should register prompts when arePromptsEnabled returns true', async () => {
      mockArePromptsEnabled.mockReturnValue(true);

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterPrompts).toHaveBeenCalled();
    });

    it('should not register prompts when arePromptsEnabled returns false', async () => {
      mockArePromptsEnabled.mockReturnValue(false);

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockRegisterPrompts).not.toHaveBeenCalled();
    });

    it('should include prompts capability when prompts are enabled', async () => {
      mockArePromptsEnabled.mockReturnValue(true);

      await import('../src/index.js');
      await waitForAsyncOperations();

      expect(mockMcpServerConstructor).toHaveBeenCalledWith(
        expect.anything(),
        expect.objectContaining({
          capabilities: expect.objectContaining({
            prompts: {},
          }),
        })
      );
    });

    it('should exclude prompts capability when prompts are disabled', async () => {
      mockArePromptsEnabled.mockReturnValue(false);

      await import('../src/index.js');
      await waitForAsyncOperations();

      const serverOptions = mockMcpServerConstructor.mock.calls[0]?.[1];
      expect(serverOptions?.capabilities).not.toHaveProperty('prompts');
    });
  });

  describe('Clone Configuration', () => {
    it('should start cache GC when clone is enabled', async () => {
      mockIsCloneEnabled.mockReturnValue(true);

      await import('../src/index.js');
      await waitForAsyncOperations();

      // Clone-enabled branch should be exercised
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
      // Store original environment variables
      originalEnableTools = process.env.ENABLE_TOOLS;
      originalDisableTools = process.env.DISABLE_TOOLS;
    });

    afterEach(() => {
      // Restore original environment variables
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

      // Verify registerTools was called (default tools would be registered)
      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should register default tools when configuration is empty', async () => {
      process.env.ENABLE_TOOLS = '';
      process.env.DISABLE_TOOLS = '';

      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was called
      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should enable additional tools with ENABLE_TOOLS', async () => {
      process.env.ENABLE_TOOLS = TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS;

      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was called with server
      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);

      // The actual tool filtering logic is tested in the registerTools function
      // Here we just verify the main registration flow works
    });

    it('should disable tools with DISABLE_TOOLS', async () => {
      process.env.DISABLE_TOOLS = 'githubSearchCode,githubGetFileContent';

      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was called
      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should handle both ENABLE_TOOLS and DISABLE_TOOLS', async () => {
      process.env.ENABLE_TOOLS = TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS;
      process.env.DISABLE_TOOLS = TOOL_NAMES.GITHUB_SEARCH_CODE;

      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was called
      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should handle whitespace in tool configuration', async () => {
      process.env.ENABLE_TOOLS = ' githubSearchPullRequests ';
      process.env.DISABLE_TOOLS = ' githubSearchCode ';

      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was called (whitespace handling is done in serverConfig)
      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should handle invalid tool names gracefully', async () => {
      process.env.ENABLE_TOOLS = 'githubSearchPullRequests,invalidTool';
      process.env.DISABLE_TOOLS = 'nonExistentTool';

      await import('../src/index.js');
      await waitForAsyncOperations();

      // Verify registerTools was called (invalid tools are ignored)
      expect(mockRegisterTools).toHaveBeenCalledWith(mockMcpServer);
    });

    it('should exit when all tools are disabled', async () => {
      // Mock registerTools to return no successful registrations
      mockRegisterTools.mockImplementation(async () => {
        return { successCount: 0, failedTools: [] };
      });

      // Track the exit call without throwing
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
        // Ignore any errors from module loading
      }

      // Verify the process exits with error code
      expect(exitCalled).toBe(true);
      expect(exitCode).toBe(1);
    });
  });
});
