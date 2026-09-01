import { describe, expect, it, vi } from 'vitest';

// Mock getOctokit so compareRefs never makes real network calls
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(),
  resolveDefaultBranch: vi.fn(async () => 'main'),
  OctokitWithThrottling: class {},
  resolveCacheAuthFingerprint: vi.fn(async () => 'anon'),
}));

import { getOctokit } from '../../src/github/client.js';
import { compareRefs } from '../../src/github/compare.js';

const mockGetOctokit = vi.mocked(getOctokit);

function makeCompareResponse(overrides: Record<string, unknown> = {}) {
  return {
    data: {
      status: 'ahead',
      ahead_by: 3,
      behind_by: 0,
      total_commits: 3,
      commits: [
        {
          sha: 'abc123',
          commit: {
            message: 'feat: add thing\nLonger body',
            author: { name: 'Alice', date: '2024-01-01T00:00:00Z' },
          },
          author: { login: 'alice' },
        },
      ],
      files: [
        {
          filename: 'src/index.ts',
          status: 'modified',
          additions: 5,
          deletions: 2,
          patch: '@@ -1 +1 @@ change',
        },
      ],
      ...overrides,
    },
    status: 200,
    headers: {},
  };
}

describe('compareRefs', () => {
  it('returns ahead/behind counts and commit list', async () => {
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi
            .fn()
            .mockResolvedValue(makeCompareResponse()),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main',
      head: 'feat/branch',
    });

    expect(result.status).toBe(200);
    const d = result.data!;
    expect(d.type).toBe('compare');
    expect(d.aheadBy).toBe(3);
    expect(d.behindBy).toBe(0);
    expect(d.totalCommits).toBe(3);
    expect(d.commits).toHaveLength(1);
    expect(d.commits[0]!.sha).toBe('abc123');
    expect(d.commits[0]!.messageHeadline).toBe('feat: add thing');
  });

  it('returns changedFiles count when includeDiff is false (default)', async () => {
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi
            .fn()
            .mockResolvedValue(makeCompareResponse()),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main',
      head: 'feat/branch',
    });

    expect(result.data!.changedFiles).toBe(1);
    expect(result.data!.files).toBeUndefined();
  });

  it('returns full file diffs when includeDiff is true', async () => {
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi
            .fn()
            .mockResolvedValue(makeCompareResponse()),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main',
      head: 'feat/branch',
      includeDiff: true,
    });

    expect(result.data!.files).toBeDefined();
    expect(result.data!.files![0]!.filename).toBe('src/index.ts');
    expect(result.data!.files![0]!.patch).toBeDefined();
    expect(result.data!.changedFiles).toBeUndefined();
    // Diffs are now shaped like the history-walk path, exposing file pagination.
    expect(result.data!.filesPagination).toBeDefined();
    expect(result.data!.filesPagination!.totalFiles).toBe(1);
  });

  it('reports the schema page limit instead of emitting compare file page 1001', async () => {
    const files = Array.from({ length: 1001 }, (_, index) => ({
      filename: `src/${index}.ts`,
      status: 'modified',
      additions: 1,
      deletions: 0,
      patch: 'x',
    }));
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi
            .fn()
            .mockResolvedValue(makeCompareResponse({ files })),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main',
      head: 'feat/branch',
      includeDiff: true,
      filePage: 1000,
      itemsPerPage: 1,
    });

    expect(result.data!.filesPagination).toMatchObject({
      currentPage: 1000,
      hasMore: true,
      terminalLimit: true,
      continuationUnavailable: {
        reason: 'schemaPageLimit',
        maxPage: 1000,
      },
    });
    expect(result.data!.filesPagination).not.toHaveProperty('nextFilePage');
  });

  it('scopes the diff to a single file when path is given', async () => {
    const resp = makeCompareResponse({
      files: [
        {
          filename: 'src/index.ts',
          status: 'modified',
          additions: 5,
          deletions: 2,
          patch: '@@ -1 +1 @@ change',
        },
        {
          filename: 'README.md',
          status: 'modified',
          additions: 1,
          deletions: 0,
          patch: '@@ -1 +1 @@ doc',
        },
      ],
    });
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi.fn().mockResolvedValue(resp),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main^',
      head: 'main',
      includeDiff: true,
      path: 'src/index.ts',
    });

    expect(result.data!.files).toHaveLength(1);
    expect(result.data!.files![0]!.filename).toBe('src/index.ts');
    expect(result.data!.filesPagination!.totalFiles).toBe(1);
  });

  it('windows a scoped patch with charOffset/charLength', async () => {
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi
            .fn()
            .mockResolvedValue(makeCompareResponse()),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main^',
      head: 'main',
      includeDiff: true,
      charOffset: 0,
      charLength: 5,
    });

    const file = result.data!.files![0]! as {
      patch: string;
      patchPagination?: { totalChars: number; hasMore: boolean };
    };
    expect(file.patch.length).toBe(5);
    expect(file.patchPagination!.hasMore).toBe(true);
  });

  it('uses author.name for commit author when available', async () => {
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi
            .fn()
            .mockResolvedValue(makeCompareResponse()),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main',
      head: 'feat/branch',
    });
    expect(result.data!.commits[0]!.author).toBe('Alice');
  });

  it('falls back to GitHub login when commit author.name is missing', async () => {
    const resp = makeCompareResponse();
    resp.data.commits[0].commit.author.name = undefined as never;

    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi.fn().mockResolvedValue(resp),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main',
      head: 'feat/branch',
    });
    expect(result.data!.commits[0]!.author).toBe('alice');
  });

  it('returns an error on API failure', async () => {
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi
            .fn()
            .mockRejectedValue(
              Object.assign(new Error('Not Found'), { status: 404 })
            ),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main',
      head: 'nonexistent',
    });

    expect(result.error).toBeDefined();
  });

  it('handles missing files array gracefully', async () => {
    const resp = makeCompareResponse({ files: undefined });

    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          compareCommitsWithBasehead: vi.fn().mockResolvedValue(resp),
        },
      },
    } as never);

    const result = await compareRefs({
      owner: 'facebook',
      repo: 'react',
      base: 'main',
      head: 'feat/branch',
    });

    // files defaults to [] so changedFiles = 0
    expect(result.data!.changedFiles).toBe(0);
  });
});
