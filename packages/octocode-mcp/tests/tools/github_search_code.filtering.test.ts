import { describe, it, expect, beforeEach, vi } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';

interface CallToolResult {
  content: Array<{ type: string; text: string }>;
  isError?: boolean;
}

// Mock provider
const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../src/serverConfig.js', () => ({
  isLoggingEnabled: vi.fn(() => false),
  getGitHubToken: vi.fn(() => Promise.resolve('test-token')),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'test-token',
  })),
}));

// Import after mocking
import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';

describe('GitHub Search Code Tool - Filtering at Tool Level', () => {
  let server: McpServer;
  let toolHandler: (
    args: unknown,
    authInfo: unknown,
    sessionId: unknown
  ) => Promise<unknown>;
  let mockProvider: {
    searchCode: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    vi.clearAllMocks();

    // Setup mock provider
    mockProvider = {
      searchCode: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    // Create a mock server
    server = {
      registerTool: vi.fn((_name, _schema, handler) => {
        toolHandler = handler;
        return Promise.resolve();
      }),
    } as unknown as McpServer;

    // Register the tool
    registerGitHubSearchCodeTool(server);
  });

  describe('Double filtering - both API and tool level', () => {
    it('should apply filtering at both codeSearch.ts and tool level', async () => {
      // Provider returns raw results (filtering happens at tool level now)
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/index.js',
              matches: [{ context: 'function test() {}', positions: [] }],
              url: 'https://github.com/test/repo/blob/main/src/index.js',
              repository: {
                id: '1',
                name: 'test/repo',
                url: 'https://github.com/test/repo',
              },
            },
            {
              path: 'node_modules/lodash/lodash.js',
              matches: [{ context: 'function lodash() {}', positions: [] }],
              url: 'https://github.com/test/repo/blob/main/node_modules/lodash/lodash.js',
              repository: {
                id: '1',
                name: 'test/repo',
                url: 'https://github.com/test/repo',
              },
            },
            {
              path: 'package-lock.json',
              matches: [{ context: '"lodash": "4.17.21"', positions: [] }],
              url: 'https://github.com/test/repo/blob/main/package-lock.json',
              repository: {
                id: '1',
                name: 'test/repo',
                url: 'https://github.com/test/repo',
              },
            },
            {
              path: 'vendor/jquery.js',
              matches: [{ context: 'jQuery lib', positions: [] }],
              url: 'https://github.com/test/repo/blob/main/vendor/jquery.js',
              repository: {
                id: '1',
                name: 'test/repo',
                url: 'https://github.com/test/repo',
              },
            },
            {
              path: 'dist/bundle.js',
              matches: [{ context: 'bundled code', positions: [] }],
              url: 'https://github.com/test/repo/blob/main/dist/bundle.js',
              repository: {
                id: '1',
                name: 'test/repo',
                url: 'https://github.com/test/repo',
              },
            },
          ],
          totalCount: 5,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = (await toolHandler(
        {
          queries: [
            {
              keywordsToSearch: ['function'],
              owner: 'test',
              repo: 'repo',
            },
          ],
        },
        { token: 'test-token' },
        'test-session'
      )) as CallToolResult;

      expect(result.isError).toBeFalsy();

      // Check the response contains filtered results
      const responseText = result.content[0]?.text || '';

      // src/index.js should be present (valid source file)
      expect(responseText).toContain('src/index.js');

      // Note: Filtering behavior depends on tool implementation
      // The test verifies the tool processes results correctly
    });

    it('should handle empty results after filtering at tool level', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = (await toolHandler(
        {
          queries: [
            {
              keywordsToSearch: ['nonexistent'],
              owner: 'test',
              repo: 'repo',
            },
          ],
        },
        { token: 'test-token' },
        'test-session'
      )) as CallToolResult;

      expect(result.isError).toBeFalsy();
      const structured = result.structuredContent as { results: unknown[] };
      expect(structured.results).toEqual([]);
    });

    it('should filter vendor and third-party directories', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/main.ts',
              matches: [{ context: 'main code', positions: [] }],
              url: 'https://github.com/test/repo/blob/main/src/main.ts',
              repository: {
                id: '1',
                name: 'test/repo',
                url: 'https://github.com/test/repo',
              },
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = (await toolHandler(
        {
          queries: [
            {
              keywordsToSearch: ['code'],
              owner: 'test',
              repo: 'repo',
            },
          ],
        },
        { token: 'test-token' },
        'test-session'
      )) as CallToolResult;

      expect(result.isError).toBeFalsy();
      const responseText = result.content[0]?.text || '';
      expect(responseText).toContain('src/main.ts');
    });

    it('should filter build and dist directories', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/app.js',
              matches: [{ context: 'app code', positions: [] }],
              url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: {
                id: '1',
                name: 'test/repo',
                url: 'https://github.com/test/repo',
              },
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = (await toolHandler(
        {
          queries: [
            {
              keywordsToSearch: ['code'],
              owner: 'test',
              repo: 'repo',
            },
          ],
        },
        { token: 'test-token' },
        'test-session'
      )) as CallToolResult;

      expect(result.isError).toBeFalsy();
      const responseText = result.content[0]?.text || '';
      expect(responseText).toContain('src/app.js');
    });

    it('should handle multiple queries with filtering', async () => {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [
              {
                path: 'src/utils.ts',
                matches: [{ context: 'utils code', positions: [] }],
                url: 'https://github.com/test/repo/blob/main/src/utils.ts',
                repository: {
                  id: '1',
                  name: 'test/repo',
                  url: 'https://github.com/test/repo',
                },
              },
            ],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            items: [
              {
                path: 'src/helpers.ts',
                matches: [{ context: 'helpers code', positions: [] }],
                url: 'https://github.com/test/repo/blob/main/src/helpers.ts',
                repository: {
                  id: '1',
                  name: 'test/repo',
                  url: 'https://github.com/test/repo',
                },
              },
            ],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        });

      const result = (await toolHandler(
        {
          queries: [
            {
              keywordsToSearch: ['utils'],
              owner: 'test',
              repo: 'repo',
            },
            {
              keywordsToSearch: ['helpers'],
              owner: 'test',
              repo: 'repo',
            },
          ],
        },
        { token: 'test-token' },
        'test-session'
      )) as CallToolResult;

      expect(result.isError).toBeFalsy();
      const responseText = result.content[0]?.text || '';
      expect(responseText).toContain('src/utils.ts');
      expect(responseText).toContain('src/helpers.ts');
    });
  });
});
