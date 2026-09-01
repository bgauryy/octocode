import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestError } from 'octokit';

// ---------------------------------------------------------------------------
// Mock fetchers so we never hit real GitHub API
// ---------------------------------------------------------------------------

const mockFetchIssueByNumber = vi.fn();
const mockSearchIssues = vi.fn();
const mockListIssues = vi.fn();

vi.mock('../../src/github/issues/fetchers.js', () => ({
  fetchIssueByNumber: (...args: unknown[]) => mockFetchIssueByNumber(...args),
  searchIssues: (...args: unknown[]) => mockSearchIssues(...args),
  listIssues: (...args: unknown[]) => mockListIssues(...args),
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(),
  resolveDefaultBranch: vi.fn(async () => 'main'),
  OctokitWithThrottling: class {},
  resolveCacheAuthFingerprint: vi.fn(async () => 'anon'),
}));

const { fetchIssues } = await import('../../src/github/issues/orchestrator.js');

const EMPTY_ISSUES_RESPONSE = {
  data: {
    type: 'issues',
    owner: 'microsoft',
    repo: 'TypeScript',
    issues: [],
    totalCount: 0,
    pagination: { currentPage: 1, perPage: 30, hasMore: false },
  },
  status: 200,
};

describe('fetchIssues (orchestrator)', () => {
  beforeEach(() => {
    mockFetchIssueByNumber.mockReset();
    mockSearchIssues.mockReset();
    mockListIssues.mockReset();
  });

  it('returns an error when state is "merged"', async () => {
    const result = await fetchIssues({
      owner: 'microsoft',
      repo: 'TypeScript',
      state: 'merged',
    });
    expect('error' in result).toBe(true);
    expect((result as { error: string }).error).toContain('merged');
    expect((result as { hints?: string[] }).hints).toEqual([
      'Use ghSearchHistory with {"operation":"pullRequests","owner":"microsoft","repo":"TypeScript","state":"merged"}.',
    ]);
    expect(JSON.stringify(result)).not.toContain('type:\\"prs\\"');
  });

  it('routes to fetchIssueByNumber when issueNumber is provided', async () => {
    mockFetchIssueByNumber.mockResolvedValue(EMPTY_ISSUES_RESPONSE);

    await fetchIssues({
      owner: 'microsoft',
      repo: 'TypeScript',
      issueNumber: 42,
    });

    expect(mockFetchIssueByNumber).toHaveBeenCalledOnce();
    expect(mockSearchIssues).not.toHaveBeenCalled();
    expect(mockListIssues).not.toHaveBeenCalled();
  });

  it('routes to searchIssues when keywords are provided', async () => {
    mockSearchIssues.mockResolvedValue(EMPTY_ISSUES_RESPONSE);

    await fetchIssues({
      owner: 'microsoft',
      repo: 'TypeScript',
      keywordsToSearch: ['crash'],
    });

    expect(mockSearchIssues).toHaveBeenCalledOnce();
    expect(mockListIssues).not.toHaveBeenCalled();
    expect(mockFetchIssueByNumber).not.toHaveBeenCalled();
  });

  it('routes to listIssues when no keywords or issueNumber', async () => {
    mockListIssues.mockResolvedValue(EMPTY_ISSUES_RESPONSE);

    await fetchIssues({
      owner: 'microsoft',
      repo: 'TypeScript',
      state: 'open',
    });

    expect(mockListIssues).toHaveBeenCalledOnce();
    expect(mockSearchIssues).not.toHaveBeenCalled();
    expect(mockFetchIssueByNumber).not.toHaveBeenCalled();
  });

  it('returns an empty issues response on no-results error', async () => {
    // isNoResultsSearchError requires a RequestError with status 422 and a
    // specific error payload containing a "no results" message.
    const noResults = new RequestError('Unprocessable Entity', 422, {
      request: { method: 'GET', url: 'https://api.github.com', headers: {} },
      response: {
        status: 422,
        url: 'https://api.github.com',
        headers: {},
        data: {
          errors: [{ message: 'Repository does not exist' }],
        },
      },
    });
    mockListIssues.mockRejectedValue(noResults);

    const result = await fetchIssues({
      owner: 'microsoft',
      repo: 'TypeScript',
    });

    // Should degrade to an empty result instead of propagating
    const data = (result as { data?: { issues?: unknown[] } }).data;
    expect(Array.isArray(data?.issues)).toBe(true);
    expect(data?.issues).toHaveLength(0);
  });
});
