import { describe, it, expect } from 'vitest';
import { applyBulkResponsePagination } from '../../../../octocode-tools-core/src/utils/response/structuredPagination.js';
import {
  getOutputCharLimit,
  getBulkDefaultCharLength,
} from '../../../../octocode-tools-core/src/utils/pagination/charLimit.js';

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

describe('applyBulkResponsePagination sibling queries', () => {
  it('keeps all modest sibling queries on page 1 (no auto-pagination)', () => {
    const base = getOutputCharLimit();
    const per = Math.floor(base * 0.1);
    const results = Array.from({ length: 4 }, (_, i) => ({
      id: `q${i + 1}`,
      data: { blob: 'x'.repeat(per) },
    }));

    const out = applyBulkResponsePagination(
      { results } as never,
      {},
      'someTool'
    );

    expect(out.results).toHaveLength(4);
    expect(out).not.toHaveProperty('responsePagination');
  });
});
