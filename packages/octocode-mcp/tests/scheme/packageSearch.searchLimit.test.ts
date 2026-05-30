/**
 * Regression: packageSearch `limit` vs `searchLimit` mapping.
 *
 * The overlay transform used to do `searchLimit: limit` unconditionally, so
 * `limit`'s default (5) silently clobbered an explicit caller-supplied
 * `searchLimit`. The fix honors an explicit `searchLimit` and only falls back
 * to `limit` when `searchLimit` is omitted.
 */
import { describe, it, expect } from 'vitest';
import { PackageSearchBulkQueryLocalSchema } from '../../src/scheme/remoteSchemaOverlay.js';

function resolvedSearchLimit(
  query: Record<string, unknown>
): number | undefined {
  const parsed = PackageSearchBulkQueryLocalSchema.parse({ queries: [query] });
  return (parsed.queries[0] as { searchLimit?: number }).searchLimit;
}

describe('packageSearch limit/searchLimit resolution', () => {
  it('honors an explicit searchLimit over the defaulted limit', () => {
    expect(resolvedSearchLimit({ name: 'lodash', searchLimit: 1 })).toBe(1);
  });

  it('falls back to limit when searchLimit is omitted', () => {
    expect(resolvedSearchLimit({ name: 'lodash', limit: 2 })).toBe(2);
  });

  it('defaults to 5 when neither limit nor searchLimit is provided', () => {
    expect(resolvedSearchLimit({ name: 'lodash' })).toBe(5);
  });

  it('prefers an explicit searchLimit even when limit is also set', () => {
    expect(
      resolvedSearchLimit({ name: 'lodash', limit: 5, searchLimit: 1 })
    ).toBe(1);
  });
});
