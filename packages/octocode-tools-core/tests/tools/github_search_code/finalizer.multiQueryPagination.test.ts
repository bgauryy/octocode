import { describe, expect, it } from 'vitest';

import { buildGhSearchCodeFinalizer } from '../../../src/tools/github_search_code/finalizer.js';

type AnyRec = Record<string, unknown>;

function runFinalizer(queries: AnyRec[], results: AnyRec[]) {
  const finalize = buildGhSearchCodeFinalizer();
  const out = finalize({
    queries: queries as never,
    results: results as never,
    config: {} as never,
  });
  return out.structuredContent as AnyRec;
}

function groupResult(
  owner: string,
  repo: string,
  path: string,
  value: string
) {
  return {
    id: `${owner}/${repo}`,
    owner,
    repo,
    matches: [{ path, value }],
  };
}

const pagination = (nextPage: number) => ({
  currentPage: nextPage - 1,
  totalPages: nextPage + 1,
  hasMore: true,
  nextPage,
});

describe('ghSearchCode finalizer — multi-query bulk pagination is not dropped', () => {
  it('surfaces pagination for BOTH paginating queries (not just one)', () => {
    const queries = [{ id: 'q1' }, { id: 'q2' }];
    const results = [
      {
        id: 'q1',
        status: 'success',
        data: {
          results: [groupResult('octo', 'a', 'src/a.ts', 'foo')],
          pagination: pagination(2),
        },
      },
      {
        id: 'q2',
        status: 'success',
        data: {
          results: [groupResult('octo', 'b', 'src/b.ts', 'bar')],
          pagination: pagination(3),
        },
      },
    ];

    const sc = runFinalizer(queries, results);
    const records = sc.results as Array<{
      id: string;
      data: { pagination?: { hasMore?: boolean; nextPage?: number } };
    }>;

    const byId = new Map(records.map(r => [r.id, r]));
    // Both queries' pagination must be reachable — neither dropped.
    expect(byId.get('q1')?.data.pagination?.hasMore).toBe(true);
    expect(byId.get('q1')?.data.pagination?.nextPage).toBe(2);
    expect(byId.get('q2')?.data.pagination?.hasMore).toBe(true);
    expect(byId.get('q2')?.data.pagination?.nextPage).toBe(3);
  });

  it('keeps single-query output identical (pagination on the single record)', () => {
    const queries = [{ id: 'only' }];
    const results = [
      {
        id: 'only',
        status: 'success',
        data: {
          results: [groupResult('octo', 'a', 'src/a.ts', 'foo')],
          pagination: pagination(2),
        },
      },
    ];

    const sc = runFinalizer(queries, results);
    const records = sc.results as Array<{
      id: string;
      data: { pagination?: { nextPage?: number } };
    }>;
    expect(records).toHaveLength(1);
    expect(records[0]!.id).toBe('only');
    expect(records[0]!.data.pagination?.nextPage).toBe(2);
  });
});
