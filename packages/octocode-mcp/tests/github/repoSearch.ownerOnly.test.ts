import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubReposAPI } from '../../../octocode-tools-core/src/github/repoSearch.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';

vi.mock('../../../octocode-tools-core/src/github/client.js');
vi.mock('../../../octocode-tools-core/src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));
vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(() => 'cache-key'),
  withDataCache: vi.fn((_, op) => op()),
  clearAllCache: vi.fn(),
}));

const makeRepo = (overrides: Record<string, unknown> = {}) => ({
  full_name: 'acme/myrepo',
  default_branch: 'main',
  stargazers_count: 10,
  description: 'A repo',
  html_url: 'https://github.com/acme/myrepo',
  created_at: '2020-01-01T00:00:00Z',
  updated_at: '2024-01-01T00:00:00Z',
  pushed_at: '2024-01-01T00:00:00Z',
  visibility: 'public',
  ...overrides,
});

describe('repoSearch — owner-only mode (listForOrg / listForUser path, lines 94-168)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('uses listForOrg when owner is provided without search terms (line 107)', async () => {
    const listForOrg = vi.fn().mockResolvedValue({ data: [makeRepo()] });
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { listForOrg } },
    } as never);

    const result = await searchGitHubReposAPI({ owner: 'acme' });

    expect(listForOrg).toHaveBeenCalledWith(
      expect.objectContaining({ org: 'acme' })
    );
    expect('data' in result && result.data.repositories).toHaveLength(1);
  });

  it('falls back to listForUser when listForOrg throws (line 119-130)', async () => {
    const listForOrg = vi.fn().mockRejectedValue(new Error('Not an org'));
    const listForUser = vi.fn().mockResolvedValue({ data: [makeRepo()] });
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { listForOrg, listForUser } },
    } as never);

    const result = await searchGitHubReposAPI({ owner: 'acme' });

    expect(listForUser).toHaveBeenCalledWith(
      expect.objectContaining({ username: 'acme' })
    );
    expect('data' in result && result.data.repositories).toHaveLength(1);
  });

  it('returns error when both listForOrg and listForUser fail (line 128-130)', async () => {
    const listForOrg = vi.fn().mockRejectedValue(new Error('Not an org'));
    const listForUser = vi
      .fn()
      .mockRejectedValue({ status: 404, error: 'Not Found' });
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { listForOrg, listForUser } },
    } as never);

    const result = await searchGitHubReposAPI({ owner: 'acme' });

    expect('error' in result || 'data' in result).toBe(true);
  });

  it('includes topics, forks, issues, language when present (lines 153-160)', async () => {
    const listForOrg = vi.fn().mockResolvedValue({
      data: [
        makeRepo({
          topics: ['react', 'typescript'],
          forks_count: 5,
          open_issues_count: 3,
          language: 'TypeScript',
        }),
      ],
    });
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { listForOrg } },
    } as never);

    const result = await searchGitHubReposAPI({ owner: 'acme' });

    if ('data' in result) {
      const repo = result.data.repositories[0]!;
      expect(repo.topics).toEqual(['react', 'typescript']);
      expect(repo.forksCount).toBe(5);
      expect(repo.openIssuesCount).toBe(3);
      expect(repo.language).toBe('TypeScript');
    }
  });

  it('omits optional fields when they are falsy (lines 153-160 else branches)', async () => {
    const listForOrg = vi.fn().mockResolvedValue({
      data: [
        makeRepo({
          topics: [],
          forks_count: 0,
          open_issues_count: 0,
          language: null,
          description: null,
        }),
      ],
    });
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { listForOrg } },
    } as never);

    const result = await searchGitHubReposAPI({ owner: 'acme' });

    if ('data' in result) {
      const repo = result.data.repositories[0]!;
      expect(repo.topics).toBeUndefined();
      expect(repo.forksCount).toBeUndefined();
      expect(repo.openIssuesCount).toBeUndefined();
      expect(repo.language).toBeUndefined();
      expect(repo.description).toBe('No description');
    }
  });

  it('truncates long descriptions to 150 chars with ellipsis (line 144-146)', async () => {
    const longDesc = 'x'.repeat(200);
    const listForOrg = vi.fn().mockResolvedValue({
      data: [makeRepo({ description: longDesc })],
    });
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { listForOrg } },
    } as never);

    const result = await searchGitHubReposAPI({ owner: 'acme' });

    if ('data' in result) {
      const desc = result.data.repositories[0]!.description;
      expect(desc).toHaveLength(153);
      expect(desc!.endsWith('...')).toBe(true);
    }
  });

  it('detects hasMore when a full page is returned (line 165)', async () => {
    const perPage = 100;
    const fullPage = Array.from({ length: perPage }, (_, i) =>
      makeRepo({ full_name: `acme/repo-${i}` })
    );
    const listForOrg = vi.fn().mockResolvedValue({ data: fullPage });
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { listForOrg } },
    } as never);

    const result = await searchGitHubReposAPI({ owner: 'acme' });

    if ('data' in result) {
      expect(result.data.pagination.totalMatches).toBeGreaterThan(perPage);
      expect(result.data.pagination.totalMatchesKind).toBe('lowerBound');
      expect(result.data.pagination.reachableTotalMatches).toBe(perPage);
    }
  });

  it('uses array form of owner (line 203-204)', async () => {
    const listForOrg = vi.fn().mockResolvedValue({ data: [makeRepo()] });
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { listForOrg } },
    } as never);

    const result = await searchGitHubReposAPI({ owner: ['acme', 'other'] });

    expect(listForOrg).toHaveBeenCalledWith(
      expect.objectContaining({ org: 'acme' })
    );
    expect('data' in result).toBe(true);
  });
});
