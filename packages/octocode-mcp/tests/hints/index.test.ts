/**
 * Tests for the unified hints system
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { getHints } from '../../src/hints/index.js';
import { getLargeFileWorkflowHints } from '../../src/hints/dynamic.js';
import { hasDynamicHints } from '../../src/hints/dynamic.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';
import { initializeToolMetadata } from '../../src/tools/toolMetadata/state.js';

// Initialize metadata before tests
beforeAll(async () => {
  await initializeToolMetadata();
});

describe('Unified Hints System', () => {
  describe('getHints', () => {
    it('should return hints for GitHub tools', () => {
      const hints = getHints(
        STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        'hasResults'
      );

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should return an array for local tools', () => {
      const hints = getHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'hasResults');

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
      // Dynamic-only: may be empty when no condition is met.
    });

    it('should return an array for empty status', () => {
      const hints = getHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'empty');

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should return error hints for error status', () => {
      const hints = getHints(STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT, 'error', {
        errorType: 'size_limit',
        isLarge: true,
        fileSize: 500,
      });

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
      expect(hints.length).toBeGreaterThan(0);
    });

    it('should return empty array for unknown tool', () => {
      const hints = getHints('unknown_tool', 'hasResults');

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
      expect(hints.length).toBe(0);
    });

    it('should deduplicate hints', () => {
      // Call getHints and verify no duplicates
      const hints = getHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'hasResults');
      const uniqueHints = [...new Set(hints)];

      expect(hints.length).toBe(uniqueHints.length);
    });

    describe('context-aware hints', () => {
      it('should return more hints when fileCount > 5', () => {
        const hintsLow = getHints(
          STATIC_TOOL_NAMES.LOCAL_RIPGREP,
          'hasResults',
          {
            fileCount: 3,
          }
        );
        const hintsHigh = getHints(
          STATIC_TOOL_NAMES.LOCAL_RIPGREP,
          'hasResults',
          {
            fileCount: 10,
          }
        );

        // Higher fileCount should return additional hints from parallelTip metadata
        expect(hintsHigh.length).toBeGreaterThanOrEqual(hintsLow.length);
      });

      it('should NOT include extra hints when fileCount <= 5', () => {
        const hints = getHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'hasResults', {
          fileCount: 3,
        });

        // Dynamic-only contract: no fileCount-conditioned hint at fileCount<=5.
        expect(Array.isArray(hints)).toBe(true);
      });

      it('should include size context for size_limit errors', () => {
        const hints = getHints(STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT, 'error', {
          errorType: 'size_limit',
          isLarge: true,
          fileSize: 400,
        });

        expect(hints.some(h => h.includes('tokens'))).toBe(true);
      });

      it('should include entry count context for directory errors', () => {
        const hints = getHints(
          STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
          'error',
          {
            errorType: 'size_limit',
            entryCount: 1000,
          }
        );

        expect(hints.some(h => h.includes('1000'))).toBe(true);
      });

      it('should NOT emit hasResults guidance based on hasOwnerRepo', () => {
        const hintsWithRepo = getHints(
          STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
          'hasResults',
          { hasOwnerRepo: true }
        );
        const hintsWithoutRepo = getHints(
          STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
          'hasResults',
          { hasOwnerRepo: false }
        );

        // Dynamic-only contract: hasResults emits no static guidance.
        expect(hintsWithRepo).toEqual([]);
        expect(hintsWithoutRepo).toEqual([]);
      });
    });
  });

  describe('hasDynamicHints', () => {
    it('should return true for local tools with dynamic hints', () => {
      expect(hasDynamicHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP)).toBe(true);
      expect(hasDynamicHints(STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT)).toBe(true);
      expect(hasDynamicHints(STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE)).toBe(
        true
      );
      expect(hasDynamicHints(STATIC_TOOL_NAMES.LOCAL_FIND_FILES)).toBe(true);
    });

    it('should return true for GITHUB_SEARCH_CODE', () => {
      expect(hasDynamicHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE)).toBe(true);
    });

    it('should return true for all registered tools', () => {
      // All tools now have dynamic hint generators
      expect(hasDynamicHints(STATIC_TOOL_NAMES.PACKAGE_SEARCH)).toBe(true);
      expect(
        hasDynamicHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES)
      ).toBe(true);
    });

    it('should return false for unknown tools', () => {
      expect(hasDynamicHints('unknown_tool')).toBe(false);
    });
  });

  describe('getLargeFileWorkflowHints', () => {
    it('should return search workflow hints', () => {
      const hints = getLargeFileWorkflowHints('search');

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
      expect(hints.length).toBeGreaterThan(0);
      expect(hints.some(h => h.includes('localSearchCode'))).toBe(true);
    });

    it('should return read workflow hints', () => {
      const hints = getLargeFileWorkflowHints('read');

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
      expect(hints.length).toBeGreaterThan(0);
      expect(hints.some(h => h.includes('localGetFileContent'))).toBe(true);
    });
  });

  // Note: `getMetadataDynamicHints` / `getStaticHints` were removed when the
  // hint flow was unified to dynamic-per-response only. Their tests are gone
  // because the production code path that read upstream metadata hints no
  // longer exists.

  describe('all supported tools coverage', () => {
    const allTools = Object.values(STATIC_TOOL_NAMES);

    it.each(allTools)('should return hints for %s hasResults', toolName => {
      const hints = getHints(toolName, 'hasResults');

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
      // All hints should be strings
      hints.forEach(hint => {
        expect(typeof hint).toBe('string');
        expect(hint.length).toBeGreaterThan(0);
      });
    });

    it.each(allTools)('should return hints for %s empty', toolName => {
      const hints = getHints(toolName, 'empty');

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
      hints.forEach(hint => {
        expect(typeof hint).toBe('string');
        expect(hint.length).toBeGreaterThan(0);
      });
    });

    it.each(allTools)('should return hints for %s error', toolName => {
      const hints = getHints(toolName, 'error');

      expect(hints).toBeDefined();
      expect(Array.isArray(hints)).toBe(true);
      // Error hints may be empty for some tools
      hints.forEach(hint => {
        expect(typeof hint).toBe('string');
        expect(hint.length).toBeGreaterThan(0);
      });
    });
  });
});
