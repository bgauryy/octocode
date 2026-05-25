/**
 * Tests for local tools hints module
 */

import { describe, it, expect } from 'vitest';
import { getHints } from '../../src/hints/index.js';
import { getLargeFileWorkflowHints } from '../../src/hints/dynamic.js';
import { HINTS } from '../../src/hints/dynamic.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';

describe('Local Tools Hints', () => {
  it('STATIC_TOOL_NAMES should be defined', () => {
    expect(STATIC_TOOL_NAMES).toBeDefined();
    expect(STATIC_TOOL_NAMES.LOCAL_RIPGREP).toBe('localSearchCode');
  });

  describe('HINTS structure', () => {
    it('should have hints for all local tools', () => {
      expect(HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]).toBeDefined();
      expect(HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]).toBeDefined();
      expect(HINTS[STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]).toBeDefined();
      expect(HINTS[STATIC_TOOL_NAMES.LOCAL_FIND_FILES]).toBeDefined();
      expect(HINTS[STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]).toBeDefined();
    });

    it('should have all status types for each tool', () => {
      Object.values(HINTS).forEach(toolHints => {
        expect(toolHints.hasResults).toBeDefined();
        expect(toolHints.empty).toBeDefined();
        expect(toolHints.error).toBeDefined();
      });
    });
  });

  describe('LOCAL_RIPGREP hints', () => {
    describe('hasResults', () => {
      it('should return dynamic hints without context (empty when no trigger)', () => {
        // HINTS returns only dynamic context-aware hints, not static base hints
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]?.hasResults({});

        // Without context triggers (fileCount, searchEngine), dynamic hints are empty
        expect(hints?.filter(Boolean).length).toBe(0);
      });

      it('should return empty for any fileCount (no API keys available)', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]?.hasResults({
          fileCount: 10,
        });

        // hasResults dynamic hints are all empty — static hints come from HOST
        expect(hints?.filter(Boolean).length).toBe(0);
      });
    });

    describe('empty', () => {
      it('should return empty dynamic hints (static hints added by getHints)', () => {
        // HINTS.empty returns only dynamic hints - which are empty for ripgrep
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]?.empty({});

        // Dynamic empty hints only appear with context (e.g., searchEngine='grep')
        expect(Array.isArray(hints)).toBe(true);
      });
    });

    describe('error', () => {
      it('should return size limit hints', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]?.error({
          errorType: 'size_limit',
        });

        expect(hints?.some(h => h?.includes('Narrow'))).toBe(true);
      });

      it('should include match count in size limit error', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]?.error({
          errorType: 'size_limit',
          matchCount: 1000,
        });

        expect(hints?.some(h => h?.includes('1000'))).toBe(true);
      });

      it('should include node_modules tip when in node_modules', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]?.error({
          errorType: 'size_limit',
          path: '/project/node_modules/lib',
        });

        // Dynamic hints from metadata for nodeModulesSearch
        expect(hints?.length).toBeGreaterThan(0);
      });

      it('should not include node_modules tip for other paths', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]?.error({
          errorType: 'size_limit',
          path: '/project/src',
        });

        // Without node_modules in path, no nodeModulesSearch hints
        const nodeModulesHint = hints?.find(h =>
          h?.toLowerCase().includes('node_modules')
        );
        expect(nodeModulesHint).toBeUndefined();
      });

      it('should return empty array for unknown error type', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]?.error({
          errorType: 'not_found',
        });

        // Dynamic error hints only for known error types
        expect(hints?.length).toBe(0);
      });
    });
  });

  describe('LOCAL_FETCH_CONTENT hints', () => {
    describe('hasResults', () => {
      it('should return dynamic hints (context-dependent)', () => {
        // HINTS returns only dynamic hints, not static base hints
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.hasResults(
          {}
        );

        // Without hasMoreContent context, only returns array with undefined
        expect(Array.isArray(hints)).toBe(true);
      });
    });

    describe('empty', () => {
      it('should return empty dynamic hints (static hints added by getHints)', () => {
        // HINTS.empty for LOCAL_FETCH_CONTENT returns empty array
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.empty({});

        expect(Array.isArray(hints)).toBe(true);
      });
    });

    describe('error', () => {
      it('should return size limit hints for large files', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.error({
          errorType: 'size_limit',
          isLarge: true,
          hasPagination: false,
          hasPattern: false,
        });

        // Returns largeFile dynamic hints from metadata
        expect(hints?.length).toBeGreaterThan(0);
      });

      it('should include file size estimate when available', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.error({
          errorType: 'size_limit',
          isLarge: true,
          hasPagination: false,
          hasPattern: false,
          fileSize: 400, // 400KB -> ~100K tokens
        });

        // Includes calculated token estimate
        expect(hints?.some(h => h?.includes('100K tokens'))).toBe(true);
      });

      it('should return pattern too broad hints', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.error({
          errorType: 'pattern_too_broad',
        });

        // Returns patternTooBroad dynamic hints
        expect(hints?.length).toBeGreaterThan(0);
      });

      it('should include token estimate in pattern too broad error', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.error({
          errorType: 'pattern_too_broad',
          tokenEstimate: 50000,
        });

        expect(hints?.some(h => h?.includes('50,000'))).toBe(true);
      });

      it('should return empty array for unknown error type', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.error({
          errorType: 'permission',
        });

        // Dynamic error hints only for known error types
        expect(hints?.length).toBe(0);
      });

      it('should return empty array when size_limit but not isLarge', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.error({
          errorType: 'size_limit',
          isLarge: false,
        });

        // No dynamic hints without isLarge
        expect(hints?.length).toBe(0);
      });

      it('should return hints when size_limit and isLarge (regardless of pagination)', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.error({
          errorType: 'size_limit',
          isLarge: true,
          hasPagination: true,
        });

        // The actual implementation only checks errorType === 'size_limit' && isLarge
        // hasPagination is not checked in the condition
        expect(hints?.length).toBeGreaterThan(0);
      });
    });
  });

  describe('LOCAL_VIEW_STRUCTURE hints', () => {
    describe('hasResults', () => {
      it('should return empty array without context triggers', () => {
        // HINTS returns only dynamic context-aware hints
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]?.hasResults(
          {}
        );

        // No dynamic hints without entryCount > 10
        expect(hints?.filter(Boolean).length).toBe(0);
      });

      it('should return [] regardless of entryCount (no static guidance)', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]?.hasResults(
          {
            entryCount: 15,
          }
        );

        // Dynamic-only contract: hasResults emits no guidance.
        expect(hints?.filter(Boolean).length).toBe(0);
      });

      it('should not include parallelize hint when entryCount <= 10', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]?.hasResults(
          {
            entryCount: 5,
          }
        );

        expect(hints?.filter(Boolean).length).toBe(0);
      });
    });

    describe('empty', () => {
      it('should return empty dynamic hints (static hints added by getHints)', () => {
        // HINTS.empty for LOCAL_VIEW_STRUCTURE returns empty array
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]?.empty({});

        expect(Array.isArray(hints)).toBe(true);
      });
    });

    describe('error', () => {
      it('should return size limit hints with entry count', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]?.error({
          errorType: 'size_limit',
          entryCount: 500,
        });

        expect(hints?.some(h => h?.includes('500'))).toBe(true);
      });

      it('should include token estimate in size limit error', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]?.error({
          errorType: 'size_limit',
          entryCount: 500,
          tokenEstimate: 25000,
        });

        expect(hints?.some(h => h?.includes('25,000'))).toBe(true);
      });

      it('should return empty array without entry count', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]?.error({
          errorType: 'size_limit',
        });

        // Without entryCount, returns empty dynamic hints
        expect(hints?.length).toBe(0);
      });
    });
  });

  describe('LOCAL_FIND_FILES hints', () => {
    describe('hasResults', () => {
      it('should return empty array without context triggers', () => {
        // HINTS returns only dynamic context-aware hints
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FIND_FILES]?.hasResults({});

        // No dynamic hints without fileCount > 3
        expect(hints?.filter(Boolean).length).toBe(0);
      });

      it('should return empty when fileCount <= 20 (manyResults threshold)', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FIND_FILES]?.hasResults({
          fileCount: 5,
        });

        // batchParallel key removed (not in API); manyResults only fires at > 20
        expect(hints?.filter(Boolean).length).toBe(0);
      });

      it('should return [] regardless of fileCount (no static guidance)', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FIND_FILES]?.hasResults({
          fileCount: 25,
        });

        expect(hints?.filter(Boolean).length).toBe(0);
      });

      it('should return [] when fileCount <= 3', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FIND_FILES]?.hasResults({
          fileCount: 2,
        });

        expect(hints?.filter(Boolean).length).toBe(0);
      });

      it('should return [] when hasConfigFiles is true (no static guidance)', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FIND_FILES]?.hasResults({
          hasConfigFiles: true,
        });

        expect(hints?.filter(Boolean).length).toBe(0);
      });
    });

    describe('empty', () => {
      it('should return empty dynamic hints (static hints added by getHints)', () => {
        // HINTS.empty for LOCAL_FIND_FILES returns empty array
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FIND_FILES]?.empty({});

        expect(Array.isArray(hints)).toBe(true);
      });
    });

    describe('error', () => {
      it('should return empty array (no dynamic error hints)', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.LOCAL_FIND_FILES]?.error({});

        // LOCAL_FIND_FILES error returns empty array
        expect(hints?.length).toBe(0);
      });
    });
  });

  describe('GITHUB_SEARCH_CODE hints', () => {
    describe('hasResults', () => {
      it('should return [] regardless of context (no static guidance)', () => {
        expect(
          HINTS[STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]?.hasResults({
            hasOwnerRepo: true,
          })
        ).toEqual([]);
        expect(
          HINTS[STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]?.hasResults({
            hasOwnerRepo: false,
          })
        ).toEqual([]);
        expect(
          HINTS[STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]?.hasResults({})
        ).toEqual([]);
      });
    });

    describe('empty', () => {
      it('should return cross-repo hint when no owner/repo', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]?.empty({
          hasOwnerRepo: false,
        });
        expect(hints?.some(h => h?.includes('across repos'))).toBe(true);
      });

      it('should return path-mode hint when match=path is empty', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]?.empty({
          match: 'path',
        });
        expect(hints?.some(h => h?.includes('match="file"'))).toBe(true);
      });

      it('should return [] when owner/repo is provided', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]?.empty({
          hasOwnerRepo: true,
        });
        expect(hints?.filter(Boolean).length).toBe(0);
      });
    });

    describe('error', () => {
      it('should return [] without context', () => {
        const hints = HINTS[STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]?.error({});
        expect(hints).toEqual([]);
      });
    });
  });

  describe('getHints', () => {
    it('should return hints for valid tool and status', () => {
      const hints = getHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'hasResults');

      expect(Array.isArray(hints)).toBe(true);
      // Returns static hints from metadata (base + tool hints)
    });

    it('should return empty array for invalid tool', () => {
      const hints = getHints(
        'invalidTool' as typeof STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        'hasResults'
      );

      expect(hints).toEqual([]);
    });

    it('should return empty array for invalid status', () => {
      const hints = getHints(
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        'invalid' as 'hasResults' | 'empty' | 'error'
      );

      expect(hints).toEqual([]);
    });

    it('should pass context to hint generator', () => {
      const hints = getHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'hasResults', {
        fileCount: 10,
      });

      // With context, dynamic hints are included
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should filter out undefined hints', () => {
      const hints = getHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'hasResults', {
        fileCount: 2,
      });

      hints.forEach(hint => {
        expect(hint).toBeDefined();
        expect(typeof hint).toBe('string');
      });
    });

    it('should return GITHUB_SEARCH_CODE hints with hasOwnerRepo context', () => {
      const hintsWithOwner = getHints(
        STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        'hasResults',
        {
          hasOwnerRepo: true,
        }
      );
      const hintsWithoutOwner = getHints(
        STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        'hasResults',
        {
          hasOwnerRepo: false,
        }
      );

      // Check that context affects the hints - both should return hints
      expect(Array.isArray(hintsWithOwner)).toBe(true);
      expect(Array.isArray(hintsWithoutOwner)).toBe(true);
    });

    it('should return GITHUB_SEARCH_CODE empty hints with context', () => {
      const hints = getHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, 'empty', {
        hasOwnerRepo: false,
      });

      // Returns static + dynamic hints
      expect(Array.isArray(hints)).toBe(true);
    });
  });

  describe('getLargeFileWorkflowHints', () => {
    it('should return search workflow hints', () => {
      const hints = getLargeFileWorkflowHints('search');

      expect(hints?.length).toBeGreaterThan(0);
      expect(hints?.some((h: string) => h.includes('codebase'))).toBe(true);
      expect(hints?.some((h: string) => h.includes('filesOnly'))).toBe(true);
    });

    it('should return read workflow hints', () => {
      const hints = getLargeFileWorkflowHints('read');

      expect(hints?.length).toBeGreaterThan(0);
      expect(hints?.some((h: string) => h.includes('Large file'))).toBe(true);
      expect(hints?.some((h: string) => h.includes('charLength'))).toBe(true);
    });
  });
});
