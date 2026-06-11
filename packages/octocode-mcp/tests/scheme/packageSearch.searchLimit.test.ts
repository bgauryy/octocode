import { describe, it, expect } from 'vitest';
import { PackageSearchBulkQueryLocalSchema } from '../../src/tools/package_search/scheme.js';

function parsedQuery(query: Record<string, unknown>): Record<string, unknown> {
  const parsed = PackageSearchBulkQueryLocalSchema.parse({ queries: [query] });
  return parsed.queries[0] as Record<string, unknown>;
}

describe('packageSearch pagination (page-based exact fields)', () => {
  it('defaults page to 1 when omitted', () => {
    expect(parsedQuery({ name: 'lodash' }).page).toBe(1);
  });

  it('accepts explicit page=2', () => {
    expect(parsedQuery({ name: 'lodash', page: 2 }).page).toBe(2);
  });

  it('does not expose itemsPerPage or searchLimit', () => {
    const q = parsedQuery({ name: 'lodash' });
    expect('itemsPerPage' in q).toBe(false);
    expect('searchLimit' in q).toBe(false);
    expect('limit' in q).toBe(false);
  });

  it('accepts explicit detail modes', () => {
    expect(parsedQuery({ name: 'lodash', mode: 'lean' }).mode).toBe('lean');
    expect(parsedQuery({ name: 'lodash', mode: 'full' }).mode).toBe('full');
    expect(parsedQuery({ name: 'lodash', mode: 'smart' }).mode).toBe('smart');
  });

  it('rejects unsupported detail modes', () => {
    expect(() => parsedQuery({ name: 'lodash', mode: 'compact' })).toThrow();
  });
});
