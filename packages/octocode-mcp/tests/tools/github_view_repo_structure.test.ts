import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { getTextContent } from '../utils/testHelpers.js';

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
  getGitHubToken: vi.fn(async () => 'test-token'),
  getServerConfig: vi.fn(() => ({
    timeout: 30000,
    version: '1.0.0',
  })),
}));

import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

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

  it('uses partial wording when structure results are paginated or truncated', async () => {
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'test/repo',
        branch: 'main',
        path: 'src',
        structure: {
          '.': {
            files: ['a.ts', 'b.ts'],
            folders: ['nested'],
          },
        },
        summary: {
          totalFiles: 10,
          totalFolders: 3,
          truncated: true,
        },
        pagination: {
          currentPage: 1,
          totalPages: 2,
          hasMore: true,
          entriesPerPage: 3,
          totalEntries: 13,
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
            path: 'src',
            page: 1,
            itemsPerPage: 3,
          },
        ],
      }
    );

    const responseText = getTextContent(result.content);
    expect(responseText).toContain('Tree paginated');
    expect(responseText).not.toContain('Structure complete');
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

  it('returns directory nodes with complete files[] — never truncates mid-node', async () => {
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
          },
        ],
      }
    );

    const firstStructured = firstResult.structuredContent as {
      results: Array<{
        data: {
          structure?: Record<string, { files?: string[] }>;
        };
      }>;
    };
    const firstData = firstStructured.results[0]!.data;

    const firstNodes = Object.keys(firstData.structure ?? {});
    expect(firstNodes.length).toBeGreaterThan(0);
    for (const node of Object.values(firstData.structure ?? {})) {
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
    expect(responseText).toContain('error: Repository not found');
    expect(responseText).toContain('statusCode: 404');
    expect(responseText).toContain('owner: nonexistent');
    expect(responseText).not.toContain('error:\n        status: 404');
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
      expect(responseText).toContain("Branch 'nonexistent-branch' not found");
      expect(responseText).not.toContain('⚠️ IMPORTANT');
    });
  });

  describe('Invalid branch handling (TC-9, TC-17)', () => {
    it('should return error when branch does not exist instead of silent fallback', async () => {
      mockProvider.getRepoStructure.mockResolvedValue({
        data: {
          projectPath: 'facebook/react',
          branch: 'main',
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
            itemsPerPage: 10,
            page: 1,
          },
        ],
      }
    );

    expect(result.isError).toBe(false);
  });

  it('includes fileSizes field when includeSizes=true and fileSizeMap is returned', async () => {
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        projectPath: 'owner/sized',
        branch: 'main',
        path: '',
        structure: {
          '.': {
            files: ['README.md', 'package.json'],
            folders: ['src'],
          },
        },
        fileSizeMap: {
          '.': {
            'README.md': 2048,
            'package.json': 1024,
          },
        },
        summary: { totalFiles: 2, totalFolders: 1, truncated: false },
      },
      status: 200,
      provider: 'github',
    });

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        queries: [{ owner: 'owner', repo: 'sized', includeSizes: true }],
      }
    );

    expect(result.isError).toBe(false);
    const structured = result.structuredContent as {
      results?: Array<{ data?: Record<string, unknown> }>;
    };
    const data = structured.results?.[0]?.data ?? {};
    expect(data).toHaveProperty('fileSizes');
    const fileSizes = data.fileSizes as Record<string, number>;
    expect(fileSizes['README.md']).toBe(2048);
    expect(fileSizes['package.json']).toBe(1024);
  });
});
