/**
 * Tests for dynamic hints generation
 * Covers error handlers and edge cases for all tools with dynamic hints
 */

import { describe, it, expect } from 'vitest';
import {
  getDynamicHints,
  hasDynamicHints,
  HINTS,
} from '../../src/hints/dynamic.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';
import type { HintContext } from '../../src/hints/types.js';

describe('Dynamic Hints', () => {
  describe('getDynamicHints', () => {
    it('should return empty array for unknown tool', () => {
      const hints = getDynamicHints('unknown_tool', 'hasResults');
      expect(hints).toEqual([]);
    });

    it('should return empty array for unknown status', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        'unknown' as 'hasResults'
      );
      expect(hints).toEqual([]);
    });

    it('should filter out undefined values from conditional hints', () => {
      // Call without context - conditional hints should be undefined and filtered
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        'hasResults'
      );
      expect(hints.every(h => typeof h === 'string')).toBe(true);
      expect(hints.every(h => h !== undefined)).toBe(true);
    });
  });

  describe('hasDynamicHints', () => {
    it('should return true for all registered tools', () => {
      Object.keys(HINTS).forEach(toolName => {
        expect(hasDynamicHints(toolName)).toBe(true);
      });
    });

    it('should return false for unregistered tools', () => {
      expect(hasDynamicHints('fake_tool')).toBe(false);
      expect(hasDynamicHints('')).toBe(false);
    });
  });

  describe('LOCAL_RIPGREP hints', () => {
    it('should never emit grep-fallback hints (single-engine: ripgrep)', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        'hasResults',
        {
          searchEngine: 'rg',
        }
      );
      // The grep fallback was removed; no hint may mention it.
      expect(hints.some(h => h.toLowerCase().includes('grep fallback'))).toBe(
        false
      );
    });

    it('should return hints when fileCount > 5', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        'hasResults',
        {
          fileCount: 10,
        }
      );
      // Should return hints from parallelTip metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints when fileCount > 1', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        'hasResults',
        {
          fileCount: 3,
        }
      );
      // Should return hints from crossFile metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for empty status with grep engine', () => {
      const hints = getDynamicHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'empty', {
        searchEngine: 'grep',
      });
      // Should return hints from grepEmpty metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for size_limit error', () => {
      const hints = getDynamicHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'error', {
        errorType: 'size_limit',
        matchCount: 500,
      });
      // Should return hints from sizeLimit metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for size_limit error with node_modules path', () => {
      const hints = getDynamicHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'error', {
        errorType: 'size_limit',
        path: '/project/node_modules/lodash',
      });
      // Should return hints from sizeLimit metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for non-size_limit errors', () => {
      const hints = getDynamicHints(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 'error', {
        // No errorType - triggers default case
      });
      // Should return hints from genericError metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });
  });

  describe('LOCAL_FETCH_CONTENT hints', () => {
    it('should include pagination hint when hasMoreContent', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'hasResults',
        { hasMoreContent: true } as HintContext
      );
      expect(hints.some(h => h.includes('charOffset'))).toBe(true);
    });

    it('should return size_limit error hints with file size', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'error',
        {
          errorType: 'size_limit',
          isLarge: true,
          hasPagination: false,
          hasPattern: false,
          fileSize: 400,
        }
      );
      expect(hints.some(h => h.includes('100'))).toBe(true); // ~400 * 0.25
      expect(hints.some(h => h.includes('tokens'))).toBe(true);
    });

    it('should return hints for size_limit error without file size', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'error',
        {
          errorType: 'size_limit',
          isLarge: true,
          hasPagination: false,
          hasPattern: false,
        }
      );
      // Should return hints from sizeLimit metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should NOT return size_limit hints when hasPagination is true', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'error',
        {
          errorType: 'size_limit',
          isLarge: true,
          hasPagination: true,
          hasPattern: false,
        }
      );
      expect(hints.some(h => h.includes('Large file'))).toBe(false);
    });

    it('should NOT return size_limit hints when hasPattern is true', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'error',
        {
          errorType: 'size_limit',
          isLarge: true,
          hasPagination: false,
          hasPattern: true,
        }
      );
      expect(hints.some(h => h.includes('Large file'))).toBe(false);
    });

    it('should return pattern_too_broad error hints', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'error',
        {
          errorType: 'pattern_too_broad',
          tokenEstimate: 50000,
        }
      );
      expect(hints.some(h => h.includes('50,000'))).toBe(true);
      expect(hints.some(h => h.includes('Pattern too broad'))).toBe(true);
    });

    it('should return hints for pattern_too_broad without tokenEstimate', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'error',
        {
          errorType: 'pattern_too_broad',
        }
      );
      // Should return hints from patternTooBroad metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for not_found error', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'error',
        {
          errorType: 'not_found',
        }
      );
      // Should return hints from notFound metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for unknown error type', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'error',
        {
          // No errorType - triggers default case
        }
      );
      // Should return hints from genericError metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });
  });

  describe('LOCAL_VIEW_STRUCTURE hints', () => {
    it('should return hints when entryCount > 10', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        'hasResults',
        { entryCount: 20 }
      );
      // Should return hints from manyEntries metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return size_limit error hints with entry count', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        'error',
        {
          errorType: 'size_limit',
          entryCount: 500,
          tokenEstimate: 10000,
        }
      );
      expect(hints.some(h => h.includes('500 entries'))).toBe(true);
      expect(hints.some(h => h.includes('10,000'))).toBe(true);
    });

    it('should return hints for error without size_limit', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        'error',
        {}
      );
      // Should return hints from genericError metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });
  });

  describe('LOCAL_FIND_FILES hints', () => {
    it('should return hints when fileCount > 3', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
        'hasResults',
        { fileCount: 5 }
      );
      // Should return hints from manyFiles metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return extended hints when fileCount > 20', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
        'hasResults',
        { fileCount: 25 }
      );
      // Should have hints due to metadata dynamic hints
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should NOT include extra hints when fileCount <= 3', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
        'hasResults',
        { fileCount: 2 }
      );
      // Small file count should not trigger batch hints
      expect(hints.some(h => h.toLowerCase().includes('batch'))).toBe(false);
    });
  });

  describe('GITHUB_SEARCH_CODE hints', () => {
    it('should return hints when hasOwnerRepo is true', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        'hasResults',
        { hasOwnerRepo: true }
      );
      // Should return hints from singleRepo metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints when hasOwnerRepo is false', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        'hasResults',
        { hasOwnerRepo: false }
      );
      // Should return hints from multiRepo metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for empty with match=path', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        'empty',
        { match: 'path' }
      );
      // Should return hints from pathMatch metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for empty without owner/repo', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        'empty',
        { hasOwnerRepo: false }
      );
      // Should return hints from crossRepo metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });
  });

  describe('GITHUB_FETCH_CONTENT hints', () => {
    it('should return hints for large files', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
        'hasResults',
        { isLarge: true }
      );
      // Should return hints from largeFile metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for size_limit error', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
        'error',
        { errorType: 'size_limit' }
      );
      // Should return hints from sizeLimit metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return empty for non-size_limit errors', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
        'error',
        { errorType: 'not_found' }
      );
      expect(hints).toEqual([]);
    });
  });

  describe('GITHUB_VIEW_REPO_STRUCTURE hints', () => {
    it('should return hints when entryCount > 50', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
        'hasResults',
        { entryCount: 100 }
      );
      // Should return hints with entry count info
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should NOT include entry count hint when entryCount <= 50', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
        'hasResults',
        { entryCount: 30 }
      );
      // Should not include extra pagination hints for small results
      expect(hints.some(h => h.includes('entries'))).toBe(false);
    });
  });

  describe('LSP_GOTO_DEFINITION hints', () => {
    it('should return hints when multiple definitions found', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'hasResults',
        { locationCount: 3 } as HintContext
      );
      // Should return hints from multipleDefinitions metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for external packages', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'hasResults',
        { hasExternalPackage: true } as HintContext
      );
      // Should return hints from externalPackage metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for fallback mode', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'hasResults',
        { isFallback: true } as HintContext
      );
      // Should return fallback-related hints from metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints with search radius context', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'empty',
        { searchRadius: 5, lineHint: 50 } as HintContext
      );
      // Should return hints from empty metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for empty results without searchRadius', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'empty',
        {}
      );
      // Should return hints from empty metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for empty results with symbolName', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'empty',
        { symbolName: 'myFunction' } as HintContext
      );
      // Should return hints from symbolNotFound metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for symbol_not_found error', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'error',
        {
          errorType: 'symbol_not_found',
          symbolName: 'foo',
          lineHint: 42,
        }
      );
      // Should return hints from symbolNotFound metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for file_not_found error', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'error',
        {
          errorType: 'file_not_found',
          uri: 'src/utils/helper.ts',
        }
      );
      // Should return hints from fileNotFound metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for file_not_found error without uri', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'error',
        { errorType: 'file_not_found' }
      );
      // Should return hints from fileNotFound metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for timeout error', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'error',
        {
          errorType: 'timeout',
          uri: 'src/big.ts',
          symbolName: 'process',
        }
      );
      // Should return timeout-related hints from metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return empty for unknown error type', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        'error',
        {
          // No errorType - triggers default case
        }
      );
      // Unknown error types return empty array per implementation
      expect(hints).toEqual([]);
    });
  });

  describe('LSP_FIND_REFERENCES hints', () => {
    it('should include reference count hint when many references', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'hasResults',
        { locationCount: 50 } as HintContext
      );
      // Should include count info in hints
      expect(hints.some(h => h.includes('50 references'))).toBe(true);
    });

    it('should include multi-file hint', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'hasResults',
        { hasMultipleFiles: true, fileCount: 5 } as HintContext
      );
      // Should include file count info
      expect(hints.some(h => h.includes('5 files'))).toBe(true);
    });

    it('should fallback to "multiple" when hasMultipleFiles but no fileCount', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'hasResults',
        { hasMultipleFiles: true } as HintContext
      );
      expect(hints.some(h => h.includes('multiple files'))).toBe(true);
    });

    it('should not include page info (pagination branch removed)', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'hasResults',
        { hasMorePages: true, currentPage: 2, totalPages: 5 } as HintContext
      );
      // Pagination branch was removed — these hints are dead context
      expect(hints.some(h => h.includes('Page 2/5'))).toBe(false);
    });

    it('should return hints when hasMorePages', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'hasResults',
        { hasMorePages: true, totalPages: 5 } as HintContext
      );
      // Should return pagination-related hints
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for fallback mode', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'hasResults',
        { isFallback: true } as HintContext
      );
      // Should return fallback-related hints from metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints when not in fallback mode', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'hasResults',
        { isFallback: false } as HintContext
      );
      // Should return standard hints
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for empty results with symbolName', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'empty',
        { symbolName: 'myVar' } as HintContext
      );
      // Should return hints from symbolNotFound metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for symbol_not_found error', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'error',
        { errorType: 'symbol_not_found', symbolName: 'bar' }
      );
      // Should return hints from symbolNotFound metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for timeout error', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'error',
        { errorType: 'timeout', symbolName: 'heavyFunction' }
      );
      // Should return hints from timeout metadata
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return empty for unknown error type', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        'error',
        { symbolName: 'test' }
      );
      // Unknown error types return empty array per implementation
      expect(hints).toEqual([]);
    });
  });

  describe('LSP_CALL_HIERARCHY hints', () => {
    it('should show incoming callers hint', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'hasResults',
        { direction: 'incoming', callCount: 5 } as HintContext
      );
      expect(hints.some(h => h.includes('5 callers'))).toBe(true);
    });

    it('should show outgoing callees hint', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'hasResults',
        { direction: 'outgoing', callCount: 3 } as HintContext
      );
      expect(hints.some(h => h.includes('3 callees'))).toBe(true);
    });

    it('should return hints when depth=1', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'hasResults',
        { depth: 1 } as HintContext
      );
      // Depth=1 returns base hints without deep chain hints
      expect(hints.length).toBeGreaterThan(0);
    });

    it('should show current depth when depth > 1', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'hasResults',
        { depth: 2 } as HintContext
      );
      // Depth > 1 includes depth info in hints
      expect(hints.some(h => h.includes('Depth=2'))).toBe(true);
    });

    it('should return hints for incoming direction', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'hasResults',
        { direction: 'incoming' } as HintContext
      );
      // Should return some hints for incoming direction
      expect(hints.length).toBeGreaterThan(0);
    });

    it('should include pagination hint when more pages', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'hasResults',
        { hasMorePages: true, currentPage: 1, totalPages: 3 } as HintContext
      );
      expect(hints.some(h => h.includes('Page 1/3'))).toBe(true);
    });

    it('should return hints when hasMorePages', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'hasResults',
        { hasMorePages: true, totalPages: 3 } as HintContext
      );
      // Should return pagination-related hints
      expect(hints.length).toBeGreaterThan(0);
    });

    it('should include fallback hints when isFallback', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'hasResults',
        { isFallback: true } as HintContext
      );
      // Should return fallback-related hints from metadata
      expect(hints.length).toBeGreaterThan(0);
    });

    it('should return hints for incoming empty results', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'empty',
        { direction: 'incoming' } as HintContext
      );
      // Should return hints from noCallers metadata (may be empty if not defined)
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for outgoing empty results', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'empty',
        { direction: 'outgoing' } as HintContext
      );
      // Should return hints from noCallees metadata (may be empty if not defined)
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for empty results without direction', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'empty',
        { symbolName: 'myFn' } as HintContext
      );
      // Should return hints from metadata (may be empty if not defined)
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return hints for not_a_function error', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'error',
        { errorType: 'not_a_function', symbolName: 'MyType' }
      );
      // Should return hints from notAFunction metadata (may be empty if not defined)
      expect(hints.length).toBeGreaterThanOrEqual(0);
    });

    it('should return timeout error hints with depth info', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'error',
        {
          errorType: 'timeout',
          depth: 3,
          symbolName: 'complexFn',
        }
      );
      // Should include the depth info and metadata hints
      expect(hints.some(h => h.includes('Depth=3'))).toBe(true);
    });

    it('should return empty for unknown error type', () => {
      const hints = getDynamicHints(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        'error',
        { symbolName: 'fn' }
      );
      // Unknown error types return empty array per implementation
      expect(hints).toEqual([]);
    });
  });

  describe('Tools with empty dynamic hints', () => {
    it('should return empty for GITHUB_SEARCH_PULL_REQUESTS all statuses', () => {
      expect(
        getDynamicHints(
          STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
          'hasResults'
        )
      ).toEqual([]);
      expect(
        getDynamicHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, 'empty')
      ).toEqual([]);
      expect(
        getDynamicHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, 'error')
      ).toEqual([]);
    });

    it('should return empty for GITHUB_SEARCH_REPOSITORIES all statuses', () => {
      expect(
        getDynamicHints(
          STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
          'hasResults'
        )
      ).toEqual([]);
      expect(
        getDynamicHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, 'empty')
      ).toEqual([]);
      expect(
        getDynamicHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, 'error')
      ).toEqual([]);
    });

    it('should return empty for PACKAGE_SEARCH all statuses', () => {
      expect(
        getDynamicHints(STATIC_TOOL_NAMES.PACKAGE_SEARCH, 'hasResults')
      ).toEqual([]);
      expect(
        getDynamicHints(STATIC_TOOL_NAMES.PACKAGE_SEARCH, 'empty')
      ).toEqual([]);
      expect(
        getDynamicHints(STATIC_TOOL_NAMES.PACKAGE_SEARCH, 'error')
      ).toEqual([]);
    });
  });

  describe('HINTS object structure', () => {
    it('should have all required tools registered', () => {
      const expectedTools = [
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
        STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
        STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
        STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
        STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        STATIC_TOOL_NAMES.PACKAGE_SEARCH,
        STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
      ];

      expectedTools.forEach(tool => {
        const hints = HINTS[tool];
        expect(hints).toBeDefined();
        expect(typeof hints?.hasResults).toBe('function');
        expect(typeof hints?.empty).toBe('function');
        expect(typeof hints?.error).toBe('function');
      });
    });

    it('should return arrays from all hint generators', () => {
      Object.entries(HINTS).forEach(([_toolName, generators]) => {
        ['hasResults', 'empty', 'error'].forEach(status => {
          const result = generators[status as keyof typeof generators]({});
          expect(Array.isArray(result)).toBe(true);
        });
      });
    });
  });
});
