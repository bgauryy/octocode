/**
 * GitHub code lane paginates FILE items while OQL code rows are per-match.
 * The runner must NOT slice match rows under a soft page size (that silently
 * dropped the tail matches of page-1 files when `next.page` advanced the file
 * page) — it defers to backend file paging and marks the unit honestly
 * (itemUnit:'files' + rowCount). An explicit `limit` stays a hard row cap.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { runDirect } = vi.hoisted(() => ({ runDirect: vi.fn() }));

vi.mock('../../src/oql/adapters/runner.js', async importOriginal => ({
  ...(await importOriginal<object>()),
  runDirect,
}));

import { runOqlSearch } from '../../src/oql/run.js';
import { isBatchEnvelope } from '../../src/oql/types.js';

function ghCodeResult(hasMore: boolean) {
  const files = ['a.ts', 'b.ts', 'c.ts'].map(path => ({
    path,
    matches: [{ value: `${path}: match one` }, { value: `${path}: match two` }],
  }));
  return {
    content: [],
    structuredContent: {
      results: [
        {
          id: 'q1',
          status: 'hasResults',
          data: {
            files,
            pagination: hasMore
              ? {
                  currentPage: 1,
                  totalPages: 4,
                  hasMore: true,
                  perPage: 2,
                  totalMatches: 8,
                  totalMatchesKind: 'lowerBound',
                }
              : {
                  currentPage: 1,
                  totalPages: 1,
                  hasMore: false,
                  perPage: 10,
                  totalMatches: 3,
                  totalMatchesKind: 'exact',
                },
          },
        },
      ],
    },
  };
}

async function run(query: Record<string, unknown>, hasMore = true) {
  runDirect.mockResolvedValue(ghCodeResult(hasMore));
  const env = await runOqlSearch({
    target: 'code',
    from: { kind: 'github', repo: 'facebook/react' },
    where: { kind: 'text', value: 'match' },
    ...query,
  } as never);
  if (isBatchEnvelope(env)) throw new Error('expected single envelope');
  return env;
}

describe('GitHub code file-unit paging', () => {
  beforeEach(() => {
    runDirect.mockReset();
  });

  it('soft itemsPerPage does NOT slice match rows (defers to file paging)', async () => {
    const env = await run({ itemsPerPage: 2 });
    // 3 files x 2 matches: all 6 rows survive even though itemsPerPage is 2.
    expect(env.results).toHaveLength(6);
    expect(env.pagination?.itemUnit).toBe('files');
    expect(env.pagination?.rowCount).toBe(6);
    // Exactness metadata from the provider is preserved, not overwritten.
    expect(env.pagination?.totalItemsKind).not.toBe('files');
  });

  it('emits next.page advancing the file page with an honest why', async () => {
    const env = await run({ itemsPerPage: 2 });
    const nextPage = env.next?.['next.page'];
    expect(nextPage).toBeDefined();
    expect((nextPage?.query as { page?: number }).page).toBe(2);
    expect(nextPage?.why).toMatch(/matched files/i);
    expect(nextPage?.why).toMatch(/per-match/i);
  });

  it('omits next.page when the backend reports no more file pages', async () => {
    const env = await run({ itemsPerPage: 2 }, false);
    expect(env.next?.['next.page']).toBeUndefined();
  });

  it('explicit limit stays a hard row cap', async () => {
    const env = await run({ limit: 3 });
    expect(env.results).toHaveLength(3);
    expect(env.pagination?.totalItemsCapped).toBe(true);
  });
});
