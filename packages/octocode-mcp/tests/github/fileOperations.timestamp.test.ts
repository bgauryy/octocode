import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchGitHubFileContentAPI } from '@octocodeai/octocode-tools-core';
import { getOctokit } from '@octocodeai/octocode-tools-core';
import { clearAllCache } from '@octocodeai/octocode-tools-core';

vi.mock('@octocodeai/octocode-tools-core');
vi.mock('@octocodeai/octocode-tools-core', () => ({
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
