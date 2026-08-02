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

/**
 * Benchmark finding (compare-run-20260802-b, suite-1 Q5): a path-scoped walk
 * of pallets/flask src/flask/sansio/scaffold.py with until:"2023-06-15"
 * returned a bare `empty` even though the move commit exists — it was
 * AUTHORED 2023-06-10 but COMMITTED 2023-08-19, and GitHub's since/until
 * filter matches the committer date. Correct API behavior, but the bare
 * empty reads as a false absence. An empty date-filtered walk must say why.
 */
describe('fetchHistory: empty result under a since/until window', () => {
  beforeEach(() => {
    mocks.listCommits.mockReset();
    mocks.getCommit.mockReset();
    mocks.listCommits.mockResolvedValue({ data: [], headers: {} });
  });

  const base = {
    type: 'file' as const,
    owner: 'pallets',
    repo: 'flask',
    path: 'src/flask/sansio/scaffold.py',
    page: 1,
    perPage: 30,
    includeDiff: false,
  };

  it('warns that since/until filter by committer date when the window empties the walk', async () => {
    const result = await fetchHistory({ ...base, until: '2023-06-15' });
    expect('data' in result).toBe(true);
    if (!('data' in result)) return;
    expect(result.data.commits).toHaveLength(0);
    const warnings = result.data.warnings ?? [];
    expect(warnings.some(w => /committer date/i.test(w))).toBe(true);
    expect(warnings.some(w => /since\/until/i.test(w))).toBe(true);
  });

  it('also warns when both since and until are set', async () => {
    const result = await fetchHistory({
      ...base,
      since: '2023-06-01',
      until: '2023-06-30',
    });
    if (!('data' in result)) return;
    expect(
      (result.data.warnings ?? []).some(w => /committer date/i.test(w))
    ).toBe(true);
  });

  it('stays silent on an empty walk with no date filters', async () => {
    const result = await fetchHistory({ ...base });
    if (!('data' in result)) return;
    expect(result.data.commits).toHaveLength(0);
    expect(result.data.warnings ?? []).toHaveLength(0);
  });

  it('stays silent when the window matches commits', async () => {
    mocks.listCommits.mockResolvedValue({
      data: [
        {
          sha: 'a64588f',
          commit: {
            message: 'Move file to sansio',
            author: { name: 'p', email: 'p@x', date: '2023-06-10T00:00:00Z' },
            committer: {
              name: 'p',
              email: 'p@x',
              date: '2023-08-19T17:35:00Z',
            },
          },
        },
      ],
      headers: {},
    });
    const result = await fetchHistory({ ...base, since: '2023-08-01' });
    if (!('data' in result)) return;
    expect(result.data.commits).toHaveLength(1);
    expect(
      (result.data.warnings ?? []).some(w => /committer date/i.test(w))
    ).toBe(false);
  });
});
