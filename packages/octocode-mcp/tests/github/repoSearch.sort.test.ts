import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubReposAPI } from '../../src/github/repoSearch.js';
import { getOctokit } from '../../src/github/client.js';
import { clearAllCache } from '../../src/utils/http/cache.js';

vi.mock('../../src/github/client.js');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('Repo Search - Sorting', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  it('should respect API sort order and NOT force re-sort', async () => {
    const searchReposMock = vi.fn();

    const mockOctokit = {
      rest: {
        search: {
          repos: searchReposMock,
        },
      },
    };

    vi.mocked(getOctokit).mockResolvedValue(
      mockOctokit as unknown as ReturnType<typeof getOctokit>
    );

    searchReposMock.mockResolvedValue({
      data: {
        items: [
          {
            full_name: 'user/repoA',
            stargazers_count: 5,
            updated_at: '2023-01-01T00:00:00Z',
            owner: { login: 'user' },
          },
          {
            full_name: 'user/repoB',
            stargazers_count: 10,
            updated_at: '2023-01-01T00:00:00Z',
            owner: { login: 'user' },
          },
        ],
        total_count: 2,
      },
      headers: {},
    });

    const result = await searchGitHubReposAPI({
      keywordsToSearch: ['test'],
      sort: 'stars',
    });

    if ('data' in result) {
      expect(result.data).toBeDefined();
      expect(result.data.repositories.length).toBe(2);
      expect(result.data.repositories?.[0]?.repo).toBe('repoA');
      expect(result.data.repositories?.[1]?.repo).toBe('repoB');
    }
  });

  it.each([
    ['stars', 'stars'],
    ['forks', 'forks'],
    ['updated', 'updated'],
  ] as const)(
    'forwards GitHub-supported sort "%s" to the API',
    async (sort, expected) => {
      const searchReposMock = vi.fn().mockResolvedValue({
        data: { items: [], total_count: 0 },
        headers: {},
      });
      vi.mocked(getOctokit).mockResolvedValue({
        rest: { search: { repos: searchReposMock } },
      } as unknown as ReturnType<typeof getOctokit>);

      await searchGitHubReposAPI({ keywordsToSearch: ['test'], sort });

      expect(searchReposMock).toHaveBeenCalledWith(
        expect.objectContaining({ sort: expected })
      );
    }
  );

  it.each(['created', 'best-match'] as const)(
    'does NOT forward client-only sort "%s" to the API (GitHub would ignore/reject it)',
    async sort => {
      const searchReposMock = vi.fn().mockResolvedValue({
        data: { items: [], total_count: 0 },
        headers: {},
      });
      vi.mocked(getOctokit).mockResolvedValue({
        rest: { search: { repos: searchReposMock } },
      } as unknown as ReturnType<typeof getOctokit>);

      await searchGitHubReposAPI({
        keywordsToSearch: ['test'],
        sort: sort as 'created',
      });

      const passed = searchReposMock.mock.calls[0]?.[0] ?? {};
      expect(passed).not.toHaveProperty('sort');
    }
  );
});
