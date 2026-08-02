import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  listCommits: vi.fn(),
  getCommit: vi.fn(),
}));

vi.mock('../../src/github/client.js', () => ({
  OctokitWithThrottling: class {},
  getOctokit: vi.fn(async () => ({
    rest: {
      repos: { listCommits: mocks.listCommits, getCommit: mocks.getCommit },
    },
  })),
  resolveCacheAuthFingerprint: vi.fn(async () => 'test-auth'),
}));

const { fetchHistory } = await import('../../src/github/history.js');

const COMMIT_ROW = {
  sha: 'abc123',
  commit: {
    message: 'change things',
    author: { name: 'dev', email: 'dev@x.y', date: '2026-01-01T00:00:00Z' },
    committer: { name: 'dev', email: 'dev@x.y', date: '2026-01-01T00:00:00Z' },
  },
  author: { login: 'dev' },
  committer: { login: 'dev' },
};

/**
 * Regression for the benchmark-found defect: `path` pointing at a DIRECTORY
 * (written without a trailing slash, as everyone does) was classified as a
 * file, the per-commit diff lookup found no file with that exact name, and
 * `includeDiff:true` silently returned bare commits — no patches, no warning.
 */
describe('fetchHistory includeDiff with a directory path (file-mode fallback)', () => {
  beforeEach(() => {
    mocks.listCommits.mockReset();
    mocks.getCommit.mockReset();
    mocks.listCommits.mockResolvedValue({ data: [COMMIT_ROW], headers: {} });
    mocks.getCommit.mockResolvedValue({
      data: {
        files: [
          {
            filename: 'src/inner/a.ts',
            status: 'modified',
            additions: 1,
            deletions: 0,
            patch: '@@ -1 +1 @@\n-old\n+new',
          },
          {
            filename: 'other/b.ts',
            status: 'modified',
            additions: 2,
            deletions: 2,
            patch: '@@ -2 +2 @@',
          },
        ],
      },
    });
  });

  it('falls back to directory-prefix diffing instead of silently dropping the diff', async () => {
    const result = await fetchHistory({
      owner: 'o',
      repo: 'r',
      type: 'file',
      path: 'src',
      includeDiff: true,
      perPage: 5,
      page: 1,
    } as never);

    const data = result.data as {
      commits: Array<{ files?: Array<{ filename: string }>; patch?: string }>;
      warnings?: string[];
    };
    const commit = data.commits[0]!;
    const files = commit.files ?? [];
    expect(files.map(f => f.filename)).toEqual(['src/inner/a.ts']);
    expect(
      (data.warnings ?? []).some(w => w.toLowerCase().includes('directory'))
    ).toBe(true);
  });

  it('an exact file path still attaches the patch directly (no regression)', async () => {
    const result = await fetchHistory({
      owner: 'o',
      repo: 'r',
      type: 'file',
      path: 'src/inner/a.ts',
      includeDiff: true,
      perPage: 5,
      page: 1,
    } as never);

    const data = result.data as {
      commits: Array<{ patch?: string; additions?: number }>;
    };
    expect(data.commits[0]!.patch).toContain('@@ -1 +1 @@');
    expect(data.commits[0]!.additions).toBe(1);
  });

  it('warns when a commit has no file matching the path at all, instead of silence', async () => {
    const result = await fetchHistory({
      owner: 'o',
      repo: 'r',
      type: 'file',
      path: 'nonexistent/zzz.ts',
      includeDiff: true,
      perPage: 5,
      page: 1,
    } as never);

    const data = result.data as {
      commits: Array<{ patch?: string }>;
      warnings?: string[];
    };
    expect(data.commits[0]!.patch).toBeUndefined();
    expect(
      (data.warnings ?? []).some(w => w.includes('nonexistent/zzz.ts'))
    ).toBe(true);
  });
});
