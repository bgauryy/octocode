import { describe, it, expect, vi } from 'vitest';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
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
  getGitHubToken: vi.fn(() => Promise.resolve('mock-token')),
  getServerConfig: vi.fn(() => ({
    version: '7.0.0',
    enableTools: [],
    disableTools: [],
    timeout: 30000,
    maxRetries: 3,
  })),
}));

import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';

describe('GitHub Search Repositories Response Structure Test', () => {
  it('should return YAML response with correct structure', async () => {
    const mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    mockProvider.searchRepos.mockResolvedValue({
      data: {
        repositories: [
          {
            id: '1',
            name: 'react',
            fullPath: 'facebook/react',
            description:
              'A declarative, efficient, and flexible JavaScript library for building user interfaces.',
            url: 'https://github.com/facebook/react',
            stars: 200000,
            forks: 40000,
            language: 'JavaScript',
            topics: ['javascript', 'react'],
            createdAt: '15/01/2024',
            updatedAt: '15/01/2024',
            pushedAt: '15/01/2024',
            defaultBranch: 'main',
            isPrivate: false,
          },
          {
            id: '2',
            name: 'next.js',
            fullPath: 'vercel/next.js',
            description: 'The React Framework for Production',
            url: 'https://github.com/vercel/next.js',
            stars: 100000,
            forks: 20000,
            language: 'JavaScript',
            topics: ['nextjs'],
            createdAt: '14/01/2024',
            updatedAt: '14/01/2024',
            pushedAt: '14/01/2024',
            defaultBranch: 'main',
            isPrivate: false,
          },
        ],
        totalCount: 2,
        pagination: { currentPage: 1, totalPages: 1, hasMore: false },
      },
      status: 200,
      provider: 'github',
    });

    const mockServer = createMockMcpServer();
    registerSearchGitHubReposTool(mockServer.server);

    const result = await mockServer.callTool(
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      {
        queries: [
          {
            id: 'repos_response_structure',
            mainResearchGoal: 'Inspect repository search output shape',
            researchGoal: 'Verify returned repository search results',
            reasoning: 'Testing response structure',
            keywordsToSearch: ['react', 'hooks'],
            limit: 2,
          },
        ],
      }
    );

    const responseText = getTextContent(result.content);

    expect(result.isError).toBe(false);
    expect(responseText).toContain('results:');
    expect(responseText).toContain('id: "repos_response_structure"');
    // hasResults is now signaled by ABSENT status — emitted only for empty/error.
    expect(responseText).not.toContain('status: "hasResults"');
    expect(responseText).toContain('repositories:');
    expect(responseText).toContain('facebook/react');
    expect(responseText).toContain('vercel/next.js');
  }, 5000);
});
