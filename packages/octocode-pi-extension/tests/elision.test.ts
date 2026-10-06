import { describe, expect, it } from 'vitest';
import { shrinkArgument } from '../src/compaction/register.js';
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

  it('allows markers that are already there or being replaced, and other types', () => {
    expect(copiedElision({ type: 'write', content: `x ${resultMarker}` }, `old ${resultMarker}`)).toBeUndefined();
    expect(copiedElision({ type: 'edit', edits: [{ oldText: `k ${resultMarker}`, newText: `k2 ${resultMarker}` }] }, `k ${resultMarker}`)).toBeUndefined();
    expect(copiedElision({ type: 'delete' }, '')).toBeUndefined();
    expect(copiedElision({ type: 'write', content: 'plain' }, '')).toBeUndefined();
  });
});
