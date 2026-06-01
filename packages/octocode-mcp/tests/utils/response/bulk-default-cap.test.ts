import { describe, it, expect, afterEach, type Mock } from 'vitest';
import { getConfigSync } from 'octocode-shared';
import { applyBulkResponsePagination } from '../../../src/utils/response/structuredPagination.js';
import {
  getOutputCharLimit,
  getBulkDefaultCharLength,
} from '../../../src/utils/pagination/charLimit.js';

// #T2: even when the deployment config sets a very large
// output.pagination.defaultCharLength, a single aggregated bulk response must
// be clamped to the documented max (100000) so it self-paginates instead of
// overflowing the client token budget.
describe('applyBulkResponsePagination default-cap clamp (#T2)', () => {
  const base = (getConfigSync as unknown as Mock)();

  afterEach(() => {
    (getConfigSync as unknown as Mock).mockReturnValue(base);
  });

  it('clamps a huge configured default so an oversized response still paginates', () => {
    (getConfigSync as unknown as Mock).mockReturnValue({
      ...base,
      output: {
        ...base.output,
        pagination: {
          ...base.output?.pagination,
          defaultCharLength: 5_000_000,
        },
      },
    });

    // ~300KB of result data, far above the 100000 ceiling but below the
    // (clamped-away) 5,000,000 config default.
    const results = [
      {
        id: 'q1',
        data: {
          results: Array.from({ length: 6000 }, (_, i) => ({
            path: `src/file${i}.ts`,
            value: 'x'.repeat(40),
          })),
        },
      },
    ];

    const out = applyBulkResponsePagination(
      { results } as never,
      {},
      'someTool'
    );

    expect(out.responsePagination).toBeDefined();
    expect(out.responsePagination!.hasMore).toBe(true);
    // Clamped near the 100000 ceiling (item-boundary overshoot allowed), and
    // nowhere near the 5,000,000 config default — proving the clamp engaged.
    expect(out.responsePagination!.charLength).toBeLessThan(110_000);
  });
});

// #3: A multi-query bulk must reserve a per-query share of the default budget,
// so a modestly-sized set of sibling queries all land on page 1 instead of one
// query consuming the single-query default and starving the rest onto page 2.
describe('getBulkDefaultCharLength — per-query reserve', () => {
  it('returns the single base for one query', () => {
    expect(getBulkDefaultCharLength(1)).toBe(getOutputCharLimit());
  });

  it('scales the default by query count (each query reserved one base window)', () => {
    const base = getOutputCharLimit();
    expect(getBulkDefaultCharLength(4)).toBe(Math.min(base * 4, 100_000));
  });

  it('never exceeds the 100000 ceiling regardless of count', () => {
    expect(getBulkDefaultCharLength(10_000)).toBe(100_000);
  });

  it('treats a zero/negative count as a single query', () => {
    expect(getBulkDefaultCharLength(0)).toBe(getOutputCharLimit());
  });
});

describe('applyBulkResponsePagination does not starve siblings (#3)', () => {
  it('keeps all modest sibling queries on page 1 (no auto-pagination)', () => {
    const base = getOutputCharLimit();
    // Four queries, each ~0.6×base. Total ~2.4×base: over the single-query
    // default (would paginate before the fix) but under the 4×base reserve.
    const per = Math.floor(base * 0.6);
    const results = Array.from({ length: 4 }, (_, i) => ({
      id: `q${i + 1}`,
      data: { blob: 'x'.repeat(per) },
    }));

    const out = applyBulkResponsePagination(
      { results } as never,
      {},
      'someTool'
    );

    // All four queries present, nothing deferred to a second page.
    expect(out.results).toHaveLength(4);
    expect(out.responsePagination?.hasMore ?? false).toBe(false);
  });

  it('still paginates when the total exceeds even the scaled reserve', () => {
    const base = getOutputCharLimit();
    // Four queries each ~1.5×base → total ~6×base, above the 4×base reserve.
    const per = Math.floor(base * 1.5);
    const results = Array.from({ length: 4 }, (_, i) => ({
      id: `q${i + 1}`,
      data: { blob: 'x'.repeat(per) },
    }));

    const out = applyBulkResponsePagination(
      { results } as never,
      {},
      'someTool'
    );

    expect(out.responsePagination?.hasMore).toBe(true);
  });
});
