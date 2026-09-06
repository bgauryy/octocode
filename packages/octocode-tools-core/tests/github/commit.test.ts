import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(),
  resolveCacheAuthFingerprint: vi.fn(async () => 'test'),
}));

import { getOctokit } from '../../src/github/client.js';
import { fetchCommit } from '../../src/github/commit.js';
import { clearAllCache } from '../../src/utils/http/cache/management.js';

const mockGetOctokit = vi.mocked(getOctokit);

function commitResponse() {
  return {
    data: {
      sha: 'resolved-sha',
      commit: {
        message: 'feat: exact commit\nLong body',
        author: {
          name: 'Author',
          email: 'author@example.com',
          date: '2026-01-01T00:00:00Z',
        },
        committer: {
          name: 'Committer',
          email: 'committer@example.com',
          date: '2026-01-02T00:00:00Z',
        },
      },
      author: { login: 'author-login' },
      committer: { login: 'committer-login' },
      parents: [{ sha: 'parent-sha' }],
      stats: { additions: 7, deletions: 3 },
      files: [
        {
          filename: 'src/first.ts',
          status: 'modified',
          additions: 5,
          deletions: 2,
          patch: '0123456789',
        },
        {
          filename: 'src/second.ts',
          status: 'added',
          additions: 2,
          deletions: 0,
          patch: 'abcdefghij',
        },
        {
          filename: 'docs/readme.md',
          status: 'modified',
          additions: 1,
          deletions: 1,
          patch: 'ignored',
        },
      ],
    },
    status: 200,
    headers: {},
  };
}

describe('fetchCommit', () => {
  beforeEach(() => clearAllCache());
  it('uses the exact commit endpoint with the caller ref', async () => {
    const getCommit = vi.fn().mockResolvedValue(commitResponse());
    mockGetOctokit.mockResolvedValue({
      rest: { repos: { getCommit } },
    } as never);

    const result = await fetchCommit({
      owner: 'octo',
      repo: 'repo',
      ref: 'feature~2',
    });

    expect(getCommit).toHaveBeenCalledWith({
      owner: 'octo',
      repo: 'repo',
      ref: 'feature~2',
      per_page: 100,
      page: 1,
    });
    expect(result.data).toMatchObject({
      type: 'commit',
      ref: 'feature~2',
      sha: 'resolved-sha',
      messageHeadline: 'feat: exact commit',
      parents: ['parent-sha'],
      changedFiles: 3,
    });
    expect(result.data).not.toHaveProperty('files');
  });

  it('scopes, pages, and windows exact-commit diffs independently', async () => {
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: { getCommit: vi.fn().mockResolvedValue(commitResponse()) },
      },
    } as never);

    const result = await fetchCommit({
      owner: 'octo',
      repo: 'repo',
      ref: 'abc123',
      includeDiff: true,
      path: 'src/',
      filePage: 2,
      itemsPerPage: 1,
      charOffset: 2,
      charLength: 4,
    });

    expect(result.data.changedFiles).toBe(2);
    expect(result.data.filesPagination).toMatchObject({
      currentPage: 2,
      totalPages: 2,
      totalFiles: 2,
      hasMore: false,
    });
    expect(result.data.files).toEqual([
      expect.objectContaining({
        filename: 'src/second.ts',
        patch: 'cdef',
        patchPagination: expect.objectContaining({
          charOffset: 2,
          charLength: 4,
          hasMore: true,
          nextCharOffset: 6,
        }),
      }),
    ]);
  });

  it('reports the schema page limit instead of emitting file page 1001', async () => {
    const response = commitResponse();
    response.data.files = Array.from({ length: 1001 }, (_, index) => ({
      filename: `src/${index}.ts`,
      status: 'modified',
      additions: 1,
      deletions: 0,
      patch: 'x',
    }));
    mockGetOctokit.mockResolvedValue({
      rest: { repos: { getCommit: vi.fn().mockResolvedValue(response) } },
    } as never);

    const result = await fetchCommit({
      owner: 'octo',
      repo: 'repo',
      ref: 'abc123',
      includeDiff: true,
      filePage: 1000,
      itemsPerPage: 1,
    });

    expect(result.data.filesPagination).toMatchObject({
      currentPage: 1000,
      hasMore: true,
      terminalLimit: true,
      continuationUnavailable: {
        reason: 'schemaPageLimit',
        maxPage: 1000,
      },
    });
    expect(result.data.filesPagination).not.toHaveProperty('nextFilePage');
  });
});
