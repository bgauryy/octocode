import { describe, expect, it, vi } from 'vitest';

const searchPackageMock = vi.fn();
vi.mock('../../../src/utils/package/common.js', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('../../../src/utils/package/common.js')
    >();
  return {
    ...actual,
    searchPackage: (...args: unknown[]) => searchPackageMock(...args),
  };
});

import {
  normalizeRepoUrl,
  formatPackageData,
  searchPackages,
} from '../../../src/tools/package_search/execution.js';
import {
  foldKeywords,
  isPackageNotFoundError,
} from '../../../src/tools/package_search/queryHelpers.js';
import { NpmSearchBulkQueryLocalSchema } from '../../../src/tools/package_search/scheme.js';
import { NpmSearchQueryLocalSchema } from '../../../src/tools/package_search/scheme.js';

type AnyPkg = Parameters<typeof formatPackageData>[0];

const npmPkg = (repoUrl: string | undefined): AnyPkg =>
  ({
    npmUrl: 'https://www.npmjs.com/package/x',
    name: 'x',
    repoUrl,
  }) as unknown as AnyPkg;

describe('normalizeRepoUrl — npm shorthand repository URLs', () => {
  it('resolves github:owner/repo to an https URL', () => {
    expect(normalizeRepoUrl('github:octokit/rest.js')).toBe(
      'https://github.com/octokit/rest.js'
    );
  });

  it('resolves gitlab:/bitbucket: shorthands to their hosts', () => {
    expect(normalizeRepoUrl('gitlab:foo/bar')).toBe(
      'https://gitlab.com/foo/bar'
    );
    expect(normalizeRepoUrl('bitbucket:foo/bar')).toBe(
      'https://bitbucket.org/foo/bar'
    );
  });

  it('does not regress existing scheme/scp/git+ shapes', () => {
    expect(normalizeRepoUrl('git+https://github.com/a/b.git')).toBe(
      'https://github.com/a/b'
    );
    expect(normalizeRepoUrl('git@github.com:a/b.git')).toBe(
      'https://github.com/a/b'
    );
    expect(normalizeRepoUrl('git://github.com/a/b.git')).toBe(
      'https://github.com/a/b'
    );
  });

  it('drives a GitHub tool chain from a shorthand repository', () => {
    const data = formatPackageData(npmPkg('github:octokit/rest.js'));
    expect(data.repository).toBe('https://github.com/octokit/rest.js');
    const next = data.next as
      { viewTree?: { query?: Record<string, unknown> } } | undefined;
    expect(next?.viewTree?.query).toMatchObject({
      owner: 'octokit',
      repo: 'rest.js',
    });
    expect(next).not.toHaveProperty('viewRepoStructure');
    expect(next).not.toHaveProperty('searchCode');
  });
});

describe('foldKeywords', () => {
  it('joins an array of terms with spaces', () => {
    expect(foldKeywords(['state', 'management'])).toBe('state management');
  });

  it('returns undefined for empty inputs', () => {
    expect(foldKeywords(undefined)).toBeUndefined();
    expect(foldKeywords([])).toBeUndefined();
    expect(foldKeywords(['   '])).toBeUndefined();
  });
});

describe('isPackageNotFoundError', () => {
  it('recognizes 404 / not-found messages', () => {
    expect(isPackageNotFoundError('npm view: 404 Not Found - GET ...')).toBe(
      true
    );
    expect(isPackageNotFoundError('E404 no such package available')).toBe(true);
  });

  it('does not treat network failures as not-found', () => {
    expect(
      isPackageNotFoundError('NPM registry search failed: fetch failed')
    ).toBe(false);
    expect(isPackageNotFoundError('request to ... failed, ENOTFOUND')).toBe(
      false
    );
  });
});

describe('searchPackages — keywords fold drives the registry query', () => {
  it.each([
    { packageName: 'react', keywords: ['state'] },
    { packageName: 'react', pageSize: 10 },
    { packageName: 'react', page: 1 },
    { packageName: 'react', page: 2 },
    { keywords: [' '] },
  ])(
    'rejects invalid mode inputs before reaching the registry: %j',
    async query => {
      searchPackageMock.mockReset();
      const result = await searchPackages({ queries: [query] } as never);
      expect(searchPackageMock).not.toHaveBeenCalled();
      expect(JSON.stringify(result.structuredContent)).toContain('error');
    }
  );
  it('searches using "state management" when only keywords is given', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({
      packages: [],
      totalFound: 0,
      rawResponseChars: 0,
    });

    await searchPackages({
      queries: [{ keywords: ['state', 'management'] }],
    } as never);

    expect(searchPackageMock).toHaveBeenCalledTimes(1);
    expect(searchPackageMock.mock.calls[0]?.[0]).toMatchObject({
      name: 'state management',
    });
  });

  it('applies keyword pageSize and emits a compact executable next page', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({
      packages: [npmPkg('github:octo/one'), npmPkg('github:octo/two')],
      totalFound: 20,
      rawResponseChars: 100,
    });
    const firstQuery = {
      keywords: ['state', 'management'],
      page: 1,
      pageSize: 2,
      goal: 'discover packages',
      reasoning: 'compare candidates',
    };

    const first = await searchPackages({ queries: [firstQuery] } as never);
    expect(searchPackageMock).toHaveBeenLastCalledWith(
      expect.objectContaining({
        name: 'state management',
        page: 1,
        itemsPerPage: 2,
      })
    );
    const firstRow = (
      first.structuredContent as {
        results: Array<{ data: Record<string, any> }>;
      }
    ).results[0]!;
    const continuation = firstRow.data.next.nextPage;
    expect(continuation).toMatchObject({
      tool: 'npmSearch',
      query: {
        keywords: ['state', 'management'],
        page: 2,
        pageSize: 2,
      },
    });
    expect(continuation.query).not.toHaveProperty('goal');
    expect(continuation.query).not.toHaveProperty('reasoning');

    const replay = NpmSearchBulkQueryLocalSchema.parse({
      queries: [continuation.query],
    });
    await searchPackages({ queries: replay.queries } as never);
    expect(searchPackageMock).toHaveBeenLastCalledWith(
      expect.objectContaining({ page: 2, itemsPerPage: 2 })
    );
  });

  it('keeps a one-result keyword page in discovery pagination mode', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({
      packages: [npmPkg('github:octo/one')],
      totalFound: 11,
      rawResponseChars: 50,
    });

    const result = await searchPackages({
      queries: [{ keywords: ['narrow'], page: 2, pageSize: 10 }],
    } as never);
    const row = (
      result.structuredContent as {
        results: Array<{ data: Record<string, any> }>;
      }
    ).results[0]!;
    expect(row.data.pagination).toMatchObject({
      currentPage: 2,
      perPage: 10,
      returned: 1,
    });
  });

  it('marks the schema page ceiling as a terminal partial result', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({
      packages: [npmPkg('github:octo/one')],
      totalFound: 1001,
      rawResponseChars: 50,
    });

    const result = await searchPackages({
      queries: [{ keywords: ['schema'], page: 1000, pageSize: 1 }],
    } as never);
    const row = (
      result.structuredContent as {
        results: Array<{
          data: Record<string, any>;
          meta: { diagnostics?: { codes?: string[] } };
        }>;
      }
    ).results[0]!;

    expect(row.data).toMatchObject({
      terminalLimit: true,
      pagination: {
        hasMore: true,
        continuationUnavailable: {
          reason: 'schemaPageLimit',
          maxPage: 1000,
        },
      },
    });
    expect(row.data.next?.nextPage).toBeUndefined();
    expect(row.meta.diagnostics?.codes).toContain('terminalLimitReached');
    expect(row.meta.diagnostics?.codes).not.toContain('continuationMissing');
  });
});

describe('npmSearch public selector schema', () => {
  it('allows pageSize only for keyword discovery', () => {
    expect(
      NpmSearchBulkQueryLocalSchema.safeParse({
        queries: [{ keywords: ['schema'], pageSize: 25 }],
      }).success
    ).toBe(true);
    expect(
      NpmSearchBulkQueryLocalSchema.safeParse({
        queries: [{ packageName: 'zod', pageSize: 25 }],
      }).success
    ).toBe(false);
    expect(
      NpmSearchQueryLocalSchema.safeParse({
        packageName: 'zod',
        pageSize: 25,
      }).success
    ).toBe(false);
  });
});

describe('searchPackages — exact not-found becomes a guided empty', () => {
  const dataOf = (result: unknown): Record<string, unknown> => {
    const structured = (result as { structuredContent?: unknown })
      .structuredContent as {
      results?: Array<{ data?: Record<string, unknown>; status?: string }>;
    };
    const row = structured.results?.[0];
    return { ...row?.data, status: row?.data?.status ?? row?.status };
  };

  it('a 404 on an exact name yields an empty result with spelling/scoped guidance', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({
      error: 'NPM registry lookup failed: 404 Not Found',
    });

    const result = await searchPackages({
      queries: [{ packageName: 'defintely-not-a-real-pkg' }],
    } as never);

    const data = dataOf(result);
    expect(data.status).toBe('empty');
    // The bulk layer strips the empty `packages: []` array; the guidance is the
    // load-bearing signal.
    expect((data.hints as string[])?.join(' ')).toMatch(/spelling|scoped/i);
  });

  it('a network failure on an exact name stays a hard error', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({
      error: 'NPM registry search failed: fetch failed',
    });

    const result = await searchPackages({
      queries: [{ packageName: 'react' }],
    } as never);

    const data = dataOf(result);
    expect(data.status).toBe('error');
  });
});
