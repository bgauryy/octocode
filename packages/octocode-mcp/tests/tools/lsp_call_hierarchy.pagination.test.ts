import { beforeAll, describe, expect, it } from 'vitest';
import { applyQueryOutputPagination } from '../../src/utils/response/structuredPagination.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import { initializeToolMetadata } from '../../src/tools/toolMetadata/state.js';
import { getOutputCharLimit } from '../../src/utils/pagination/charLimit.js';

/**
 * Call-hierarchy output is bounded by the unified bulk char-paginator — NOT by
 * a per-node content pre-clip. A fat node's nested `from.content` is sliced to
 * the page budget with a cursor to the rest; nothing is clipped to a fixed cap
 * and no "[clipped]" / "[truncated]" marker is left behind.
 *
 * This is the contract that makes removing `clampCallHierarchyBudget` safe.
 */
beforeAll(async () => {
  await initializeToolMetadata();
});

describe('lspCallHierarchy pagination — engine-owned, lossless', () => {
  const BIG = 'x'.repeat(40_000);
  const fat = {
    item: { name: 'target', uri: '/w/a.ts' },
    direction: 'incoming',
    depth: 2,
    incomingCalls: [
      { from: { name: 'c1', uri: '/w/b.ts', content: BIG }, fromRanges: [] },
      { from: { name: 'c2', uri: '/w/c.ts', content: BIG }, fromRanges: [] },
    ],
  };

  it('char-paginates a fat nested node body (bounded + cursor, no clip)', () => {
    const limit = getOutputCharLimit();
    const result = applyQueryOutputPagination(
      { id: 'ch-fat', data: fat },
      { charLength: limit },
      TOOL_NAMES.LSP_CALL_HIERARCHY
    );
    const data = result.data as {
      incomingCalls?: Array<{ from?: { content?: string } }>;
      outputPagination?: { hasMore: boolean };
    };
    const emitted = JSON.stringify(data.incomingCalls);

    // Page 1 is bounded near the budget — NOT the full ~80K of bodies.
    expect(emitted.length).toBeLessThan(limit * 4);
    // A cursor reaches the rest — nothing dropped.
    expect(data.outputPagination?.hasMore).toBe(true);
    // No lossy clip/truncation marker anywhere.
    expect(emitted).not.toMatch(/\[clipped\]|\[truncated/i);
    // The surviving content is a genuine prefix of the body (char-sliced),
    // and it is NOT pre-clipped to a tiny fixed cap (it fills the page).
    const firstContent = data.incomingCalls?.[0]?.from?.content ?? '';
    expect(BIG.startsWith(firstContent)).toBe(true);
    expect(firstContent.length).toBeGreaterThan(500);
  });

  it('keeps small node content in full (single page, hasMore=false)', () => {
    const small = {
      item: { name: 'target', uri: '/w/a.ts' },
      direction: 'incoming',
      depth: 1,
      incomingCalls: [
        {
          from: { name: 'c1', uri: '/w/b.ts', content: 'tiny body' },
          fromRanges: [],
        },
      ],
    };
    const result = applyQueryOutputPagination(
      { id: 'ch-small', data: small },
      { charLength: getOutputCharLimit() },
      TOOL_NAMES.LSP_CALL_HIERARCHY
    );
    const data = result.data as {
      incomingCalls: Array<{ from: { content: string } }>;
      outputPagination?: { hasMore: boolean };
    };
    // Full content, nothing clipped; the single page has no "more".
    expect(data.incomingCalls[0]!.from.content).toBe('tiny body');
    expect(data.outputPagination?.hasMore ?? false).toBe(false);
  });
});
