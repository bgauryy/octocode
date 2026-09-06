import { beforeEach, expect, it, vi } from 'vitest';
const mocks = vi.hoisted(() => ({ compare: vi.fn() }));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(async () => ({
    rest: { repos: { compareCommitsWithBasehead: mocks.compare } },
  })),
  resolveCacheAuthFingerprint: vi.fn(async () => 'fixture'),
}));
import { clearAllCache } from '../../src/utils/http/cache/management.js';
import { getMultipleGitHubHistoryItems } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import { GitHubGetHistoryItemQueryLocalSchema } from '../../src/tools/github_search_pull_requests/historySchemes.js';
beforeEach(() => {
  clearAllCache();
  vi.clearAllMocks();
});

it('executes local comparison file continuations with one upstream download', async () => {
  const filenames = ['a.ts', 'b.ts', 'c.ts'];
  mocks.compare.mockResolvedValue({
    data: {
      status: 'ahead',
      ahead_by: 1,
      behind_by: 0,
      total_commits: 1,
      commits: [
        { sha: 'sha', commit: { message: 'fixture', author: { name: 'A' } } },
      ],
      files: filenames.map(filename => ({
        filename,
        patch: filename,
        status: 'added',
      })),
    },
    headers: {},
  });
  let query: Record<string, unknown> | undefined = {
    operation: 'compare',
    owner: 'o',
    repo: 'r',
    base: 'v1',
    head: 'v2',
    includeDiff: true,
    pageSize: 1,
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
    seen.push(...data.files.map((file: { filename: string }) => file.filename));
    query = data.next?.nextFilePage?.query;
  }
  expect(seen).toEqual(filenames);
  expect(mocks.compare).toHaveBeenCalledTimes(1);
});
