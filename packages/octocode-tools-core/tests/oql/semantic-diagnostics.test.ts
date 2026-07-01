/**
 * semanticDiagnostics branch coverage: LSP zero-results and anchor misses
 * must never read as proof. symbolNotFound/anchorFailed = the anchor never
 * resolved (blocking error); noReferences/noCalls and zero counts = candidate
 * evidence only (blocking partialResult).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { runDirect } = vi.hoisted(() => ({ runDirect: vi.fn() }));

vi.mock('../../src/oql/adapters/runner.js', async importOriginal => ({
  ...(await importOriginal<object>()),
  runDirect,
}));

import { runOqlSearch } from '../../src/oql/run.js';
import { isBatchEnvelope } from '../../src/oql/types.js';

function toolResult(data: Record<string, unknown>) {
  return {
    content: [],
    structuredContent: { results: [{ status: 'success', data }] },
  };
}

async function runSemantics(data: Record<string, unknown>, type: string) {
  runDirect.mockResolvedValue(toolResult(data));
  const env = await runOqlSearch({
    target: 'semantics',
    from: { kind: 'local', path: '/tmp/x.ts' },
    params: { type, symbolName: 'StateGraph', lineHint: 1 },
  } as never);
  if (isBatchEnvelope(env)) throw new Error('expected single envelope');
  return env;
}

describe('semanticDiagnostics branches', () => {
  beforeEach(() => {
    runDirect.mockReset();
  });

  it('payload.kind:empty + category:symbolNotFound -> blocking symbolNotFound with reason', async () => {
    const env = await runSemantics(
      {
        payload: {
          kind: 'empty',
          category: 'symbolNotFound',
          reason: 'No symbol "StateGraph" near line 1.',
        },
      },
      'references'
    );
    const d = env.diagnostics.find(x => x.code === 'symbolNotFound');
    expect(d).toBeDefined();
    expect(d?.severity).toBe('error');
    expect(d?.blocksAnswer).toBe(true);
    expect(d?.message).toContain('No symbol "StateGraph" near line 1.');
    expect(d?.message).toMatch(/refresh the lineHint/i);
    expect(env.evidence.answerReady).toBe(false);
  });

  it('payload.kind:empty + category:anchorFailed -> symbolNotFound with fallback reason', async () => {
    const env = await runSemantics(
      { payload: { kind: 'empty', category: 'anchorFailed' } },
      'references'
    );
    const d = env.diagnostics.find(x => x.code === 'symbolNotFound');
    expect(d).toBeDefined();
    expect(d?.message).toContain('Symbol anchor resolution failed.');
  });

  it('payload.kind:empty + category:noReferences -> blocking partialResult, NOT symbolNotFound', async () => {
    const env = await runSemantics(
      { payload: { kind: 'empty', category: 'noReferences' } },
      'references'
    );
    expect(env.diagnostics.some(x => x.code === 'symbolNotFound')).toBe(false);
    const d = env.diagnostics.find(
      x => x.code === 'partialResult' && /not proof of unused/i.test(x.message)
    );
    expect(d).toBeDefined();
    expect(d?.blocksAnswer).toBe(true);
    expect(env.evidence.kind).not.toBe('proof');
  });

  it('zero-count payloads for every relation kind -> blocking partialResult', async () => {
    const payloads: Array<Record<string, unknown>> = [
      { kind: 'references', totalReferences: 0 },
      { kind: 'callers', incomingCalls: 0 },
      { kind: 'callees', outgoingCalls: 0 },
      { kind: 'callHierarchy', incomingCalls: 0, outgoingCalls: 0 },
    ];
    for (const payload of payloads) {
      const env = await runSemantics({ payload }, String(payload.kind));
      const d = env.diagnostics.find(
        x =>
          x.code === 'partialResult' && /not proof of unused/i.test(x.message)
      );
      expect(d, `payload.kind=${payload.kind}`).toBeDefined();
      expect(env.evidence.answerReady, `payload.kind=${payload.kind}`).toBe(
        false
      );
    }
  });

  it('non-zero results emit no zero-result diagnostic', async () => {
    const env = await runSemantics(
      {
        payload: { kind: 'references', totalReferences: 3 },
        references: [
          { uri: '/tmp/a.ts', line: 5 },
          { uri: '/tmp/b.ts', line: 9 },
          { uri: '/tmp/c.ts', line: 12 },
        ],
      },
      'references'
    );
    expect(env.diagnostics.some(x => x.code === 'symbolNotFound')).toBe(false);
    expect(
      env.diagnostics.some(
        x =>
          x.code === 'partialResult' && /not proof of unused/i.test(x.message)
      )
    ).toBe(false);
  });

  it('callHierarchy with only incoming zero but outgoing present does not warn', async () => {
    const env = await runSemantics(
      {
        payload: { kind: 'callHierarchy', incomingCalls: 0, outgoingCalls: 4 },
      },
      'callHierarchy'
    );
    expect(
      env.diagnostics.some(
        x =>
          x.code === 'partialResult' && /not proof of unused/i.test(x.message)
      )
    ).toBe(false);
  });
});
