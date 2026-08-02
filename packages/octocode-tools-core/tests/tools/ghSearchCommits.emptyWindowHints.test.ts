import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  fetchHistory: vi.fn(),
}));

vi.mock('../../src/github/history.js', () => ({
  fetchHistory: mocks.fetchHistory,
}));

const { handleCommitsMode } = await import(
  '../../src/tools/github_search_pull_requests/execution/commitsMode.js'
);

const query = {} as never;

/**
 * Benchmark finding (compare-run-20260802-b): an empty date-windowed history
 * walk surfaced as a bare `status:"empty"` — fetchHistory's explanatory
 * warnings are stripped by the no-warnings egress contract. On EMPTY results
 * the explanation is the payload, so it must ride the `hints` channel.
 */
describe('ghSearchCommits commits mode: empty result under a date window', () => {
  beforeEach(() => mocks.fetchHistory.mockReset());

  const emptyHistory = (warnings?: string[]) => ({
    status: 200,
    data: {
      type: 'file',
      owner: 'pallets',
      repo: 'flask',
      path: 'src/flask/sansio/scaffold.py',
      commits: [],
      pagination: { currentPage: 1, perPage: 30, hasMore: false },
      ...(warnings ? { warnings } : {}),
    },
  });

  it('surfaces fetchHistory warnings as hints when the walk is empty', async () => {
    mocks.fetchHistory.mockResolvedValue(
      emptyHistory([
        'no commits matched the since/until window — GitHub filters by committer date …',
      ])
    );
    const result = (await handleCommitsMode(
      query,
      {
        owner: 'pallets',
        repo: 'flask',
        path: 'src/flask/sansio/scaffold.py',
        until: '2023-06-15',
      } as never,
      undefined
    )) as Record<string, unknown>;
    expect(result.status).toBe('empty');
    const hints = result.hints as string[] | undefined;
    expect(hints?.some(h => /committer date/i.test(h))).toBe(true);
  });

  it('adds no hints on an empty walk without warnings', async () => {
    mocks.fetchHistory.mockResolvedValue(emptyHistory());
    const result = (await handleCommitsMode(
      query,
      { owner: 'o', repo: 'r', path: 'p' } as never,
      undefined
    )) as Record<string, unknown>;
    expect(result.status).toBe('empty');
    expect(result.hints).toBeUndefined();
  });

  it('keeps hints off non-empty results', async () => {
    const withCommit = emptyHistory(['some internal warning']);
    withCommit.data.commits = [
      {
        sha: 'abc',
        date: '2023-08-19T00:00:00Z',
        messageHeadline: 'Move file to sansio',
        author: { name: 'p', email: 'p@x' },
      },
    ] as never;
    mocks.fetchHistory.mockResolvedValue(withCommit);
    const result = (await handleCommitsMode(
      query,
      { owner: 'o', repo: 'r', path: 'p', since: '2023-08-01' } as never,
      undefined
    )) as Record<string, unknown>;
    expect(result.status).toBeUndefined();
    expect(result.hints).toBeUndefined();
  });
});
