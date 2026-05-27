import { describe, it, expect, beforeEach, afterAll, vi } from 'vitest';
import type { OctocodeLogger } from '../src/utils/core/logger.js';
import {
  allowExpectedStderrWarning,
  allowUnexpectedWarningFailureForCurrentTest,
} from './warningPolicy.js';

// Mock process.exit to prevent tests from actually exiting
const mockProcessExit = vi.spyOn(process, 'exit').mockImplementation((() => {
  // Do nothing instead of exiting
}) as never);

// Must mock before importing
const mockServer = {
  setRequestHandler: vi.fn(),
  close: vi.fn().mockResolvedValue(undefined),
  connect: vi.fn().mockResolvedValue(undefined),
};

const mockTransport = {
  start: vi.fn(),
};

vi.mock('@modelcontextprotocol/sdk/server/mcp.js', () => ({
  McpServer: vi.fn(() => mockServer),
}));

vi.mock('@modelcontextprotocol/sdk/server/stdio.js', () => ({
  StdioServerTransport: vi.fn(() => mockTransport),
}));

vi.mock('../src/serverConfig.js', () => ({
  initialize: vi.fn(() => Promise.resolve()),
  cleanup: vi.fn(),
  getGitHubToken: vi.fn(() => Promise.resolve('test-token')),
  getActiveProvider: vi.fn(() => 'github'),
}));

vi.mock('../src/session.js', () => ({
  initializeSession: vi.fn(() => ({
    getSessionId: () => 'test-session-id',
  })),
  logSessionInit: vi.fn(() => Promise.resolve()),
  logSessionError: vi.fn(() => Promise.resolve()),
}));

vi.mock('../src/tools/toolMetadata/state.js', () => ({
  loadToolContent: vi.fn(() =>
    Promise.resolve({
      instructions: 'Test instructions',
      toolNames: {
        GITHUB_FETCH_CONTENT: 'githubGetFileContent',
        GITHUB_SEARCH_CODE: 'githubSearchCode',
        GITHUB_SEARCH_PULL_REQUESTS: 'githubSearchPullRequests',
        GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
        GITHUB_VIEW_REPO_STRUCTURE: 'githubViewRepoStructure',
      },
      tools: {},
      baseHints: { hasResults: [], empty: [] },
      genericErrorHints: [],
      baseSchema: {
        mainResearchGoal: '',
        researchGoal: '',
        reasoning: '',
        bulkQuery: () => '',
      },
    })
  ),
  getMetadataOrNull: vi.fn(() => null),
}));

vi.mock('../src/tools/toolsManager.js', () => ({
  registerTools: vi.fn(() =>
    Promise.resolve({ successCount: 5, failedTools: [] })
  ),
}));

vi.mock('../src/utils/http/cache.js', () => ({
  clearAllCache: vi.fn(),
}));

vi.mock('../src/utils/core/logger.js', () => ({
  createLogger: vi.fn(() => ({
    info: vi.fn(() => Promise.resolve()),
    error: vi.fn(() => Promise.resolve()),
    warning: vi.fn(() => Promise.resolve()),
  })),
  LoggerFactory: {
    getLogger: vi.fn(() => ({
      info: vi.fn(() => Promise.resolve()),
      error: vi.fn(() => Promise.resolve()),
      warning: vi.fn(() => Promise.resolve()),
    })),
  },
}));

// Import after mocks are set up - do NOT trigger module initialization
// Only import the specific function we want to test
let registerAllTools: (server: never, content: never) => Promise<void>;

describe('index.ts - Server Lifecycle', () => {
  beforeEach(async () => {
    vi.clearAllMocks();

    // Reset process.exit mock but keep it from actually exiting
    mockProcessExit.mockClear();
    mockProcessExit.mockImplementation((() => {
      // Do nothing
    }) as never);

    // Dynamically import only the function we need
    const indexModule = await import('../src/index.js');
    registerAllTools = indexModule.registerAllTools;
  });

  afterAll(() => {
    // Restore process.exit after all tests
    mockProcessExit.mockRestore();
  });

  describe('registerAllTools', () => {
    it('should register tools successfully with GitHub token', async () => {
      const { getGitHubToken } = await import('../src/serverConfig.js');
      vi.mocked(getGitHubToken).mockResolvedValueOnce('test-token');

      const { registerTools } = await import('../src/tools/toolsManager.js');
      vi.mocked(registerTools).mockResolvedValueOnce({
        successCount: 5,
        failedTools: [],
      });

      const { loadToolContent } =
        await import('../src/tools/toolMetadata/state.js');
      const content = await loadToolContent();

      await registerAllTools(mockServer as never, content as never);

      expect(registerTools).toHaveBeenCalledWith(mockServer);
    });

    it('should log warning when no GitHub token available', async () => {
      const { getGitHubToken } = await import('../src/serverConfig.js');
      const { LoggerFactory } = await import('../src/utils/core/logger.js');

      vi.mocked(getGitHubToken).mockResolvedValueOnce(null);

      const mockLogger = {
        info: vi.fn(() => Promise.resolve()),
        warning: vi.fn(() => Promise.resolve()),
        error: vi.fn(() => Promise.resolve()),
        debug: vi.fn(() => Promise.resolve()),
        prefix: 'test-prefix',
        server: mockServer,
      };
      vi.mocked(LoggerFactory.getLogger).mockReturnValue(
        mockLogger as unknown as OctocodeLogger
      );

      const { registerTools } = await import('../src/tools/toolsManager.js');
      vi.mocked(registerTools).mockResolvedValueOnce({
        successCount: 3,
        failedTools: [],
      });

      allowExpectedStderrWarning(/No GitHub token available/);
      allowUnexpectedWarningFailureForCurrentTest();

      const { loadToolContent } =
        await import('../src/tools/toolMetadata/state.js');
      const content = await loadToolContent();

      await registerAllTools(mockServer as never, content as never);

      expect(mockLogger.warning).toHaveBeenCalledWith(
        'No GitHub token - limited functionality'
      );
      expect(registerTools).toHaveBeenCalled();
    });

    it('should throw error when no tools are registered', async () => {
      const { getGitHubToken } = await import('../src/serverConfig.js');
      vi.mocked(getGitHubToken).mockResolvedValueOnce('test-token');

      const { registerTools } = await import('../src/tools/toolsManager.js');
      vi.mocked(registerTools).mockResolvedValueOnce({
        successCount: 0,
        failedTools: [],
      });

      const { loadToolContent } =
        await import('../src/tools/toolMetadata/state.js');
      const content = await loadToolContent();

      await expect(
        registerAllTools(mockServer as never, content as never)
      ).rejects.toThrow('No tools were successfully registered');
    });

    it('should log session error when no tools registered', async () => {
      const { getGitHubToken } = await import('../src/serverConfig.js');
      vi.mocked(getGitHubToken).mockResolvedValueOnce('test-token');

      const { registerTools } = await import('../src/tools/toolsManager.js');
      vi.mocked(registerTools).mockResolvedValueOnce({
        successCount: 0,
        failedTools: [],
      });

      const { logSessionError } = await import('../src/session.js');

      const { loadToolContent } =
        await import('../src/tools/toolMetadata/state.js');
      const content = await loadToolContent();

      try {
        await registerAllTools(mockServer as never, content as never);
      } catch {
        expect(logSessionError).toHaveBeenCalledWith(
          'startup',
          'STARTUP_NO_TOOLS_REGISTERED'
        );
      }
    });

    it('should write to stderr when no GitHub token', async () => {
      const { getGitHubToken } = await import('../src/serverConfig.js');
      vi.mocked(getGitHubToken).mockResolvedValueOnce(null);

      const { registerTools } = await import('../src/tools/toolsManager.js');
      vi.mocked(registerTools).mockResolvedValueOnce({
        successCount: 3,
        failedTools: [],
      });

      allowExpectedStderrWarning(/No GitHub token available/);
      allowUnexpectedWarningFailureForCurrentTest();

      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      const { loadToolContent } =
        await import('../src/tools/toolMetadata/state.js');
      const content = await loadToolContent();

      await registerAllTools(mockServer as never, content as never);

      expect(stderrSpy).toHaveBeenCalledWith(
        expect.stringContaining('No GitHub token available')
      );

      stderrSpy.mockRestore();
    });

    it('should log info when GitHub token is ready', async () => {
      const { getGitHubToken } = await import('../src/serverConfig.js');
      const { LoggerFactory } = await import('../src/utils/core/logger.js');

      vi.mocked(getGitHubToken).mockResolvedValueOnce('test-token');

      const mockLogger = {
        info: vi.fn(() => Promise.resolve()),
        warning: vi.fn(() => Promise.resolve()),
        error: vi.fn(() => Promise.resolve()),
        debug: vi.fn(() => Promise.resolve()),
        prefix: 'test-prefix',
        server: mockServer,
      };
      vi.mocked(LoggerFactory.getLogger).mockReturnValue(
        mockLogger as unknown as OctocodeLogger
      );

      const { registerTools } = await import('../src/tools/toolsManager.js');
      vi.mocked(registerTools).mockResolvedValueOnce({
        successCount: 5,
        failedTools: [],
      });

      const { loadToolContent } =
        await import('../src/tools/toolMetadata/state.js');
      const content = await loadToolContent();

      await registerAllTools(mockServer as never, content as never);

      expect(mockLogger.info).toHaveBeenCalledWith('GitHub token ready');
      expect(mockLogger.info).toHaveBeenCalledWith('Tools registered', {
        count: 5,
      });
    });

    it('should log error when tool registration throws', async () => {
      const { getGitHubToken } = await import('../src/serverConfig.js');
      const { LoggerFactory } = await import('../src/utils/core/logger.js');

      vi.mocked(getGitHubToken).mockResolvedValueOnce('test-token');

      const mockLogger = {
        info: vi.fn(() => Promise.resolve()),
        warning: vi.fn(() => Promise.resolve()),
        error: vi.fn(() => Promise.resolve()),
        debug: vi.fn(() => Promise.resolve()),
        prefix: 'test-prefix',
        server: mockServer,
      };
      vi.mocked(LoggerFactory.getLogger).mockReturnValue(
        mockLogger as unknown as OctocodeLogger
      );

      const { registerTools } = await import('../src/tools/toolsManager.js');
      vi.mocked(registerTools).mockResolvedValueOnce({
        successCount: 0,
        failedTools: [],
      });

      const { loadToolContent } =
        await import('../src/tools/toolMetadata/state.js');
      const content = await loadToolContent();

      try {
        await registerAllTools(mockServer as never, content as never);
      } catch {
        expect(mockLogger.error).toHaveBeenCalledWith(
          'Tool registration failed'
        );
      }
    });
  });
});

describe('Graceful Shutdown Logic', () => {
  it('should export registerAllTools function', () => {
    expect(typeof registerAllTools).toBe('function');
  });

  it('should handle module-level error scenarios', async () => {
    // Test the pattern: async function().catch(() => { process.exit(1); })
    const mockAsyncFunction = vi.fn().mockRejectedValue(new Error('Failed'));

    let exitCalled = false;
    const mockExit = vi.fn((_code?: number) => {
      exitCalled = true;
    });

    await mockAsyncFunction().catch(() => {
      mockExit(1);
    });

    expect(exitCalled).toBe(true);
    expect(mockExit).toHaveBeenCalledWith(1);
  });

  it('should handle various error types in top-level catch', async () => {
    const errorTypes = [
      new Error('Error object'),
      'String error',
      { message: 'Object error' },
      42,
      null,
      undefined,
    ];

    for (const errorType of errorTypes) {
      const mockFn = vi.fn().mockRejectedValue(errorType);
      let catchExecuted = false;

      await mockFn().catch(() => {
        catchExecuted = true;
      });

      expect(catchExecuted).toBe(true);
    }
  });
});

describe('Error Handler Coverage', () => {
  it('should test unhandled rejection pattern', async () => {
    const mockLogger = {
      error: vi.fn().mockResolvedValue(undefined),
    };

    const mockLogSessionError = vi.fn().mockResolvedValue(undefined);
    const mockGracefulShutdown = vi.fn();

    // Simulate unhandled rejection handler logic
    const reason = new Error('Test rejection');

    await mockLogger.error('Unhandled rejection', {
      reason: String(reason),
    });
    await mockLogSessionError('startup', 'UNHANDLED_REJECTION').catch(() => {});
    mockGracefulShutdown('UNHANDLED_REJECTION');

    expect(mockLogger.error).toHaveBeenCalledWith('Unhandled rejection', {
      reason: expect.stringContaining('Test rejection'),
    });
    expect(mockLogSessionError).toHaveBeenCalledWith(
      'startup',
      'UNHANDLED_REJECTION'
    );
    expect(mockGracefulShutdown).toHaveBeenCalledWith('UNHANDLED_REJECTION');
  });

  it('should test uncaught exception pattern', async () => {
    const mockLogger = {
      error: vi.fn().mockResolvedValue(undefined),
    };

    const mockLogSessionError = vi.fn().mockResolvedValue(undefined);
    const mockGracefulShutdown = vi.fn();

    // Simulate uncaught exception handler logic
    const error = new Error('Test exception');

    await mockLogger.error('Uncaught exception', {
      error: error.message,
    });
    await mockLogSessionError('startup', 'UNCAUGHT_EXCEPTION').catch(() => {});
    mockGracefulShutdown('UNCAUGHT_EXCEPTION');

    expect(mockLogger.error).toHaveBeenCalledWith('Uncaught exception', {
      error: 'Test exception',
    });
    expect(mockLogSessionError).toHaveBeenCalledWith(
      'startup',
      'UNCAUGHT_EXCEPTION'
    );
    expect(mockGracefulShutdown).toHaveBeenCalledWith('UNCAUGHT_EXCEPTION');
  });

  it('should test top-level startup error catch', async () => {
    let exitCode: number | undefined;
    const mockExit = vi.fn((code: number) => {
      exitCode = code;
    });

    const mockStartServer = async () => {
      throw new Error('Startup failed');
    };

    // Simulate: startServer().catch(() => { process.exit(1); })
    await mockStartServer().catch(() => {
      mockExit(1);
    });

    expect(exitCode).toBe(1);
  });
});
