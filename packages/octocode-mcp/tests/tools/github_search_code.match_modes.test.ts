import { getTextContent } from '../utils/testHelpers.js';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  isLoggingEnabled: vi.fn(() => false),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  getGitHubToken: vi.fn(() => Promise.resolve('test-token')),
  getServerConfig: vi.fn(() => ({
    version: '1.0.0',
    timeout: 30000,
    maxRetries: 3,
    loggingEnabled: false,
  })),
}));

import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

describe('GitHub Search Code - match Parameter Modes', () => {
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

    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    registerGitHubSearchCodeTool(mockServer.server);
    vi.clearAllMocks();
    mockGetProvider.mockReturnValue(mockProvider);
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  describe('match="file" (content search mode)', () => {
    it('should return files with text_matches from content search', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/tools/utils.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [
                {
                  context: 'export function createSuccessResult<T>(',
                  positions: [],
                },
                {
                  context: 'return result as ToolSuccessResult',
                  positions: [],
                },
              ],
              url: '',
            },
            {
              path: 'src/tools/github_search_code.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [
                { context: 'return createSuccessResult(query,', positions: [] },
              ],
              url: '',
            },
          ],
          totalCount: 2,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['createSuccessResult'],
            owner: 'test',
            repo: 'repo',
            match: 'file',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('src/tools/utils.ts');
      expect(responseText).toContain('matches:');
    });

    it('should search IN file content and return matching code snippets', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/components/App.tsx',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [
                {
                  context: 'const [state, setState] = useState()',
                  positions: [],
                },
              ],
              url: '',
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['useState'],
            owner: 'test',
            repo: 'repo',
            match: 'file',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('useState');
    });

    it('should return multiple text matches per file when keyword appears multiple times', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/hooks/useData.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [
                { context: 'export function useData() {', positions: [] },
                { context: 'const data = useData();', positions: [] },
              ],
              url: '',
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['useData'],
            owner: 'test',
            repo: 'repo',
            match: 'file',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('useData');
    });

    it('marks file-mode results as path-only when GitHub returns no text matches', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/no-snippet.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
              url: '',
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['noSnippet'],
            owner: 'test',
            repo: 'repo',
            match: 'file',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('pathOnly: true');
      expect(responseText).toContain('GitHub did not return text matches');
    });
  });

  describe('match="path" (filename/directory search mode)', () => {
    it('should find files by path/filename and return ONLY paths (no text_matches)', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'scripts/jest/TestFlags.js',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
              url: '',
            },
            {
              path: 'packages/react-dom/src/__tests__/ReactDOM-test.js',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
              url: '',
            },
          ],
          totalCount: 2,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['test'],
            owner: 'test',
            repo: 'repo',
            match: 'path',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('TestFlags.js');
      expect(responseText).not.toContain('text_matches');
    });

    it('should search in file/directory NAMES and return content previews from matching files', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/utils/helpers.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
              url: '',
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['utils'],
            owner: 'test',
            repo: 'repo',
            match: 'path',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('helpers.ts');
    });

    it('should find files by directory names in path', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'components/Button/index.tsx',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
              url: '',
            },
            {
              path: 'components/Modal/index.tsx',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
              url: '',
            },
          ],
          totalCount: 2,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['components'],
            owner: 'test',
            repo: 'repo',
            match: 'path',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Button');
      expect(responseText).toContain('Modal');
    });
  });

  describe('Bulk operations with both modes', () => {
    it('should handle multiple queries with different match modes', async () => {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [
              {
                path: 'src/utils.ts',
                repository: { id: '1', name: 'test/repo', url: '' },
                matches: [{ context: 'function util() {}', positions: [] }],
                url: '',
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
                path: 'config/settings.json',
                repository: { id: '1', name: 'test/repo', url: '' },
                matches: [],
                url: '',
              },
            ],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['function'],
            owner: 'test',
            repo: 'repo',
            match: 'file',
          },
          {
            keywordsToSearch: ['config'],
            owner: 'test',
            repo: 'repo',
            match: 'path',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('utils.ts');
      expect(responseText).toContain('settings.json');
    });
  });

  describe('match parameter passed to provider', () => {
    it('should pass match="file" to the provider searchCode call', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['useState'],
            owner: 'test',
            repo: 'repo',
            match: 'file',
          },
        ],
      });

      expect(mockProvider.searchCode).toHaveBeenCalledTimes(1);
      const providerQuery = mockProvider.searchCode.mock.calls[0]?.[0];
      expect(providerQuery.match).toBe('file');
    });

    it('provider receives undefined limit when agent omits it (execution layer applies default)', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['useState'],
            owner: 'test',
            repo: 'repo',
            match: 'file',
          },
        ],
      });

      const providerQuery = mockProvider.searchCode.mock.calls[0]?.[0];
      expect(providerQuery.limit).toBeUndefined();
    });

    it('provider receives agent-supplied limit when limit is set', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['useState'],
            owner: 'test',
            repo: 'repo',
            match: 'file',
            limit: 100,
          },
        ],
      });

      const providerQuery = mockProvider.searchCode.mock.calls[0]?.[0];
      expect(providerQuery.limit).toBe(100);
    });

    it('should pass match="path" to the provider searchCode call', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['config'],
            owner: 'test',
            repo: 'repo',
            match: 'path',
          },
        ],
      });

      expect(mockProvider.searchCode).toHaveBeenCalledTimes(1);
      const providerQuery = mockProvider.searchCode.mock.calls[0]?.[0];
      expect(providerQuery.match).toBe('path');
    });

    it('should not pass match when it is undefined', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['test'],
            owner: 'test',
            repo: 'repo',
          },
        ],
      });

      expect(mockProvider.searchCode).toHaveBeenCalledTimes(1);
      const providerQuery = mockProvider.searchCode.mock.calls[0]?.[0];
      expect(providerQuery.match).toBeUndefined();
    });
  });

  describe('Empty results handling', () => {
    it('should handle no results gracefully', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['nonexistentKeyword12345'],
            owner: 'test',
            repo: 'repo',
            match: 'file',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const structured = result.structuredContent as { results: unknown[] };
      expect(structured.results).toEqual([]);
    });
  });
});
