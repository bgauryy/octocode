/**
 * Branch coverage tests for lsp_find_references/hints.ts.
 *
 * After the dynamic-only refactor, this file no longer imports upstream
 * static guidance, so the mock that used to backfill `dynamic-${key}` is
 * dead. We keep these tests for the local conditional branches
 * (filteredAll, etc.).
 */

import { describe, it, expect } from 'vitest';
import type { HintContext } from '../../src/types/metadata.js';

import { hints } from '../../src/tools/lsp_find_references/hints.js';

describe('lspFindReferences hints - branch coverage', () => {
  describe('hasResults hints', () => {
    it('should not add filtered hints (isFiltered branch removed)', () => {
      // isFiltered/filteredCount branches were removed — dead dynamic context
      const result = hints.hasResults!({
        isFiltered: true,
        filteredCount: 5,
        totalUnfiltered: 20,
      } as Record<string, unknown> as HintContext);
      expect(result.some(h => h?.includes('Filtered: 5 of 20'))).toBe(false);
      expect(
        result.some(h => h?.includes('includePattern/excludePattern'))
      ).toBe(false);
    });

    it('should return empty when no matching context', () => {
      const result = hints.hasResults!({});
      expect(
        result.some(h => h?.includes('includePattern/excludePattern'))
      ).toBe(false);
    });
  });

  describe('empty hints', () => {
    it('should add filteredAll hint when all refs filtered', () => {
      const result = hints.empty!({
        filteredAll: true,
      } as Record<string, unknown> as HintContext);
      expect(
        result.some(h => h?.includes('All references were excluded'))
      ).toBe(true);
      expect(result.some(h => h?.includes('TIP: Use includePattern'))).toBe(
        false
      );
    });

    it('should not add usage tips when not filteredAll (branch removed)', () => {
      // TIP hints for non-filteredAll were removed — dead dynamic context
      const result = hints.empty!({
        filteredAll: false,
      } as Record<string, unknown> as HintContext);
      expect(result.some(h => h?.includes('TIP: Use includePattern'))).toBe(
        false
      );
    });

    it('should return empty when symbolName present (symbolNotFound key removed from API)', () => {
      const result = hints.empty!({
        symbolName: 'myFunc',
      });
      expect(result.filter(Boolean)).toHaveLength(0);
    });

    it('should return empty when symbolName missing', () => {
      const result = hints.empty!({});
      expect(result.some(h => h?.includes('dynamic-symbolNotFound'))).toBe(
        false
      );
    });
  });

  describe('error hints', () => {
    it('should return empty for symbol_not_found (symbolNotFound/timeout keys removed from API)', () => {
      const result = hints.error!({ errorType: 'symbol_not_found' });
      expect(result).toHaveLength(0);
    });

    it('should return empty for timeout (symbolNotFound/timeout keys removed from API)', () => {
      const result = hints.error!({ errorType: 'timeout' });
      expect(result).toHaveLength(0);
    });

    it('should return empty for unknown error type', () => {
      const result = hints.error!({ errorType: 'other' } as Record<
        string,
        unknown
      > as HintContext);
      expect(result).toHaveLength(0);
    });
  });
});
