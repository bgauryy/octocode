import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchGitHubFileContentAPI } from '../../../octocode-tools-core/src/github/fileContent.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';
import { clearAllCache } from '../../../octocode-tools-core/src/utils/http/cache.js';

vi.mock('../../../octocode-tools-core/src/github/client.js');
vi.mock('../../../octocode-tools-core/src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('File Operations - Timestamp Optimization', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  it('should fetch timestamp by default', async () => {
    const listCommitsMock = vi.fn();
    const getContentMock = vi.fn();

    const mockOctokit = {
      rest: {
        repos: {
          getContent: getContentMock,
          listCommits: listCommitsMock,
        },
      },
    };

    vi.mocked(getOctokit).mockResolvedValue(
      mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
    );

    getContentMock.mockResolvedValue({
      data: { type: 'file', content: 'content', encoding: 'utf-8' },
    });
    listCommitsMock.mockResolvedValue({ data: [] });

    await fetchGitHubFileContentAPI({
      owner: 'test',
      repo: 'repo',
      path: 'file.txt',
    });

    expect(listCommitsMock).toHaveBeenCalled();
  });

  it('should skip timestamp fetch when requested', async () => {
    const listCommitsMock = vi.fn();
    const getContentMock = vi.fn();

    const mockOctokit = {
      rest: {
        repos: {
          getContent: getContentMock,
          listCommits: listCommitsMock,
        },
      },
    };

    vi.mocked(getOctokit).mockResolvedValue(
      mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
    );

    getContentMock.mockResolvedValue({
      data: { type: 'file', content: 'content', encoding: 'utf-8' },
    });

    await fetchGitHubFileContentAPI({
      owner: 'test',
      repo: 'repo',
      path: 'file.txt',
      noTimestamp: true,
    });

    expect(listCommitsMock).not.toHaveBeenCalled();
  });
});
