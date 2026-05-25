/**
 * Tests for githubGetFileContent's structured `content-truncated` warning.
 * Mirrors the `match-value-truncated` test on the search-code side so both
 * grouped tools share the same warning vocabulary and agents can branch on
 * `kind` instead of parsing in-band markers.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';

const mockGetServerConfig = vi.hoisted(() => vi.fn());
const mockGetGitHubToken = vi.hoisted(() => vi.fn());
const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../src/serverConfig.js', () => ({
  initialize: vi.fn(),
  getServerConfig: mockGetServerConfig,
  isLoggingEnabled: vi.fn(() => false),
  getGitHubToken: mockGetGitHubToken,
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  isCloneEnabled: vi.fn(() => false),
}));

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';

type Warning =
  | {
      kind: 'content-truncated';
      groupId: string;
      path: string;
      fullContentLength: number;
      truncatedAt: number;
      recovery: string;
    }
  | { kind: string; [key: string]: unknown };

type FlatResponse = {
  results: Array<{
    id: string;
    owner: string;
    repo: string;
    files?: Array<{
      path: string;
      content: string;
      warnings?: string[];
    }>;
  }>;
  warnings?: Warning[];
};

describe('githubGetFileContent — content-truncated structured warning', () => {
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
    mockGetServerConfig.mockReturnValue({ version: '0.0.0' });
    mockGetGitHubToken.mockResolvedValue('mock-token');
    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);
    registerFetchGitHubFileContentTool(mockServer.server);
    vi.clearAllMocks();
    mockGetProvider.mockReturnValue(mockProvider);
    mockGetGitHubToken.mockResolvedValue('mock-token');
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  it('emits a structured content-truncated warning when a single file content exceeds responseCharLength', async () => {
    const huge = 'A'.repeat(50_000);
    mockProvider.getFileContent.mockResolvedValue({
      data: {
        path: 'src/giant.ts',
        content: huge,
        encoding: 'utf-8',
        size: huge.length,
        ref: 'main',
        lastModified: '2026-05-23T00:00:00.000Z',
      },
      status: 200,
      provider: 'github',
      rawResponseChars: huge.length,
    });

    const result = await mockServer.callTool(TOOL_NAMES.GITHUB_FETCH_CONTENT, {
      queries: [
        {
          owner: 'owner',
          repo: 'giant',
          path: 'src/giant.ts',
          branch: 'main',
        },
      ],
      responseCharLength: 5_000,
    });

    const data = result.structuredContent as FlatResponse;

    // Top-level structured warning is the canonical signal.
    expect(data.warnings).toBeDefined();
    const truncWarning = data.warnings!.find(
      w => w.kind === 'content-truncated'
    ) as Extract<Warning, { kind: 'content-truncated' }> | undefined;
    expect(truncWarning).toBeDefined();
    expect(truncWarning!.groupId).toBe('owner/giant');
    expect(truncWarning!.path).toBe('src/giant.ts');
    expect(truncWarning!.fullContentLength).toBe(50_000);
    expect(truncWarning!.truncatedAt).toBeLessThan(50_000);
    expect(truncWarning!.recovery).toMatch(
      /responseCharLength|startLine|endLine/i
    );

    // Inline file.warnings still pointer to the structured signal.
    const file = data.results[0]?.files?.[0];
    expect(file?.content.length).toBeLessThan(huge.length);
    expect(file?.warnings?.some(w => /warnings\[\]/i.test(w))).toBe(true);
  });

  it('emits no warnings when content fits the budget', async () => {
    mockProvider.getFileContent.mockResolvedValue({
      data: {
        path: 'src/small.ts',
        content: 'tiny',
        encoding: 'utf-8',
        size: 4,
        ref: 'main',
      },
      status: 200,
      provider: 'github',
      rawResponseChars: 4,
    });

    const result = await mockServer.callTool(TOOL_NAMES.GITHUB_FETCH_CONTENT, {
      queries: [
        {
          owner: 'owner',
          repo: 'small',
          path: 'src/small.ts',
          branch: 'main',
        },
      ],
      responseCharLength: 5_000,
    });

    const data = result.structuredContent as FlatResponse;
    expect(data.warnings).toBeUndefined();
  });
});
