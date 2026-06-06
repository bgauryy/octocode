import { beforeAll, describe, expect, it } from 'vitest';
import { applyQueryOutputPagination } from '../../src/utils/response/structuredPagination.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import { initializeToolMetadata } from '../../src/tools/toolMetadata/state.js';
import { getOutputCharLimit } from '../../src/utils/pagination/charLimit.js';

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

    expect(emitted.length).toBeLessThan(limit * 4);
    expect(data.outputPagination?.hasMore).toBe(true);
    expect(emitted).not.toMatch(/\[clipped\]|\[truncated/i);
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
    expect(data.incomingCalls[0]!.from.content).toBe('tiny body');
    expect(data.outputPagination?.hasMore ?? false).toBe(false);
  });
});
