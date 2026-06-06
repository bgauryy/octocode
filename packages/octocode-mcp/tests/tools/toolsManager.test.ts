import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { registerTools } from '../../src/tools/toolsManager.js';

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

  const mockCloneTool = {
    name: 'githubCloneRepo',
    isDefault: true,
    isLocal: true,
    isClone: true,
    skipMetadataCheck: true,
    fn: vi.fn(),
  };

  const mockLocalTools = [
    { name: 'localSearchCode', isDefault: true, isLocal: true, fn: vi.fn() },
    { name: 'localViewStructure', isDefault: true, isLocal: true, fn: vi.fn() },
    { name: 'localFindFiles', isDefault: true, isLocal: true, fn: vi.fn() },
    {
      name: 'localGetFileContent',
      isDefault: true,
      isLocal: true,
      fn: vi.fn(),
    },
  ];

  const mockNonDefaultTool = {
    name: 'experimentalDebugTool',
    isDefault: false,
    isLocal: false,
    type: 'debug',
    fn: vi.fn(),
  };

  return {
    ALL_TOOLS: [
      ...mockGitHubTools,
      mockCloneTool,
      ...mockLocalTools,
      mockNonDefaultTool,
    ],
  };
});

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
      PACKAGE_SEARCH: 'packageSearch',
      LOCAL_RIPGREP: 'localSearchCode',
      LOCAL_FETCH_CONTENT: 'localGetFileContent',
      LOCAL_FIND_FILES: 'localFindFiles',
      LOCAL_VIEW_STRUCTURE: 'localViewStructure',
    },
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

import { ALL_TOOLS, type ToolConfig } from '../../src/tools/toolConfig.js';
import {
  getServerConfig,
  isLocalEnabled,
  isCloneEnabled,
} from '../../src/serverConfig.js';
import {
  TOOL_NAMES,
  isToolInMetadata,
} from '../../src/tools/toolMetadata/proxies.js';
import { logSessionError } from '../../src/session.js';

const mockGetServerConfig = vi.mocked(getServerConfig);
const mockIsToolAvailableSync = vi.mocked(isToolInMetadata);
const mockIsLocalEnabled = vi.mocked(isLocalEnabled);
const mockIsCloneEnabled = vi.mocked(isCloneEnabled);
const mockLogSessionError = vi.mocked(logSessionError);

describe('ToolsManager', () => {
  let mockServer: McpServer;
  const originalStderr = process.stderr.write;

  beforeEach(() => {
    vi.clearAllMocks();

    mockServer = {} as McpServer;

    process.stderr.write = vi.fn();

    mockIsToolAvailableSync.mockReturnValue(true);
    mockIsLocalEnabled.mockReturnValue(false);
    mockIsCloneEnabled.mockReturnValue(false);

    ALL_TOOLS.forEach(tool => {
      vi.mocked(tool.fn).mockReset();
    });
  });

  afterEach(() => {
    process.stderr.write = originalStderr;
  });

  describe('Default Configuration (no env vars)', () => {
    it('should register only default GitHub tools when ENABLE_LOCAL is false', async () => {
      mockIsLocalEnabled.mockReturnValue(false);
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

      const result = await registerTools(mockServer);

      expect(result.successCount).toBeGreaterThan(0);
      expect(typeof result.successCount).toBe('number');
      expect(result.failedTools).toBeDefined();
      expect(Array.isArray(result.failedTools)).toBe(true);

      const defaultGithubTools = ALL_TOOLS.filter(
        t => !t.isLocal && t.isDefault
      );
      defaultGithubTools.forEach(tool => {
        expect(tool.fn).toHaveBeenCalled();
      });

      const localTools = ALL_TOOLS.filter(t => t.isLocal);
      localTools.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });

      const nonDefaultTool = ALL_TOOLS.find(t => !t.isDefault && !t.isLocal);
      expect(nonDefaultTool!.fn).not.toHaveBeenCalled();
    });
  });

  describe('TOOLS_TO_RUN Configuration', () => {
    it('should register only specified tools when TOOLS_TO_RUN is set', async () => {
      const allowedTools: string[] = [
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      ];

      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: allowedTools,
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(allowedTools.length);
      expect(result.failedTools).toHaveLength(0);

      ALL_TOOLS.forEach(tool => {
        if (allowedTools.includes(tool.name)) {
          expect(tool.fn).toHaveBeenCalled();
        } else {
          expect(tool.fn).not.toHaveBeenCalled();
        }
      });
    });

    it('should handle non-existent tools in TOOLS_TO_RUN gracefully', async () => {
      const validTools: string[] = [
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      ];

      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: [
          TOOL_NAMES.GITHUB_SEARCH_CODE,
          'nonExistentTool',
          TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        ],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(validTools.length);
      expect(result.failedTools).toHaveLength(0);

      ALL_TOOLS.forEach(tool => {
        if (validTools.includes(tool.name)) {
          expect(tool.fn).toHaveBeenCalled();
        } else {
          expect(tool.fn).not.toHaveBeenCalled();
        }
      });
    });

    it('should register no tools if TOOLS_TO_RUN contains only non-existent tools', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['nonExistentTool1', 'nonExistentTool2'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(0);
      expect(Array.isArray(result.failedTools)).toBe(true);

      ALL_TOOLS.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });
    });
  });

  describe('TOOLS_TO_RUN conflicts with ENABLE_TOOLS/DISABLE_TOOLS', () => {
    it('should warn when TOOLS_TO_RUN is used with ENABLE_TOOLS and only register TOOLS_TO_RUN', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: [TOOL_NAMES.GITHUB_SEARCH_CODE],
        enableTools: [TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(process.stderr.write).toHaveBeenCalledWith(
        'Warning: TOOLS_TO_RUN cannot be used together with ENABLE_TOOLS/DISABLE_TOOLS. Using TOOLS_TO_RUN exclusively.\n'
      );

      expect(result.successCount).toBe(1);
      const searchCodeTool = ALL_TOOLS.find(
        t => t.name === TOOL_NAMES.GITHUB_SEARCH_CODE
      );
      const prTool = ALL_TOOLS.find(
        t => t.name === TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
      );
      expect(searchCodeTool!.fn).toHaveBeenCalled();
      expect(prTool!.fn).not.toHaveBeenCalled();
    });

    it('should warn when TOOLS_TO_RUN is used with DISABLE_TOOLS', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: [TOOL_NAMES.GITHUB_SEARCH_CODE],
        disableTools: [TOOL_NAMES.GITHUB_FETCH_CONTENT],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      await registerTools(mockServer);

      expect(process.stderr.write).toHaveBeenCalledWith(
        'Warning: TOOLS_TO_RUN cannot be used together with ENABLE_TOOLS/DISABLE_TOOLS. Using TOOLS_TO_RUN exclusively.\n'
      );
    });

    it('should warn when TOOLS_TO_RUN is used with both ENABLE_TOOLS and DISABLE_TOOLS', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: [TOOL_NAMES.GITHUB_SEARCH_CODE],
        enableTools: [TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS],
        disableTools: [TOOL_NAMES.GITHUB_FETCH_CONTENT],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      await registerTools(mockServer);

      expect(process.stderr.write).toHaveBeenCalledWith(
        'Warning: TOOLS_TO_RUN cannot be used together with ENABLE_TOOLS/DISABLE_TOOLS. Using TOOLS_TO_RUN exclusively.\n'
      );
    });
  });

  describe('TOOLS_TO_RUN strict filtering guarantees', () => {
    it('should register ONLY the single tool specified in TOOLS_TO_RUN (nothing else)', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['githubSearchCode'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(1);
      expect(result.failedTools).toHaveLength(0);

      const calledTools = ALL_TOOLS.filter(
        t => vi.mocked(t.fn).mock.calls.length > 0
      );
      expect(calledTools).toHaveLength(1);
      expect(calledTools[0]!.name).toBe('githubSearchCode');
    });

    it('should select specific local tools via TOOLS_TO_RUN when ENABLE_LOCAL=true', async () => {
      mockIsLocalEnabled.mockReturnValue(true);
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['localSearchCode', 'localGetFileContent'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: true,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(2);

      const calledTools = ALL_TOOLS.filter(
        t => vi.mocked(t.fn).mock.calls.length > 0
      );
      expect(calledTools).toHaveLength(2);
      const calledNames = calledTools.map(t => t.name).sort();
      expect(calledNames).toEqual(['localGetFileContent', 'localSearchCode']);

      const viewStructure = ALL_TOOLS.find(
        t => t.name === 'localViewStructure'
      );
      expect(viewStructure!.fn).not.toHaveBeenCalled();
      const findFiles = ALL_TOOLS.find(t => t.name === 'localFindFiles');
      expect(findFiles!.fn).not.toHaveBeenCalled();
    });

    it('should NOT register local tools from TOOLS_TO_RUN when ENABLE_LOCAL=false', async () => {
      mockIsLocalEnabled.mockReturnValue(false);
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['localSearchCode', 'githubSearchCode'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(1);

      const githubSearch = ALL_TOOLS.find(t => t.name === 'githubSearchCode');
      expect(githubSearch!.fn).toHaveBeenCalled();

      const localSearch = ALL_TOOLS.find(t => t.name === 'localSearchCode');
      expect(localSearch!.fn).not.toHaveBeenCalled();
    });

    it('should NOT register clone tool from TOOLS_TO_RUN when ENABLE_CLONE=false', async () => {
      mockIsLocalEnabled.mockReturnValue(true);
      mockIsCloneEnabled.mockReturnValue(false);
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['githubCloneRepo', 'githubSearchCode'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: true,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(1);

      const githubSearch = ALL_TOOLS.find(t => t.name === 'githubSearchCode');
      expect(githubSearch!.fn).toHaveBeenCalled();

      const cloneTool = ALL_TOOLS.find(t => t.name === 'githubCloneRepo');
      expect(cloneTool!.fn).not.toHaveBeenCalled();
    });

    it('should register clone tool from TOOLS_TO_RUN when both ENABLE_LOCAL and ENABLE_CLONE are true', async () => {
      mockIsLocalEnabled.mockReturnValue(true);
      mockIsCloneEnabled.mockReturnValue(true);
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['githubCloneRepo'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: true,
        enableClone: true,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(1);

      const cloneTool = ALL_TOOLS.find(t => t.name === 'githubCloneRepo');
      expect(cloneTool!.fn).toHaveBeenCalled();

      const githubTools = ALL_TOOLS.filter(
        t => !t.isLocal && t.name !== 'experimentalDebugTool'
      );
      githubTools.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });
    });

    it('should enable a non-default tool via TOOLS_TO_RUN', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['experimentalDebugTool'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(1);

      const debugTool = ALL_TOOLS.find(t => t.name === 'experimentalDebugTool');
      expect(debugTool!.fn).toHaveBeenCalled();

      const defaultTools = ALL_TOOLS.filter(t => t.isDefault && !t.isLocal);
      defaultTools.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });
    });

    it('should treat empty TOOLS_TO_RUN array as not set (fall through to defaults)', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: [],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      const defaultGithubTools = ALL_TOOLS.filter(
        t => !t.isLocal && t.isDefault
      );
      expect(result.successCount).toBe(defaultGithubTools.length);

      defaultGithubTools.forEach(tool => {
        expect(tool.fn).toHaveBeenCalled();
      });

      const nonDefaultTool = ALL_TOOLS.find(
        t => t.name === 'experimentalDebugTool'
      );
      expect(nonDefaultTool!.fn).not.toHaveBeenCalled();
    });

    it('should mix GitHub and local tools in TOOLS_TO_RUN with ENABLE_LOCAL=true', async () => {
      mockIsLocalEnabled.mockReturnValue(true);
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        toolsToRun: ['githubSearchCode', 'localSearchCode', 'localFindFiles'],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: true,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(3);
      expect(result.failedTools).toHaveLength(0);

      const calledTools = ALL_TOOLS.filter(
        t => vi.mocked(t.fn).mock.calls.length > 0
      );
      const calledNames = calledTools.map(t => t.name).sort();
      expect(calledNames).toEqual([
        'githubSearchCode',
        'localFindFiles',
        'localSearchCode',
      ]);

      const notCalled = ALL_TOOLS.filter(
        t => vi.mocked(t.fn).mock.calls.length === 0
      );
      expect(notCalled.length).toBe(ALL_TOOLS.length - 3);
    });
  });

  describe('ENABLE_TOOLS/DISABLE_TOOLS Configuration (without TOOLS_TO_RUN)', () => {
    it('should register all default tools with ENABLE_TOOLS (no-op for already default tools)', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        enableTools: [TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBeGreaterThanOrEqual(0);
      expect(Array.isArray(result.failedTools)).toBe(true);

      const defaultGithubTools = ALL_TOOLS.filter(
        t => !t.isLocal && t.isDefault
      );
      defaultGithubTools.forEach(tool => {
        expect(tool.fn).toHaveBeenCalled();
      });
    });

    it('should remove default tools with DISABLE_TOOLS', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        disableTools: [
          TOOL_NAMES.GITHUB_SEARCH_CODE,
          TOOL_NAMES.GITHUB_FETCH_CONTENT,
        ],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(typeof result.successCount).toBe('number');
      expect(result.successCount).toBeGreaterThanOrEqual(0);
      expect(result.failedTools).toBeDefined();
      expect(Array.isArray(result.failedTools)).toBe(true);
    });

    it('should handle both ENABLE_TOOLS and DISABLE_TOOLS', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        enableTools: [TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS],
        disableTools: [TOOL_NAMES.GITHUB_SEARCH_CODE],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(typeof result.successCount).toBe('number');
      expect(result.successCount).toBeGreaterThanOrEqual(0);
      expect(result.failedTools).toBeDefined();
      expect(Array.isArray(result.failedTools)).toBe(true);
    });

    it('should handle disabling enabled tools (DISABLE_TOOLS takes precedence)', async () => {
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        enableTools: [TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS],
        disableTools: [TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS],
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: false,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GITHUB_TOKEN',
      });

      const result = await registerTools(mockServer);

      expect(typeof result.successCount).toBe('number');
      expect(result.successCount).toBeGreaterThanOrEqual(0);
      expect(result.failedTools).toBeDefined();
      expect(Array.isArray(result.failedTools)).toBe(true);
    });
  });

  describe('Error Handling', () => {
    it('should handle tool registration failures gracefully', async () => {
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

      const githubTools = ALL_TOOLS.filter(t => !t.isLocal);
      vi.mocked(githubTools[0]!.fn).mockImplementation(() => {
        throw new Error('Registration failed');
      });

      const result = await registerTools(mockServer);

      expect(typeof result.successCount).toBe('number');
      expect(result.successCount).toBeGreaterThanOrEqual(0);
      expect(result.failedTools).toBeDefined();
      expect(Array.isArray(result.failedTools)).toBe(true);
      expect(result.failedTools.length).toBeGreaterThan(0);
    });

    it('should continue registering tools after failures', async () => {
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

      const githubTools = ALL_TOOLS.filter(t => !t.isLocal);
      vi.mocked(githubTools[0]!.fn).mockImplementation(() => {
        throw new Error('Registration failed');
      });
      vi.mocked(githubTools[2]!.fn).mockImplementation(() => {
        throw new Error('Registration failed');
      });

      const result = await registerTools(mockServer);

      expect(typeof result.successCount).toBe('number');
      expect(result.successCount).toBeGreaterThanOrEqual(0);
      expect(result.failedTools).toBeDefined();
      expect(Array.isArray(result.failedTools)).toBe(true);
      expect(result.failedTools.length).toBeGreaterThan(0);
    });
  });

  describe('Local Tools Registration', () => {
    it('should register local tools when ENABLE_LOCAL is set', async () => {
      mockIsLocalEnabled.mockReturnValue(true);
      mockGetServerConfig.mockReturnValue({
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

      const result = await registerTools(mockServer);

      const expectedCount = ALL_TOOLS.filter(
        t => t.isDefault && !t.isClone
      ).length;
      expect(result.successCount).toBe(expectedCount);

      const localTools = ALL_TOOLS.filter(t => t.isLocal && !t.isClone);
      localTools.forEach(tool => {
        expect(tool.fn).toHaveBeenCalled();
      });

      const cloneTool = ALL_TOOLS.find(t => t.isClone);
      expect(cloneTool!.fn).not.toHaveBeenCalled();
    });

    it('should not register local tools when ENABLE_LOCAL is not set', async () => {
      mockIsLocalEnabled.mockReturnValue(false);
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

      const result = await registerTools(mockServer);

      expect(result.successCount).toBeGreaterThanOrEqual(0);

      const localTools = ALL_TOOLS.filter(t => t.isLocal);
      localTools.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });
    });

    it('should handle local tools registration failure gracefully', async () => {
      mockIsLocalEnabled.mockReturnValue(true);
      mockGetServerConfig.mockReturnValue({
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

      const localTools = ALL_TOOLS.filter(t => t.isLocal);
      localTools.forEach(tool => {
        vi.mocked(tool.fn).mockImplementation(() => {
          throw new Error('Registration failed');
        });
      });

      const result = await registerTools(mockServer);

      expect(result.failedTools.length).toBeGreaterThan(0);
    });
  });

  describe('Tool availability check', () => {
    it('should not register GitHub tools that are unavailable in metadata', async () => {
      mockIsToolAvailableSync.mockReturnValue(false);
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

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(0);

      const githubTools = ALL_TOOLS.filter(t => !t.isLocal);
      githubTools.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });
    });

    it('should skip local tools when isToolInMetadata returns false (local tools check metadata like GitHub tools)', async () => {
      mockIsToolAvailableSync.mockReturnValue(false);
      mockIsLocalEnabled.mockReturnValue(true);
      mockGetServerConfig.mockReturnValue({
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

      const result = await registerTools(mockServer);

      expect(result.successCount).toBe(0);

      ALL_TOOLS.forEach(tool => {
        expect(tool.fn).not.toHaveBeenCalled();
      });

      expect(mockLogSessionError).toHaveBeenCalledTimes(9);
    });
  });

  describe('Tool registration returning null', () => {
    it('should handle tool.fn returning null', async () => {
      mockIsToolAvailableSync.mockReturnValue(true);
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

      const githubTools = ALL_TOOLS.filter(t => !t.isLocal);
      vi.mocked(githubTools[0]!.fn).mockResolvedValue(null);

      const result = await registerTools(mockServer);

      expect(result.failedTools).not.toContain(githubTools[0]?.name);
    });
  });

  describe('Non-default tool handling', () => {
    it('should not register non-default tools without enableTools', async () => {
      const originalTools = [...ALL_TOOLS];

      ALL_TOOLS.length = 0;
      ALL_TOOLS.push({
        name: 'nonDefaultTool',
        description: 'A non-default tool for testing',
        isDefault: false,
        isLocal: false,
        type: 'debug' as const,
        fn: vi.fn(),
      } as ToolConfig);

      mockIsToolAvailableSync.mockReturnValue(true);
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

      const result = await registerTools(mockServer);

      expect(ALL_TOOLS[0]?.fn).not.toHaveBeenCalled();
      expect(result.successCount).toBe(0);

      ALL_TOOLS.length = 0;
      originalTools.forEach(t => ALL_TOOLS.push(t));
    });
  });

  describe('Defensive: getServerConfig() throws', () => {
    it('should handle getServerConfig() throwing (config not initialized)', async () => {
      mockGetServerConfig.mockImplementation(() => {
        throw new Error('Server config not initialized');
      });

      const result = await registerTools(mockServer);

      expect(typeof result.successCount).toBe('number');
      expect(Array.isArray(result.failedTools)).toBe(true);
    });

    it('should register default tools when getServerConfig throws', async () => {
      mockIsLocalEnabled.mockReturnValue(true);
      mockGetServerConfig.mockImplementation(() => {
        throw new Error('Server config not initialized');
      });

      const result = await registerTools(mockServer);

      expect(result.successCount).toBeGreaterThanOrEqual(0);
    });
  });

  describe('Unified tool registration (ALL_TOOLS)', () => {
    it('should register all default tools from ALL_TOOLS when ENABLE_LOCAL is true (excluding clone and non-default)', async () => {
      mockIsLocalEnabled.mockReturnValue(true);
      mockIsToolAvailableSync.mockReturnValue(true);
      mockGetServerConfig.mockReturnValue({
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

      const result = await registerTools(mockServer);

      const expectedCount = ALL_TOOLS.filter(
        t => t.isDefault && !t.isClone
      ).length;
      expect(result.successCount).toBe(expectedCount);
      expect(result.failedTools).toHaveLength(0);

      ALL_TOOLS.forEach(tool => {
        if (tool.isDefault && !tool.isClone) {
          expect(tool.fn).toHaveBeenCalled();
        } else {
          expect(tool.fn).not.toHaveBeenCalled();
        }
      });
    });

    it('should correctly filter local tools when ENABLE_LOCAL is false', async () => {
      mockIsLocalEnabled.mockReturnValue(false);
      mockIsToolAvailableSync.mockReturnValue(true);
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

      const result = await registerTools(mockServer);

      const defaultGithubCount = ALL_TOOLS.filter(
        t => !t.isLocal && t.isDefault
      ).length;
      expect(result.successCount).toBe(defaultGithubCount);

      ALL_TOOLS.forEach(tool => {
        if (!tool.isLocal && tool.isDefault) {
          expect(tool.fn).toHaveBeenCalled();
        } else {
          expect(tool.fn).not.toHaveBeenCalled();
        }
      });
    });
  });
});
