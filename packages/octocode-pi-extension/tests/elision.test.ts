import { describe, expect, it } from 'vitest';
import { elisionGate, shrinkArgument } from '../src/compaction/register.js';
import { copiedElision } from '../src/files/tool.js';
import { capChars, elisionMarkers } from '../src/shared/util.js';

const argMarker = String(shrinkArgument('x'.repeat(5000))).slice(-80);
const resultMarker = '[… 9061 more characters from this earlier tool result were trimmed; re-run the tool if you need them.]';

describe('elision markers', () => {
  it('match what compaction and capChars emit', () => {
    expect(elisionMarkers(String(shrinkArgument('y'.repeat(5000))))).toHaveLength(1);
    expect(elisionMarkers(`head ${resultMarker} tail`)).toEqual([resultMarker]);
    expect(elisionMarkers(capChars('z'.repeat(100), 10, 'use a range'))).toHaveLength(1);
    expect(elisionMarkers('ordinary text [… more] and 12 more characters')).toEqual([]);
  });

  it('refuses a write or edit that adds a marker', () => {
    const marker = elisionMarkers(String(shrinkArgument('q'.repeat(5000))))[0]!;
    expect(argMarker.length).toBeGreaterThan(0);
    expect(copiedElision({ type: 'write', content: `a\n${marker}\nb` }, '')).toBe(marker);
    expect(copiedElision({ type: 'edit', edits: [{ oldText: 'a', newText: `| row ${resultMarker}` }] }, 'a')).toBe(resultMarker);
  });

  it('refuses any other tool call whose arguments carry a copied marker, wherever it sits', async () => {
    const marker = elisionMarkers(String(shrinkArgument('t'.repeat(5000))))[0]!;
    // The observed failure: a delegated task written as the compacted head of an earlier task plus its marker.
    const task = `TDD-fix review findings in the Rust crates engine and github… ${marker}`;
    const agent = await elisionGate({ toolName: 'agent', input: { task, background: true } } as never);
    expect(agent).toMatchObject({ block: true });
    expect(agent?.reason).toContain(marker);
    expect(agent?.reason).toMatch(/full text/);
    expect(await elisionGate({ toolName: 'sendMessage', input: { to: 'all', message: `see ${resultMarker}` } } as never)).toMatchObject({ block: true });
    expect(await elisionGate({ toolName: 'mcp__x', input: { queries: [{ path: '/a', note: { text: `x ${marker}` } }] } } as never)).toMatchObject({ block: true });
  });

  it('lets clean calls through and leaves file changes to their own content-aware check', async () => {
    expect(await elisionGate({ toolName: 'agent', input: { task: 'Fix the bug in a.rs; 12 more characters of context.' } } as never)).toBeUndefined();
    expect(await elisionGate({ toolName: 'bash', input: { command: 'rg -n "more characters" src' } } as never)).toBeUndefined();
    expect(await elisionGate({ toolName: 'file', input: { queries: [{ type: 'edit', path: 'a', edits: [{ oldText: `k ${resultMarker}`, newText: `k2 ${resultMarker}` }] }] } } as never)).toBeUndefined();
  });

  it('allows markers that are already there or being replaced, and other types', () => {
    expect(copiedElision({ type: 'write', content: `x ${resultMarker}` }, `old ${resultMarker}`)).toBeUndefined();
    expect(copiedElision({ type: 'edit', edits: [{ oldText: `k ${resultMarker}`, newText: `k2 ${resultMarker}` }] }, `k ${resultMarker}`)).toBeUndefined();
    expect(copiedElision({ type: 'delete' }, '')).toBeUndefined();
    expect(copiedElision({ type: 'write', content: 'plain' }, '')).toBeUndefined();
  });
});
