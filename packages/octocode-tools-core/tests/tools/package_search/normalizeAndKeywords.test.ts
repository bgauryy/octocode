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
  foldKeywords,
  isPackageNotFoundError,
  formatPackageData,
  searchPackages,
} from '../../../src/tools/package_search/execution.js';

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
      { viewRepoStructure?: { query?: Record<string, unknown> } } | undefined;
    expect(next?.viewRepoStructure?.query).toMatchObject({
      owner: 'octokit',
      repo: 'rest.js',
    });
  });
});

describe('foldKeywords', () => {
  it('joins an array of terms with spaces', () => {
    expect(foldKeywords(['state', 'management'])).toBe('state management');
  });

  it('passes a string through and trims', () => {
    expect(foldKeywords('  react hooks ')).toBe('react hooks');
  });

  it('returns undefined for empty inputs', () => {
    expect(foldKeywords(undefined)).toBeUndefined();
    expect(foldKeywords([])).toBeUndefined();
    expect(foldKeywords('   ')).toBeUndefined();
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

  it('lets an explicit packageName win over keywords', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({
      packages: [],
      totalFound: 0,
      rawResponseChars: 0,
    });

    await searchPackages({
      queries: [{ packageName: 'react', keywords: ['state', 'management'] }],
    } as never);

    expect(searchPackageMock.mock.calls[0]?.[0]).toMatchObject({
      name: 'react',
    });
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
