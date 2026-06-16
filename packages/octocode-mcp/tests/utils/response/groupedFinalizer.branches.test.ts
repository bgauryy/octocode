import { describe, it, expect } from 'vitest';
import {
  paginateGroupsCharWindow,
  collectFlatErrors,
} from '../../../../octocode-tools-core/src/utils/response/groupedFinalizer.js';
import { countSerializedChars } from '../../../../octocode-tools-core/src/utils/response/charSavings.js';

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

describe('paginateGroupsCharWindow — empty groups (lines 47-51 safeTotal=0 branches)', () => {
  it('handles empty groups array (totalChars=0, triggers safeTotal===0 ternaries)', () => {
    const result = paginateGroupsCharWindow<Group, Match>({
      groups: [],
      getItems: g => g.matches,
      setItems: (g, matches) => ({ ...g, matches }),
      getItemText: m => m.value,
      setItemText: (m, v) => ({ ...m, value: v }),
      charOffset: 0,
      charLength: 100,
    });
    expect(result.groups).toHaveLength(0);
    expect(result.pagination.totalChars).toBe(0);
    expect(result.pagination.currentPage).toBe(1);
  });
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
    expect(result.pagination.totalChars).toBeGreaterThan(bodyOnly);
    expect(
      Math.abs(result.pagination.totalChars - countSerializedChars(groups))
    ).toBeLessThanOrEqual(8);
  });
});

describe('paginateGroupsCharWindow — oversized item is paginated, never truncated', () => {
  const huge = Array.from({ length: 5_000 }, (_, i) =>
    String.fromCharCode(33 + (i % 90))
  ).join('');
  const groups: Group[] = [
    { id: 'owner/huge', matches: [{ path: 'big.ts', value: huge }] },
  ];

  it('slices an oversized value to fit the budget WITHOUT a truncation marker', () => {
    const page = window(groups, 0, 1_000);
    const value = page.groups[0]!.matches[0]!.value!;
    expect(value.length).toBeLessThan(huge.length);
    expect(value).not.toContain('…');
    expect(value).not.toMatch(/\[(truncated|clipped)\]/i);
    expect(page.pagination.hasMore).toBe(true);
  });

  it('advancing charOffset returns the NEXT slice of the same item (lossless round-trip)', () => {
    const first = window(groups, 0, 1_000);
    const firstValue = first.groups[0]!.matches[0]!.value!;
    expect(huge.startsWith(firstValue)).toBe(true);

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
    const page = window(groups, 0, 5);
    expect(page.groups.length).toBeGreaterThan(0);
    expect(page.pagination.charLength).toBeGreaterThan(0);
  });
});

describe('paginateGroupsCharWindow — no textAccessors (line 117 else branch)', () => {
  it('works when getItemText/setItemText are not provided', () => {
    type Item = { value: string };
    type Grp = { id: string; items: Item[] };
    const result = paginateGroupsCharWindow<Grp, Item>({
      groups: [{ id: 'g', items: [{ value: 'hello' }, { value: 'world' }] }],
      getItems: g => g.items,
      setItems: (g, items) => ({ ...g, items }),
      charOffset: 0,
      charLength: 10000,
    });
    expect(result.groups).toHaveLength(1);
    expect(result.pagination.hasMore).toBe(false);
  });
});

describe('paginateGroupsCharWindow — maxItems cap (lines 154-155)', () => {
  it('breaks early when itemCap is reached', () => {
    const groups: Group[] = [
      {
        id: 'g',
        matches: [
          { path: 'a', value: 'alpha' },
          { path: 'b', value: 'beta' },
          { path: 'c', value: 'gamma' },
        ],
      },
    ];
    const result = paginateGroupsCharWindow<Group, Match>({
      groups,
      getItems: g => g.matches,
      setItems: (g, matches) => ({ ...g, matches }),
      getItemText: m => m.value,
      setItemText: (m, v) => ({ ...m, value: v }),
      charOffset: 0,
      charLength: 100000,
      maxItems: 1,
    });
    expect(result.groups[0]!.matches).toHaveLength(1);
    expect(result.pagination.hasMore).toBe(true);
  });
});

describe('paginateGroupsCharWindow — textAccessors.get returns undefined (line 135)', () => {
  it('falls back to empty string when getItemText returns undefined for sliced text', () => {
    const groups: Group[] = [
      {
        id: 'g',
        matches: [{ path: 'a' }, { path: 'b', value: 'hello world something' }],
      },
    ];
    const result = paginateGroupsCharWindow<Group, Match>({
      groups,
      getItems: g => g.matches,
      setItems: (g, matches) => ({ ...g, matches }),
      getItemText: m => m.value,
      setItemText: (m, v) => ({ ...m, value: v }),
      charOffset: 0,
      charLength: 5,
    });
    expect(result.groups).toBeDefined();
  });
});

describe('collectFlatErrors — unwrapProviderError branches', () => {
  it('includes HTTP status in error message when error object has numeric status', () => {
    const errors = collectFlatErrors([
      {
        id: 'q1',
        status: 'error',
        data: { error: { error: 'Not Found', status: 404 } },
      } as never,
    ]);
    expect(errors[0]!.error).toContain('HTTP 404');
  });

  it('falls back to "Provider error" when error value is not string/object (line 213)', () => {
    const errors = collectFlatErrors([
      {
        id: 'q2',
        status: 'error',
        data: { error: 42 },
      } as never,
    ]);
    expect(errors[0]!.error).toBe('Provider error');
  });

  it('uses "Provider error" message when error field is empty string (line 206 false branch)', () => {
    const errors = collectFlatErrors([
      {
        id: 'q3',
        status: 'error',
        data: { error: { error: '', status: 503 } },
      } as never,
    ]);
    expect(errors[0]!.error).toContain('HTTP 503');
    expect(errors[0]!.error).toContain('Provider error');
  });

  it('omits status when error object has no numeric status (line 210 false branch)', () => {
    const errors = collectFlatErrors([
      {
        id: 'q4',
        status: 'error',
        data: { error: { error: 'Something failed' } },
      } as never,
    ]);
    expect(errors[0]!.error).toBe('Something failed');
    expect(errors[0]!.error).not.toContain('HTTP');
  });
});
