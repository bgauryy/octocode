import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { getTextContent } from '../utils/testHelpers.js';

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../src/serverConfig.js', () => ({
  isLoggingEnabled: vi.fn(() => false),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  getGitHubToken: vi.fn(async () => 'test-token'),
  getServerConfig: vi.fn(() => ({
    timeout: 30000,
    version: '1.0.0',
  })),
}));

import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';

describe('GitHub View Repository Structure Tool', () => {
  let mockServer: MockMcpServer;
  let mockProvider: {
    searchCode: ReturnType<typeof vi.fn>;
    getFileContent: ReturnType<typeof vi.fn>;
    searchRepos: ReturnType<typeof vi.fn>;
    searchPullRequests: ReturnType<typeof vi.fn>;
    getRepoStructure: ReturnType<typeof vi.fn>;
    resolveDefaultBranch: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    mockServer = createMockMcpServer();

    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
      resolveDefaultBranch: vi.fn().mockResolvedValue('main'),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    vi.clearAllMocks();
    mockGetProvider.mockReturnValue(mockProvider);
    mockProvider.resolveDefaultBranch.mockResolvedValue('main');
    registerViewGitHubRepoStructureTool(mockServer.server);

    // Default mock response - uses structure format
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'test/repo',
        branch: 'main',
        path: '',
        structure: {
          '.': {
            files: ['README.md', 'package.json'],
            folders: ['src', 'tests'],
          },
        },
        summary: {
          totalFiles: 2,
          totalFolders: 2,
          truncated: false,
        },
      },
      status: 200,
      provider: 'github',
    });
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  it('should handle valid requests', async () => {
    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
          },
        ],
      }
    );

    expect(result.isError).toBe(false);
    const responseText = getTextContent(result.content);
    expect(responseText).toContain('README.md');
    expect(responseText).toContain('package.json');
  });

  it('should resolve default branch when branch is omitted', async () => {
    mockProvider.resolveDefaultBranch.mockResolvedValue('master');

    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'expressjs/express',
        branch: 'master',
        path: '',
        structure: {
          '.': {
            files: ['Readme.md', 'package.json'],
            folders: ['lib', 'test'],
          },
        },
        summary: {
          totalFiles: 2,
          totalFolders: 2,
          truncated: false,
        },
      },
      status: 200,
      provider: 'github',
    });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'expressjs',
            repo: 'express',
          },
        ],
      }
    );

    expect(result.isError).toBe(false);
    expect(mockProvider.resolveDefaultBranch).toHaveBeenCalledWith(
      'expressjs/express'
    );
    expect(mockProvider.getRepoStructure).toHaveBeenCalledWith(
      expect.objectContaining({ ref: 'master' })
    );
    const responseText = getTextContent(result.content);
    expect(responseText).toContain('Readme.md');
  });

  it('should handle custom path', async () => {
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'test/repo',
        branch: 'main',
        path: 'src',
        structure: {
          src: {
            files: ['index.ts'],
            folders: ['utils'],
          },
        },
        summary: {
          totalFiles: 1,
          totalFolders: 1,
          truncated: false,
        },
      },
      status: 200,
      provider: 'github',
    });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
            path: 'src',
          },
        ],
      }
    );

    expect(result.isError).toBe(false);
    const responseText = getTextContent(result.content);
    expect(responseText).toContain('index.ts');
  });

  it('should handle depth parameter', async () => {
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'test/repo',
        branch: 'main',
        path: '',
        structure: {
          '.': {
            files: ['README.md'],
            folders: ['src'],
          },
          src: {
            files: ['index.ts'],
            folders: [],
          },
        },
        summary: {
          totalFiles: 2,
          totalFolders: 1,
          truncated: false,
        },
      },
      status: 200,
      provider: 'github',
    });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
            depth: 2,
          },
        ],
      }
    );

    expect(result.isError).toBe(false);
  });

  it('paginates structure at the directory-NODE level — a node files[] is never sliced — and continues', async () => {
    const mkFiles = (p: string) => [`${p}-1.ts`, `${p}-2.ts`, `${p}-3.ts`];

    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'test/repo',
        branch: 'main',
        path: '',
        structure: {
          alpha: { files: mkFiles('a'), folders: [] },
          bravo: { files: mkFiles('b'), folders: [] },
          charlie: { files: mkFiles('c'), folders: [] },
          delta: { files: mkFiles('d'), folders: [] },
        },
        summary: {
          totalFiles: 12,
          totalFolders: 0,
          truncated: false,
        },
      },
      status: 200,
      provider: 'github',
    });

    const firstResult = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
            charLength: 120,
          },
        ],
      }
    );

    const firstStructured = firstResult.structuredContent as {
      results: Array<{
        data: {
          structure?: Record<string, { files?: string[] }>;
          outputPagination?: {
            hasMore: boolean;
            charOffset: number;
            charLength: number;
          };
        };
      }>;
    };
    const firstData = firstStructured.results[0]!.data;
    const nextOffset =
      (firstData.outputPagination?.charOffset ?? 0) +
      (firstData.outputPagination?.charLength ?? 0);

    // Node-atomic: every directory node on the page carries its FULL files[]
    // (never a truncated slice); pagination happens between whole nodes.
    const firstNodes = Object.keys(firstData.structure ?? {});
    expect(firstNodes.length).toBeGreaterThan(0);
    expect(firstNodes.length).toBeLessThan(4); // not all 4 fit → paginated
    for (const node of Object.values(firstData.structure ?? {})) {
      expect(node.files?.length).toBe(3);
    }
    expect(firstData.outputPagination?.hasMore).toBe(true);

    const secondResult = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
            charOffset: nextOffset,
            charLength: 120,
          },
        ],
      }
    );

    const secondStructured = secondResult.structuredContent as {
      results: Array<{
        data: {
          structure?: Record<string, { files?: string[] }>;
        };
      }>;
    };
    const secondData = secondStructured.results[0]!.data;
    // Cursor advanced to different nodes, each still whole.
    expect(Object.keys(secondData.structure ?? {})).not.toEqual(firstNodes);
    for (const node of Object.values(secondData.structure ?? {})) {
      expect(node.files?.length).toBe(3);
    }
  });

  it('should handle not found error', async () => {
    mockProvider.getRepoStructure.mockResolvedValue({
      error: 'Repository not found',
      status: 404,
      provider: 'github',
    });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'nonexistent',
            repo: 'repo',
            branch: 'main',
          },
        ],
      }
    );

    expect(result.isError).toBe(true);
    const responseText = getTextContent(result.content);
    expect(responseText).toContain('error');
  });

  it('should filter out directories with only ignored files and folders', async () => {
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'test/repo',
        branch: 'main',
        path: '',
        structure: {
          '.': {
            files: ['README.md', 'package.json'],
            folders: ['src', 'tests'],
          },
          'ignored-only': {
            files: ['.DS_Store', 'Thumbs.db'],
            folders: ['node_modules', '.git'],
          },
        },
        summary: {
          totalFiles: 4,
          totalFolders: 4,
          truncated: false,
        },
      },
      status: 200,
      provider: 'github',
    });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
          },
        ],
      }
    );

    expect(result.isError).toBe(false);
    const responseText = getTextContent(result.content);
    expect(responseText).toContain('README.md');
    expect(responseText).toContain('src');
    expect(responseText).not.toContain('.DS_Store');
    expect(responseText).not.toContain('node_modules');
  });

  it('should handle empty directory', async () => {
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'test/repo',
        branch: 'main',
        path: 'empty-dir',
        structure: {},
        summary: {
          totalFiles: 0,
          totalFolders: 0,
          truncated: false,
        },
      },
      status: 200,
      provider: 'github',
    });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
            path: 'empty-dir',
          },
        ],
      }
    );

    expect(result.isError).toBe(false);
  });

  it('should handle bulk queries', async () => {
    mockProvider.getRepoStructure
      .mockResolvedValueOnce({
        data: {
          projectPath: 'test/repo1',
          branch: 'main',
          path: '',
          structure: {
            '.': {
              files: ['README.md'],
              folders: [],
            },
          },
          summary: {
            totalFiles: 1,
            totalFolders: 0,
            truncated: false,
          },
        },
        status: 200,
        provider: 'github',
      })
      .mockResolvedValueOnce({
        data: {
          projectPath: 'test/repo2',
          branch: 'main',
          path: '',
          structure: {
            '.': {
              files: ['index.js'],
              folders: [],
            },
          },
          summary: {
            totalFiles: 1,
            totalFolders: 0,
            truncated: false,
          },
        },
        status: 200,
        provider: 'github',
      });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          { owner: 'test', repo: 'repo1', branch: 'main' },
          { owner: 'test', repo: 'repo2', branch: 'main' },
        ],
      }
    );

    expect(result.isError).toBe(false);
    const responseText = getTextContent(result.content);
    expect(responseText).toContain('README.md');
    expect(responseText).toContain('index.js');
  });

  it('should handle truncated results', async () => {
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'test/repo',
        branch: 'main',
        path: '',
        structure: {
          '.': {
            files: ['file1.ts'],
            folders: [],
          },
        },
        summary: {
          totalFiles: 1,
          totalFolders: 0,
          truncated: true,
        },
      },
      status: 200,
      provider: 'github',
    });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
          },
        ],
      }
    );

    expect(result.isError).toBe(false);
  });

  it('should handle provider exception', async () => {
    mockProvider.getRepoStructure.mockRejectedValue(new Error('Network error'));

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
          },
        ],
      }
    );

    expect(result.isError).toBe(true);
    const responseText = getTextContent(result.content);
    expect(responseText).toContain('error');
  });

  describe('Branch fallback with defaultBranch', () => {
    it('should include defaultBranch in branchFallback when API returns it', async () => {
      mockProvider.getRepoStructure.mockResolvedValue({
        data: {
          projectPath: 'facebook/react',
          branch: 'main',
          defaultBranch: 'main',
          path: '',
          structure: {
            '.': {
              files: ['README.md'],
              folders: ['src'],
            },
          },
          summary: {
            totalFiles: 1,
            totalFolders: 1,
            truncated: false,
          },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
        {
          queries: [
            {
              owner: 'facebook',
              repo: 'react',
              branch: 'nonexistent-branch',
            },
          ],
        }
      );

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('branchFallback');
      expect(responseText).toContain('nonexistent-branch');
      expect(responseText).toContain('main');
      expect(responseText).toContain('defaultBranch');
      expect(responseText).toContain(
        "WARNING: Branch 'nonexistent-branch' not found"
      );
      expect(responseText).not.toContain('⚠️ IMPORTANT');
    });
  });

  describe('Invalid branch handling (TC-9, TC-17)', () => {
    it('should return error when branch does not exist instead of silent fallback', async () => {
      // Provider returns branch "main" even though we asked for "nonexistent-branch"
      // This simulates the silent fallback behavior - the provider should instead
      // return an error or include a warning
      mockProvider.getRepoStructure.mockResolvedValue({
        data: {
          projectPath: 'facebook/react',
          branch: 'main', // silently fell back from 'nonexistent-branch'
          path: '',
          structure: {
            '.': {
              files: ['README.md'],
              folders: ['src'],
            },
          },
          summary: {
            totalFiles: 1,
            totalFolders: 1,
            truncated: false,
          },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
        {
          queries: [
            {
              owner: 'facebook',
              repo: 'react',
              branch: 'nonexistent-branch',
            },
          ],
        }
      );

      const responseText = getTextContent(result.content);
      // When branch doesn't match what was requested, user should be informed
      // Either via error OR via a warning in the response
      const branchMismatchDetected =
        responseText.includes('nonexistent-branch') ||
        (responseText.includes('branch') &&
          responseText.includes('not found')) ||
        responseText.includes('branchFallback') ||
        responseText.includes('warning');

      expect(branchMismatchDetected).toBe(true);
    });
  });

  it('should handle pagination', async () => {
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'test/repo',
        branch: 'main',
        path: '',
        structure: {
          '.': {
            files: ['file1.ts'],
            folders: [],
          },
        },
        summary: {
          totalFiles: 1,
          totalFolders: 0,
          truncated: false,
        },
        pagination: { currentPage: 1, totalPages: 5, hasMore: true },
      },
      status: 200,
      provider: 'github',
    });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
            entriesPerPage: 10,
            entryPageNumber: 1,
          },
        ],
      }
    );

    expect(result.isError).toBe(false);
  });
});
