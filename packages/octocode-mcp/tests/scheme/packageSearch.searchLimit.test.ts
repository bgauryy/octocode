import { describe, it, expect } from 'vitest';
import { PackageSearchBulkQueryLocalSchema } from '../../src/tools/package_search/scheme.js';

function parsedQuery(query: Record<string, unknown>): Record<string, unknown> {
  const parsed = PackageSearchBulkQueryLocalSchema.parse({ queries: [query] });
  return parsed.queries[0] as Record<string, unknown>;
}

describe('packageSearch schema', () => {
  it('defaults page to 1 when omitted', () => {
    expect(parsedQuery({ packageName: 'lodash' }).page).toBe(1);
  });

  it('accepts explicit page=2', () => {
    expect(parsedQuery({ packageName: 'lodash', page: 2 }).page).toBe(2);
  });

  it('does not expose itemsPerPage, searchLimit, or mode', () => {
    const q = parsedQuery({ packageName: 'lodash' });
    expect('itemsPerPage' in q).toBe(false);
    expect('searchLimit' in q).toBe(false);
    expect('limit' in q).toBe(false);
    expect('mode' in q).toBe(false);
    expect('verbose' in q).toBe(false);
  });

  it('unknown fields are stripped (mode, verbose)', () => {
    // Zod strips unknown fields — neither mode nor verbose is part of the schema
    const q = parsedQuery({
      packageName: 'lodash',
      mode: 'lean',
      verbose: true,
    });
    expect('mode' in q).toBe(false);
    expect('verbose' in q).toBe(false);
  });
});
