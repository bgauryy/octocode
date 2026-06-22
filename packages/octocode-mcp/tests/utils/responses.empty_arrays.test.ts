import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { getTextContent } from './testHelpers.js';

const mockGetServerConfig = vi.hoisted(() => vi.fn());
const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  initialize: vi.fn(),
  getServerConfig: mockGetServerConfig,
  getGitHubToken: vi.fn(() => Promise.resolve('mock-token')),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
}));

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

describe('Empty Arrays Removal in Responses', () => {
  let mockServer: MockMcpServer;
  let mockProvider: {
    searchCode: ReturnType<typeof vi.fn>;
    searchRepos: ReturnType<typeof vi.fn>;
    getRepoStructure: ReturnType<typeof vi.fn>;
    getFileContent: ReturnType<typeof vi.fn>;
    searchPullRequests: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    mockServer = createMockMcpServer();
    vi.clearAllMocks();

    mockGetServerConfig.mockReturnValue({
      version: '4.0.5',
      enableTools: [],
      disableTools: [],
      timeout: 30000,
      maxRetries: 3,
    });

    mockProvider = {
      searchCode: vi.fn(),
      searchRepos: vi.fn(),
      getRepoStructure: vi.fn(),
      getFileContent: vi.fn(),
      searchPullRequests: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  describe('GitHub Search Code - Empty Results', () => {
    beforeEach(() => {
      registerGitHubSearchCodeTool(mockServer.server);
    });

    it('should not include empty files array in response', async () => {
      mockProvider.searchCode.mockResolvedValueOnce({
        data: {
          items: [],
          totalCount: 0,
          pagination: {
            currentPage: 1,
            totalPages: 0,
            hasMore: false,
          },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        {
          queries: [
            {
              id: 'search_empty_query',
              keywords: ['nonexistent'],
              researchGoal: 'Verify empty search responses stay clean',
              reasoning: 'Test empty array removal',
            },
          ],
        },
        { authInfo: { token: 'mock-token' } }
      );

      const responseText = getTextContent(result.content);

      expect(responseText).not.toMatch(/files:\s*\[\]/);
      expect(responseText).toContain('results: []');
    });

    it('should include file even when matches array is empty', async () => {
      mockProvider.searchCode.mockResolvedValueOnce({
        data: {
          items: [
            {
              path: 'test.js',
              matches: [],
              url: 'https://github.com/test/repo/blob/main/test.js',
              repository: {
                id: '1',
                name: 'test-repo',
                url: 'https://github.com/test/repo',
              },
            },
          ],
          totalCount: 1,
          pagination: {
            currentPage: 1,
            totalPages: 1,
            hasMore: false,
          },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        {
          queries: [
            {
              id: 'search_empty_matches_query',
              keywords: ['test'],
              researchGoal:
                'Verify files remain even when match arrays are empty',
              reasoning: 'Test file presence with empty matches',
            },
          ],
        },
        { authInfo: { token: 'mock-token' } }
      );

      const responseText = getTextContent(result.content);

      expect(responseText).toContain('test.js');
      expect(responseText).toContain('repo: test-repo');
    });
  });

  describe('Mixed Results - Some Empty Arrays', () => {
    beforeEach(() => {
      registerGitHubSearchCodeTool(mockServer.server);
    });

    it('should not include empty arrays even when there are successful results', async () => {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [],
            totalCount: 0,
            pagination: { currentPage: 1, totalPages: 0, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            items: [
              {
                path: 'found.js',
                matches: [{ context: 'const found = 1', positions: [] }],
                url: 'https://github.com/test/repo/blob/main/found.js',
                repository: {
                  id: '1',
                  name: 'test-repo',
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

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        {
          queries: [
            {
              id: 'mixed_empty_query',
              keywords: ['empty'],
              researchGoal: 'Verify empty result rendering',
              reasoning: 'Will be empty',
            },
            {
              id: 'mixed_found_query',
              keywords: ['found'],
              researchGoal: 'Verify successful result rendering',
              reasoning: 'Will find results',
            },
          ],
        },
        { authInfo: { token: 'mock-token' } }
      );

      const responseText = getTextContent(result.content);

      expect(responseText).toContain('found.js');
      expect(responseText).not.toMatch(/matches:\s*\[\]/);
    });
  });

  describe('Nested Empty Arrays', () => {
    beforeEach(() => {
      registerGitHubSearchCodeTool(mockServer.server);
    });

    it('should recursively remove all empty arrays', async () => {
      mockProvider.searchCode.mockResolvedValueOnce({
        data: {
          items: [
            {
              path: 'file1.js',
              matches: [{ context: 'const test = 1', positions: [] }],
              url: 'https://github.com/test/repo/blob/main/file1.js',
              repository: {
                id: '1',
                name: 'test-repo',
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

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        {
          queries: [
            {
              id: 'nested_empty_arrays_query',
              keywords: ['test'],
              researchGoal: 'Verify nested empty arrays are removed',
              reasoning: 'Test nested empty array removal',
            },
          ],
        },
        { authInfo: { token: 'mock-token' } }
      );

      const responseText = getTextContent(result.content);

      expect(responseText).not.toMatch(/:\s*\[\]/);

      expect(responseText).toContain('file1.js');
      expect(responseText).toContain('const test = 1');
    });
  });

  describe('Empty Hints Arrays', () => {
    beforeEach(() => {
      registerGitHubSearchCodeTool(mockServer.server);
    });

    it('should not include empty hints arrays', async () => {
      mockProvider.searchCode.mockResolvedValueOnce({
        data: {
          items: [
            {
              path: 'file.js',
              matches: [{ context: 'code', positions: [] }],
              url: 'https://github.com/test/repo/blob/main/file.js',
              repository: {
                id: '1',
                name: 'test-repo',
                url: 'https://github.com/test/repo',
              },
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
        hints: [],
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        {
          queries: [
            {
              id: 'empty_hints_query',
              keywords: ['test'],
              researchGoal: 'Verify empty hints do not appear',
              reasoning: 'Test empty hints removal',
            },
          ],
        },
        { authInfo: { token: 'mock-token' } }
      );

      const responseText = getTextContent(result.content);

      expect(responseText).toContain('results:');
      expect(responseText).toContain('file.js');
      expect(responseText).not.toMatch(/hints:\s*\[\]/);
    });
  });

  describe('Path-only Match Preservation', () => {
    beforeEach(() => {
      registerGitHubSearchCodeTool(mockServer.server);
    });

    it('should preserve files even if matches array is empty (path match)', async () => {
      mockProvider.searchCode.mockResolvedValueOnce({
        data: {
          items: [
            {
              path: 'path/to/empty_match_file.ts',
              matches: [],
              url: 'https://github.com/test/repo/blob/main/path/to/empty_match_file.ts',
              repository: {
                id: '1',
                name: 'test-repo',
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

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        {
          queries: [
            {
              keywords: ['empty_match'],
              match: 'path',
              reasoning: 'Test path-only match preservation',
            },
          ],
        },
        { authInfo: { token: 'mock-token' } }
      );

      const responseText = getTextContent(result.content);

      expect(responseText).toContain('path/to/empty_match_file.ts');
    });
  });
});
