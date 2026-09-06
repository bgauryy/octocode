import { describe, expect, it } from 'vitest';

import { buildGhSearchCodeFinalizer } from '../../../src/tools/github_search_code/finalizer/build.js';

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

function groupResult(owner: string, repo: string, path: string, value: string) {
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

describe('github.code finalizer — multi-query bulk pagination is not dropped', () => {
  it('surfaces pagination for BOTH paginating queries (not just one)', () => {
    const queries = [{}, {}];
    const results = [
      {
        index: 0,
        status: 'success',
        data: {
          results: [groupResult('octo', 'a', 'src/a.ts', 'foo')],
          pagination: pagination(2),
        },
      },
      {
        index: 1,
        status: 'success',
        data: {
          results: [groupResult('octo', 'b', 'src/b.ts', 'bar')],
          pagination: pagination(3),
        },
      },
    ];

    const sc = runFinalizer(queries, results);
    const records = sc.results as Array<{
      index: number;
      data: { pagination?: { hasMore?: boolean; nextPage?: number } };
    }>;

    const byIndex = new Map(records.map(r => [r.index, r]));
    // Both queries' pagination must be reachable — neither dropped.
    expect(byIndex.get(0)?.data.pagination?.hasMore).toBe(true);
    expect(byIndex.get(0)?.data.pagination?.nextPage).toBe(2);
    expect(byIndex.get(1)?.data.pagination?.hasMore).toBe(true);
    expect(byIndex.get(1)?.data.pagination?.nextPage).toBe(3);
  });

  it('keeps single-query output identical (pagination on the single record)', () => {
    const queries = [{}];
    const results = [
      {
        index: 0,
        status: 'success',
        data: {
          results: [groupResult('octo', 'a', 'src/a.ts', 'foo')],
          pagination: pagination(2),
        },
      },
    ];

    const sc = runFinalizer(queries, results);
    const records = sc.results as Array<{
      index: number;
      data: { pagination?: { nextPage?: number } };
    }>;
    expect(records).toHaveLength(1);
    expect(records[0]!.index).toBe(0);
    expect(records[0]!.data.pagination?.nextPage).toBe(2);
  });
});
