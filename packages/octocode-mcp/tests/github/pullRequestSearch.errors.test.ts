import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchGitHubPullRequestByNumberAPI } from '../../../octocode-tools-core/src/github/prByNumber.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';
import { clearAllCache } from '../../../octocode-tools-core/src/utils/http/cache.js';

vi.mock('../../../octocode-tools-core/src/github/client.js');
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
      mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
      content: { changedFiles: true, patches: { mode: 'all' } },
    });

    expect(result.pull_requests?.length).toBe(1);
    const pr = result.pull_requests?.[0] as
      { _sanitization_warnings?: string[] } | undefined;

    expect(pr?._sanitization_warnings).toBeDefined();
    expect(
      pr?._sanitization_warnings?.some((w: string) =>
        w.includes('API Rate Limit Exceeded')
      )
    ).toBe(true);
  });
});
