import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../../../src/utils/package/npm/npmRegistry.js', () => ({
  resolveNpmRegistryContext: vi.fn(),
  fetchNpmRegistryJson: vi.fn(),
}));
vi.mock('../../../src/utils/http/cache/dataCache.js', () => ({
  withDataCache: (_key: string, run: () => unknown) => run(),
}));
vi.mock('../../../src/utils/http/circuitBreaker.js', () => ({
  isCircuitOpen: () => false,
}));

import {
  resolveNpmRegistryContext,
  fetchNpmRegistryJson,
} from '../../../src/utils/package/npm/npmRegistry.js';
import { searchPackages } from '../../../src/tools/package_search/execution.js';
import { NpmSearchBulkQueryLocalSchema } from '../../../src/tools/package_search/scheme.js';

const registry = 'https://packages.example.test';
const fixture = ['octokit', '@octokit/core', '@octokit/rest', '@octokit/types'];

function row(result: Awaited<ReturnType<typeof searchPackages>>) {
  return (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(resolveNpmRegistryContext).mockResolvedValue({
    registry,
    cacheIdentity: 'fixture',
    options: {},
  });
  vi.mocked(fetchNpmRegistryJson).mockImplementation(
    async (_context, input) => {
      const url = new URL(String(input), registry + '/');
      if (url.pathname === '/-/v1/search') {
        const from = Number(url.searchParams.get('from') ?? 0);
        const size = Number(url.searchParams.get('size'));
        return {
          total: fixture.length,
          objects: fixture.slice(from, from + size).map(name => ({
            package: {
              name,
              version: '1.0.0',
              license: 'MIT',
              links: { repository: 'https://github.com/octokit/octokit.js' },
            },
          })),
        };
      }
      return {
        name: 'octokit',
        version: '1.0.0',
        license: 'MIT',
        repository: {
          url: 'https://github.com/octokit/octokit.js',
          directory: 'packages/core',
        },
      };
    }
  );
});

describe('npm public registry flow', () => {
  it('executes continuations until every fixture result is reached exactly once with one bounded request per page', async () => {
    let query: Record<string, unknown> = { keywords: ['octokit'], pageSize: 2 };
    const names: string[] = [];
    let pages = 0;
    while (true) {
      const parsed = NpmSearchBulkQueryLocalSchema.parse({ queries: [query] });
      const data = row(await searchPackages({ queries: parsed.queries }));
      names.push(
        ...(data.packages ?? []).map((pkg: { name: string }) => pkg.name)
      );
      pages++;
      if (!data.pagination.hasMore) break;
      expect(pages).toBeLessThan(3);
      query = data.next.nextPage.query;
    }
    expect(names).toEqual(fixture);
    expect(pages).toBe(2);
    expect(fetchNpmRegistryJson).toHaveBeenCalledTimes(2);
    expect(
      vi
        .mocked(fetchNpmRegistryJson)
        .mock.calls.map(([, url]) => `${registry}/${url}`)
    ).toEqual([
      `${registry}/-/v1/search?text=octokit&size=2`,
      `${registry}/-/v1/search?text=octokit&size=2&from=2`,
    ]);
    expect(resolveNpmRegistryContext).toHaveBeenCalled();
  });

  it('uses one exact metadata request and preserves monorepo directory, version and license', async () => {
    const data = row(
      await searchPackages({ queries: [{ packageName: 'octokit' }] })
    );
    expect(data.packages[0]).toMatchObject({
      name: 'octokit',
      version: '1.0.0',
      license: 'MIT',
      repositoryDirectory: 'packages/core',
    });
    expect(fetchNpmRegistryJson).toHaveBeenCalledTimes(1);
    expect(resolveNpmRegistryContext).toHaveBeenCalled();
  });

  it('does not turn a registry failure into a differently ordered discovery provider', async () => {
    vi.mocked(fetchNpmRegistryJson).mockRejectedValue(
      new Error('network unavailable')
    );
    const data = row(
      await searchPackages({
        queries: [{ keywords: ['octokit'], page: 2, pageSize: 2 }],
      })
    );
    expect(data.error).toBeDefined();
    expect(fetchNpmRegistryJson).toHaveBeenCalledTimes(1);
  });

  it('treats an unavailable registry search endpoint as an error rather than an empty keyword result', async () => {
    vi.mocked(fetchNpmRegistryJson).mockRejectedValue(
      new Error('404 Not Found')
    );
    const data = row(
      await searchPackages({ queries: [{ keywords: ['octokit'] }] })
    );
    expect(data.error).toBeDefined();
    expect(data.pagination).toBeUndefined();
  });

  it('returns a retryable error when the registry omits the total instead of claiming the page is complete', async () => {
    vi.mocked(fetchNpmRegistryJson).mockResolvedValue({
      objects: [{ package: { name: 'octokit', version: '1' } }],
    });
    const data = row(
      await searchPackages({
        queries: [{ keywords: ['octokit'], pageSize: 2 }],
      })
    );
    expect(JSON.stringify(data.error)).toContain('valid total');
    expect(data.pagination).toBeUndefined();
  });

  it('does not rewrite an empty keyword query or retry a different provider', async () => {
    vi.mocked(fetchNpmRegistryJson).mockResolvedValue({
      objects: [],
      total: 0,
    });
    const data = row(
      await searchPackages({
        queries: [{ keywords: ['@octokit/core'], pageSize: 2 }],
      })
    );
    expect(data.pagination.hasMore).toBe(false);
    expect(fetchNpmRegistryJson).toHaveBeenCalledTimes(1);
    expect(vi.mocked(fetchNpmRegistryJson).mock.calls[0]?.[1]).toContain(
      'text=%40octokit%2Fcore'
    );
  });

  it('uses the keyword default page size for a single valid package-name term', async () => {
    const result = await searchPackages({
      queries: [{ keywords: ['octokit'] }],
    });
    const data = row(result);
    expect(data.pagination).toMatchObject({
      perPage: 10,
      totalFound: 4,
      hasMore: false,
    });
    expect(vi.mocked(fetchNpmRegistryJson).mock.calls[0]?.[1]).toContain(
      'size=10'
    );
    expect(JSON.stringify(result.structuredContent)).toContain('MIT');
  });

  it('reports an exact authentication failure without retrying a different acquisition path', async () => {
    vi.mocked(fetchNpmRegistryJson).mockRejectedValue(
      new Error('npm registry authentication failed (401).')
    );
    const data = row(
      await searchPackages({ queries: [{ packageName: '@octokit/core' }] })
    );
    expect(data.error).toContain('authentication failed');
    expect(fetchNpmRegistryJson).toHaveBeenCalledTimes(1);
    expect(vi.mocked(fetchNpmRegistryJson).mock.calls[0]?.[1]).toBe(
      '@octokit%2Fcore/latest'
    );
  });

  it('returns an empty exact lookup only for an authenticated registry miss', async () => {
    vi.mocked(fetchNpmRegistryJson).mockRejectedValue(
      new Error('npm registry returned 404 Not Found.')
    );
    const data = row(
      await searchPackages({ queries: [{ packageName: 'missing' }] })
    );
    expect(data.pagination).toMatchObject({ totalFound: 0, hasMore: false });
    expect(fetchNpmRegistryJson).toHaveBeenCalledTimes(1);
  });
});
