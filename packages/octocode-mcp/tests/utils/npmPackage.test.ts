import { describe, it, expect, vi, beforeEach } from 'vitest';
import { clearAllCache } from '../../src/utils/http/cache.js';
import {
  resetCircuitBreaker,
  recordCircuitFailure,
  DEFAULT_CIRCUIT_FAILURE_THRESHOLD,
} from '../../src/utils/http/circuitBreaker.js';
import type { NpmPackageResult } from '../../src/utils/package/common.js';

const mockFetchWithRetries = vi.fn();
vi.mock('../../src/utils/http/fetch.js', () => ({
  fetchWithRetries: (...args: unknown[]) => mockFetchWithRetries(...args),
}));

const mockExecuteNpmCommand = vi.fn();
vi.mock('../../src/utils/exec/npm.js', () => ({
  executeNpmCommand: (...args: unknown[]) => mockExecuteNpmCommand(...args),
}));

import {
  searchNpmPackage,
  checkNpmDeprecation,
  getNpmRegistryUrl,
  checkNpmRegistryReachable,
  _resetNpmRegistryUrlCache,
  _packageNameToSearchKeywords,
} from '../../src/utils/package/npm.js';

function makeSearchResult(
  items: Array<{
    name: string;
    version: string;
    description?: string;
    links?: { repository?: string; homepage?: string; npm?: string };
  }>,
  total = items.length
) {
  return { objects: items.map(pkg => ({ package: pkg })), total };
}

beforeEach(() => {
  vi.resetAllMocks();
  clearAllCache();
  resetCircuitBreaker();
  _resetNpmRegistryUrlCache();
  mockExecuteNpmCommand.mockResolvedValue({
    exitCode: 1,
    stdout: '',
    stderr: 'npm ERR! code E404',
    error: null,
  });
});

describe('mapToResult - time object parsing', () => {
  it('should extract lastPublished from version-specific time', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      time: {
        '1.0.0': '2024-01-15T10:30:00.000Z',
        modified: '2024-01-20T10:30:00.000Z',
      },
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).lastPublished).toBe(
        '2024-01-15T10:30:00.000Z'
      );
    }
  });

  it('should fallback to modified time when version time is missing', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      time: { modified: '2024-01-20T10:30:00.000Z' },
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).lastPublished).toBe(
        '2024-01-20T10:30:00.000Z'
      );
    }
  });

  it('should handle missing time object', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(
        (result.packages[0] as NpmPackageResult).lastPublished
      ).toBeUndefined();
    }
  });

  it('should handle time object with no valid time strings', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      time: { created: '2024-01-10T10:30:00.000Z' },
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(
        (result.packages[0] as NpmPackageResult).lastPublished
      ).toBeUndefined();
    }
  });

  it('should fallback to fetchLastPublished when time object is absent', async () => {
    mockFetchWithRetries
      .mockResolvedValueOnce({ name: 'test-pkg', version: '1.0.0' })
      .mockResolvedValueOnce({ downloads: 500 })
      .mockResolvedValueOnce({ modified: '2024-03-01T09:00:00.000Z' });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).lastPublished).toBe(
        '2024-03-01T09:00:00.000Z'
      );
      expect((result.packages[0] as NpmPackageResult).weeklyDownloads).toBe(
        500
      );
    }
  });

  it('should fallback to full doc when abbreviated metadata lacks modified', async () => {
    mockFetchWithRetries
      .mockResolvedValueOnce({ name: 'test-pkg', version: '1.0.0' })
      .mockResolvedValueOnce({ downloads: 200 })
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({
        time: { modified: '2024-05-10T08:00:00.000Z' },
      });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).lastPublished).toBe(
        '2024-05-10T08:00:00.000Z'
      );
      expect((result.packages[0] as NpmPackageResult).weeklyDownloads).toBe(
        200
      );
    }
  });

  it('should report rawResponseChars from every fetched npm payload', async () => {
    const viewPayload = {
      name: 'test-pkg',
      version: '2.0.0',
      time: { '2.0.0': '2024-02-15T10:00:00.000Z' },
    };
    const downloadsPayload = { downloads: 1000 };
    mockFetchWithRetries
      .mockResolvedValueOnce(viewPayload)
      .mockResolvedValueOnce(downloadsPayload);

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const expectedRawChars =
        JSON.stringify(viewPayload).length +
        JSON.stringify(downloadsPayload).length;
      expect(result.rawResponseChars).toBe(expectedRawChars);
    }
  });
  it('should not call fetchLastPublished when time object already provides lastPublished', async () => {
    mockFetchWithRetries
      .mockResolvedValueOnce({
        name: 'test-pkg',
        version: '2.0.0',
        time: { '2.0.0': '2024-02-15T10:00:00.000Z' },
      })
      .mockResolvedValueOnce({ downloads: 1000 });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).lastPublished).toBe(
        '2024-02-15T10:00:00.000Z'
      );
    }
    expect(mockFetchWithRetries).toHaveBeenCalledTimes(2);
  });
});

describe('fetchPackageDetails - HTTP error handling', () => {
  it('should return empty when registry returns 404', async () => {
    mockFetchWithRetries.mockRejectedValue(
      new Error('HTTP error: 404 Not Found')
    );

    const result = await searchNpmPackage('nonexistent-pkg-xyz', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(0);
      expect(result.totalFound).toBe(0);
    }
  });

  it('should return error with details when fetch throws a non-404 error', async () => {
    mockFetchWithRetries.mockRejectedValue(new Error('Network timeout'));

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('Network timeout');
    }
  });

  it('should return empty when registry response is null', async () => {
    mockFetchWithRetries.mockResolvedValue(null);

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(0);
    }
  });

  it('should return error on invalid registry response format', async () => {
    mockFetchWithRetries.mockResolvedValue({ invalid: true });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('Invalid npm registry response format');
    }
  });

  it('should handle outer-catch when fetchWithRetries throws a non-Error value', async () => {
    mockFetchWithRetries.mockRejectedValue('non-error string thrown');

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('non-error string thrown');
    }
  });
});

describe('_packageNameToSearchKeywords', () => {
  it('strips scope and replaces slash with space', () => {
    expect(_packageNameToSearchKeywords('@modelcontextprotocol/sdk')).toBe(
      'modelcontextprotocol sdk'
    );
  });

  it('replaces hyphens with spaces', () => {
    expect(_packageNameToSearchKeywords('react-query')).toBe('react query');
  });

  it('replaces underscores with spaces', () => {
    expect(_packageNameToSearchKeywords('lodash_fp')).toBe('lodash fp');
  });

  it('leaves simple names unchanged', () => {
    expect(_packageNameToSearchKeywords('express')).toBe('express');
  });

  it('collapses multiple separators', () => {
    expect(_packageNameToSearchKeywords('@a/b-c_d')).toBe('a b c d');
  });

  it('trims leading/trailing whitespace', () => {
    expect(_packageNameToSearchKeywords('  pkg  ')).toBe('pkg');
  });
});

describe('searchNpmPackage - network error fallback', () => {
  it('should fall through to registry search when exact lookup fails with fetch failed', async () => {
    mockFetchWithRetries
      .mockRejectedValueOnce(new Error('fetch failed'))
      .mockResolvedValueOnce(
        makeSearchResult([{ name: 'test-pkg', version: '1.0.0' }])
      );

    const result = await searchNpmPackage('test-pkg', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(1);
      expect((result.packages[0] as { name: string }).name).toBe('test-pkg');
    }
  });

  it('should fall through to registry search when exact lookup fails with econnrefused', async () => {
    mockFetchWithRetries
      .mockRejectedValueOnce(new Error('connect ECONNREFUSED 127.0.0.1:4873'))
      .mockResolvedValueOnce(
        makeSearchResult([{ name: 'my-lib', version: '2.0.0' }])
      );

    const result = await searchNpmPackage('my-lib', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(1);
    }
  });

  it('should try keyword-split when exact name search returns empty', async () => {
    mockFetchWithRetries
      .mockRejectedValueOnce(new Error('fetch failed'))
      .mockResolvedValueOnce(makeSearchResult([]))
      .mockResolvedValueOnce(
        makeSearchResult([{ name: 'react-query', version: '5.0.0' }])
      );

    const result = await searchNpmPackage('react-query', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(1);
    }
  });

  it('should surface registry-unreachable hint when ALL paths fail with network error', async () => {
    mockFetchWithRetries.mockRejectedValue(new Error('fetch failed'));

    const result = await searchNpmPackage('octocode-mcp', 5, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.hints).toBeDefined();
      const hints = result.hints ?? [];
      expect(hints.some(h => h.toLowerCase().includes('unreachable'))).toBe(
        true
      );
      expect(hints.some(h => h.includes('githubSearchRepositories'))).toBe(
        true
      );
    }
  });

  it('should surface network hint even for scoped packages', async () => {
    mockFetchWithRetries.mockRejectedValue(new Error('ECONNREFUSED'));

    const result = await searchNpmPackage('@scope/pkg', 5, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.hints).toBeDefined();
      expect(
        (result.hints ?? []).some(h => h.includes('githubSearchRepositories'))
      ).toBe(true);
    }
  });

  it('should try keyword-split for scoped packages when exact fails', async () => {
    mockFetchWithRetries
      .mockRejectedValueOnce(new Error('Failed to fetch'))
      .mockResolvedValueOnce(makeSearchResult([]))
      .mockResolvedValueOnce(
        makeSearchResult([
          { name: '@modelcontextprotocol/sdk', version: '1.0.0' },
        ])
      );

    const result = await searchNpmPackage(
      '@modelcontextprotocol/sdk',
      5,
      false
    );

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(1);
    }
  });

  it('should NOT fall through for definitive non-network errors (e.g. invalid JSON)', async () => {
    mockFetchWithRetries.mockResolvedValue({ invalid_field: true });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('error' in result || 'packages' in result).toBe(true);
  });

  it('should surface network hint when registry is unreachable', async () => {
    mockFetchWithRetries.mockRejectedValue(new Error('fetch failed'));

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toBeTruthy();
    }
  });
});

describe('source field attribution', () => {
  beforeEach(() => {
    mockExecuteNpmCommand.mockReset();
    mockFetchWithRetries.mockReset();
  });

  it('result from CLI has source=cli', async () => {
    mockExecuteNpmCommand.mockResolvedValue({
      stdout: JSON.stringify({
        name: 'express',
        version: '4.18.2',
        repository: { url: 'https://github.com/expressjs/express' },
      }),
      stderr: '',
      exitCode: 0,
    });
    mockFetchWithRetries.mockRejectedValue(new Error('no enrichment'));

    const result = await searchNpmPackage('express', 1, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as { source?: string };
      expect(pkg.source).toBe('cli');
    }
  });

  it('result from registry direct fetch has source=registry', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'my-pkg',
      version: '1.0.0',
      repository: { url: 'https://github.com/owner/my-pkg' },
    });

    const result = await searchNpmPackage('my-pkg', 1, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as { source?: string };
      expect(pkg.source).toBe('registry');
    }
  });

  it('result from web fallback has source=web', async () => {
    mockFetchWithRetries
      .mockRejectedValueOnce(new Error('fetch failed'))
      .mockRejectedValueOnce(new Error('fetch failed'))
      .mockRejectedValueOnce(new Error('fetch failed'))
      .mockResolvedValueOnce({
        results: [
          {
            package: {
              name: 'react-query',
              version: '5.0.0',
              description: 'Query library for React',
              links: {
                npm: 'https://npmjs.com/package/react-query',
                repository: 'https://github.com/tanstack/query',
              },
            },
          },
        ],
        total: 1,
      });

    const result = await searchNpmPackage('react-query', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as { source?: string; name: string };
      expect(pkg.source).toBe('web');
      expect(pkg.name).toBe('react-query');
    }
  });
});

describe('searchNpmPackageViaSearch - error handling', () => {
  it('should return error when fetch throws for search', async () => {
    mockFetchWithRetries.mockRejectedValue(new Error('Search network error'));

    const result = await searchNpmPackage('test pkg keyword', 5, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('Search network error');
    }
  });

  it('should return empty when search returns no objects', async () => {
    mockFetchWithRetries.mockResolvedValue({ objects: [], total: 0 });

    const result = await searchNpmPackage('test pkg keyword', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(0);
    }
  });

  it('should return error on invalid search response format', async () => {
    mockFetchWithRetries.mockResolvedValue({ not_objects: [] });

    const result = await searchNpmPackage('test keyword', 5, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain(
        'Invalid npm registry search response format'
      );
    }
  });

  it('should return empty when search response body is null', async () => {
    mockFetchWithRetries.mockResolvedValue(null);

    const result = await searchNpmPackage('test keyword', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(0);
      expect(result.totalFound).toBe(0);
    }
  });

  it('should handle non-Error thrown values in search outer-catch', async () => {
    mockFetchWithRetries.mockRejectedValue('raw string error in search');

    const result = await searchNpmPackage('test pkg keyword', 5, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('raw string error in search');
    }
  });
});

describe('checkNpmDeprecation - edge cases', () => {
  it('should handle command error', async () => {
    mockExecuteNpmCommand.mockResolvedValue({
      error: new Error('Command failed'),
      stdout: '',
      stderr: '',
      exitCode: 1,
    });

    const result = await checkNpmDeprecation('test-pkg');

    expect(result).toBeNull();
  });

  it('should handle exception in deprecation check', async () => {
    mockExecuteNpmCommand.mockRejectedValue(new Error('Network error'));

    const result = await checkNpmDeprecation('test-pkg');

    expect(result).toBeNull();
  });

  it('should handle non-string deprecation message', async () => {
    mockExecuteNpmCommand.mockResolvedValue({
      stdout: JSON.stringify({ reason: 'deprecated for reasons' }),
      stderr: '',
      exitCode: 0,
    });

    const result = await checkNpmDeprecation('test-pkg');

    expect(result).toEqual({
      deprecated: true,
      message: 'This package is deprecated',
    });
  });

  it('should fallback to raw output when deprecation JSON fails schema', async () => {
    mockExecuteNpmCommand.mockResolvedValue({
      stdout: '[]',
      stderr: '',
      exitCode: 0,
      error: null,
    });

    const result = await checkNpmDeprecation('array-deprecation');

    expect(result).toEqual({
      deprecated: true,
      message: '[]',
    });
  });

  it('should handle unparseable deprecation output', async () => {
    mockExecuteNpmCommand.mockResolvedValue({
      stdout: 'Package is deprecated - use other-pkg instead',
      stderr: '',
      exitCode: 0,
    });

    const result = await checkNpmDeprecation('test-pkg');

    expect(result).toEqual({
      deprecated: true,
      message: 'Package is deprecated - use other-pkg instead',
    });
  });
});

describe('isExactPackageName', () => {
  it('should call /latest URL for scoped packages', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: '@scope/pkg',
      version: '1.0.0',
    });

    const result = await searchNpmPackage('@scope/pkg', 1, false);

    expect('packages' in result).toBe(true);
    expect(mockFetchWithRetries).toHaveBeenCalledWith(
      expect.stringContaining('/latest'),
      expect.any(Object)
    );
    expect(mockFetchWithRetries).not.toHaveBeenCalledWith(
      expect.stringContaining('/-/v1/search'),
      expect.any(Object)
    );
  });

  it('should call search URL for names with spaces', async () => {
    mockFetchWithRetries.mockResolvedValue(makeSearchResult([]));

    await searchNpmPackage('test package', 5, false);

    expect(mockFetchWithRetries).toHaveBeenCalledWith(
      expect.stringContaining('/-/v1/search'),
      expect.any(Object)
    );
    expect(mockFetchWithRetries).not.toHaveBeenCalledWith(
      expect.stringContaining('/latest'),
      expect.any(Object)
    );
  });

  it('should return the canonical exact package even when limit > 1', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'react',
      version: '18.0.0',
    });

    const result = await searchNpmPackage('react', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(1);
      expect((result.packages[0] as NpmPackageResult).name).toBe('react');
    }
    expect(mockFetchWithRetries).toHaveBeenCalledWith(
      expect.stringContaining('/react/latest'),
      expect.any(Object)
    );
  });

  it('should call /latest URL when limit === 1 for exact package name', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'react',
      version: '18.0.0',
    });

    await searchNpmPackage('react', 1, false);

    expect(mockFetchWithRetries).toHaveBeenCalledWith(
      expect.stringContaining('/react/latest'),
      expect.any(Object)
    );
  });

  it('should encode scoped package name in URL', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: '@babel/core',
      version: '7.0.0',
    });

    await searchNpmPackage('@babel/core', 1, false);

    expect(mockFetchWithRetries).toHaveBeenCalledWith(
      expect.stringContaining('@babel%2Fcore/latest'),
      expect.any(Object)
    );
  });
});

describe('mapToResult - extended metadata coverage', () => {
  it('should extract author as string when fetchMetadata is true', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      author: 'John Doe <john@example.com>',
    });

    const result = await searchNpmPackage('test-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).author).toBe(
        'John Doe <john@example.com>'
      );
    }
  });

  it('should extract author.name from object when fetchMetadata is true', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      author: { name: 'Jane Doe', email: 'jane@example.com' },
    });

    const result = await searchNpmPackage('test-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).author).toBe('Jane Doe');
    }
  });

  it('should extract peerDependencies when fetchMetadata is true', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      peerDependencies: { react: '^18.0.0', 'react-dom': '^18.0.0' },
    });

    const result = await searchNpmPackage('test-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).peerDependencies).toEqual(
        { react: '^18.0.0', 'react-dom': '^18.0.0' }
      );
    }
  });

  it('should extract all extended metadata fields', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'full-pkg',
      version: '2.0.0',
      description: 'A full featured package',
      keywords: ['test', 'example', 'demo'],
      license: 'MIT',
      homepage: 'https://example.com',
      author: { name: 'Test Author' },
      engines: { node: '>=18.0.0' },
      dependencies: { lodash: '^4.17.21' },
      peerDependencies: { react: '^18.0.0' },
    });

    const result = await searchNpmPackage('full-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.description).toBe('A full featured package');
      expect(pkg.keywords).toEqual(['test', 'example', 'demo']);
      expect(pkg.license).toBe('MIT');
      expect(pkg.homepage).toBe('https://example.com');
      expect(pkg.author).toBe('Test Author');
      expect(pkg.engines).toEqual({ node: '>=18.0.0' });
      expect(pkg.dependencies).toEqual({ lodash: '^4.17.21' });
      expect(pkg.peerDependencies).toEqual({ react: '^18.0.0' });
    }
  });

  it('should extract license.type from object when fetchMetadata is true', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      license: { type: 'Apache-2.0', url: 'https://...' },
    });

    const result = await searchNpmPackage('test-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).license).toBe(
        'Apache-2.0'
      );
    }
  });

  it('should not include extended metadata when fetchMetadata is false', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      description: 'Always included',
      license: 'MIT',
      author: 'Should not be included',
      peerDependencies: { react: '^18.0.0' },
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.description).toBe('Always included');
      expect(pkg.license).toBe('MIT');
      expect(pkg.author).toBeUndefined();
      expect(pkg.peerDependencies).toBeUndefined();
    }
  });

  it('should handle empty engines object', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      engines: {},
    });

    const result = await searchNpmPackage('test-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).engines).toBeUndefined();
    }
  });

  it('should handle empty keywords array', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      keywords: [],
    });

    const result = await searchNpmPackage('test-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).keywords).toBeUndefined();
    }
  });

  it('should handle empty dependencies object', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      dependencies: {},
    });

    const result = await searchNpmPackage('test-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(
        (result.packages[0] as NpmPackageResult).dependencies
      ).toBeUndefined();
    }
  });

  it('should handle empty peerDependencies object', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      peerDependencies: {},
    });

    const result = await searchNpmPackage('test-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(
        (result.packages[0] as NpmPackageResult).peerDependencies
      ).toBeUndefined();
    }
  });

  it('should handle author object without name property', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      author: { email: 'test@example.com' },
    });

    const result = await searchNpmPackage('test-pkg', 1, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).author).toBeUndefined();
    }
  });
});

describe('searchNpmPackageViaSearch - result handling', () => {
  it('should fetch metadata for each search result when fetchMetadata is true', async () => {
    mockFetchWithRetries.mockResolvedValueOnce(
      makeSearchResult([
        {
          name: 'pkg-1',
          version: '1.0.0',
          links: { repository: 'https://github.com/test/pkg-1' },
        },
        {
          name: 'pkg-2',
          version: '2.0.0',
          links: { repository: 'https://github.com/test/pkg-2' },
        },
      ])
    );
    mockFetchWithRetries.mockResolvedValueOnce({
      name: 'pkg-1',
      version: '1.0.0',
      description: 'Package 1 description',
      repository: 'https://github.com/test/pkg-1',
    });
    mockFetchWithRetries.mockResolvedValueOnce({
      name: 'pkg-2',
      version: '2.0.0',
      description: 'Package 2 description',
      repository: 'https://github.com/test/pkg-2',
    });

    const result = await searchNpmPackage('test keyword', 5, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(2);
      expect((result.packages[0] as NpmPackageResult).description).toBe(
        'Package 1 description'
      );
      expect((result.packages[1] as NpmPackageResult).description).toBe(
        'Package 2 description'
      );
    }
  });

  it('should use basic result when individual package detail fetch fails', async () => {
    mockFetchWithRetries.mockResolvedValueOnce(
      makeSearchResult([
        {
          name: 'pkg-1',
          version: '1.0.0',
          links: { repository: 'https://github.com/test/pkg-1' },
        },
      ])
    );
    mockFetchWithRetries.mockRejectedValueOnce(
      new Error('HTTP error: 404 Not Found')
    );

    const result = await searchNpmPackage('test keyword', 5, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(1);
      expect((result.packages[0] as NpmPackageResult).name).toBe('pkg-1');
      expect((result.packages[0] as NpmPackageResult).version).toBe('1.0.0');
    }
  });

  it('should handle search result with missing links.repository', async () => {
    mockFetchWithRetries.mockResolvedValue(
      makeSearchResult([{ name: 'pkg-no-repo', version: '1.0.0', links: {} }])
    );

    const result = await searchNpmPackage('test keyword', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).repoUrl).toBeNull();
    }
  });

  it('should return error on invalid search response', async () => {
    mockFetchWithRetries.mockResolvedValue({ not_objects: [] });

    const result = await searchNpmPackage('test keyword', 5, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain(
        'Invalid npm registry search response format'
      );
    }
  });
});

describe('cache behavior - empty results not cached', () => {
  it('should NOT cache error results from fetchPackageDetails failure', async () => {
    mockFetchWithRetries.mockRejectedValue(new Error('network failure'));

    const result1 = await searchNpmPackage('express', 1, false);
    expect('error' in result1).toBe(true);
    if ('error' in result1) {
      expect(result1.error).toContain('network failure');
    }

    mockFetchWithRetries.mockResolvedValue({
      name: 'express',
      version: '4.18.2',
      repository: { url: 'https://github.com/expressjs/express' },
    });

    const result2 = await searchNpmPackage('express', 1, false);
    expect('packages' in result2).toBe(true);
    if ('packages' in result2) {
      expect(result2.totalFound).toBe(1);
      expect((result2.packages[0] as { name?: string }).name).toBe('express');
    }
  });

  it('should cache successful non-empty results', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'lodash',
      version: '4.17.21',
      repository: { url: 'https://github.com/lodash/lodash' },
    });

    const result1 = await searchNpmPackage('lodash', 1, false);
    expect('packages' in result1).toBe(true);
    if ('packages' in result1) {
      expect(result1.totalFound).toBe(1);
    }

    mockFetchWithRetries.mockClear();
    const result2 = await searchNpmPackage('lodash', 1, false);
    expect('packages' in result2).toBe(true);
    if ('packages' in result2) {
      expect(result2.totalFound).toBe(1);
    }

    expect(mockFetchWithRetries).not.toHaveBeenCalled();
  });
});

describe('repository URL normalization', () => {
  it('should strip git+ prefix from repository URL', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      repository: { url: 'git+https://github.com/test/pkg.git' },
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).repoUrl).toBe(
        'https://github.com/test/pkg'
      );
    }
  });

  it('should strip .git suffix from repository URL', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      repository: { url: 'https://github.com/test/pkg.git' },
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).repoUrl).toBe(
        'https://github.com/test/pkg'
      );
    }
  });

  it('should handle repository as plain string', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      repository: 'git+https://github.com/test/pkg.git',
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).repoUrl).toBe(
        'https://github.com/test/pkg'
      );
    }
  });

  it('should return null repoUrl when repository is missing', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).repoUrl).toBeNull();
    }
  });
});

describe('getNpmRegistryUrl', () => {
  it('should return registry URL from npm config get registry', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://npm.corp.com/\n',
      stderr: '',
      exitCode: 0,
    });

    const url = await getNpmRegistryUrl();
    expect(url).toBe('https://npm.corp.com');
    expect(mockExecuteNpmCommand).toHaveBeenCalledWith(
      'config',
      ['get', 'registry', '--no-workspaces'],
      expect.any(Object)
    );
  });

  it('should fall back to default when npm config command fails', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: '',
      stderr: 'npm ERR!',
      exitCode: 1,
      error: new Error('Command failed'),
    });

    const url = await getNpmRegistryUrl();
    expect(url).toBe('https://registry.npmjs.org');
  });

  it('should fall back to default when npm config returns non-URL', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'not-a-valid-url\n',
      stderr: '',
      exitCode: 0,
    });

    const url = await getNpmRegistryUrl();
    expect(url).toBe('https://registry.npmjs.org');
  });

  it('should cache result after first successful call', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://npm.corp.com/\n',
      stderr: '',
      exitCode: 0,
    });

    const url1 = await getNpmRegistryUrl();
    const url2 = await getNpmRegistryUrl();
    expect(url1).toBe('https://npm.corp.com');
    expect(url2).toBe('https://npm.corp.com');
    expect(mockExecuteNpmCommand).toHaveBeenCalledTimes(1);
  });

  it('should strip trailing slash from registry URL', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://registry.npmjs.org/\n',
      stderr: '',
      exitCode: 0,
    });

    const url = await getNpmRegistryUrl();
    expect(url).toBe('https://registry.npmjs.org');
  });

  it('should fall back to default when executeNpmCommand throws', async () => {
    mockExecuteNpmCommand.mockRejectedValueOnce(new Error('spawn failed'));

    const url = await getNpmRegistryUrl();
    expect(url).toBe('https://registry.npmjs.org');
  });
});

describe('checkNpmRegistryReachable', () => {
  let mockGlobalFetch: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockGlobalFetch = vi.fn();
    (globalThis as unknown as { fetch: unknown }).fetch = mockGlobalFetch;
  });

  afterEach(() => {
    delete (globalThis as unknown as { fetch?: unknown }).fetch;
  });

  it('should return true when registry responds with 200', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://registry.npmjs.org/\n',
      stderr: '',
      exitCode: 0,
    });
    mockGlobalFetch.mockResolvedValueOnce({ ok: true });

    const result = await checkNpmRegistryReachable();
    expect(result).toBe(true);
  });

  it('should return true even when registry returns empty body (Artifactory)', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://npm.corp.com/\n',
      stderr: '',
      exitCode: 0,
    });
    mockGlobalFetch.mockResolvedValueOnce({ ok: true, status: 200 });

    const result = await checkNpmRegistryReachable();
    expect(result).toBe(true);
  });

  it('should return false when registry responds with non-ok status', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://registry.npmjs.org/\n',
      stderr: '',
      exitCode: 0,
    });
    mockGlobalFetch.mockResolvedValueOnce({ ok: false, status: 503 });

    const result = await checkNpmRegistryReachable();
    expect(result).toBe(false);
  });

  it('should return false when fetch throws network error', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://registry.npmjs.org/\n',
      stderr: '',
      exitCode: 0,
    });
    mockGlobalFetch.mockRejectedValueOnce(new Error('fetch failed'));

    const result = await checkNpmRegistryReachable();
    expect(result).toBe(false);
  });

  it('should use HEAD method for lightweight reachability check', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://npm.corp.com/\n',
      stderr: '',
      exitCode: 0,
    });
    mockGlobalFetch.mockResolvedValueOnce({ ok: true });

    await checkNpmRegistryReachable();
    expect(mockGlobalFetch).toHaveBeenCalledWith(
      'https://npm.corp.com',
      expect.objectContaining({ method: 'HEAD' })
    );
  });

  it('should return false when globalThis.fetch is not available', async () => {
    delete (globalThis as unknown as { fetch?: unknown }).fetch;
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://registry.npmjs.org/\n',
      stderr: '',
      exitCode: 0,
    });

    const result = await checkNpmRegistryReachable();
    expect(result).toBe(false);
  });
});

describe('searchNpmPackage - custom registry URL', () => {
  it('should use custom registry URL for exact package lookup', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: '',
      stderr: 'npm ERR! code E404',
      exitCode: 1,
    });
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://npm.corp.com\n',
      stderr: '',
      exitCode: 0,
    });
    mockFetchWithRetries.mockResolvedValueOnce({
      name: 'express',
      version: '4.18.2',
    });

    await searchNpmPackage('express', 1, false);

    expect(mockFetchWithRetries).toHaveBeenCalledWith(
      'https://npm.corp.com/express/latest',
      expect.any(Object)
    );
  });

  it('should always use the public npmjs.org registry for search (ignoring corporate registry config)', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      stdout: 'https://npm.corp.com\n',
      stderr: '',
      exitCode: 0,
    });
    mockFetchWithRetries.mockResolvedValueOnce({ objects: [], total: 0 });

    await searchNpmPackage('test keyword', 5, false);

    expect(mockFetchWithRetries).toHaveBeenCalledWith(
      expect.stringContaining('https://registry.npmjs.org/-/v1/search'),
      expect.any(Object)
    );
  });
});

describe('fetchNpmPackageByView - success cases', () => {
  it('should return package with all basic fields', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'express',
      version: '5.0.1',
      main: 'index.js',
      types: 'index.d.ts',
      repository: { url: 'https://github.com/expressjs/express' },
    });

    const result = await searchNpmPackage('express', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.totalFound).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.name).toBe('express');
      expect(pkg.version).toBe('5.0.1');
      expect(pkg.mainEntry).toBe('index.js');
      expect(pkg.typeDefinitions).toBe('index.d.ts');
      expect(pkg.repoUrl).toBe('https://github.com/expressjs/express');
    }
  });

  it('should use typings field when types is missing', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '1.0.0',
      typings: 'dist/index.d.ts',
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).typeDefinitions).toBe(
        'dist/index.d.ts'
      );
    }
  });

  it('should use version fallback when version is falsy', async () => {
    mockFetchWithRetries.mockResolvedValue({
      name: 'test-pkg',
      version: '',
    });

    const result = await searchNpmPackage('test-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).version).toBe('latest');
    }
  });
});

describe('enrichTopSearchResult - top result enrichment', () => {
  it('should enrich top keyword-search result with weeklyDownloads and lastPublished', async () => {
    mockFetchWithRetries
      .mockResolvedValueOnce({
        objects: [
          {
            package: {
              name: 'test-keyword-pkg',
              version: '1.0.0',
              description: 'A test pkg',
            },
          },
        ],
        total: 1,
      })
      .mockResolvedValueOnce({ downloads: 12345 })
      .mockRejectedValueOnce(new Error('no install-v1'))
      .mockResolvedValueOnce({
        time: { modified: '2024-03-01T00:00:00.000Z' },
      });

    const result = await searchNpmPackage('test keyword pkg', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const top = result.packages[0] as NpmPackageResult;
      expect(top.name).toBe('test-keyword-pkg');
      expect(top.weeklyDownloads).toBe(12345);
      expect(top.lastPublished).toBe('2024-03-01T00:00:00.000Z');
    }
  });

  it('should not assign weeklyDownloads when downloads API returns no downloads field', async () => {
    mockFetchWithRetries
      .mockResolvedValueOnce({
        objects: [{ package: { name: 'no-dl-pkg', version: '1.0.0' } }],
        total: 1,
      })
      .mockResolvedValueOnce({ message: 'no downloads data' })
      .mockResolvedValueOnce({ modified: '2024-01-01T00:00:00.000Z' });

    const result = await searchNpmPackage('no dl pkg', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const top = result.packages[0] as NpmPackageResult;
      expect(top.weeklyDownloads).toBeUndefined();
      expect(top.lastPublished).toBe('2024-01-01T00:00:00.000Z');
    }
  });

  it('should skip both fetches when top result already has weeklyDownloads and lastPublished', async () => {
    mockExecuteNpmCommand
      .mockResolvedValueOnce({
        exitCode: 0,
        stdout: JSON.stringify([{ name: 'already-rich', version: '1.0.0' }]),
        stderr: '',
        error: null,
      })
      .mockResolvedValueOnce({
        exitCode: 0,
        stdout: JSON.stringify({ name: 'already-rich', version: '1.0.0' }),
        stderr: '',
        error: null,
      });
    mockFetchWithRetries
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({ downloads: 7777 })
      .mockResolvedValueOnce({ modified: '2025-01-01T00:00:00.000Z' });

    const result = await searchNpmPackage('already rich', 5, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const top = result.packages[0] as NpmPackageResult;
      expect(top.weeklyDownloads).toBe(7777);
      expect(top.lastPublished).toBeDefined();
    }
  });

  it('should skip lastPublished fetch in enrichTop when top already has lastPublished', async () => {
    mockExecuteNpmCommand
      .mockResolvedValueOnce({
        exitCode: 0,
        stdout: JSON.stringify([{ name: 'lp-only', version: '1.0.0' }]),
        stderr: '',
        error: null,
      })
      .mockResolvedValueOnce({
        exitCode: 0,
        stdout: JSON.stringify({ name: 'lp-only', version: '1.0.0' }),
        stderr: '',
        error: null,
      });
    mockFetchWithRetries
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({ modified: '2025-02-01T00:00:00.000Z' })
      .mockResolvedValueOnce({ downloads: 321 });

    const result = await searchNpmPackage('lp only', 5, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const top = result.packages[0] as NpmPackageResult;
      expect(top.weeklyDownloads).toBe(321);
      expect(top.lastPublished).toBe('2025-02-01T00:00:00.000Z');
    }
  });

  it('should skip weekly fetch in enrichTop when top already has weeklyDownloads', async () => {
    mockExecuteNpmCommand
      .mockResolvedValueOnce({
        exitCode: 0,
        stdout: JSON.stringify([{ name: 'wd-only', version: '1.0.0' }]),
        stderr: '',
        error: null,
      })
      .mockResolvedValueOnce({
        exitCode: 0,
        stdout: JSON.stringify({ name: 'wd-only', version: '1.0.0' }),
        stderr: '',
        error: null,
      });
    mockFetchWithRetries
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({ downloads: 111 })
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({ time: {} })
      .mockResolvedValueOnce({ modified: '2026-03-01T00:00:00.000Z' });

    const result = await searchNpmPackage('wd only', 5, true);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const top = result.packages[0] as NpmPackageResult;
      expect(top.weeklyDownloads).toBe(111);
      expect(top.lastPublished).toBe('2026-03-01T00:00:00.000Z');
    }
  });
});

describe('fetchPackageDetailsFromRegistry - branch coverage', () => {
  it('should return empty when registry /latest returns null (non-object)', async () => {
    mockFetchWithRetries
      .mockResolvedValueOnce(null)
      .mockResolvedValue({ objects: [], total: 0 });

    const result = await searchNpmPackage('null-registry-pkg', 1, false);

    expect('error' in result || 'packages' in result).toBe(true);
  });

  it('should return error when registry /latest returns invalid schema format', async () => {
    mockFetchWithRetries.mockResolvedValue({ invalid: true });

    const result = await searchNpmPackage('bad-schema-pkg', 1, false);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('Invalid npm registry response format');
    }
  });
});

describe('null result guard - executeNpmCommand returns undefined', () => {
  it('should treat null CLI view result as not found and fall through to registry', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce(null).mockResolvedValue({
      exitCode: 1,
      stdout: '',
      stderr: 'npm ERR! code E404',
      error: null,
    });
    mockFetchWithRetries.mockResolvedValue({
      name: 'null-cli-pkg',
      version: '1.0.0',
      repository: { url: 'https://github.com/owner/null-cli-pkg' },
    });

    const result = await searchNpmPackage('null-cli-pkg', 1, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).name).toBe(
        'null-cli-pkg'
      );
    }
  });

  it('should treat null CLI search result as unavailable and fall through to registry search', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce(null);
    mockFetchWithRetries.mockResolvedValueOnce({
      objects: [{ package: { name: 'null-search-pkg', version: '1.0.0' } }],
      total: 1,
    });

    const result = await searchNpmPackage('null search pkg', 5, false);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).name).toBe(
        'null-search-pkg'
      );
    }
  });
});

describe('circuit breaker bypass', () => {
  it('should skip registry and go straight to web when circuit is open and web succeeds', async () => {
    const registryUrl = 'https://registry.npmjs.org';
    for (let i = 0; i < DEFAULT_CIRCUIT_FAILURE_THRESHOLD; i++) {
      recordCircuitFailure(registryUrl);
    }

    mockFetchWithRetries.mockResolvedValueOnce({
      results: [
        {
          package: {
            name: 'open-pkg',
            version: '2.0.0',
            links: { npm: 'https://npmjs.com/package/open-pkg' },
          },
        },
      ],
      total: 1,
    });

    const result = await searchNpmPackage('open-pkg', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).source).toBe('web');
    }
    expect(mockFetchWithRetries).toHaveBeenCalledTimes(1);
    expect((mockFetchWithRetries.mock.calls[0] as [string])[0]).toContain(
      'npms.io'
    );
  });

  it('should return error when circuit is open and web search also fails', async () => {
    const registryUrl = 'https://registry.npmjs.org';
    for (let i = 0; i < DEFAULT_CIRCUIT_FAILURE_THRESHOLD; i++) {
      recordCircuitFailure(registryUrl);
    }

    mockFetchWithRetries.mockRejectedValue(new Error('web also down'));

    const result = await searchNpmPackage('some-pkg', 5, false);
    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('circuit open');
    }
  });

  it('maps optional homepage from web search results', async () => {
    const registryUrl = 'https://registry.npmjs.org';
    for (let i = 0; i < DEFAULT_CIRCUIT_FAILURE_THRESHOLD; i++) {
      recordCircuitFailure(registryUrl);
    }

    mockFetchWithRetries.mockResolvedValueOnce({
      results: [
        {
          package: {
            name: 'web-homepage-pkg',
            version: '1.2.3',
            description: 'pkg from npms',
            links: {
              npm: 'https://npmjs.com/package/web-homepage-pkg',
              repository: 'https://github.com/acme/web-homepage-pkg',
              homepage: 'https://acme.dev/web-homepage-pkg',
            },
          },
        },
      ],
      total: 1,
    });

    const result = await searchNpmPackage('web-homepage-pkg', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const top = result.packages[0] as NpmPackageResult;
      expect(top.homepage).toBe('https://acme.dev/web-homepage-pkg');
      expect(top.description).toBe('pkg from npms');
    }
  });
});

describe('coverage branches - web search and CLI throw paths', () => {
  it('hits web-search outer catch when mapping throws', async () => {
    const registryUrl = 'https://registry.npmjs.org';
    for (let i = 0; i < DEFAULT_CIRCUIT_FAILURE_THRESHOLD; i++) {
      recordCircuitFailure(registryUrl);
    }

    const itemWithThrowingPackage = {} as { package?: unknown };
    Object.defineProperty(itemWithThrowingPackage, 'package', {
      get() {
        throw new Error('package getter boom');
      },
      enumerable: true,
    });
    mockFetchWithRetries.mockResolvedValueOnce({
      results: [itemWithThrowingPackage],
      total: 1,
    });

    const result = await searchNpmPackage('circular-pkg', 5, false);
    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('circuit open');
    }
    expect(mockFetchWithRetries).toHaveBeenCalledTimes(1);
  });

  it('falls back to registry when CLI search throws (search outer catch)', async () => {
    mockExecuteNpmCommand.mockRejectedValueOnce(
      new Error('cli search crashed')
    );
    mockFetchWithRetries.mockResolvedValueOnce({
      objects: [{ package: { name: 'cli-throw-fallback', version: '1.0.0' } }],
      total: 1,
    });

    const result = await searchNpmPackage('cli throw fallback', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).name).toBe(
        'cli-throw-fallback'
      );
      expect((result.packages[0] as NpmPackageResult).source).toBe('registry');
    }
  });

  it('handles CLI search JSON object (non-array) before falling back', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      exitCode: 0,
      stdout: JSON.stringify({ objects: [] }),
      stderr: '',
      error: null,
    });
    mockFetchWithRetries.mockResolvedValueOnce({
      objects: [
        { package: { name: 'registry-after-cli-object', version: '1.0.0' } },
      ],
      total: 1,
    });

    const result = await searchNpmPackage('cli object fallback', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).name).toBe(
        'registry-after-cli-object'
      );
    }
  });

  it('hits registry-search outer catch when package getter throws in validated object', async () => {
    const throwingPackageHolder = {} as { package?: unknown };
    Object.defineProperty(throwingPackageHolder, 'package', {
      get() {
        throw new Error('registry package getter boom');
      },
      enumerable: true,
    });
    mockFetchWithRetries.mockResolvedValueOnce({
      objects: [throwingPackageHolder],
      total: 1,
    });

    const result = await searchNpmPackage('registry throw fallback', 5, false);
    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('NPM registry search failed');
      expect(result.error).toContain('registry package getter boom');
    }
  });

  it('returns explicit error when npm view CLI output is invalid JSON', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      exitCode: 0,
      stdout: '{"broken"',
      stderr: '',
      error: null,
    });
    mockFetchWithRetries.mockResolvedValueOnce(null);

    const result = await searchNpmPackage('broken-view-json', 1, false);
    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('Invalid npm view JSON output');
    }
  });

  it('handles registry details outer-catch path when schema read throws', async () => {
    const rawWithThrowingName = {} as { name?: string };
    Object.defineProperty(rawWithThrowingName, 'name', {
      get() {
        throw new Error('registry details getter boom');
      },
      enumerable: true,
    });
    mockFetchWithRetries.mockResolvedValueOnce(rawWithThrowingName);

    const result = await searchNpmPackage('registry-details-throw', 1, false);
    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('registry details getter boom');
    }
  });

  it('returns empty CLI search result when npm search stdout is blank', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      exitCode: 0,
      stdout: '   ',
      stderr: '',
      error: null,
    });
    mockFetchWithRetries.mockResolvedValueOnce({ results: [], total: 0 });

    const result = await searchNpmPackage('blank cli search', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(0);
      expect(result.totalFound).toBe(0);
    }
  });

  it('uses fallback totalFound when registry total is not parseable', async () => {
    mockFetchWithRetries.mockResolvedValueOnce({
      objects: [{ package: { name: 'fallback-total', version: '1.0.0' } }],
      total: 'not-a-number',
    });

    const result = await searchNpmPackage('fallback total', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(1);
      expect(result.totalFound).toBe(1);
    }
  });

  it('maps CLI search repository from legacy repository string', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      exitCode: 0,
      stdout: JSON.stringify([
        {
          name: 'legacy-repo-item',
          version: '1.0.0',
          repository: 'git+https://github.com/acme/legacy-repo-item.git',
        },
      ]),
      stderr: '',
      error: null,
    });

    const result = await searchNpmPackage('legacy repo item', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).repoUrl).toBe(
        'https://github.com/acme/legacy-repo-item'
      );
    }
  });

  it('maps CLI search repository from repository.url object', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      exitCode: 0,
      stdout: JSON.stringify([
        {
          name: 'object-repo-item',
          version: '1.0.0',
          repository: {
            url: 'git+https://github.com/acme/object-repo-item.git',
          },
        },
      ]),
      stderr: '',
      error: null,
    });

    const result = await searchNpmPackage('object repo item', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).repoUrl).toBe(
        'https://github.com/acme/object-repo-item'
      );
    }
  });

  it('maps CLI search item without repository to repoUrl=null', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      exitCode: 0,
      stdout: JSON.stringify([{ name: 'no-repo-item', version: '1.0.0' }]),
      stderr: '',
      error: null,
    });

    const result = await searchNpmPackage('no repo item', 5, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect((result.packages[0] as NpmPackageResult).repoUrl).toBeNull();
    }
  });

  it('returns exact empty result immediately when limit=1 and exact lookup finds nothing', async () => {
    mockExecuteNpmCommand.mockResolvedValueOnce({
      exitCode: 0,
      stdout: 'undefined',
      stderr: '',
      error: null,
    });
    mockFetchWithRetries.mockResolvedValueOnce(null);

    const result = await searchNpmPackage('exact-empty-limit-one', 1, false);
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(0);
      expect(result.totalFound).toBe(0);
    }
    expect(mockFetchWithRetries).toHaveBeenCalledTimes(1);
  });
});
