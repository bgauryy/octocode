import { describe, it, expect } from 'vitest';
import { ArtifactSearchBulkQueryLocalSchema } from '@octocodeai/octocode-core/schema';

function parsedQuery(query: Record<string, unknown>): Record<string, unknown> {
  const parsed = ArtifactSearchBulkQueryLocalSchema.parse({ queries: [query] });
  return parsed.queries[0] as Record<string, unknown>;
}

describe('artifactSearch schema', () => {
  it('keeps exact package lookup unpaginated', () => {
    expect(parsedQuery({ type: 'npm', packageName: 'lodash' })).toEqual({
      debug: false,
      type: 'npm',
      packageName: 'lodash',
    });
    expect(() =>
      parsedQuery({ type: 'npm', packageName: 'lodash', page: 2 })
    ).toThrow(/Unrecognized key/);
  });

  it('accepts pagination for keyword discovery', () => {
    expect(
      parsedQuery({
        type: 'npm',
        keywords: ['schema', 'validation'],
        cursor: 'opaque',
        pageSize: 25,
      })
    ).toMatchObject({ cursor: 'opaque', pageSize: 25 });
  });

  it('does not expose itemsPerPage, searchLimit, limit, or verbose', () => {
    const q = parsedQuery({ type: 'npm', packageName: 'lodash' });
    expect('itemsPerPage' in q).toBe(false);
    expect('searchLimit' in q).toBe(false);
    expect('limit' in q).toBe(false);
    expect('verbose' in q).toBe(false);
  });

  it('rejects unknown fields', () => {
    expect(() =>
      parsedQuery({
        type: 'npm',
        packageName: 'lodash',
        verbose: true,
      })
    ).toThrow(/Unrecognized key/);
  });

  it.each([
    'npm',
    'pypi',
    'crates',
    'maven',
    'nuget',
    'go',
    'packagist',
    'rubygems',
  ])('requires an explicit %s ecosystem for exact lookup', type => {
    expect(parsedQuery({ type, packageName: 'example' })).toMatchObject({
      type,
      packageName: 'example',
    });
  });

  it('rejects missing or aggregate ecosystem types', () => {
    for (const query of [
      { packageName: 'react' },
      { type: 'all', keywords: ['http'] },
    ]) {
      expect(() => parsedQuery(query)).toThrow();
    }
  });

  it('leaves unsupported PyPI discovery to the typed runtime capability response', () => {
    expect(parsedQuery({ type: 'pypi', keywords: ['http'] })).toMatchObject({
      type: 'pypi',
      keywords: ['http'],
    });
  });

  it('limits custom registry routing to npm', () => {
    expect(
      parsedQuery({
        type: 'npm',
        packageName: '@example/widget',
        registry: 'https://registry.example.com/',
      })
    ).toHaveProperty('registry');
    expect(() =>
      parsedQuery({
        type: 'pypi',
        packageName: 'requests',
        registry: 'https://registry.example.com/',
      })
    ).toThrow();
  });

  it('rejects discovery pagination on exact lookup', () => {
    for (const pagination of [{ cursor: 'opaque' }, { pageSize: 10 }]) {
      expect(() =>
        parsedQuery({ type: 'npm', packageName: 'react', ...pagination })
      ).toThrow();
    }
  });
});
