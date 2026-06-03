import { describe, expect, it } from 'vitest';
import { applyCallHierarchyVerbosity } from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';
import { attachLspEvidence } from '../../src/lsp/evidence.js';

// #T3 / #5b: concise call hierarchy is a tiny probe, but it must not lie about
// the graph. It keeps call arrays, strips heavy node content, and drops stale
// char outputPagination computed before content stripping.
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

  it('drops stale outputPagination and preserves calls without content', () => {
    const out = applyCallHierarchyVerbosity(fullResult, {
      verbosity: 'concise',
      direction: 'outgoing',
    } as never) as Record<string, unknown>;

    expect(out.calls).toHaveLength(2);
    expect(
      ((out.calls as Array<{ to?: { content?: string } }>)[0]?.to ?? {}).content
    ).toBeUndefined();
    expect(out.pagination).toEqual(fullResult.pagination);
    expect(out.outputPagination).toBeUndefined();
    expect(Array.isArray(out.hints)).toBe(true);
    expect((out.hints as string[])[0]).toMatch(/edge\(s\)/);
    // payload must stay tiny now
    expect(JSON.stringify(out).length).toBeLessThan(2048);
  });

  it('is complete:true once stale outputPagination is gone', () => {
    const out = applyCallHierarchyVerbosity(fullResult, {
      verbosity: 'concise',
      direction: 'outgoing',
    } as never);
    const evidenced = attachLspEvidence(out, {
      kind: 'calls',
      paginationKey: 'outputPagination',
    }) as { evidence?: { complete?: boolean; answerReady?: boolean } };
    expect(evidenced.evidence?.answerReady).toBe(true);
    expect(evidenced.evidence?.complete).toBe(true);
  });

  it('concise renders incoming edges from incomingCalls with call-site multiplicity', () => {
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

    const out = applyCallHierarchyVerbosity(incomingResult, {
      verbosity: 'concise',
      direction: 'incoming',
    } as never) as Record<string, unknown>;

    expect(Array.isArray(out.incomingCalls)).toBe(true);
    const edgesHint = (out.hints as string[])[1] ?? '';
    // caller with 2 call sites gets a ×2 suffix; single-site caller has none.
    expect(edgesHint).toContain('callerA → target (×2)');
    expect(edgesHint).toContain('callerB → target');
    // heavy node content is stripped
    const first = (
      out.incomingCalls as Array<{ from?: { content?: string } }>
    )[0];
    expect(first?.from?.content).toBeUndefined();
  });

  it('concise renders outgoing edges from outgoingCalls', () => {
    const outgoingResult = {
      item: { name: 'root' },
      direction: 'outgoing',
      depth: 2,
      outgoingCalls: [{ to: { name: 'callee' }, fromRanges: [] }],
    } as never;

    const out = applyCallHierarchyVerbosity(outgoingResult, {
      verbosity: 'concise',
    } as never) as Record<string, unknown>;

    expect(Array.isArray(out.outgoingCalls)).toBe(true);
    expect((out.hints as string[])[1]).toContain('root → callee');
  });

  it('concise leaves empty/error results untouched', () => {
    const emptyResult = {
      status: 'empty',
      errorCode: 'LSP_NOT_INSTALLED',
      hints: ['x'],
    } as never;
    const out = applyCallHierarchyVerbosity(emptyResult, {
      verbosity: 'concise',
    } as never);
    expect(out).toBe(emptyResult);
  });

  it('compact trims advisory hints; basic passes through unchanged', () => {
    const withHints = {
      item: { name: 'fn' },
      direction: 'incoming',
      depth: 1,
      incomingCalls: [],
      hints: ['Prefer depth=1 to avoid timeouts', 'real data hint'],
    } as never;

    const compact = applyCallHierarchyVerbosity(withHints, {
      verbosity: 'compact',
    } as never) as Record<string, unknown>;
    expect(Array.isArray(compact.hints)).toBe(true);

    const basic = applyCallHierarchyVerbosity(withHints, {} as never);
    expect(basic).toBe(withHints);
  });
});
