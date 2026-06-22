import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';

const mockGetServerConfig = vi.hoisted(() => vi.fn());
const mockGetGitHubToken = vi.hoisted(() => vi.fn());
const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  initialize: vi.fn(),
  getServerConfig: mockGetServerConfig,
  getGitHubToken: mockGetGitHubToken,
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  isCloneEnabled: vi.fn(() => false),
}));

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

type Warning = { kind: string; [key: string]: unknown };

type FlatResponse = {
  results: Array<{
    id: string;
    owner: string;
    repo: string;
    files?: Array<{
      path: string;
      content: string;
      warnings?: string[];
      matchNotFound?: boolean;
      searchedFor?: string;
    }>;
  }>;
  warnings?: Warning[];
  hints?: string[];
};

describe('ghGetFileContent — content-truncated structured warning', () => {
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

  it('returns large file content without truncation markers', async () => {
    const huge = Array.from(
      { length: 3_000 },
      (_, i) => `export const value${i} = ${i};`
    ).join('\n');
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
    });

    const first = result.structuredContent as FlatResponse;

    expect(first.warnings).toBeUndefined();
    const file = first.results[0]?.files?.[0];
    expect(file?.content).not.toMatch(/\[(truncated|clipped)\]/i);
  });

  it('preserves match-not-found metadata without generic empty-file hints', async () => {
    mockProvider.getFileContent.mockResolvedValue({
      data: {
        path: 'src/small.ts',
        content: '',
        encoding: 'utf-8',
        size: 0,
        ref: 'main',
        totalLines: 10,
        matchNotFound: true,
        searchedFor: 'missingAnchor',
        warnings: [
          'No matches for "missingAnchor" in file (10 lines scanned). Try matchStringIsRegex=true, a different anchor, or fullContent=true.',
        ],
      },
      status: 200,
      provider: 'github',
      rawResponseChars: 0,
    });

    const result = await mockServer.callTool(TOOL_NAMES.GITHUB_FETCH_CONTENT, {
      queries: [
        {
          owner: 'owner',
          repo: 'small',
          path: 'src/small.ts',
          matchString: 'missingAnchor',
        },
      ],
    });

    const data = result.structuredContent as FlatResponse;
    const file = data.results[0]?.files?.[0];
    expect(result.isError).toBe(false);
    expect(file).toMatchObject({
      path: 'src/small.ts',
      content: '',
      matchNotFound: true,
      searchedFor: 'missingAnchor',
    });
    expect(file?.warnings?.[0]).toContain('No matches for "missingAnchor"');
    expect(data.hints?.join('\n') ?? '').not.toContain('may be an empty file');
    expect(data.results[0]).toBeDefined();
  });

  it('exposes fileSize (bytes) from provider size field', async () => {
    mockProvider.getFileContent.mockResolvedValue({
      data: {
        path: 'src/measured.ts',
        content: 'const x = 1;',
        encoding: 'utf-8',
        size: 98765,
        ref: 'main',
      },
      status: 200,
      provider: 'github',
      rawResponseChars: 13,
    });

    const result = await mockServer.callTool(TOOL_NAMES.GITHUB_FETCH_CONTENT, {
      queries: [
        {
          owner: 'owner',
          repo: 'measured',
          path: 'src/measured.ts',
          branch: 'main',
        },
      ],
    });

    const data = result.structuredContent as FlatResponse;
    const file = data.results[0]?.files?.[0] as Record<string, unknown>;
    expect(result.isError).toBe(false);
    expect(file?.fileSize).toBe(98765);
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
