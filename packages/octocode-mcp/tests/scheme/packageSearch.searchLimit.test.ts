import { describe, it, expect } from 'vitest';
import { PackageSearchBulkQueryLocalSchema } from '../../src/tools/package_search/scheme.js';

function parsedQuery(query: Record<string, unknown>): Record<string, unknown> {
  const parsed = PackageSearchBulkQueryLocalSchema.parse({ queries: [query] });
  return parsed.queries[0] as Record<string, unknown>;
}

describe('packageSearch pagination (page-based exact fields)', () => {
  it('defaults page to 1 when omitted', () => {
    expect(parsedQuery({ packageName: 'lodash' }).page).toBe(1);
  });

  it('accepts explicit page=2', () => {
    expect(parsedQuery({ packageName: 'lodash', page: 2 }).page).toBe(2);
  });

  it('does not expose itemsPerPage or searchLimit', () => {
    const q = parsedQuery({ packageName: 'lodash' });
    expect('itemsPerPage' in q).toBe(false);
    expect('searchLimit' in q).toBe(false);
    expect('limit' in q).toBe(false);
  });

  it('accepts explicit detail modes', () => {
    expect(parsedQuery({ packageName: 'lodash', mode: 'lean' }).mode).toBe(
      'lean'
    );
    expect(parsedQuery({ packageName: 'lodash', mode: 'full' }).mode).toBe(
      'full'
    );
    expect(parsedQuery({ packageName: 'lodash', mode: 'smart' }).mode).toBe(
      'smart'
    );
  });

  it('rejects unsupported detail modes', () => {
    expect(() =>
      parsedQuery({ packageName: 'lodash', mode: 'compact' })
    ).toThrow();
  });
});
