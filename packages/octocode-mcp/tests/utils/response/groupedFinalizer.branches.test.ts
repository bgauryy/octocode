/**
 * Branch coverage for the unified `paginateGroupsCharWindow` paginator:
 * overflowed offset, oversized item windowing (no truncation marker), the
 * forward-progress backstop, and the wrapper-inclusive totalChars contract.
 */

import { describe, it, expect } from 'vitest';
import { paginateGroupsCharWindow } from '../../../src/utils/response/groupedFinalizer.js';
import { countSerializedChars } from '../../../src/utils/response/charSavings.js';

type Match = { path: string; value?: string };
type Group = { id: string; matches: Match[] };

const getItems = (g: Group): readonly Match[] => g.matches;
const setItems = (g: Group, matches: Match[]): Group => ({ ...g, matches });
const getItemText = (m: Match): string | undefined => m.value;
const setItemText = (m: Match, value: string): Match => ({ ...m, value });

const window = (groups: Group[], charOffset: number, charLength: number) =>
  paginateGroupsCharWindow<Group, Match>({
    groups,
    getItems,
    setItems,
    getItemText,
    setItemText,
    charOffset,
    charLength,
  });

describe('paginateGroupsCharWindow — edge branches', () => {
  it('returns empty selection when charOffset exceeds total size', () => {
    const result = window(
      [{ id: 'g', matches: [{ path: 'a', value: 'x' }] }],
      1_000_000,
      100
    );
    expect(result.groups).toHaveLength(0);
    expect(result.pagination.hasMore).toBe(false);
  });

  it('totalChars counts the group wrapper, not just item bodies', () => {
    const groups: Group[] = [
      { id: 'owner/one', matches: [{ path: 'a.ts', value: 'alpha' }] },
      { id: 'owner/two', matches: [{ path: 'b.ts', value: 'beta' }] },
    ];
    const bodyOnly = groups
      .flatMap(g => g.matches)
      .reduce((n, m) => n + (m.value?.length ?? 0), 0);
    const result = window(groups, 0, Number.MAX_SAFE_INTEGER);
    // Must far exceed the item bodies alone (the old undercount) and track the
    // real serialized size within the outer-array bracket/comma constant.
    expect(result.pagination.totalChars).toBeGreaterThan(bodyOnly);
    expect(
      Math.abs(result.pagination.totalChars - countSerializedChars(groups))
    ).toBeLessThanOrEqual(8);
  });
});

describe('paginateGroupsCharWindow — oversized item is paginated, never truncated', () => {
  // Varied content so contiguity/overlap is detectable (a uniform string would
  // make every slice identical).
  const huge = Array.from({ length: 5_000 }, (_, i) =>
    String.fromCharCode(33 + (i % 90))
  ).join('');
  const groups: Group[] = [
    { id: 'owner/huge', matches: [{ path: 'big.ts', value: huge }] },
  ];

  it('slices an oversized value to fit the budget WITHOUT a truncation marker', () => {
    const page = window(groups, 0, 1_000);
    const value = page.groups[0]!.matches[0]!.value!;
    // Bounded: the page never exceeds the budget by more than one wrapper.
    expect(value.length).toBeLessThan(huge.length);
    expect(value).not.toContain('…');
    expect(value).not.toMatch(/\[(truncated|clipped)\]/i);
    expect(page.pagination.hasMore).toBe(true);
  });

  it('advancing charOffset returns the NEXT slice of the same item (lossless round-trip)', () => {
    const first = window(groups, 0, 1_000);
    const firstValue = first.groups[0]!.matches[0]!.value!;
    // Page 1 starts at the beginning of the value.
    expect(huge.startsWith(firstValue)).toBe(true);

    // Walking the cursor to the end reassembles the value EXACTLY — contiguous,
    // non-overlapping, nothing dropped. This is the no-truncation guarantee.
    let assembled = '';
    let offset = 0;
    for (let i = 0; i < 100; i++) {
      const page = window(groups, offset, 1_000);
      assembled += page.groups[0]?.matches[0]?.value ?? '';
      if (!page.pagination.hasMore) break;
      offset = page.pagination.charOffset + page.pagination.charLength;
    }
    expect(assembled).toBe(huge);
  });

  it('makes forward progress even when one item envelope alone exceeds the budget', () => {
    const page = window(groups, 0, 5); // absurdly small budget
    expect(page.groups.length).toBeGreaterThan(0);
    expect(page.pagination.charLength).toBeGreaterThan(0); // cursor advanced
  });
});
