import { describe, expect, it } from 'vitest';
import { applyCallHierarchyVerbosity } from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';
import { attachLspEvidence } from '../../src/lsp/evidence.js';

// #T3 / #5b: concise call hierarchy is a tiny probe — the edge list lives in
// `hints`. The stale pagination/outputPagination (computed from the full
// payload before calls[] was emptied) must be dropped, otherwise the response
// stays large and is falsely reported `complete:false`.
describe('lspCallHierarchy concise (#T3)', () => {
  const fullResult = {
    item: { name: 'startServer', content: 'x'.repeat(5000) },
    direction: 'outgoing',
    depth: 1,
    calls: [
      { from: { name: 'a' }, to: { name: 'b' } },
      { from: { name: 'a' }, to: { name: 'c' } },
    ],
    pagination: { currentPage: 1, totalPages: 3, hasMore: true },
    outputPagination: {
      charOffset: 0,
      charLength: 8000,
      totalChars: 19024,
      hasMore: true,
      currentPage: 1,
      totalPages: 3,
    },
  } as never;

  it('drops stale pagination/outputPagination and empties calls', () => {
    const out = applyCallHierarchyVerbosity(fullResult, {
      verbosity: 'concise',
      direction: 'outgoing',
    } as never) as Record<string, unknown>;

    expect(out.calls).toEqual([]);
    expect(out.pagination).toBeUndefined();
    expect(out.outputPagination).toBeUndefined();
    expect(Array.isArray(out.hints)).toBe(true);
    expect((out.hints as string[])[0]).toMatch(/edge\(s\)/);
    // payload must be tiny now
    expect(JSON.stringify(out).length).toBeLessThan(2048);
  });

  it('is complete:true once the stale outputPagination is gone', () => {
    const out = applyCallHierarchyVerbosity(fullResult, {
      verbosity: 'concise',
      direction: 'outgoing',
    } as never);
    const evidenced = attachLspEvidence(out, {
      kind: 'calls',
      paginationKey: 'outputPagination',
      fallbackReason: 'x',
    }) as { evidence?: { complete?: boolean; answerReady?: boolean } };
    expect(evidenced.evidence?.answerReady).toBe(true);
    expect(evidenced.evidence?.complete).toBe(true);
  });
});
