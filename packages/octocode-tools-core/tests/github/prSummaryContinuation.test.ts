import { beforeEach, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ listCommits: vi.fn(), getCommit: vi.fn() }));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      pulls: {
        get: async () => ({
          data: {
            number: 91,
            title: 'Summary pages',
            html_url: '',
            user: { login: 'a' },
          },
        }),
        listCommits: mocks.listCommits,
      },
      repos: { getCommit: mocks.getCommit },
    },
  }),
  resolveCacheAuthFingerprint: async () => 'pr-summary-continuation',
  OctokitWithThrottling: class {},
}));
vi.mock('../../src/providers/factory.js', () => ({
  getProvider: () => ({
    type: 'github',
    capabilities: {},
    searchPullRequests: async (query: never) =>
      (
        await import('../../src/providers/github/githubPullRequests.js')
      ).searchPullRequests(query),
  }),
}));
import { clearAllCache } from '../../src/utils/http/cache/management.js';
import { getMultipleGitHubHistoryItems } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import { GitHubGetHistoryItemQueryLocalSchema } from '../../src/tools/github_search_pull_requests/historySchemes.js';

beforeEach(() => {
  clearAllCache();
  vi.clearAllMocks();
});

it('executes every public summary continuation without detail requests', async () => {
  const names = ['a', 'b', 'c', 'd', 'e'];
  mocks.listCommits.mockResolvedValue({
    data: names.map(sha => ({ sha, commit: { message: sha, author: null } })),
    headers: {},
  });
  let query: Record<string, unknown> | undefined = {
    operation: 'pullRequest',
    owner: 'o',
    repo: 'r',
    number: 91,
    content: { commits: {} },
    pageSize: 2,
  };
  const seen: string[] = [];
  let pages = 0;
  while (query && pages < 4) {
    const parsed = GitHubGetHistoryItemQueryLocalSchema.parse(query);
    const response = await getMultipleGitHubHistoryItems({
      queries: [parsed],
    } as never);
    const data = (
      response.structuredContent as {
        results: Array<{ data: Record<string, any> }>;
      }
    ).results[0]!.data;
    seen.push(
      ...(data.pullRequests[0].commits ?? []).map(
        (commit: { sha: string }) => commit.sha
      )
    );
    const next = Object.values(data.next ?? {}).find(
      (call: any) => call.query?.commitPage
    ) as { query: Record<string, unknown> } | undefined;
    query = next?.query;
    pages++;
  }
  expect(seen).toEqual(names);
  expect(pages).toBe(3);
  expect(mocks.getCommit).not.toHaveBeenCalled();
});

it('executes the final-envelope commit-summary discovery call', async () => {
  mocks.listCommits.mockResolvedValue({
    data: [{ sha: 'discovered', commit: { message: 'Summary', author: null } }],
    headers: {},
  });
  const initial = await getMultipleGitHubHistoryItems({
    queries: [{ operation: 'pullRequest', owner: 'o', repo: 'r', number: 91 }],
  } as never);
  const data = (
    initial.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
  const call = data.pullRequests[0].next.getCommits;
  expect(call.tool).toBe('ghGetHistoryItem');
  expect(call.query.content).toEqual({ commits: {} });
  expect(mocks.listCommits).not.toHaveBeenCalled();
  const query = GitHubGetHistoryItemQueryLocalSchema.parse(call.query);
  const result = await getMultipleGitHubHistoryItems({
    queries: [query],
  } as never);
  const recovered = (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
  expect(
    recovered.pullRequests[0].commits.map(
      (commit: { sha: string }) => commit.sha
    )
  ).toEqual(['discovered']);
  expect(mocks.listCommits).toHaveBeenCalledTimes(1);
  expect(mocks.getCommit).not.toHaveBeenCalled();
});
it('enriches every commit page with its own files without caching only the first page', async () => {
  mocks.listCommits.mockResolvedValue({
    data: ['a', 'b', 'c'].map(sha => ({
      sha,
      commit: { message: sha, author: null },
    })),
    headers: {},
  });
  mocks.getCommit.mockImplementation(async ({ ref }) => ({
    data: {
      sha: ref,
      commit: { message: ref },
      parents: [],
      files: [{ filename: `${ref}.ts`, patch: ref }],
    },
    headers: {},
  }));
  let query: Record<string, unknown> | undefined = {
    operation: 'pullRequest',
    owner: 'o',
    repo: 'r',
    number: 91,
    content: { commits: { includeFiles: true } },
    pageSize: 1,
  };
  const seen: string[] = [];
  for (let budget = 0; query && budget < 4; budget++) {
    const result = await getMultipleGitHubHistoryItems({
      queries: [GitHubGetHistoryItemQueryLocalSchema.parse(query)],
    });
    const data = (
      result.structuredContent as {
        results: Array<{ data: Record<string, any> }>;
      }
    ).results[0]!.data;
    seen.push(
      ...data.pullRequests[0].commits.flatMap((commit: any) =>
        commit.files.map((file: any) => file.filename)
      )
    );
    query = data.next?.nextCommitsPage?.query;
  }
  expect(query).toBeUndefined();
  expect(seen).toEqual(['a.ts', 'b.ts', 'c.ts']);
  expect(mocks.getCommit).toHaveBeenCalledTimes(3);
  expect(mocks.listCommits).toHaveBeenCalledTimes(1);
});
it('reports failed requested commit files instead of caching an empty file list', async () => {
  mocks.listCommits.mockResolvedValue({
    data: [{ sha: 'one', commit: { message: 'one', author: null } }],
    headers: {},
  });
  mocks.getCommit.mockRejectedValueOnce(
    Object.assign(new Error('rate limited'), { status: 429 })
  );
  const query = {
    operation: 'pullRequest',
    owner: 'o',
    repo: 'r',
    number: 91,
    content: { commits: { includeFiles: true } },
  };
  const result = await getMultipleGitHubHistoryItems({
    queries: [GitHubGetHistoryItemQueryLocalSchema.parse(query)],
  });
  const data = (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
  expect(data.error).toBeDefined();
});
