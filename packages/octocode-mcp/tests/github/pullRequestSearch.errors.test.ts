import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchGitHubPullRequestByNumberAPI } from '../../src/github/prByNumber.js';
import { getOctokit } from '../../src/github/client.js';
import { clearAllCache } from '../../src/utils/http/cache.js';

vi.mock('../../src/github/client.js');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('PR Search - Error Propagation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  it('should include warnings when sub-resource fetching fails', async () => {
    const getPRMock = vi.fn();
    const listFilesMock = vi.fn();

    const mockOctokit = {
      rest: {
        pulls: {
          get: getPRMock,
          listFiles: listFilesMock,
        },
      },
    };

    vi.mocked(getOctokit).mockResolvedValue(
      mockOctokit as unknown as ReturnType<typeof getOctokit>
    );

    getPRMock.mockResolvedValue({
      data: {
        number: 123,
        title: 'Test PR',
        state: 'open',
        user: { login: 'author' },
        created_at: '2023-01-01T00:00:00Z',
        updated_at: '2023-01-01T00:00:00Z',
        html_url: 'url',
        head: { ref: 'feature', sha: 'sha1' },
        base: { ref: 'main', sha: 'sha2' },
      },
    });

    listFilesMock.mockRejectedValue(new Error('API Rate Limit Exceeded'));

    const result = await fetchGitHubPullRequestByNumberAPI({
      owner: 'test',
      repo: 'repo',
      prNumber: 123,
      type: 'fullContent',
    });

    expect(result.pull_requests?.length).toBe(1);
    const pr = result.pull_requests?.[0] as
      | { _sanitization_warnings?: string[] }
      | undefined;

    expect(pr?._sanitization_warnings).toBeDefined();
    expect(
      pr?._sanitization_warnings?.some((w: string) =>
        w.includes('API Rate Limit Exceeded')
      )
    ).toBe(true);
  });
});
