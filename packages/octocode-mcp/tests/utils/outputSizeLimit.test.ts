import { describe, it, expect } from 'vitest';
import { applyOutputSizeLimit } from '../../src/utils/pagination/outputSizeLimit.js';

const DEFAULT_OUTPUT_CHAR_LENGTH = 8000;

describe('applyOutputSizeLimit', () => {
  describe('no pagination needed', () => {
    it('should return content unchanged when under MAX_OUTPUT_CHARS', () => {
      const smallContent = JSON.stringify({ data: 'small' });
      const result = applyOutputSizeLimit(smallContent, {});

      expect(result.wasLimited).toBe(false);
      expect(result.content).toBe(smallContent);
      expect(result.pagination).toBeUndefined();
      expect(result.warnings).toHaveLength(0);
    });

    it('should return content unchanged when exactly at MAX_OUTPUT_CHARS threshold', () => {
      const content = 'x'.repeat(DEFAULT_OUTPUT_CHAR_LENGTH);
      const result = applyOutputSizeLimit(content, {});

      expect(result.wasLimited).toBe(false);
      expect(result.content).toBe(content);
    });
  });

  describe('auto-pagination (no explicit charOffset/charLength)', () => {
    it('should auto-paginate when content exceeds MAX_OUTPUT_CHARS', () => {
      const largeContent = 'x'.repeat(DEFAULT_OUTPUT_CHAR_LENGTH + 1000);
      const result = applyOutputSizeLimit(largeContent, {});

      expect(result.wasLimited).toBe(true);
      expect(result.content.length).toBe(DEFAULT_OUTPUT_CHAR_LENGTH);
      expect(result.pagination).toBeDefined();
      expect(result.pagination!.hasMore).toBe(true);
      expect(result.warnings).toHaveLength(1);
      expect(result.warnings[0]).toContain('Auto-paginated');
    });

    it('should truncate very large content and include pagination with hasMore=true', () => {
      const hugeContent = 'a'.repeat(80000);
      const result = applyOutputSizeLimit(hugeContent, {});

      expect(result.wasLimited).toBe(true);
      expect(result.content.length).toBeLessThanOrEqual(
        DEFAULT_OUTPUT_CHAR_LENGTH
      );
      expect(result.pagination).toBeDefined();
      expect(result.pagination!.hasMore).toBe(true);
      expect(result.pagination!.totalChars).toBe(80000);
      expect(result.pagination!.charOffset).toBe(0);
      expect(result.pagination!.charLength).toBeLessThanOrEqual(
        DEFAULT_OUTPUT_CHAR_LENGTH
      );
    });

    it('should include correct pagination metadata for auto-paginated content', () => {
      const content = 'y'.repeat(25000);
      const result = applyOutputSizeLimit(content, {});

      expect(result.pagination).toBeDefined();
      expect(result.pagination!.currentPage).toBe(1);
      expect(result.pagination!.totalPages).toBe(4);
      expect(result.pagination!.hasMore).toBe(true);
    });
  });

  describe('explicit charOffset/charLength', () => {
    it('should paginate with explicit charLength', () => {
      const content = 'z'.repeat(5000);
      const result = applyOutputSizeLimit(content, { charLength: 2000 });

      expect(result.wasLimited).toBe(true);
      expect(result.content.length).toBe(2000);
      expect(result.pagination).toBeDefined();
      expect(result.pagination!.charOffset).toBe(0);
      expect(result.pagination!.charLength).toBe(2000);
      expect(result.pagination!.hasMore).toBe(true);
      expect(result.pagination!.totalChars).toBe(5000);
    });

    it('should paginate with explicit charOffset and charLength', () => {
      const content = 'abcdefghij'.repeat(1000);
      const result = applyOutputSizeLimit(content, {
        charOffset: 2000,
        charLength: 3000,
      });

      expect(result.wasLimited).toBe(true);
      expect(result.content.length).toBe(3000);
      expect(result.pagination).toBeDefined();
      expect(result.pagination!.charOffset).toBe(2000);
      expect(result.pagination!.hasMore).toBe(true);
    });

    it('should handle charOffset at the end of content', () => {
      const content = 'x'.repeat(5000);
      const result = applyOutputSizeLimit(content, {
        charOffset: 5000,
        charLength: 1000,
      });

      expect(result.content).toBe('');
      expect(result.pagination).toBeDefined();
      expect(result.pagination!.hasMore).toBe(false);
    });

    it('should handle charOffset beyond content length', () => {
      const content = 'x'.repeat(100);
      const result = applyOutputSizeLimit(content, {
        charOffset: 500,
        charLength: 100,
      });

      expect(result.content).toBe('');
      expect(result.pagination!.hasMore).toBe(false);
    });

    it('should apply charLength even on small content when explicitly provided', () => {
      const content = 'small content here';
      const result = applyOutputSizeLimit(content, { charLength: 5 });

      expect(result.wasLimited).toBe(true);
      expect(result.content).toBe('small');
      expect(result.pagination!.hasMore).toBe(true);
    });
  });

  describe('custom thresholds', () => {
    it('should respect custom maxOutputChars', () => {
      const content = 'x'.repeat(500);
      const result = applyOutputSizeLimit(content, { maxOutputChars: 100 });

      expect(result.wasLimited).toBe(true);
      expect(result.warnings[0]).toContain('100');
    });

    it('should respect custom recommendedCharLength', () => {
      const content = 'x'.repeat(5000);
      const result = applyOutputSizeLimit(content, {
        maxOutputChars: 100,
        recommendedCharLength: 3000,
      });

      expect(result.wasLimited).toBe(true);
      expect(result.content.length).toBe(3000);
    });
  });

  describe('pagination hints generation', () => {
    it('should generate next page hint when hasMore', () => {
      const content = 'x'.repeat(30000);
      const result = applyOutputSizeLimit(content, {});

      expect(result.paginationHints).toBeDefined();
      expect(result.paginationHints!.length).toBeGreaterThan(0);
      expect(result.paginationHints!.some(h => h.includes('charOffset'))).toBe(
        true
      );
    });

    it('should return empty hints array when content fits', () => {
      const content = 'small';
      const result = applyOutputSizeLimit(content, {});

      expect(result.paginationHints).toEqual([]);
    });
  });
});
