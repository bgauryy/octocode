import { describe, it, expect } from 'vitest';
import { ArtifactSearchBulkQueryLocalSchema } from '@octocodeai/config/schema';

const GOAL = 'test goal';
const REASONING = 'exercise artifact schema';

function parsedQuery(query: Record<string, unknown>): Record<string, unknown> {
  const parsed = ArtifactSearchBulkQueryLocalSchema.parse({
    queries: [{ mainGoal: GOAL, reasoning: REASONING, ...query }],
  });
  return parsed.queries[0] as Record<string, unknown>;
}

describe('artifactSearch schema', () => {
  it('keeps exact package lookup unpaginated', () => {
    expect(parsedQuery({ ecosystem: 'npm', packageName: 'lodash' })).toEqual({
      mainGoal: 'test goal',
      reasoning: REASONING,
      debug: false,
      ecosystem: 'npm',
      packageName: 'lodash',
    });
    expect(() =>
      parsedQuery({ ecosystem: 'npm', packageName: 'lodash', page: 2 })
    ).toThrow(/Remove 'page': it applies only with keywords/);
  });

  it('accepts pagination for keyword discovery', () => {
    expect(
      parsedQuery({
        ecosystem: 'npm',
        keywords: ['schema', 'validation'],
        page: 2,
        pageSize: 25,
      })
    ).toMatchObject({ page: 2, pageSize: 25 });
  });

  it('does not expose itemsPerPage, searchLimit, limit, or verbose', () => {
    const q = parsedQuery({ ecosystem: 'npm', packageName: 'lodash' });
    expect('itemsPerPage' in q).toBe(false);
    expect('searchLimit' in q).toBe(false);
    expect('limit' in q).toBe(false);
    expect('verbose' in q).toBe(false);
  });

  it('rejects unknown fields', () => {
    expect(() =>
      parsedQuery({
        ecosystem: 'npm',
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
    expect(
      parsedQuery({ ecosystem: type, packageName: 'example' })
    ).toMatchObject({
      ecosystem: type,
      packageName: 'example',
    });
  });

  it('rejects missing or aggregate ecosystem types', () => {
    for (const query of [
      { packageName: 'react' },
      { ecosystem: 'all', keywords: ['http'] },
    ]) {
      expect(() => parsedQuery(query)).toThrow();
    }
  });

  it('rejects unsupported PyPI keyword discovery at the schema', () => {
    // PyPI has no keyword search; the structural contract excludes it from
    // the discovery branch, so the unservable call never reaches the runtime.
    expect(() =>
      parsedQuery({ ecosystem: 'pypi', keywords: ['http'] })
    ).toThrow();
    expect(
      parsedQuery({ ecosystem: 'pypi', packageName: 'requests' })
    ).toMatchObject({ ecosystem: 'pypi', packageName: 'requests' });
  });

  it('limits custom registry routing to npm', () => {
    expect(
      parsedQuery({
        ecosystem: 'npm',
        packageName: '@example/widget',
        registryUrl: 'https://registry.example.com/',
      })
    ).toHaveProperty('registryUrl');
    expect(() =>
      parsedQuery({
        ecosystem: 'pypi',
        packageName: 'requests',
        registryUrl: 'https://registry.example.com/',
      })
    ).toThrow();
  });

  it('rejects discovery pagination on exact lookup', () => {
    for (const pagination of [{ page: 2 }, { pageSize: 10 }]) {
      expect(() =>
        parsedQuery({ ecosystem: 'npm', packageName: 'react', ...pagination })
      ).toThrow();
    }
  });
});
