import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubCodeAPI } from '../../../octocode-tools-core/src/github/codeSearch.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';
import { clearAllCache } from '../../../octocode-tools-core/src/utils/http/cache.js';
import { isGitHubAPISuccess } from '../../../octocode-tools-core/src/github/githubAPI.js';

vi.mock('../../../octocode-tools-core/src/github/client.js');
vi.mock('../../../octocode-tools-core/src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('Pagination and Hints Fixes', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  describe('Fix 3: ghSearchCode - repositoryContext with branch', () => {
    const createMockOctokit = (searchCodeMock: ReturnType<typeof vi.fn>) => ({
      rest: {
        search: {
          code: searchCodeMock,
        },
      },
    });

    const createMockResponseWithDefaultBranch = (
      totalCount: number,
      itemCount: number,
      defaultBranch: string
    ) => ({
      data: {
        total_count: totalCount,
        items: Array.from({ length: itemCount }, (_, i) => ({
          name: `file${i}.ts`,
          path: `src/file${i}.ts`,
          repository: {
            full_name: 'facebook/react',
            url: 'https://github.com/facebook/react',
            owner: { login: 'facebook' },
            default_branch: defaultBranch,
            pushed_at: '2024-01-01T00:00:00Z',
          },
          url: 'file_url',
          html_url: 'https://github.com/facebook/react/blob/main/src/file.ts',
          sha: `sha${i}`,
        })),
        incomplete_results: false,
      },
      headers: {},
    });

    it('should include branch in repositoryContext when all files are from same repo', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponseWithDefaultBranch(10, 5, 'main'));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['useState'],
        owner: 'facebook',
        repo: 'react',
        limit: 5,
      });

      expect(isGitHubAPISuccess(result)).toBe(true);
      if (!isGitHubAPISuccess(result)) return;

      expect(result.status).toBe(200);
      expect(result.data._researchContext?.repositoryContext).toBeDefined();
      expect(result.data._researchContext?.repositoryContext?.owner).toBe(
        'facebook'
      );
      expect(result.data._researchContext?.repositoryContext?.repo).toBe(
        'react'
      );
      expect(result.data._researchContext?.repositoryContext?.branch).toBe(
        'main'
      );
    });

    it('should include branch as "develop" when default_branch is "develop"', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(
          createMockResponseWithDefaultBranch(10, 5, 'develop')
        );

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        owner: 'test',
        repo: 'repo',
        limit: 5,
      });

      expect(isGitHubAPISuccess(result)).toBe(true);
      if (!isGitHubAPISuccess(result)) return;

      expect(result.data._researchContext?.repositoryContext?.branch).toBe(
        'develop'
      );
    });

    it('should have undefined branch when default_branch is not provided', async () => {
      const searchCodeMock = vi.fn().mockResolvedValue({
        data: {
          total_count: 5,
          items: Array.from({ length: 3 }, (_, i) => ({
            name: `file${i}.ts`,
            path: `src/file${i}.ts`,
            repository: {
              full_name: 'facebook/react',
              url: 'https://github.com/facebook/react',
              owner: { login: 'facebook' },
            },
            url: 'file_url',
            html_url: 'html_url',
            sha: `sha${i}`,
          })),
          incomplete_results: false,
        },
        headers: {},
      });

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        owner: 'facebook',
        repo: 'react',
        limit: 5,
      });

      expect(isGitHubAPISuccess(result)).toBe(true);
      if (!isGitHubAPISuccess(result)) return;

      expect(result.data._researchContext?.repositoryContext).toBeDefined();
      expect(
        result.data._researchContext?.repositoryContext?.branch
      ).toBeUndefined();
    });

    it('should not have repositoryContext when files are from multiple repos', async () => {
      const searchCodeMock = vi.fn().mockResolvedValue({
        data: {
          total_count: 5,
          items: [
            {
              name: 'file1.ts',
              path: 'src/file1.ts',
              repository: {
                full_name: 'facebook/react',
                url: 'url1',
                owner: { login: 'facebook' },
                default_branch: 'main',
              },
              url: 'file_url',
              html_url: 'html_url',
              sha: 'sha1',
            },
            {
              name: 'file2.ts',
              path: 'src/file2.ts',
              repository: {
                full_name: 'vercel/next.js',
                url: 'url2',
                owner: { login: 'vercel' },
                default_branch: 'canary',
              },
              url: 'file_url',
              html_url: 'html_url',
              sha: 'sha2',
            },
          ],
          incomplete_results: false,
        },
        headers: {},
      });

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['useState'],
        limit: 5,
      });

      expect(isGitHubAPISuccess(result)).toBe(true);
      if (!isGitHubAPISuccess(result)) return;

      expect(result.data._researchContext?.repositoryContext).toBeUndefined();
    });
  });
});
