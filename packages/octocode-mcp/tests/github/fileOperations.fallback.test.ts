import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchGitHubFileContentAPI } from '@octocodeai/octocode-tools-core';
import { getOctokit, resolveDefaultBranch } from '@octocodeai/octocode-tools-core';
import { clearAllCache } from '@octocodeai/octocode-tools-core';
import { RequestError } from 'octokit';

vi.mock('@octocodeai/octocode-tools-core');
vi.mock('@octocodeai/octocode-tools-core', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('File Operations - Branch Fallback & Caching', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    vi.mocked(resolveDefaultBranch).mockResolvedValue('main');
  });

  it('should fallback to default branch using resolveDefaultBranch', async () => {
    const getContentMock = vi.fn();

    const mockOctokit = {
      rest: {
        repos: {
          get: vi.fn(),
          getContent: getContentMock,
          listCommits: vi.fn().mockResolvedValue({ data: [] }),
        },
      },
    };

    vi.mocked(getOctokit).mockResolvedValue(
      mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
    );
    vi.mocked(resolveDefaultBranch).mockResolvedValue('develop');

    const create404 = () =>
      new RequestError('Not Found', 404, {
        request: { method: 'GET', url: '', headers: {} },
        response: {
          status: 404,
          url: '',
          headers: {},
          data: {},
          retryCount: 0,
        },
      });

    getContentMock.mockRejectedValueOnce(create404());
    getContentMock.mockResolvedValueOnce({
      data: { type: 'file', content: 'base64encoded', encoding: 'base64' },
    });

    await fetchGitHubFileContentAPI({
      owner: 'test',
      repo: 'repo',
      path: 'file.txt',
      branch: 'main',
    });

    expect(getContentMock).toHaveBeenCalledTimes(2);
    expect(resolveDefaultBranch).toHaveBeenCalledWith(
      'test',
      'repo',
      undefined
    );

    getContentMock.mockClear();
    getContentMock.mockRejectedValueOnce(create404());
    getContentMock.mockResolvedValueOnce({
      data: { type: 'file', content: 'base64encoded', encoding: 'base64' },
    });

    await fetchGitHubFileContentAPI({
      owner: 'test',
      repo: 'repo',
      path: 'file2.txt',
      branch: 'main',
    });

    expect(getContentMock).toHaveBeenCalledTimes(2);
  });
});
