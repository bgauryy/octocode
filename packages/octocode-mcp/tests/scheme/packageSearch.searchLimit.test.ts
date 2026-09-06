import { describe, it, expect } from 'vitest';
import { NpmSearchBulkQueryLocalSchema } from '../../../octocode-tools-core/src/tools/package_search/scheme.js';

function parsedQuery(query: Record<string, unknown>): Record<string, unknown> {
  const parsed = NpmSearchBulkQueryLocalSchema.parse({ queries: [query] });
  return parsed.queries[0] as Record<string, unknown>;
}

describe('npmSearch schema', () => {
  it('keeps exact package lookup unpaginated', () => {
    expect(parsedQuery({ packageName: 'lodash' })).toEqual({
      packageName: 'lodash',
    });
    expect(() => parsedQuery({ packageName: 'lodash', page: 2 })).toThrow(
      /Unrecognized key/
    );
  });

  it('accepts pagination for keyword discovery', () => {
    expect(
      parsedQuery({ keywords: ['schema', 'validation'], page: 2, pageSize: 25 })
    ).toMatchObject({ page: 2, pageSize: 25 });
  });

  it('does not expose itemsPerPage, searchLimit, limit, or verbose', () => {
    const q = parsedQuery({ packageName: 'lodash' });
    expect('itemsPerPage' in q).toBe(false);
    expect('searchLimit' in q).toBe(false);
    expect('limit' in q).toBe(false);
    expect('verbose' in q).toBe(false);
  });

  it('rejects unknown fields', () => {
    expect(() =>
      parsedQuery({
        packageName: 'lodash',
        verbose: true,
      })
    ).toThrow(/Unrecognized key/);
  });
});
