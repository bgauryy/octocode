import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import {
  initialize,
  cleanup,
  getGitHubToken,
  getServerConfig,
  isLoggingEnabled,
  isLocalEnabled,
  isCloneEnabled,
  getTokenSource,
  _setTokenResolvers,
  _resetTokenResolvers,
} from '../src/serverConfig.js';
import type { FullTokenResolution } from 'octocode-shared';

// Type for resolveTokenFull mock
type ResolveTokenFullMock = Mock<
  (options?: {
    hostname?: string;
    clientId?: string;
    getGhCliToken?: (
      hostname?: string
    ) => string | null | Promise<string | null>;
  }) => Promise<FullTokenResolution | null>
>;

let mockResolveTokenFull: ResolveTokenFullMock;

// Helper to create token resolution result
function mockTokenResult(
  token: string | null,
  source:
    | 'env:OCTOCODE_TOKEN'
    | 'env:GH_TOKEN'
    | 'env:GITHUB_TOKEN'
    | 'octocode-storage'
    | 'octocode-storage'
    | 'gh-cli'
    | null
): FullTokenResolution | null {
  if (!token) return null;
  return {
    token,
    source,
    wasRefreshed: false,
  };
}

// Helper to setup resolveTokenFull mock
function setupTokenMocks() {
  mockResolveTokenFull = vi.fn(async () => null);

  _setTokenResolvers({
    resolveTokenFull: mockResolveTokenFull,
  });
}

// Helper function to mock CLI token success (gh-cli source)
function mockSpawnSuccess(token: string) {
  mockResolveTokenFull.mockResolvedValue(mockTokenResult(token, 'gh-cli'));
}

// Helper function to mock CLI token failure (no token found)
function mockSpawnFailure() {
  mockResolveTokenFull.mockResolvedValue(null);
}

// Helper function to mock token resolution via env/stored credentials
function mockTokenResolution(
  token: string | null,
  source:
    | 'env:GITHUB_TOKEN'
    | 'env:GH_TOKEN'
    | 'env:OCTOCODE_TOKEN'
    | 'octocode-storage'
    | 'octocode-storage'
    | 'gh-cli'
    | null = 'env:GITHUB_TOKEN'
) {
  if (token) {
    mockResolveTokenFull.mockResolvedValue(mockTokenResult(token, source));
  } else {
    mockResolveTokenFull.mockResolvedValue(null);
  }
}

describe('ServerConfig - Simplified Version', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    vi.clearAllMocks();
    cleanup();

    // Reset environment variables
    process.env = { ...originalEnv };
    delete process.env.GITHUB_TOKEN;
    delete process.env.GH_TOKEN;
    delete process.env.Authorization;
    delete process.env.TOOLS_TO_RUN;
    delete process.env.ENABLE_TOOLS;
    delete process.env.DISABLE_TOOLS;
    delete process.env.LOG;
    delete process.env.TEST_GITHUB_TOKEN;
    delete process.env.ENABLE_LOCAL;
    delete process.env.GITHUB_API_URL;
    delete process.env.REQUEST_TIMEOUT;
    delete process.env.MAX_RETRIES;
    delete process.env.OCTOCODE_TOKEN;

    // Set up injectable mock for token resolution
    setupTokenMocks();
  });

  afterEach(() => {
    process.env = originalEnv;
    cleanup();
    _resetTokenResolvers();
  });

  describe('Configuration Initialization', () => {
    it('should initialize with default config', async () => {
      await initialize();
      const config = getServerConfig();

      expect(typeof config.version).toEqual('string');
      expect(config.timeout).toEqual(30000);
      expect(config.maxRetries).toEqual(3);
      expect(config.loggingEnabled).toEqual(true);
    });

    it('should have correct defaults for all fields when no env vars set', async () => {
      mockSpawnFailure();
      await initialize();
      const config = getServerConfig();

      expect(config.githubApiUrl).toBe('https://api.github.com');
      expect(config.toolsToRun).toBeUndefined();
      expect(config.enableTools).toBeUndefined();
      expect(config.disableTools).toBeUndefined();
      expect(config.timeout).toBe(30000);
      expect(config.maxRetries).toBe(3);
      expect(config.loggingEnabled).toBe(true);
      expect(config.enableLocal).toBe(true);
      expect(config.enableClone).toBe(false);
      expect(config.tokenSource).toBe('none');
    });

    it('should initialize with environment variables', async () => {
      process.env.REQUEST_TIMEOUT = '60000';
      process.env.MAX_RETRIES = '5';

      await initialize();
      const config = getServerConfig();

      expect(config.loggingEnabled).toBe(true);
      expect(config.timeout).toBe(60000);
      expect(config.maxRetries).toBe(5);
    });

    it('should disable logging when LOG is false', async () => {
      process.env.LOG = 'false';

      await initialize();
      const config = getServerConfig();

      expect(config.loggingEnabled).toBe(false);
    });

    it('should throw when accessing config before initialization', () => {
      expect(() => getServerConfig()).toThrow(
        'Configuration not initialized. Call initialize() and await its completion before calling getServerConfig().'
      );
    });

    it('should not re-initialize when already initialized', async () => {
      await initialize();
      const config1 = getServerConfig();

      await initialize(); // Second call
      const config2 = getServerConfig();

      expect(config1).toBe(config2); // Same reference
    });

    it('should use default GitHub API URL', async () => {
      await initialize();
      const config = getServerConfig();

      expect(config.githubApiUrl).toBe('https://api.github.com');
    });

    it('should use custom GitHub API URL from environment', async () => {
      process.env.GITHUB_API_URL = 'https://github.company.com/api/v3';

      await initialize();
      const config = getServerConfig();

      expect(config.githubApiUrl).toBe('https://github.company.com/api/v3');
    });
  });

  describe('Token Resolution', () => {
    it('should prioritize GITHUB_TOKEN env var over CLI token', async () => {
      process.env.GITHUB_TOKEN = 'env-github-token';
      cleanup();

      // Mock resolveTokenFull to return env token (simulating env var priority)
      mockTokenResolution('env-github-token', 'env:GITHUB_TOKEN');
      const token = await getGitHubToken();

      // GITHUB_TOKEN takes priority even when CLI token is available
      expect(token).toBe('env-github-token');
    });

    it('should fall back to CLI token when GITHUB_TOKEN is not set', async () => {
      delete process.env.GITHUB_TOKEN;
      cleanup();

      mockSpawnSuccess('cli-token');
      const token = await getGitHubToken();

      expect(token).toBe('cli-token');
    });

    it('should return null when no token found', async () => {
      delete process.env.GITHUB_TOKEN;

      mockSpawnFailure();
      const token = await getGitHubToken();
      expect(token).toBeNull();
    });

    it('should resolve token fresh each time (no caching)', async () => {
      delete process.env.GITHUB_TOKEN;

      // First call returns 'token-1'
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('token-1', 'gh-cli')
      );
      const token1 = await getGitHubToken();
      expect(token1).toBe('token-1');

      // Second call with different mock returns 'token-2' (no caching)
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('token-2', 'gh-cli')
      );
      const token2 = await getGitHubToken();
      expect(token2).toBe('token-2');
    });

    it('should pick up token changes dynamically', async () => {
      delete process.env.GITHUB_TOKEN;

      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('initial-token', 'gh-cli')
      );
      const token1 = await getGitHubToken();
      expect(token1).toBe('initial-token');

      // Token changes at runtime
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('new-token', 'gh-cli')
      );
      const token2 = await getGitHubToken();
      expect(token2).toBe('new-token');
    });
  });

  describe('octocode-cli Credential Fallback', () => {
    it('should use octocode-cli token when GITHUB_TOKEN and gh CLI are unavailable', async () => {
      delete process.env.GITHUB_TOKEN;

      // resolveTokenFull returns stored token
      mockTokenResolution('octocode-stored-token', 'octocode-storage');

      const token = await getGitHubToken();

      expect(token).toBe('octocode-stored-token');
    });

    it('should prioritize GITHUB_TOKEN over octocode-cli token', async () => {
      process.env.GITHUB_TOKEN = 'env-token';

      // Mock resolveTokenFull to return env token (simulating priority)
      mockTokenResolution('env-token', 'env:GITHUB_TOKEN');

      const token = await getGitHubToken();

      expect(token).toBe('env-token');
    });

    it('should prioritize gh CLI token over octocode-cli token', async () => {
      delete process.env.GITHUB_TOKEN;

      // resolveTokenFull handles priority internally, returns gh-cli token
      mockSpawnSuccess('gh-cli-token');

      const token = await getGitHubToken();

      expect(token).toBe('gh-cli-token');
    });

    it('should return null when all token sources fail', async () => {
      delete process.env.GITHUB_TOKEN;

      mockSpawnFailure();

      const token = await getGitHubToken();

      expect(token).toBeNull();
    });

    it('should handle octocode-cli token with whitespace', async () => {
      delete process.env.GITHUB_TOKEN;

      // Token is returned trimmed by resolveTokenFull
      mockTokenResolution('octocode-token-with-spaces', 'octocode-storage');

      const token = await getGitHubToken();

      expect(token).toBe('octocode-token-with-spaces');
    });

    it('should skip empty octocode-cli token', async () => {
      delete process.env.GITHUB_TOKEN;

      // Empty/whitespace tokens resolve to null
      mockTokenResolution(null);

      const token = await getGitHubToken();

      expect(token).toBeNull();
    });

    it('should handle octocode-cli errors gracefully', async () => {
      mockResolveTokenFull.mockRejectedValue(new Error('Read error'));

      const token = await getGitHubToken();

      expect(token).toBeNull();
    });
  });

  describe('getGitHubToken (direct resolution)', () => {
    it('should return token when available', async () => {
      mockTokenResolution('available-token', 'env:GITHUB_TOKEN');

      const token = await getGitHubToken();

      expect(token).toBe('available-token');
    });

    it('should return null when no token available', async () => {
      mockSpawnFailure();

      const token = await getGitHubToken();

      expect(token).toBeNull();
    });
  });

  describe('Logging Configuration', () => {
    it('should enable logging by default when LOG is not set', async () => {
      delete process.env.LOG;
      mockSpawnFailure();

      await initialize();

      expect(isLoggingEnabled()).toBe(true);
      expect(getServerConfig().loggingEnabled).toBe(true);
    });

    it('should enable logging when LOG is set to true', async () => {
      process.env.LOG = 'true';
      mockSpawnFailure();

      await initialize();

      expect(isLoggingEnabled()).toBe(true);
      expect(getServerConfig().loggingEnabled).toBe(true);
    });

    it('should disable logging when LOG is set to false', async () => {
      process.env.LOG = 'false';
      mockSpawnFailure();

      await initialize();

      expect(isLoggingEnabled()).toBe(false);
      expect(getServerConfig().loggingEnabled).toBe(false);
    });

    it('should handle various LOG flag formats', async () => {
      const testCases = [
        {
          value: undefined,
          expected: true,
          description: 'undefined (default)',
        },
        { value: 'true', expected: true, description: 'true' },
        { value: 'TRUE', expected: true, description: 'TRUE' },
        { value: 'false', expected: false, description: 'false' },
        { value: 'FALSE', expected: false, description: 'FALSE' },
        { value: 'False', expected: false, description: 'False' },
        { value: '1', expected: true, description: '1 (truthy)' },
        { value: '0', expected: false, description: '0 (falsy)' },
        { value: '', expected: true, description: 'empty string' },
        { value: 'anything', expected: true, description: 'any other value' },
      ];

      for (const testCase of testCases) {
        cleanup();
        if (testCase.value === undefined) {
          delete process.env.LOG;
        } else {
          process.env.LOG = testCase.value;
        }
        mockSpawnFailure();

        await initialize();

        expect(isLoggingEnabled()).toBe(testCase.expected);
        expect(getServerConfig().loggingEnabled).toBe(testCase.expected);
      }
    });
  });

  describe('Error Handling', () => {
    it('should handle GitHub CLI errors gracefully', async () => {
      process.env.GITHUB_TOKEN = 'fallback-token';
      // Mock resolveTokenFull to return env token
      mockTokenResolution('fallback-token', 'env:GITHUB_TOKEN');

      const token = await getGitHubToken();

      expect(token).toBe('fallback-token');
    });

    it('should handle empty string tokens correctly', async () => {
      delete process.env.GITHUB_TOKEN;

      // resolveTokenFull returns null for whitespace-only
      mockSpawnFailure();
      const token = await getGitHubToken();
      expect(token).toBeNull();
    });

    it('should handle whitespace-only tokens', async () => {
      delete process.env.GITHUB_TOKEN;

      // resolveTokenFull returns null for whitespace-only
      mockSpawnFailure();
      const token = await getGitHubToken();
      expect(token).toBeNull(); // Trimmed to empty
    });
  });

  describe('Cleanup and State Management', () => {
    it('should reset state properly', async () => {
      process.env.GITHUB_TOKEN = 'test-token';
      mockSpawnFailure();

      await initialize();
      const config = getServerConfig();
      expect(typeof config).toEqual('object');

      cleanup();
      let didThrow = false;
      try {
        getServerConfig();
      } catch {
        didThrow = true;
      }
      expect(didThrow).toEqual(true);
    });

    it('should handle multiple cleanup calls', () => {
      expect(() => {
        cleanup();
        cleanup();
        cleanup();
      }).not.toThrow();
    });
  });

  describe('Environment Variable Parsing', () => {
    it('should parse tool arrays correctly', async () => {
      process.env.ENABLE_TOOLS = 'tool1,tool2,tool3';
      process.env.DISABLE_TOOLS = 'tool4, tool5 , tool6';
      process.env.TOOLS_TO_RUN = 'onlyTool1, onlyTool2';
      mockSpawnFailure();

      await initialize();
      const config = getServerConfig();

      expect(config.enableTools).toEqual(['tool1', 'tool2', 'tool3']);
      expect(config.disableTools).toEqual(['tool4', 'tool5', 'tool6']);
      expect(config.toolsToRun).toEqual(['onlyTool1', 'onlyTool2']);
    });

    it('should handle empty tool arrays', async () => {
      process.env.ENABLE_TOOLS = '';
      process.env.DISABLE_TOOLS = '   ';
      process.env.TOOLS_TO_RUN = '';
      mockSpawnFailure();

      await initialize();
      const config = getServerConfig();

      expect(config.enableTools).toEqual(undefined);
      expect(config.disableTools).toEqual(undefined);
      expect(config.toolsToRun).toEqual(undefined);
    });

    it('should parse toolsToRun correctly', async () => {
      process.env.TOOLS_TO_RUN =
        'github_search_code,github_search_pull_requests , github_fetch_content';
      mockSpawnFailure();

      await initialize();
      const config = getServerConfig();

      expect(config.toolsToRun).toEqual([
        'github_search_code',
        'github_search_pull_requests',
        'github_fetch_content',
      ]);
    });

    it('should handle toolsToRun with single tool', async () => {
      process.env.TOOLS_TO_RUN = 'github_search_code';
      mockSpawnFailure();

      await initialize();
      const config = getServerConfig();

      expect(config.toolsToRun).toEqual(['github_search_code']);
    });

    it('should filter out empty strings from toolsToRun', async () => {
      process.env.TOOLS_TO_RUN = 'tool1,,tool2, ,tool3';
      mockSpawnFailure();

      await initialize();
      const config = getServerConfig();

      expect(config.toolsToRun).toEqual(['tool1', 'tool2', 'tool3']);
    });

    it('should handle malformed numbers gracefully', async () => {
      process.env.REQUEST_TIMEOUT = 'not-a-number';
      process.env.MAX_RETRIES = '-5';
      mockSpawnFailure();

      await initialize();
      const config = getServerConfig();

      expect(config.timeout).toBe(30000); // Falls back to default 30000
      expect(config.maxRetries).toBe(0); // Math.max(0, -5) = 0
    });
  });

  describe('ENABLE_LOCAL Configuration', () => {
    beforeEach(() => {
      delete process.env.ENABLE_LOCAL;
    });

    it('should default to true when ENABLE_LOCAL is not set', async () => {
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableLocal).toBe(true);
    });

    it('should enable local when ENABLE_LOCAL is "true"', async () => {
      process.env.ENABLE_LOCAL = 'true';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableLocal).toBe(true);
    });

    it('should enable local when ENABLE_LOCAL is "1"', async () => {
      process.env.ENABLE_LOCAL = '1';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableLocal).toBe(true);
    });

    it('should handle ENABLE_LOCAL with leading/trailing whitespace', async () => {
      process.env.ENABLE_LOCAL = '  true  ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableLocal).toBe(true);
    });

    it('should handle ENABLE_LOCAL with tabs and newlines', async () => {
      process.env.ENABLE_LOCAL = '\t true \n';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableLocal).toBe(true);
    });

    it('should handle ENABLE_LOCAL with uppercase', async () => {
      process.env.ENABLE_LOCAL = 'TRUE';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableLocal).toBe(true);
    });

    it('should handle ENABLE_LOCAL with mixed case', async () => {
      process.env.ENABLE_LOCAL = 'TrUe';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableLocal).toBe(true);
    });

    it('should handle ENABLE_LOCAL with whitespace and uppercase', async () => {
      process.env.ENABLE_LOCAL = '  TRUE  ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableLocal).toBe(true);
    });

    it('should handle ENABLE_LOCAL = "1" with whitespace', async () => {
      process.env.ENABLE_LOCAL = ' 1 ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableLocal).toBe(true);
    });

    it('should return false for explicit false ENABLE_LOCAL values', async () => {
      const explicitFalseValues = ['false', 'FALSE', '0'];

      for (const value of explicitFalseValues) {
        cleanup();
        delete process.env.ENABLE_LOCAL;
        process.env.ENABLE_LOCAL = value;
        mockSpawnFailure();
        await initialize();
        expect(getServerConfig().enableLocal).toBe(false);
      }
    });

    it('should return true (default) for invalid/unrecognized ENABLE_LOCAL values', async () => {
      const invalidValues = ['no', 'yes', 'enabled', '', '   '];

      for (const value of invalidValues) {
        cleanup();
        delete process.env.ENABLE_LOCAL;
        process.env.ENABLE_LOCAL = value;
        mockSpawnFailure();
        await initialize();
        expect(getServerConfig().enableLocal).toBe(true);
      }
    });
  });

  describe('isLocalEnabled() helper', () => {
    it('should return true when enableLocal is true (default)', async () => {
      delete process.env.ENABLE_LOCAL;
      mockSpawnFailure();
      await initialize();
      expect(isLocalEnabled()).toBe(true);
    });

    it('should return false when ENABLE_LOCAL is "false"', async () => {
      process.env.ENABLE_LOCAL = 'false';
      mockSpawnFailure();
      await initialize();
      expect(isLocalEnabled()).toBe(false);
    });

    it('should throw when config is not initialized', () => {
      expect(() => isLocalEnabled()).toThrow();
    });
  });

  describe('ENABLE_CLONE Configuration', () => {
    beforeEach(() => {
      delete process.env.ENABLE_CLONE;
      delete process.env.ENABLE_LOCAL;
    });

    it('should default to false when ENABLE_CLONE is not set', async () => {
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableClone).toBe(false);
    });

    it('should enable clone when ENABLE_CLONE is "true"', async () => {
      process.env.ENABLE_CLONE = 'true';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableClone).toBe(true);
    });

    it('should enable clone when ENABLE_CLONE is "1"', async () => {
      process.env.ENABLE_CLONE = '1';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableClone).toBe(true);
    });

    it('should disable clone when ENABLE_CLONE is "false"', async () => {
      process.env.ENABLE_CLONE = 'false';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().enableClone).toBe(false);
    });

    it('should return false for invalid/unrecognized ENABLE_CLONE values', async () => {
      const invalidValues = ['no', 'yes', 'enabled', '', '   '];

      for (const value of invalidValues) {
        cleanup();
        delete process.env.ENABLE_CLONE;
        process.env.ENABLE_CLONE = value;
        mockSpawnFailure();
        await initialize();
        expect(getServerConfig().enableClone).toBe(false);
      }
    });
  });

  describe('isCloneEnabled() helper', () => {
    beforeEach(() => {
      delete process.env.ENABLE_CLONE;
      delete process.env.ENABLE_LOCAL;
    });

    it('should return false when both local and clone are disabled', async () => {
      mockSpawnFailure();
      await initialize();
      expect(isCloneEnabled()).toBe(false);
    });

    it('should return false when local is enabled but clone is not', async () => {
      process.env.ENABLE_LOCAL = 'true';
      mockSpawnFailure();
      await initialize();
      expect(isCloneEnabled()).toBe(false);
    });

    it('should return true when clone is enabled and local is true by default', async () => {
      process.env.ENABLE_CLONE = 'true';
      mockSpawnFailure();
      await initialize();
      expect(isCloneEnabled()).toBe(true);
    });

    it('should return true when both local and clone are enabled', async () => {
      process.env.ENABLE_LOCAL = 'true';
      process.env.ENABLE_CLONE = 'true';
      mockSpawnFailure();
      await initialize();
      expect(isCloneEnabled()).toBe(true);
    });

    it('should throw when config is not initialized', () => {
      expect(() => isCloneEnabled()).toThrow();
    });
  });

  describe('getTokenSource()', () => {
    it('should return "none" when no token is available', async () => {
      mockSpawnFailure();
      const source = await getTokenSource();
      expect(source).toBe('none');
    });

    it('should return "env:GITHUB_TOKEN" when GITHUB_TOKEN is set', async () => {
      mockTokenResolution('test-token', 'env:GITHUB_TOKEN');
      const source = await getTokenSource();
      expect(source).toBe('env:GITHUB_TOKEN');
    });

    it('should return "env:GH_TOKEN" when GH_TOKEN is the source', async () => {
      mockTokenResolution('test-token', 'env:GH_TOKEN');
      const source = await getTokenSource();
      expect(source).toBe('env:GH_TOKEN');
    });

    it('should return "env:OCTOCODE_TOKEN" when OCTOCODE_TOKEN is the source', async () => {
      mockTokenResolution('test-token', 'env:OCTOCODE_TOKEN');
      const source = await getTokenSource();
      expect(source).toBe('env:OCTOCODE_TOKEN');
    });

    it('should return "gh-cli" when token comes from CLI', async () => {
      mockSpawnSuccess('cli-token');
      const source = await getTokenSource();
      expect(source).toBe('gh-cli');
    });

    it('should return "octocode-storage" when token comes from file storage', async () => {
      mockTokenResolution('stored-token', 'octocode-storage');
      const source = await getTokenSource();
      expect(source).toBe('octocode-storage');
    });

    it('should resolve fresh each time (no caching)', async () => {
      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('token-1', 'env:GITHUB_TOKEN')
      );
      const source1 = await getTokenSource();
      expect(source1).toBe('env:GITHUB_TOKEN');

      mockResolveTokenFull.mockResolvedValueOnce(
        mockTokenResult('token-2', 'gh-cli')
      );
      const source2 = await getTokenSource();
      expect(source2).toBe('gh-cli');
    });
  });

  describe('LOG Configuration with Whitespace', () => {
    beforeEach(() => {
      delete process.env.LOG;
    });

    it('should handle LOG with leading/trailing whitespace for false', async () => {
      process.env.LOG = '  false  ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().loggingEnabled).toBe(false);
    });

    it('should handle LOG with tabs for false', async () => {
      process.env.LOG = '\tfalse\t';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().loggingEnabled).toBe(false);
    });

    it('should handle LOG = "0" as false', async () => {
      process.env.LOG = '0';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().loggingEnabled).toBe(false);
    });

    it('should handle LOG = " 0 " with whitespace as false', async () => {
      process.env.LOG = ' 0 ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().loggingEnabled).toBe(false);
    });

    it('should handle LOG = "FALSE" uppercase as false', async () => {
      process.env.LOG = 'FALSE';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().loggingEnabled).toBe(false);
    });

    it('should handle LOG = "  FALSE  " with whitespace and uppercase', async () => {
      process.env.LOG = '  FALSE  ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().loggingEnabled).toBe(false);
    });

    it('should default to true for non-false values', async () => {
      const trueValues = ['true', 'TRUE', '1', 'yes', 'enabled', 'anything'];

      for (const value of trueValues) {
        cleanup();
        delete process.env.LOG;
        process.env.LOG = value;
        mockSpawnFailure();
        await initialize();
        expect(getServerConfig().loggingEnabled).toBe(true);
      }
    });
  });

  describe('Numeric Configuration with Whitespace', () => {
    it('should handle REQUEST_TIMEOUT with whitespace', async () => {
      process.env.REQUEST_TIMEOUT = '  60000  ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(60000);
    });

    it('should handle MAX_RETRIES with whitespace', async () => {
      process.env.MAX_RETRIES = '  5  ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().maxRetries).toBe(5);
    });

    it('should handle REQUEST_TIMEOUT with tabs', async () => {
      process.env.REQUEST_TIMEOUT = '\t45000\t';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(45000);
    });
  });

  describe('Timeout Configuration Floor (MIN_TIMEOUT = 5000)', () => {
    it('should allow timeout values above minimum (5000ms)', async () => {
      process.env.REQUEST_TIMEOUT = '10000';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(10000);
    });

    it('should enforce minimum timeout of 5000ms', async () => {
      process.env.REQUEST_TIMEOUT = '1000'; // Below 5s minimum
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(5000); // Clamped to MIN_TIMEOUT
    });

    it('should enforce minimum timeout for very low values', async () => {
      process.env.REQUEST_TIMEOUT = '100';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(5000);
    });

    it('should clamp REQUEST_TIMEOUT=0 to MIN_TIMEOUT (5000)', async () => {
      // 0 is a valid integer, clamped to MIN_TIMEOUT by shared config resolver
      process.env.REQUEST_TIMEOUT = '0';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(5000); // Clamped to MIN_TIMEOUT
    });

    it('should enforce minimum timeout for negative values', async () => {
      process.env.REQUEST_TIMEOUT = '-5000';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(5000);
    });

    it('should enforce maximum timeout of 300000ms (5 minutes)', async () => {
      process.env.REQUEST_TIMEOUT = '500000'; // Above MAX_TIMEOUT
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(300000); // Clamped to MAX_TIMEOUT
    });

    it('should use default timeout (30000) when REQUEST_TIMEOUT is not set', async () => {
      delete process.env.REQUEST_TIMEOUT;
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(30000);
    });

    it('should use default timeout when REQUEST_TIMEOUT is invalid', async () => {
      process.env.REQUEST_TIMEOUT = 'invalid';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().timeout).toBe(30000);
    });
  });

  describe('MaxRetries Configuration Limits', () => {
    it('should allow retry values within limits (0-10)', async () => {
      process.env.MAX_RETRIES = '5';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().maxRetries).toBe(5);
    });

    it('should clamp retries to maximum of 10', async () => {
      process.env.MAX_RETRIES = '15';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().maxRetries).toBe(10); // Clamped to MAX_RETRIES_LIMIT
    });

    it('should clamp retries to minimum of 0', async () => {
      process.env.MAX_RETRIES = '-5';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().maxRetries).toBe(0); // Clamped to MIN_RETRIES
    });

    it('should allow MAX_RETRIES=0 (valid value, no retries)', async () => {
      // 0 is a valid integer within [MIN_RETRIES, MAX_RETRIES] range
      process.env.MAX_RETRIES = '0';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().maxRetries).toBe(0); // Valid: no retries
    });

    it('should use default retries (3) when MAX_RETRIES is not set', async () => {
      delete process.env.MAX_RETRIES;
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().maxRetries).toBe(3);
    });

    it('should use default retries when MAX_RETRIES is invalid', async () => {
      process.env.MAX_RETRIES = 'invalid';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().maxRetries).toBe(3);
    });
  });

  describe('GITHUB_API_URL Configuration', () => {
    beforeEach(() => {
      delete process.env.GITHUB_API_URL;
    });

    it('should handle GITHUB_API_URL with trailing whitespace', async () => {
      process.env.GITHUB_API_URL = 'https://github.company.com/api/v3  ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().githubApiUrl).toBe(
        'https://github.company.com/api/v3'
      );
    });

    it('should handle GITHUB_API_URL with leading whitespace', async () => {
      process.env.GITHUB_API_URL = '  https://github.company.com/api/v3';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().githubApiUrl).toBe(
        'https://github.company.com/api/v3'
      );
    });

    it('should handle GITHUB_API_URL with both leading and trailing whitespace', async () => {
      process.env.GITHUB_API_URL = '  https://github.company.com/api/v3  ';
      mockSpawnFailure();
      await initialize();
      expect(getServerConfig().githubApiUrl).toBe(
        'https://github.company.com/api/v3'
      );
    });
  });

  describe('Configuration Edge Cases', () => {
    it('should handle all configs with various whitespace simultaneously', async () => {
      process.env.ENABLE_LOCAL = '  true  ';
      process.env.LOG = '  false  ';
      process.env.GITHUB_API_URL = '  https://custom.api.com  ';
      process.env.REQUEST_TIMEOUT = '  45000  ';
      process.env.MAX_RETRIES = '  7  ';
      process.env.TOOLS_TO_RUN = '  tool1  ,  tool2  ';
      mockSpawnFailure();

      await initialize();
      const config = getServerConfig();

      expect(config.enableLocal).toBe(true);
      expect(config.loggingEnabled).toBe(false);
      expect(config.githubApiUrl).toBe('https://custom.api.com');
      expect(config.timeout).toBe(45000);
      expect(config.maxRetries).toBe(7);
      expect(config.toolsToRun).toEqual(['tool1', 'tool2']);
    });

    it('should handle unicode whitespace characters', async () => {
      // Non-breaking space and other unicode whitespace
      process.env.ENABLE_LOCAL = '\u00A0true\u00A0';
      mockSpawnFailure();
      await initialize();
      // Note: trim() handles regular whitespace, unicode may vary
      expect(getServerConfig().enableLocal).toBe(true);
    });
  });

  describe('Active Provider Configuration', () => {
    let getActiveProvider: typeof import('../src/serverConfig.js').getActiveProvider;
    let getActiveProviderConfig: typeof import('../src/serverConfig.js').getActiveProviderConfig;

    beforeEach(async () => {
      const serverConfig = await import('../src/serverConfig.js');
      getActiveProvider = serverConfig.getActiveProvider;
      getActiveProviderConfig = serverConfig.getActiveProviderConfig;
    });

    it('should always return github as the active provider', () => {
      expect(getActiveProvider()).toBe('github');
    });

    it('should return github provider config with no custom API URL', () => {
      delete process.env.GITHUB_API_URL;

      const config = getActiveProviderConfig();

      expect(config.provider).toBe('github');
      expect(config.baseUrl).toBeUndefined();
      expect(config.token).toBeUndefined();
    });

    it('should return github provider config with custom API URL', () => {
      process.env.GITHUB_API_URL = 'https://github.mycompany.com/api/v3';

      const config = getActiveProviderConfig();

      expect(config.provider).toBe('github');
      expect(config.baseUrl).toBe('https://github.mycompany.com/api/v3');

      delete process.env.GITHUB_API_URL;
    });
  });
});
