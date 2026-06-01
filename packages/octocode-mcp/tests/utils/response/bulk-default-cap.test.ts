import { describe, it, expect, afterEach, type Mock } from 'vitest';
import { getConfigSync } from 'octocode-shared';
import { applyBulkResponsePagination } from '../../../src/utils/response/structuredPagination.js';

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
