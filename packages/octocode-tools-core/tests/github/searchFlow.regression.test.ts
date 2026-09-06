import { beforeEach, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  code: vi.fn(),
  repos: vi.fn(),
  listForOrg: vi.fn(),
  listForUser: vi.fn(),
  get: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(async () => ({
    rest: {
      search: { code: mocks.code, repos: mocks.repos },
      repos: {
        listForOrg: mocks.listForOrg,
        listForUser: mocks.listForUser,
        get: mocks.get,
      },
    },
  })),
  resolveCacheAuthFingerprint: vi.fn(async () => 'search-flow-fixture'),
}));

import { clearAllCache } from '../../src/utils/http/cache/management.js';
import {
  searchCode,
  searchRepos,
} from '../../src/providers/github/githubSearch.js';
import { searchGitHubRepos } from '../../src/tools/github_search_repos/execution.js';
import { searchGitHubCode } from '../../src/tools/github_search_code/execution.js';
import { searchGitHubCodeAPI } from '../../src/github/codeSearch.js';
import { searchGitHubReposAPI } from '../../src/github/repoSearch.js';
import { buildGitHubSearchFinalizer } from '../../src/tools/github_search/finalizer.js';
import { GitHubSearchQuerySchema } from '../../src/tools/github_search/scheme.js';

beforeEach(() => {
  clearAllCache();
  vi.resetAllMocks();
});

it.each([
  ['anchor comment', '// anchor comment\nexport const target = 1;'],
  [
    'anotherUniqueName',
    'export function target(uniqueArgumentName) { const anotherUniqueName = uniqueArgumentName + 1; return anotherUniqueName; }',
  ],
])(
  'retains the matched term %s after snippet compaction',
  async (term, fragment) => {
    const start = fragment.indexOf(term);
    mocks.code.mockResolvedValue(
      response([
        {
          path: 'src/target.ts',
          name: 'target.ts',
          html_url: 'https://github.com/fixture/code/blob/main/src/target.ts',
          repository: repository('code'),
          text_matches: [
            {
              fragment,
              matches: [{ indices: [start, start + term.length], text: term }],
            },
          ],
        },
      ])
    );
    const result = await searchGitHubCodeAPI({
      keywords: [term],
      limit: 2,
    } as never);
    if (!('data' in result) || !result.data)
      throw new Error(JSON.stringify(result));
    const match = result.data.items[0]!.matches[0]!;
    expect(match.context).toContain(term);
    expect(match.positions).toHaveLength(1);
    const [from, to] = match.positions[0]!;
    expect(match.context.slice(from, to)).toBe(term);
  }
);

const repository = (name: string, extra = {}) => ({
  full_name: `fixture/${name}`,
  name,
  default_branch: 'main',
  html_url: `https://github.com/fixture/${name}`,
  stargazers_count: 10,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-02T00:00:00Z',
  pushed_at: '2026-01-02T00:00:00Z',
  description: 'fixture',
  topics: ['typescript'],
  ...extra,
});
const response = (
  items: unknown[],
  total = items.length,
  incomplete = false
) => ({
  data: { items, total_count: total, incomplete_results: incomplete },
  headers: {},
});

it.each(['package-lock.json', 'dist/index.min.js', 'vendor/fixture.ts'])(
  'preserves provider results for the explicitly requested file %s',
  async path => {
    mocks.code.mockResolvedValue(response([{
      name: path.split('/').at(-1), path,
      html_url: `https://github.com/fixture/repo/blob/main/${path}`,
      repository: repository('repo'),
      text_matches: [{ fragment: 'needle', matches: [{ text: 'needle', indices: [0, 6] }] }],
    }]));
    const data = await run({ operation: 'code', keywords: ['needle'], filename: path.split('/').at(-1) });
    expect(data.files.map((file: { path: string }) => file.path)).toEqual([path]);
    expect(data.next?.nextPage).toBeUndefined();
  }
);

async function run(query: Record<string, unknown>) {
  const parsed = GitHubSearchQuerySchema.parse(query);
  const { operation, pageSize, ...rest } = parsed;
  const runner = operation === 'code' ? searchGitHubCode : searchGitHubRepos;
  const data = await runner(
    {
      ...rest,
      limit: pageSize,
      ...(operation === 'repositories'
        ? { topicsToSearch: parsed.topics }
        : {}),
    } as never,
    {} as never,
    () => ({ provider: { searchCode, searchRepos } }) as never
  );
  const result = buildGitHubSearchFinalizer()({
    queries: [parsed],
    results: [{ index: 0, data }],
    config: { toolName: 'ghSearch' },
  } as never);
  return (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
}

it.each([0, 1001])(
  'code retry refetches incomplete page even with total %i, then caches complete response',
  async total => {
    mocks.code
      .mockResolvedValueOnce(response([], total, true))
      .mockResolvedValue(response([], 0, false));
    const first = await run({
      operation: 'code',
      keywords: ['needle'],
      pageSize: 2,
    });
    expect(first).toMatchObject({ isPartial: true, incompleteResults: true });
    expect(first.next?.retry?.tool).toBe('ghSearch');
    expect(first.next.retry.query.pageSize).toBe(2);
    const second = await run(first.next.retry.query);
    expect(second.incompleteResults).not.toBe(true);
    await run(first.next.retry.query);
    expect(mocks.code).toHaveBeenCalledTimes(2);
  }
);

it('preserves pagination when a code page has no displayable matches', async () => {
  mocks.code.mockResolvedValue(response([], 3));
  const first = await run({
    operation: 'code',
    keywords: ['needle'],
    pageSize: 2,
  });
  expect(first.next?.nextPage?.query).toMatchObject({
    operation: 'code',
    page: 2,
    pageSize: 2,
  });
  const second = await run(first.next.nextPage.query);
  expect(second.pagination).toMatchObject({ currentPage: 2, hasMore: false });
  expect(mocks.code).toHaveBeenCalledTimes(2);
});
it.each([1, 2, 3])(
  'executes code continuations with pageSize %i and recovers every file exactly once',
  async pageSize => {
    const files = Array.from({ length: 5 }, (_, index) => ({
      name: `${index}.ts`,
      path: `src/${index}.ts`,
      sha: `sha-${index}`,
      html_url: `https://github.com/fixture/repo/blob/main/src/${index}.ts`,
      repository: repository('repo'),
      text_matches: [
        {
          fragment: `const needle${index} = true;`,
          matches: [{ text: `needle${index}`, indices: [6, 13] }],
        },
      ],
    }));
    mocks.code.mockImplementation(async ({ page, per_page }) =>
      response(
        files.slice((page - 1) * per_page, page * per_page),
        files.length
      )
    );
    let query: Record<string, unknown> | undefined = {
      operation: 'code',
      keywords: ['needle'],
      pageSize,
    };
    const paths: string[] = [];
    for (let budget = 0; query && budget < 6; budget++) {
      const data = await run(query);
      paths.push(...data.files.map((file: any) => file.path));
      query = data.next?.nextPage?.query;
    }
    expect(query).toBeUndefined();
    expect(paths).toEqual(files.map(file => file.path));
    expect(mocks.code).toHaveBeenCalledTimes(
      Math.ceil(files.length / pageSize)
    );
  }
);

it('combines keywords and topics in one provider query and executes lossless public pages', async () => {
  mocks.repos.mockImplementation(async ({ q, page }) => {
    const both = q.includes('needle') && q.includes('topic:typescript');
    return response(
      both
        ? page === 1
          ? [repository('a'), repository('b')]
          : [repository('c')]
        : [repository('unrelated')],
      both ? 3 : 1
    );
  });
  const first = await run({
    operation: 'repositories',
    keywords: ['needle'],
    topics: ['typescript'],
    pageSize: 2,
  });
  expect(mocks.repos).toHaveBeenCalledTimes(1);
  expect(first.repositories.map((r: any) => r.repo)).toEqual(['a', 'b']);
  expect(first.next.nextPage.tool).toBe('ghSearch');
  const second = await run(first.next.nextPage.query);
  expect(
    [...first.repositories, ...second.repositories].map(r => r.repo)
  ).toEqual(['a', 'b', 'c']);
  expect(second.next?.nextPage).toBeUndefined();
  expect(mocks.repos).toHaveBeenCalledTimes(2);
});

it.each(['best-match', 'help-wanted-issues'])(
  'preserves upstream %s ordering',
  async sort => {
    mocks.repos.mockResolvedValue(
      response([
        repository('z', { open_issues_count: 1 }),
        repository('needle', { open_issues_count: 100 }),
      ])
    );
    const data = await run({
      operation: 'repositories',
      keywords: ['needle'],
      sort,
    });
    expect(data.repositories.map((r: any) => r.repo)).toEqual(['z', 'needle']);
  }
);

it('retains license and homepage through the public repository output without enrichment calls', async () => {
  mocks.repos.mockResolvedValue(
    response([
      repository('a', {
        license: { spdx_id: 'MIT' },
        homepage: 'https://example.com',
      }),
    ])
  );
  const data = await run({ operation: 'repositories', keywords: ['needle'] });
  expect(data.repositories[0]).toMatchObject({
    license: 'MIT',
    homepage: 'https://example.com',
  });
  expect(mocks.repos).toHaveBeenCalledTimes(1);
  expect(mocks.get).not.toHaveBeenCalled();
});

it.each(['org', 'user'])(
  'follows %s listing Link on a short page, stops on a full final page',
  async kind => {
    const listing = kind === 'org' ? mocks.listForOrg : mocks.listForUser;
    if (kind === 'user') mocks.listForOrg.mockRejectedValue({ status: 404 });
    listing
      .mockResolvedValueOnce({
        data: [repository('a')],
        headers: { link: '<https://api.github.com/repos?page=2>; rel="next"' },
      })
      .mockResolvedValueOnce({
        data: [repository('b'), repository('c')],
        headers: {},
      });
    const first = await run({
      operation: 'repositories',
      owner: 'fixture',
      pageSize: 2,
    });
    expect(first.next?.nextPage?.tool).toBe('ghSearch');
    const second = await run(first.next.nextPage.query);
    expect(
      [...first.repositories, ...second.repositories].map(r => r.repo)
    ).toEqual(['a', 'b', 'c']);
    expect(second.pagination.hasMore).toBe(false);
    expect(second.next?.nextPage).toBeUndefined();
    expect(listing).toHaveBeenCalledTimes(2);
  }
);

it('preserves the actual requested page on out-of-range empty search responses', async () => {
  mocks.code.mockResolvedValue(response([], 2));
  mocks.repos.mockResolvedValue(response([], 2));
  const query = { keywords: ['needle'], limit: 2, page: 5 } as never;
  for (const result of [
    await searchGitHubCodeAPI(query),
    await searchGitHubReposAPI(query),
  ]) {
    expect(result).toMatchObject({
      data: { pagination: { currentPage: 5, hasMore: false } },
    });
  }
});

it('probes repository transfers only on the first search page', async () => {
  mocks.repos.mockImplementation(async ({ page }) =>
    response([repository(page === 1 ? 'a' : 'b')], 2)
  );
  mocks.get.mockResolvedValue({ data: { full_name: 'fixture/needle' } });
  const first = await run({
    operation: 'repositories',
    owner: 'fixture',
    keywords: ['needle'],
    pageSize: 1,
  });
  await run(first.next.nextPage.query);
  expect(mocks.repos).toHaveBeenCalledTimes(2);
  expect(mocks.get).toHaveBeenCalledTimes(1);
});

it('does not probe a keyword phrase as a repository name', async () => {
  mocks.repos.mockResolvedValue(response([repository('a')]));
  await run({
    operation: 'repositories',
    owner: 'fixture',
    keywords: ['search client'],
  });
  expect(mocks.get).not.toHaveBeenCalled();
});
