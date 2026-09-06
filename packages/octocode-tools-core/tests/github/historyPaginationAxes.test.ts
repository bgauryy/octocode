import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ getCommit: vi.fn(), compare: vi.fn() }));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(async () => ({
    rest: {
      repos: {
        getCommit: mocks.getCommit,
        compareCommitsWithBasehead: mocks.compare,
      },
    },
  })),
  resolveCacheAuthFingerprint: vi.fn(async () => 'fixture'),
}));
vi.mock('../../src/utils/http/cache/dataCache.js', () => ({
  withDataCache: vi.fn(async (_key, fetcher) => fetcher()),
}));
import { getMultipleGitHubHistoryItems } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import { GitHubGetHistoryItemQueryLocalSchema } from '../../src/tools/github_search_pull_requests/historySchemes.js';

const files = [
  { filename: 'first.ts', patch: '0123456789', status: 'modified' },
  { filename: 'second.ts', patch: 'abcdefghij', status: 'modified' },
  { filename: 'third.ts', patch: 'KLMNOPQRST', status: 'modified' },
];
const commit = {
  sha: 'pinned-sha',
  commit: { message: 'fixture' },
  parents: [],
  files,
};

async function execute(query: Record<string, unknown>) {
  expect(GitHubGetHistoryItemQueryLocalSchema.safeParse(query).success).toBe(
    true
  );
  const result = await getMultipleGitHubHistoryItems({ queries: [query] });
  return (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
}

describe('history pagination axes remain lossless when traversed together', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getCommit.mockResolvedValue({ data: commit, headers: {} });
    mocks.compare.mockResolvedValue({
      data: {
        status: 'ahead',
        ahead_by: 1,
        behind_by: 0,
        total_commits: 1,
        commits: [commit],
        files,
      },
      headers: {},
    });
  });
  it.each(['commit', 'compare'] as const)(
    '%s reports an omitted provider patch as terminal without guessing its cause',
    async operation => {
      const omittedFiles = [{ filename: 'omitted.dat', status: 'modified' }];
      mocks.getCommit.mockResolvedValue({
        data: { ...commit, files: omittedFiles },
        headers: {},
      });
      mocks.compare.mockResolvedValue({
        data: {
          status: 'ahead',
          ahead_by: 1,
          behind_by: 0,
          total_commits: 1,
          commits: [commit],
          files: omittedFiles,
        },
        headers: {},
      });
      const query = {
        operation,
        owner: 'o',
        repo: 'r',
        ...(operation === 'commit'
          ? { ref: 'main' }
          : { base: 'v1', head: 'v2' }),
        includeDiff: true,
      };
      const result = await getMultipleGitHubHistoryItems({
        queries: [GitHubGetHistoryItemQueryLocalSchema.parse(query)],
      });
      const row = (
        result.structuredContent as { results: Array<{ meta: any; data: any }> }
      ).results[0]!;
      expect(row.data.files[0]).toMatchObject({
        patchUnavailable: { reason: 'providerOmittedPatch' },
        isPartial: true,
        terminalLimit: true,
      });
      expect(row.meta.diagnostics).toMatchObject({
        partial: true,
        codes: expect.arrayContaining(['terminalLimitReached']),
      });
      expect(row.data.next?.continuePatch).toBeUndefined();
    }
  );

  it.each(['commit', 'compare'] as const)(
    '%s reconstructs every patch after finishing the prior file window',
    async operation => {
      let query: Record<string, unknown> | undefined = {
        operation,
        owner: 'o',
        repo: 'r',
        ...(operation === 'commit'
          ? { ref: 'main' }
          : { base: 'v1', head: 'v2' }),
        includeDiff: true,
        pageSize: 1,
        charLength: 4,
      };
      const reconstructed: Record<string, string> = {};
      for (let budget = 0; query && budget < 12; budget++) {
        const data = await execute(query);
        for (const file of data.files) {
          reconstructed[file.filename] =
            (reconstructed[file.filename] ?? '') + file.patch;
        }
        query =
          data.next?.continuePatch?.query ?? data.next?.nextFilePage?.query;
      }
      expect(query).toBeUndefined();
      expect(reconstructed).toEqual(
        Object.fromEntries(files.map(file => [file.filename, file.patch]))
      );
    }
  );

  it('pins commit file and patch continuations to the resolved immutable SHA', async () => {
    const data = await execute({
      operation: 'commit',
      owner: 'o',
      repo: 'r',
      ref: 'main',
      includeDiff: true,
      pageSize: 1,
      charLength: 4,
    });
    expect(data.next.continuePatch.query.ref).toBe('pinned-sha');
    expect(data.next.nextFilePage.query.ref).toBe('pinned-sha');
  });
  it('pins every compare continuation to the provider immutable permalink without extra API calls', async () => {
    const base = `o:${'a'.repeat(40)}`;
    const head = `fork:${'b'.repeat(40)}`;
    mocks.compare.mockResolvedValue({
      data: {
        status: 'ahead',
        ahead_by: 2,
        behind_by: 0,
        total_commits: 2,
        commits: [commit],
        files,
        permalink_url: `https://github.com/o/r/compare/${base}...${head}`,
      },
      headers: { link: '<https://api.github.com/fixture?page=2>; rel="next"' },
    });
    const data = await execute({
      operation: 'compare',
      owner: 'o',
      repo: 'r',
      base: 'main',
      head: 'fork:topic',
      includeDiff: true,
      pageSize: 1,
      charLength: 4,
    });
    for (const call of Object.values(data.next) as Array<{
      query: Record<string, unknown>;
    }>) {
      expect(call.query).toMatchObject({ base, head });
      expect(
        GitHubGetHistoryItemQueryLocalSchema.safeParse(call.query).success
      ).toBe(true);
    }
    await execute(data.next.nextPage.query);
    expect(mocks.compare.mock.calls[1]![0].basehead).toBe(`${base}...${head}`);
    expect(mocks.getCommit).not.toHaveBeenCalled();
  });
  it('expands actual abbreviated provider permalinks when the head is beyond the returned commit page', async () => {
    const base = 'a'.repeat(40),
      head = 'b'.repeat(40),
      first = 'c'.repeat(40);
    mocks.compare.mockResolvedValue({
      data: {
        status: 'ahead',
        ahead_by: 2,
        behind_by: 0,
        total_commits: 2,
        base_commit: { sha: base },
        commits: [{ ...commit, sha: first }],
        files,
        permalink_url: `https://github.com/o/r/compare/o:${base.slice(0, 7)}...fork:${head.slice(0, 7)}`,
      },
      headers: { link: '<https://api.github.com/fixture?page=2>; rel="next"' },
    });
    mocks.getCommit.mockResolvedValue({
      data: { ...commit, sha: head },
      headers: {},
    });
    const data = await execute({
      operation: 'compare',
      owner: 'o',
      repo: 'r',
      base: 'main',
      head: 'fork:topic',
      includeDiff: true,
      pageSize: 1,
      charLength: 4,
    });
    for (const call of Object.values(data.next) as Array<{
      query: Record<string, unknown>;
    }>) {
      expect(call.query).toMatchObject({
        base: `o:${base}`,
        head: `fork:${head}`,
      });
    }
    expect(mocks.getCommit).toHaveBeenCalledExactlyOnceWith({
      owner: 'o',
      repo: 'r',
      ref: head.slice(0, 7),
    });
    await execute(data.next.nextPage.query);
    expect(mocks.getCommit).toHaveBeenCalledTimes(1);
  });
});
