/**
 * P2 — OQL pagination normalization. `totalPages` must be derivable from
 * `totalItems`/`itemsPerPage` (the unit OQL actually emits rows in), never the
 * upstream backing-tool value which is often expressed in a different unit
 * (files vs. per-match rows). Capped/estimated totals must NOT assert an exact
 * page count.
 */
import { describe, expect, it } from 'vitest';
import { toOqlPagination } from '../../src/oql/adapters/pagination.js';

describe('toOqlPagination', () => {
  it('returns undefined for missing pagination (no fallback)', () => {
    expect(toOqlPagination(undefined)).toBeUndefined();
  });

  it('returns hasMore:true when nothing but fallbackHasMore is set', () => {
    expect(toOqlPagination(undefined, true)).toEqual({ hasMore: true });
  });

  it('recomputes totalPages from totalItems/itemsPerPage, ignoring upstream', () => {
    // The bug: upstream reports totalPages:7 while totalItems:50 / perPage:5
    // implies 10 pages. The normalizer must trust the derivable math.
    const p = toOqlPagination({
      currentPage: 1,
      totalPages: 7,
      itemsPerPage: 5,
      totalItems: 50,
    });
    expect(p?.totalPages).toBe(10);
    expect(p?.itemsPerPage).toBe(5);
    expect(p?.totalItems).toBe(50);
    expect(p?.hasMore).toBe(true);
  });

  it('rounds up partial final pages', () => {
    const p = toOqlPagination({
      currentPage: 1,
      itemsPerPage: 10,
      totalItems: 23,
    });
    expect(p?.totalPages).toBe(3);
  });

  it('derives hasMore=false on the last page', () => {
    const p = toOqlPagination({
      currentPage: 3,
      itemsPerPage: 10,
      totalItems: 23,
    });
    expect(p?.totalPages).toBe(3);
    expect(p?.hasMore).toBe(false);
  });

  it('collapses alias unit fields (totalMatches/filesPerPage) into one pair', () => {
    const p = toOqlPagination({
      currentPage: 2,
      filesPerPage: 5,
      totalMatches: 12,
    });
    expect(p?.itemsPerPage).toBe(5);
    expect(p?.totalItems).toBe(12);
    expect(p?.totalPages).toBe(3);
    expect(p?.hasMore).toBe(true);
  });

  it('does NOT assert an exact totalPages when the total is capped', () => {
    const p = toOqlPagination({
      currentPage: 1,
      totalPages: 7,
      itemsPerPage: 5,
      totalMatches: 50,
      totalMatchesCapped: true,
      hasMore: true,
    });
    expect(p?.totalPages).toBeUndefined();
    expect(p?.totalItemsCapped).toBe(true);
    expect(p?.hasMore).toBe(true);
  });

  it('preserves at least one page for an empty result set', () => {
    const p = toOqlPagination({
      currentPage: 1,
      itemsPerPage: 10,
      totalItems: 0,
    });
    expect(p?.totalPages).toBe(1);
    expect(p?.hasMore).toBe(false);
  });

  it('drops a stale nextPage once hasMore is false', () => {
    const p = toOqlPagination({
      currentPage: 3,
      nextPage: 4,
      itemsPerPage: 10,
      totalItems: 23,
    });
    expect(p?.hasMore).toBe(false);
    expect(p?.nextPage).toBeUndefined();
  });

  it('keeps nextPage while more pages remain', () => {
    const p = toOqlPagination({
      currentPage: 1,
      nextPage: 2,
      itemsPerPage: 10,
      totalItems: 23,
    });
    expect(p?.hasMore).toBe(true);
    expect(p?.nextPage).toBe(2);
  });
});
