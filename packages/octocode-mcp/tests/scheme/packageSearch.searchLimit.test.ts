import { describe, it, expect } from 'vitest';
import { NpmSearchBulkQueryLocalSchema } from '../../../octocode-tools-core/src/tools/package_search/scheme.js';

function parsedQuery(query: Record<string, unknown>): Record<string, unknown> {
  const parsed = NpmSearchBulkQueryLocalSchema.parse({ queries: [query] });
  return parsed.queries[0] as Record<string, unknown>;
}

describe('npmSearch schema', () => {
  it('defaults page to 1 when omitted', () => {
    expect(parsedQuery({ packageName: 'lodash' }).page).toBe(1);
  });

  it('accepts explicit page=2', () => {
    expect(parsedQuery({ packageName: 'lodash', page: 2 }).page).toBe(2);
  });

  it('does not expose itemsPerPage, searchLimit, limit, or verbose', () => {
    const q = parsedQuery({ packageName: 'lodash' });
    expect('itemsPerPage' in q).toBe(false);
    expect('searchLimit' in q).toBe(false);
    expect('limit' in q).toBe(false);
    expect('verbose' in q).toBe(false);
  });

  it('accepts mode from the core npmSearch schema', () => {
    const q = parsedQuery({
      packageName: 'lodash',
      mode: 'lean',
    });
    expect(q.mode).toBe('lean');
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
