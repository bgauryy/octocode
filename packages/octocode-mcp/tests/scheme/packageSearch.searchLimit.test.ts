import { describe, it, expect } from 'vitest';
import { PackageSearchBulkQueryLocalSchema } from '../../src/scheme/remoteSchemaOverlay.js';

function parsedQuery(query: Record<string, unknown>): Record<string, unknown> {
  const parsed = PackageSearchBulkQueryLocalSchema.parse({ queries: [query] });
  return parsed.queries[0] as Record<string, unknown>;
}

describe('packageSearch pagination (page-based, no legacy itemsPerPage/searchLimit)', () => {
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
});
