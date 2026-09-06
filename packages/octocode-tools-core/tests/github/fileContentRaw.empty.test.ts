import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fetchRawGitHubFileContent } from '../../src/github/fileContentRaw/fetch.js';
import { getOctokit } from '../../src/github/client.js';

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(),
  resolveDefaultBranch: vi.fn(async () => 'main'),
  OctokitWithThrottling: class {},
}));

describe('empty GitHub file content', () => {
  const getContent = vi.fn();
  const getBlob = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { getContent }, git: { getBlob } },
    } as unknown as Awaited<ReturnType<typeof getOctokit>>);
  });

  it('returns a valid zero-byte contents response without a blob request', async () => {
    getContent.mockResolvedValue({
      data: {
        type: 'file',
        content: '',
        encoding: 'base64',
        size: 0,
        sha: 'empty',
      },
      headers: { etag: '"empty"' },
    });
    const result = await fetchRawGitHubFileContent({
      owner: 'o',
      repo: 'r',
      path: 'empty.txt',
      branch: 'main',
    });
    expect(result).toMatchObject({
      status: 200,
      data: { rawContent: '', resolvedRef: 'main' },
      etag: '"empty"',
    });
    expect(getBlob).not.toHaveBeenCalled();
  });

  it('still retrieves nonempty files through the blob fallback', async () => {
    getContent.mockResolvedValue({
      data: { type: 'file', content: '', size: 10, sha: 'blob' },
    });
    getBlob.mockResolvedValue({
      data: {
        content: Buffer.from('actual').toString('base64'),
        encoding: 'base64',
      },
    });
    const result = await fetchRawGitHubFileContent({
      owner: 'o',
      repo: 'r',
      path: 'large.txt',
      branch: 'main',
    });
    expect(result).toMatchObject({
      status: 200,
      data: { rawContent: 'actual' },
    });
    expect(getBlob).toHaveBeenCalledOnce();
  });
});
