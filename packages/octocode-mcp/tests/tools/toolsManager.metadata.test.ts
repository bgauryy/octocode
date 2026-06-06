import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { registerTools } from '../../src/tools/toolsManager.js';

vi.mock('../../src/tools/toolMetadata/proxies.js', async () => {
  const actual = await vi.importActual<
    typeof import('../../src/tools/toolMetadata/proxies.js')
  >('../../src/tools/toolMetadata/proxies.js');
  return {
    ...actual,
    isToolInMetadata: vi.fn(),
    TOOL_NAMES: {
      GITHUB_FETCH_CONTENT: 'githubGetFileContent',
      GITHUB_SEARCH_CODE: 'githubSearchCode',
      GITHUB_SEARCH_PULL_REQUESTS: 'githubSearchPullRequests',
      GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
      GITHUB_VIEW_REPO_STRUCTURE: 'githubViewRepoStructure',
    },
    DESCRIPTIONS: new Proxy(
      {},
      {
        get: (_target, prop: string) => {
          return `Description for ${prop}`;
        },
      }
    ),
  };
});

vi.mock('../../src/tools/toolConfig.js', () => {
  const mockGitHubTools = [
    { name: 'githubSearchCode', isDefault: true, isLocal: false, fn: vi.fn() },
    {
      name: 'githubGetFileContent',
      isDefault: true,
      isLocal: false,
      fn: vi.fn(),
    },
    {
      name: 'githubViewRepoStructure',
      isDefault: true,
      isLocal: false,
      fn: vi.fn(),
    },
    {
      name: 'githubSearchRepositories',
      isDefault: true,
      isLocal: false,
      fn: vi.fn(),
    },
    {
      name: 'githubSearchPullRequests',
      isDefault: true,
      isLocal: false,
      fn: vi.fn(),
    },
  ];
  return {
    ALL_TOOLS: mockGitHubTools,
  };
});

vi.mock('../../src/serverConfig.js', () => ({
  getServerConfig: vi.fn(),
  isLocalEnabled: vi.fn().mockReturnValue(false),
  isCloneEnabled: vi.fn().mockReturnValue(false),
}));

vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(),
}));

import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { getServerConfig } from '../../src/serverConfig.js';
import { isToolInMetadata } from '../../src/tools/toolMetadata/proxies.js';
import { logSessionError } from '../../src/session.js';
import { TOOL_METADATA_ERRORS } from '../../src/errors/domainErrors.js';

const mockGetServerConfig = vi.mocked(getServerConfig);
const mockIsToolAvailableSync = vi.mocked(isToolInMetadata);
const mockLogSessionError = vi.mocked(logSessionError);

describe('ToolsManager - Metadata Availability', () => {
  let mockServer: McpServer;
  const originalStderr = process.stderr.write;

  beforeEach(() => {
    vi.clearAllMocks();

    mockServer = {} as McpServer;

    process.stderr.write = vi.fn();

    ALL_TOOLS.forEach(tool => {
      vi.mocked(tool.fn).mockReset();
    });

    mockGetServerConfig.mockReturnValue({
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
  });

  afterEach(() => {
    process.stderr.write = originalStderr;
  });

  describe('All Tools Available in Metadata', () => {
    it('should register all tools when all are available in metadata', async () => {
      mockIsToolAvailableSync.mockReturnValue(true);

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(5);
      expect(result.failedTools).toEqual([]);

      ALL_TOOLS.forEach(tool => {
        expect(tool.fn).toHaveBeenCalledWith(mockServer, undefined);
      });

      expect(process.stderr.write).not.toHaveBeenCalled();

      expect(mockLogSessionError).not.toHaveBeenCalled();
    });
  });

  describe('Single Tool Missing from Metadata', () => {
    it('should skip githubSearchCode when not in metadata and log error', async () => {
      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName !== 'githubSearchCode';
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(4);
      expect(result.failedTools).toEqual([]);

      expect(ALL_TOOLS[0]?.fn).not.toHaveBeenCalled();

      expect(ALL_TOOLS[1]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[2]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).toHaveBeenCalled();

      expect(process.stderr.write).not.toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubSearchCode',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledTimes(1);
    });

    it('should skip githubGetFileContent when not in metadata and log error', async () => {
      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName !== 'githubGetFileContent';
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(4);
      expect(result.failedTools).toEqual([]);

      expect(ALL_TOOLS[1]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[0]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[2]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).toHaveBeenCalled();

      expect(process.stderr.write).not.toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubGetFileContent',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledTimes(1);
    });

    it('should skip githubViewRepoStructure when not in metadata and log error', async () => {
      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName !== 'githubViewRepoStructure';
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(4);
      expect(result.failedTools).toEqual([]);

      expect(ALL_TOOLS[2]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[0]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[1]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).toHaveBeenCalled();

      expect(process.stderr.write).not.toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubViewRepoStructure',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledTimes(1);
    });

    it('should skip githubSearchRepositories when not in metadata and log error', async () => {
      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName !== 'githubSearchRepositories';
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(4);
      expect(result.failedTools).toEqual([]);

      expect(ALL_TOOLS[3]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[0]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[1]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[2]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).toHaveBeenCalled();

      expect(process.stderr.write).not.toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubSearchRepositories',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledTimes(1);
    });

    it('should skip githubSearchPullRequests when not in metadata and log error', async () => {
      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName !== 'githubSearchPullRequests';
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(4);
      expect(result.failedTools).toEqual([]);

      expect(ALL_TOOLS[4]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[0]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[1]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[2]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).toHaveBeenCalled();

      expect(process.stderr.write).not.toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubSearchPullRequests',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledTimes(1);
    });
  });

  describe('Multiple Tools Missing from Metadata', () => {
    it('should skip multiple tools when not in metadata and log errors', async () => {
      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return (
          toolName !== 'githubSearchCode' &&
          toolName !== 'githubSearchPullRequests'
        );
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(3);
      expect(result.failedTools).toEqual([]);

      expect(ALL_TOOLS[0]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).not.toHaveBeenCalled();

      expect(ALL_TOOLS[1]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[2]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).toHaveBeenCalled();

      expect(process.stderr.write).not.toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubSearchCode',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubSearchPullRequests',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledTimes(2);
    });

    it('should skip all but one tool when most are missing from metadata and log errors', async () => {
      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName === 'githubSearchCode';
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(1);
      expect(result.failedTools).toEqual([]);

      expect(ALL_TOOLS[0]?.fn).toHaveBeenCalled();

      expect(ALL_TOOLS[1]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[2]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).not.toHaveBeenCalled();

      expect(process.stderr.write).not.toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledTimes(4);
    });
  });

  describe('No Tools Available in Metadata', () => {
    it('should register no tools when all are missing from metadata and log errors', async () => {
      mockIsToolAvailableSync.mockReturnValue(false);

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(0);
      expect(result.failedTools).toEqual([]);

      ALL_TOOLS.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });

      expect(process.stderr.write).not.toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledTimes(5);
    });
  });

  describe('Metadata Availability with TOOLS_TO_RUN', () => {
    it('should skip tools not in metadata even when specified in TOOLS_TO_RUN and log error', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['githubSearchCode', 'githubGetFileContent'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName !== 'githubSearchCode';
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(1);
      expect(result.failedTools).toEqual([]);

      expect(ALL_TOOLS[0]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[1]?.fn).toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubSearchCode',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledTimes(1);
    });

    it('should register no tools when TOOLS_TO_RUN specifies tools not in metadata and log errors', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['githubSearchCode', 'githubGetFileContent'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      mockIsToolAvailableSync.mockReturnValue(false);

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(0);
      expect(result.failedTools).toEqual([]);

      ALL_TOOLS.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });

      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubSearchCode',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubGetFileContent',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledTimes(2);
    });
  });

  describe('Metadata Availability with DISABLE_TOOLS', () => {
    it('should respect both metadata availability and DISABLE_TOOLS and log error', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        disableTools: ['githubGetFileContent'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName !== 'githubSearchCode';
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(3);
      expect(result.failedTools).toEqual([]);

      expect(ALL_TOOLS[0]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[1]?.fn).not.toHaveBeenCalled();

      expect(ALL_TOOLS[2]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).toHaveBeenCalled();

      expect(mockLogSessionError).toHaveBeenCalledWith(
        'githubSearchCode',
        TOOL_METADATA_ERRORS.INVALID_FORMAT.code
      );
      expect(mockLogSessionError).toHaveBeenCalledTimes(1);
    });
  });

  describe('Error Handling with Missing Metadata', () => {
    it('should handle registration errors for available tools', async () => {
      mockIsToolAvailableSync.mockReturnValue(true);

      vi.mocked(ALL_TOOLS[0]!.fn).mockImplementation(() => {
        throw new Error('Registration failed');
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(4);
      expect(result.failedTools).toEqual(['githubSearchCode']);

      expect(ALL_TOOLS[1]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[2]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).toHaveBeenCalled();
    });

    it('should not add missing metadata tools to failedTools', async () => {
      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName !== 'githubSearchCode';
      });

      vi.mocked(ALL_TOOLS[1]!.fn).mockImplementation(() => {
        throw new Error('Registration failed');
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(3);
      expect(result.failedTools).toEqual(['githubGetFileContent']);

      expect(ALL_TOOLS[0]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[2]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).toHaveBeenCalled();
    });
  });

  describe('Metadata Availability Check Edge Cases', () => {
    it('should handle isToolInMetadata returning undefined gracefully and log errors', async () => {
      mockIsToolAvailableSync.mockReturnValue(undefined as unknown as boolean);

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(0);
      expect(result.failedTools).toEqual([]);

      ALL_TOOLS.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });

      expect(mockLogSessionError).toHaveBeenCalledTimes(5);
    });

    it('should handle isToolInMetadata throwing error gracefully and log errors', async () => {
      mockIsToolAvailableSync.mockImplementation(() => {
        throw new Error('Metadata check failed');
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(0);
      expect(result.failedTools).toEqual([]);

      ALL_TOOLS.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });

      expect(mockLogSessionError).toHaveBeenCalledTimes(5);
      expect(mockLogSessionError).toHaveBeenCalledWith(
        expect.any(String),
        TOOL_METADATA_ERRORS.INVALID_API_RESPONSE.code
      );
    });

    it('should not abort registration when metadata logging rejects', async () => {
      mockIsToolAvailableSync.mockImplementation((toolName: string) => {
        return toolName !== 'githubSearchCode';
      });
      mockLogSessionError.mockRejectedValue(
        new Error('session logging unavailable')
      );

      await expect(registerTools(mockServer)).resolves.toEqual({
        successCount: 4,
        failedTools: [],
      });

      expect(ALL_TOOLS[0]?.fn).not.toHaveBeenCalled();
      expect(ALL_TOOLS[1]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[2]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[3]?.fn).toHaveBeenCalled();
      expect(ALL_TOOLS[4]?.fn).toHaveBeenCalled();
    });
  });
});
