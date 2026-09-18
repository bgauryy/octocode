import {
  normalizeRepoUrl,
  formatPackageData,
} from '../../../src/tools/package_search/npm.js';
import { expectExecutableNext } from '../../helpers/executableNext.js';
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

import { searchPackages } from '../../../src/tools/package_search/execution.js';
import { isPackageNotFoundError } from '../../../src/tools/package_search/queryHelpers.js';
import { ArtifactSearchBulkQueryLocalSchema } from '@octocodeai/octocode-core/schema';
import { ArtifactSearchQueryLocalSchema } from '@octocodeai/octocode-core/schema';

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

  it('preserves source identity without adding unsolicited success advice', () => {
    const data = formatPackageData(npmPkg('github:octokit/rest.js'));
    expect(data.repository).toBe('https://github.com/octokit/rest.js');
    expect(data).not.toHaveProperty('next');
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

describe('searchPackages registry dispatch', () => {
  it('joins discovery terms and uses the keyword default', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({ packages: [], totalFound: 0 });
    await searchPackages({
      queries: [{ type: 'npm', keywords: ['state', 'management'] }],
    });
    expect(searchPackageMock).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'state management',
        mode: 'keywords',
        itemsPerPage: 10,
      })
    );
  });
  it('emits a schema-valid executable cursor preserving registry and terms', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({
      packages: [npmPkg('github:octo/one')],
      totalFound: 2,
      registry: 'https://registry.example.test',
    });
    const first = await searchPackages({
      queries: [
        {
          reasoning: 'Exercise artifact cursor metadata propagation.',
          debug: true,
          type: 'npm',
          keywords: ['state'],
          pageSize: 1,
        },
      ],
    });
    const row = (first.structuredContent as any).results[0].data;
    expectExecutableNext(row);
    const next = row.next.nextPage;
    expect(next.query).toMatchObject({
      type: 'npm',
      keywords: ['state'],
      pageSize: 1,
      registry: 'https://registry.example.test',
      cursor: expect.any(String),
    });
    expect(next.query).not.toHaveProperty('page');
    const replay = ArtifactSearchBulkQueryLocalSchema.parse({
      queries: [next.query],
    });
    await searchPackages({ queries: replay.queries });
    expect(searchPackageMock).toHaveBeenLastCalledWith(
      expect.objectContaining({
        offset: 1,
        registry: 'https://registry.example.test',
      })
    );
  });
});

describe('artifactSearch public selector schema', () => {
  it('allows pageSize only for keyword discovery', () => {
    expect(
      ArtifactSearchBulkQueryLocalSchema.safeParse({
        queries: [{ type: 'npm', keywords: ['schema'], pageSize: 25 }],
      }).success
    ).toBe(true);
    expect(
      ArtifactSearchBulkQueryLocalSchema.safeParse({
        queries: [{ type: 'npm', packageName: 'zod', pageSize: 25 }],
      }).success
    ).toBe(false);
    expect(
      ArtifactSearchQueryLocalSchema.safeParse({
        type: 'npm',
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
      queries: [{ type: 'npm', packageName: 'defintely-not-a-real-pkg' }],
    } as never);

    const data = dataOf(result);
    expect(data.status).toBe('empty');
    // The bulk layer strips the empty `packages: []` array; the guidance is the
    // load-bearing signal.
    expect((data.hints as string[])?.join(' ')).toMatch(/name|coordinate/i);
  });

  it('a network failure on an exact name stays a hard error', async () => {
    searchPackageMock.mockReset();
    searchPackageMock.mockResolvedValue({
      error: 'NPM registry search failed: fetch failed',
    });

    const result = await searchPackages({
      queries: [{ type: 'npm', packageName: 'react' }],
    } as never);

    const data = dataOf(result);
    expect(data.status).toBe('error');
  });
});
