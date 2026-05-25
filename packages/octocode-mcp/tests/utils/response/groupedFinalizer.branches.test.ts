/**
 * Branch coverage for the edge cases in `paginateNestedItems` and
 * `paginateGroupsWithNestedItemEscape` that the higher-level finalizer
 * tests don't reach: empty groups, overflowed offset, oversized first
 * group with truncate callback.
 */

import { describe, it, expect } from 'vitest';
import {
  paginateNestedItems,
  paginateGroupsWithNestedItemEscape,
} from '../../../src/utils/response/groupedFinalizer.js';

type Group = { id: string; items: string[] };

const getItems = (g: Group): readonly string[] => g.items;
const setItems = (g: Group, items: string[]): Group => ({ ...g, items });

describe('paginateNestedItems — edge branches', () => {
  it('preserves a group even when it has zero items (line 112-120 path)', () => {
    const result = paginateNestedItems<Group, string>({
      groups: [
        { id: 'empty', items: [] },
        { id: 'full', items: ['a', 'b'] },
      ],
      getItems,
      setItems,
      charOffset: 0,
      charLength: 9007199254740991,
    });
    expect(result.groups.map(g => g.id)).toContain('empty');
    expect(result.pagination.totalChars).toBeGreaterThan(0);
  });

  it('returns empty selection when charOffset exceeds total size', () => {
    const result = paginateNestedItems<Group, string>({
      groups: [{ id: 'g', items: ['x'] }],
      getItems,
      setItems,
      charOffset: 1_000_000,
      charLength: 100,
    });
    expect(result.groups).toHaveLength(0);
    expect(result.pagination.hasMore).toBe(false);
  });
});

describe('paginateGroupsWithNestedItemEscape — edge branches', () => {
  it('returns empty when firstIndex === -1 (offset past every group, line 186)', () => {
    const result = paginateGroupsWithNestedItemEscape<Group, string>({
      groups: [
        { id: 'a', items: ['short'] },
        { id: 'b', items: ['also-short'] },
      ],
      getItems,
      setItems,
      charOffset: 1_000_000,
      charLength: 100,
    });
    expect(result.groups).toEqual([]);
    expect(result.pagination.hasMore).toBe(false);
    expect(result.pagination.charLength).toBe(0);
  });

  it('invokes truncateOversizedItem when the first group is more than 2× charLength', () => {
    const huge = 'x'.repeat(2_000);
    const truncated: string[] = [];
    const result = paginateGroupsWithNestedItemEscape<Group, string>({
      groups: [{ id: 'huge', items: [huge] }],
      getItems,
      setItems,
      charOffset: 0,
      charLength: 100,
      truncateOversizedItem: (item, len) => {
        truncated.push(item);
        return item.slice(0, len) + '…';
      },
    });
    expect(truncated).toHaveLength(1);
    expect(result.groups[0]!.items[0]!.endsWith('…')).toBe(true);
  });
});
