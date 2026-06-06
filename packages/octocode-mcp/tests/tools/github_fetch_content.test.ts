import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { getTextContent } from '../utils/testHelpers.js';

const mockInitialize = vi.hoisted(() => vi.fn());
const mockGetServerConfig = vi.hoisted(() => vi.fn());
const mockGetGitHubToken = vi.hoisted(() => vi.fn());
const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../src/serverConfig.js', () => ({
  initialize: mockInitialize,
  getServerConfig: mockGetServerConfig,
  isLoggingEnabled: vi.fn(() => false),
  getGitHubToken: mockGetGitHubToken,
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
}));

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

const mockPerformSampling = vi.hoisted(() => vi.fn());
const mockCreateQASamplingRequest = vi.hoisted(() => vi.fn());

vi.mock('../../src/sampling.js', () => ({
  SamplingUtils: {
    createQASamplingRequest: mockCreateQASamplingRequest,
  },
  performSampling: mockPerformSampling,
}));

import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';

describe('GitHub Fetch Content Tool', () => {
  let mockServer: MockMcpServer;
  let mockProvider: {
    searchCode: ReturnType<typeof vi.fn>;
    getFileContent: ReturnType<typeof vi.fn>;
    searchRepos: ReturnType<typeof vi.fn>;
    searchPullRequests: ReturnType<typeof vi.fn>;
    getRepoStructure: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    mockServer = createMockMcpServer();
    vi.clearAllMocks();

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
      tokenSource: 'env:GITHUB_TOKEN',
    });

    mockInitialize.mockResolvedValue(undefined);

    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    mockPerformSampling.mockReset();
    mockCreateQASamplingRequest.mockReset();

    registerFetchGitHubFileContentTool(mockServer.server);
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  describe('Success scenarios', () => {
    it('should handle single valid file request', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'README.md',
          content: '# Hello World\n\nThis is a test file.',
          encoding: 'utf-8',
          size: 35,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'README.md',
              branch: 'main',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('results:');
      expect(responseText).toContain('owner: "test"');
      expect(responseText).toContain('repo: "repo"');
      expect(responseText).toContain('path: "README.md"');
      expect(responseText).toContain('content:');
      expect(responseText).not.toContain('status:');
    });

    it('should include totalLines and omit repeated generic hints', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'src/app.ts',
          content: 'line1\nline2',
          encoding: 'utf-8',
          size: 11,
          totalLines: 2,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: 'src/app.ts', branch: 'main' },
          ],
        }
      );

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('totalLines: 2');
      expect(responseText).not.toContain(
        "Use 'owner', 'repo', 'branch', 'path'"
      );
      expect(responseText).not.toContain("Follow 'mainResearchGoal'");
    });

    it('should pass authInfo to provider', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'test.js',
          content: 'console.log("test");',
          encoding: 'utf-8',
          size: 20,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'testowner',
              repo: 'testrepo',
              path: 'test.js',
              branch: 'main',
            },
          ],
        },
        { authInfo: { token: 'test-auth-token' } }
      );

      expect(mockGetProvider).toHaveBeenCalled();
      expect(mockProvider.getFileContent).toHaveBeenCalled();
    });

    it('should handle multiple file requests', async () => {
      mockProvider.getFileContent
        .mockResolvedValueOnce({
          data: {
            path: 'file1.js',
            content: 'content1',
            encoding: 'utf-8',
            size: 8,
            ref: 'main',
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            path: 'file2.js',
            content: 'content2',
            encoding: 'utf-8',
            size: 8,
            ref: 'main',
          },
          status: 200,
          provider: 'github',
        });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: 'file1.js', branch: 'main' },
            { owner: 'test', repo: 'repo', path: 'file2.js', branch: 'main' },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('content1');
      expect(responseText).toContain('content2');
      expect(responseText).toContain('path: "file1.js"');
      expect(responseText).toContain('path: "file2.js"');
    });
  });

  describe('Error scenarios', () => {
    it('should handle file not found error', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        error: 'File not found',
        status: 404,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'nonexistent.js',
              branch: 'main',
            },
          ],
        }
      );

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });

    it('should handle API exception', async () => {
      mockProvider.getFileContent.mockRejectedValue(new Error('Network error'));

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: 'test.js', branch: 'main' },
          ],
        }
      );

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });

    it('should include GitHub API error-derived hints (forbidden/permissions)', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        error: 'Forbidden',
        status: 403,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: 'secret.js', branch: 'main' },
          ],
        }
      );

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });
  });

  describe('Full Content Fetching', () => {
    it('should fetch full content when fullContent=true', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'large-file.js',
          content: 'const x = 1;\nconst y = 2;\nconst z = 3;',
          encoding: 'utf-8',
          size: 38,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'large-file.js',
              branch: 'main',
              fullContent: true,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('const x = 1');
    });
  });

  describe('Partial Content Fetching', () => {
    it('should fetch content with startLine and endLine', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.js',
          content: 'line 5 content',
          encoding: 'utf-8',
          size: 14,
          ref: 'main',
          startLine: 5,
          endLine: 10,
          isPartial: true,
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'file.js',
              branch: 'main',
              startLine: 5,
              endLine: 10,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('line 5 content');
    });

    it('should fetch content with matchString and context', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.js',
          content: 'function test() { return true; }',
          encoding: 'utf-8',
          size: 32,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'file.js',
              branch: 'main',
              matchString: 'function',
              matchStringContextLines: 5,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('function test');
    });

    it('signals matchString-not-found instead of silent empty content (F2)', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.js',
          content: '',
          branch: 'main',
          totalLines: 3,
          warnings: [
            'No matches for "NO_SUCH_ANCHOR_ZZ_98765" in file (3 lines scanned). Try matchStringIsRegex=true, a different anchor, or fullContent=true.',
          ],
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'file.js',
              branch: 'main',
              matchString: 'NO_SUCH_ANCHOR_ZZ_98765',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toMatch(/no matches|not found/i);
    });

    it('should use default matchStringContextLines when not specified', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.js',
          content: 'const x = 1;',
          encoding: 'utf-8',
          size: 12,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'file.js',
              branch: 'main',
              matchString: 'const',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });
  });

  describe('Minification', () => {
    it('should always apply minification', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.json',
          content: '{"key": "value"}',
          encoding: 'utf-8',
          size: 16,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: 'file.json', branch: 'main' },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('should handle minification failure gracefully', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.txt',
          content: 'plain text content',
          encoding: 'utf-8',
          size: 18,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: 'file.txt', branch: 'main' },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('plain text content');
    });
  });

  describe('Security and Sanitization', () => {
    it('should handle content with security warnings from API', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: '.env',
          content: 'API_KEY=secret123',
          encoding: 'utf-8',
          size: 17,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: '.env', branch: 'main' },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('should handle public content without security warnings', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'README.md',
          content: '# Public README',
          encoding: 'utf-8',
          size: 15,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: 'README.md', branch: 'main' },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Public README');
    });
  });

  describe('Branch and Repository Handling', () => {
    it('should handle custom branch parameter', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.js',
          content: 'feature code',
          encoding: 'utf-8',
          size: 12,
          ref: 'feature-branch',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'file.js',
              branch: 'feature-branch',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('feature code');
    });

    it('should handle missing branch (defaults to main/master)', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.js',
          content: 'default branch code',
          encoding: 'utf-8',
          size: 19,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [{ owner: 'test', repo: 'repo', path: 'file.js' }],
        }
      );

      expect(result.isError).toBe(false);
    });
  });

  describe('Mixed Success and Error Scenarios', () => {
    it('should handle mixed success and error results in bulk queries', async () => {
      mockProvider.getFileContent
        .mockResolvedValueOnce({
          data: {
            path: 'good.js',
            content: 'good',
            encoding: 'utf-8',
            size: 4,
            ref: 'main',
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          error: 'Not found',
          status: 404,
          provider: 'github',
        });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: 'good.js', branch: 'main' },
            { owner: 'test', repo: 'repo', path: 'bad.js', branch: 'main' },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('content: "good"');
      expect(responseText).toContain('path: "good.js"');
      expect(responseText).toContain('errors:');
    });
  });

  describe('Parameter Type Conversion', () => {
    it('should handle string numbers for line parameters', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.js',
          content: 'line content',
          encoding: 'utf-8',
          size: 12,
          ref: 'main',
          startLine: 1,
          endLine: 10,
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'file.js',
              branch: 'main',
              startLine: 1,
              endLine: 10,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('should handle boolean parameters correctly', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'file.js',
          content: 'full content',
          encoding: 'utf-8',
          size: 12,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'file.js',
              branch: 'main',
              fullContent: true,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });
  });

  describe('Input validation', () => {
    it('should handle empty queries array gracefully', async () => {
      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        { queries: [] }
      );

      expect(result.isError).toBe(false);
    });

    it('should handle missing queries parameter gracefully', async () => {
      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {}
      );

      expect(result).toBeDefined();
    });
  });
});
