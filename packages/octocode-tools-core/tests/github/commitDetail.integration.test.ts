import { beforeEach, expect, it, vi } from 'vitest';
const mocks = vi.hoisted(() => ({ getCommit: vi.fn(), identity: 'one' }));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(async () => ({
    rest: { repos: { getCommit: mocks.getCommit } },
  })),
  resolveCacheAuthFingerprint: vi.fn(async () => mocks.identity),
}));
import { clearAllCache } from '../../src/utils/http/cache/management.js';
import { getMultipleGitHubHistoryItems } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import { GitHubGetHistoryItemQueryLocalSchema } from '../../src/tools/github_search_pull_requests/historySchemes.js';
import { fetchCommit } from '../../src/github/commit.js';

beforeEach(() => {
  clearAllCache();
  vi.clearAllMocks();
  mocks.identity = 'one';
});

it('replays all public file continuations without repeating upstream downloads', async () => {
  const names = ['a.ts', 'b.ts', 'c.ts', 'd.ts', 'e.ts'];
  mocks.getCommit.mockImplementation(async ({ page }) => ({
    data: {
      sha: 'pinned',
      commit: { message: 'fixture', author: { name: 'A', email: '' } },
      parents: [],
      files: names
        .slice(page === 1 ? 0 : 3, page === 1 ? 3 : 5)
        .map(filename => ({ filename, patch: filename, status: 'added' })),
    },
    headers:
      page === 1
        ? { link: '<https://api.github.com/fixture?page=2>; rel="next"' }
        : {},
  }));
  let query: Record<string, unknown> | undefined = {
    operation: 'commit',
    owner: 'o',
    repo: 'r',
    ref: 'pinned',
    includeDiff: true,
    pageSize: 2,
  };
  const seen: string[] = [];
  for (let budget = 0; query && budget < 4; budget++) {
    expect(GitHubGetHistoryItemQueryLocalSchema.safeParse(query).success).toBe(
      true
    );
    const result = await getMultipleGitHubHistoryItems({
      queries: [query],
    } as never);
    const data = (
      result.structuredContent as {
        results: Array<{ data: Record<string, any> }>;
      }
    ).results[0]!.data;
    seen.push(...data.files.map((f: { filename: string }) => f.filename));
    query = data.next?.nextFilePage?.query;
  }
  expect(seen).toEqual(names);
  expect(mocks.getCommit).toHaveBeenCalledTimes(2);
});

it('keeps cached commit detail isolated by API/auth identity', async () => {
  mocks.getCommit.mockImplementation(async () => ({
    data: {
      sha: 'pinned',
      commit: { message: mocks.identity, author: { name: 'A', email: '' } },
      parents: [],
      files: [],
    },
    headers: {},
  }));
  const params = { owner: 'o', repo: 'r', ref: 'pinned' };
  expect((await fetchCommit(params)).data?.message).toBe('one');
  mocks.identity = 'two';
  expect((await fetchCommit(params)).data?.message).toBe('two');
  expect(mocks.getCommit).toHaveBeenCalledTimes(2);
});
