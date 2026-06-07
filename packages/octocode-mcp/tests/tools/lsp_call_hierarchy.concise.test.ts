import { describe, expect, it } from 'vitest';
import { finalizeCallHierarchyResult } from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';
import { attachLspEvidence } from '../../src/lsp/evidence.js';

describe('lspCallHierarchy pass-through contract', () => {
  const fullResult = {
    item: { name: 'startServer', content: 'x'.repeat(5000) },
    direction: 'outgoing',
    depth: 1,
    calls: [
      { from: { name: 'a' }, to: { name: 'b' } },
      { from: { name: 'a' }, to: { name: 'c' } },
    ],
    pagination: { currentPage: 1, totalPages: 3, hasMore: true },
  } as never;

  it(' — preserves full calls[] and pagination', () => {
    const out = finalizeCallHierarchyResult(fullResult, {
      direction: 'outgoing',
    } as never) as Record<string, unknown>;

    expect(out.calls).toHaveLength(2);
    expect(out.pagination).toEqual(
      (fullResult as Record<string, unknown>).pagination
    );
  });

  it(' — preserves full calls[] and pagination', () => {
    const out = finalizeCallHierarchyResult(fullResult, {
      direction: 'outgoing',
    } as never) as Record<string, unknown>;

    expect(out.calls).toHaveLength(2);
    expect(out.pagination).toEqual(
      (fullResult as Record<string, unknown>).pagination
    );
  });

  it('evidence is attached when pagination.hasMore is true', () => {
    const out = finalizeCallHierarchyResult(fullResult, {
      direction: 'outgoing',
    } as never);
    const evidenced = attachLspEvidence(out, {
      kind: 'calls',
      paginationKey: 'pagination',
    }) as { evidence?: { complete?: boolean; answerReady?: boolean } };
    expect(evidenced.evidence).toBeDefined();
  });

  it('preserves incoming call content (pass-through)', () => {
    const incomingResult = {
      item: { name: 'target', content: 'y'.repeat(100) },
      direction: 'incoming',
      depth: 1,
      incomingCalls: [
        {
          from: { name: 'callerA', content: 'z'.repeat(100) },
          fromRanges: [{ start: { line: 1 } }, { start: { line: 9 } }],
        },
        { from: { name: 'callerB', content: 'z' }, fromRanges: [] },
      ],
    } as never;

    const out = finalizeCallHierarchyResult(incomingResult, {
      direction: 'incoming',
    } as never) as Record<string, unknown>;

    expect(Array.isArray(out.incomingCalls)).toBe(true);
    const first = (
      out.incomingCalls as Array<{ from?: { content?: string } }>
    )[0];
    expect(first?.from?.content).toBe('z'.repeat(100));
    expect(out).toBe(incomingResult);
  });

  it('preserves outgoingCalls structure', () => {
    const outgoingResult = {
      item: { name: 'root' },
      direction: 'outgoing',
      depth: 2,
      outgoingCalls: [{ to: { name: 'callee' }, fromRanges: [] }],
    } as never;

    const out = finalizeCallHierarchyResult(
      outgoingResult,
      {} as never
    ) as Record<string, unknown>;

    expect(Array.isArray(out.outgoingCalls)).toBe(true);
    expect(out.outgoingCalls).toEqual(outgoingResult.outgoingCalls);
    expect(out).toBe(outgoingResult);
  });

  it('empty/error results pass through unchanged', () => {
    const emptyResult = {
      status: 'empty',
      errorCode: 'LSP_NOT_INSTALLED',
      hints: ['x'],
    } as never;
    const out = finalizeCallHierarchyResult(emptyResult, {} as never);
    expect(out).toBe(emptyResult);
  });

  it('hints are NOT trimmed', () => {
    const originalHints = [
      'Prefer depth=1 to avoid timeouts',
      'real data hint',
    ];
    const withHints = {
      item: { name: 'fn' },
      direction: 'incoming',
      depth: 1,
      incomingCalls: [],
      hints: [...originalHints],
    } as never;

    const out = finalizeCallHierarchyResult(withHints, {} as never) as Record<
      string,
      unknown
    >;
    expect(out.hints).toEqual(originalHints);

    const basic = finalizeCallHierarchyResult(withHints, {} as never) as Record<
      string,
      unknown
    >;
    expect(basic.hints).toEqual(originalHints);
  });
});
