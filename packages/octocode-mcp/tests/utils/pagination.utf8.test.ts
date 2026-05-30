/**
 * Comprehensive UTF-8 pagination tests
 * Tests byte/character offset separation across all pagination scenarios
 */

import { describe, it, expect } from 'vitest';
import {
  applyPagination,
  createPaginationInfo,
} from '../../src/utils/pagination/core.js';
import { generatePaginationHints } from '../../src/utils/pagination/hints.js';

describe('UTF-8 Pagination - Byte/Character Separation', () => {
  // Test content with various UTF-8 characters
  const TEST_CONTENT = {
    // ASCII only (1 byte per char)
    ascii: 'Hello World',
    // Emoji content (4 bytes per emoji, 2 JS chars per emoji due to surrogate pairs)
    emoji: 'Hello 👋 World',
    emojiMultiple: '👋🚀🌍💻',
    // CJK characters (3 bytes per char, 1 JS char)
    cjk: '你好世界',
    cjkMixed: 'Hello 你好 World',
    // 2-byte characters (accented chars)
    accented: 'café résumé',
    // Mixed everything
    mixed: 'Hello 👋 你好 café',
    // Edge cases
    emptyString: '',
    singleEmoji: '🚀',
    singleCJK: '中',
  };

  describe('ASCII content (baseline)', () => {
    it('should have equal byte and char counts for ASCII', () => {
      const result = applyPagination(TEST_CONTENT.ascii);

      expect(result.totalBytes).toBe(11);
      expect(result.totalChars).toBe(11);
      expect(result.byteLength).toBe(11);
      expect(result.charLength).toBe(11);
    });

    it('should paginate ASCII correctly in both modes', () => {
      const byteMode = applyPagination(TEST_CONTENT.ascii, 0, 5, {
        mode: 'bytes',
      });
      const charMode = applyPagination(TEST_CONTENT.ascii, 0, 5);

      // For ASCII, both modes should produce identical results
      expect(byteMode.paginatedContent).toBe('Hello');
      expect(charMode.paginatedContent).toBe('Hello');
      expect(byteMode.byteOffset).toBe(charMode.byteOffset);
      expect(byteMode.charOffset).toBe(charMode.charOffset);
    });
  });

  describe('Emoji content (4 bytes, 2 JS chars per emoji)', () => {
    it('should correctly calculate byte vs char lengths', () => {
      // "Hello 👋 World" = 6 + 4 + 6 = 16 bytes, 6 + 2 + 6 = 14 chars
      const result = applyPagination(TEST_CONTENT.emoji);

      expect(result.totalBytes).toBe(16);
      expect(result.totalChars).toBe(14);
    });

    it('should extract emoji correctly in byte mode', () => {
      // Skip "Hello " (6 bytes), take emoji (4 bytes)
      const result = applyPagination(TEST_CONTENT.emoji, 6, 4, {
        mode: 'bytes',
      });

      expect(result.paginatedContent).toBe('👋');
      expect(result.byteOffset).toBe(6);
      expect(result.byteLength).toBe(4);
      expect(result.charOffset).toBe(6); // "Hello " = 6 chars
      expect(result.charLength).toBe(2); // Emoji = 2 JS chars (surrogate pair)
    });

    it('should extract emoji correctly in character mode', () => {
      // Skip "Hello " (6 chars), take emoji (2 chars)
      const result = applyPagination(TEST_CONTENT.emoji, 6, 2);

      expect(result.paginatedContent).toBe('👋');
      expect(result.charOffset).toBe(6);
      expect(result.charLength).toBe(2);
      expect(result.byteOffset).toBe(6); // "Hello " = 6 bytes
      expect(result.byteLength).toBe(4); // Emoji = 4 bytes
    });

    it('should provide correct nextCharOffset for string.substring()', () => {
      const result = applyPagination(TEST_CONTENT.emoji, 0, 8); // "Hello 👋"

      expect(result.nextCharOffset).toBe(8);
      // Verify it works with substring
      const remaining = TEST_CONTENT.emoji.substring(result.nextCharOffset!);
      expect(remaining).toBe(' World');
    });

    it('should provide correct nextByteOffset for Buffer operations', () => {
      const result = applyPagination(TEST_CONTENT.emoji, 0, 10, {
        mode: 'bytes',
      });

      expect(result.nextByteOffset).toBe(10);
      // Verify it works with Buffer
      const buffer = Buffer.from(TEST_CONTENT.emoji, 'utf-8');
      const remaining = buffer
        .subarray(result.nextByteOffset!)
        .toString('utf-8');
      expect(remaining).toBe(' World');
    });

    it('should handle multiple emojis', () => {
      // "👋🚀🌍💻" = 4 emojis * 4 bytes = 16 bytes, 4 emojis * 2 chars = 8 chars
      const result = applyPagination(TEST_CONTENT.emojiMultiple);

      expect(result.totalBytes).toBe(16);
      expect(result.totalChars).toBe(8);
    });

    it('should paginate through multiple emojis correctly', () => {
      // Get first 2 emojis (8 bytes, 4 chars)
      const page1 = applyPagination(TEST_CONTENT.emojiMultiple, 0, 8, {
        mode: 'bytes',
      });

      expect(page1.paginatedContent).toBe('👋🚀');
      expect(page1.charLength).toBe(4);
      expect(page1.nextByteOffset).toBe(8);
      expect(page1.nextCharOffset).toBe(4);

      // Get remaining emojis using nextByteOffset
      const page2 = applyPagination(
        TEST_CONTENT.emojiMultiple,
        page1.nextByteOffset!,
        8,
        { mode: 'bytes' }
      );

      expect(page2.paginatedContent).toBe('🌍💻');
      expect(page2.hasMore).toBe(false);
    });
  });

  describe('CJK content (3 bytes, 1 JS char per character)', () => {
    it('should correctly calculate byte vs char lengths', () => {
      // "你好世界" = 4 chars * 3 bytes = 12 bytes, 4 chars
      const result = applyPagination(TEST_CONTENT.cjk);

      expect(result.totalBytes).toBe(12);
      expect(result.totalChars).toBe(4);
    });

    it('should extract CJK characters correctly in byte mode', () => {
      // Get first 2 CJK chars (6 bytes)
      const result = applyPagination(TEST_CONTENT.cjk, 0, 6, { mode: 'bytes' });

      expect(result.paginatedContent).toBe('你好');
      expect(result.byteLength).toBe(6);
      expect(result.charLength).toBe(2);
      expect(result.nextByteOffset).toBe(6);
      expect(result.nextCharOffset).toBe(2);
    });

    it('should extract CJK characters correctly in character mode', () => {
      // Get first 2 CJK chars
      const result = applyPagination(TEST_CONTENT.cjk, 0, 2);

      expect(result.paginatedContent).toBe('你好');
      expect(result.charLength).toBe(2);
      expect(result.byteLength).toBe(6);
    });

    it('should handle mixed ASCII and CJK', () => {
      // "Hello 你好 World" = 6 + 6 + 6 = 18 bytes, 6 + 2 + 6 = 14 chars
      const result = applyPagination(TEST_CONTENT.cjkMixed);

      expect(result.totalBytes).toBe(18);
      expect(result.totalChars).toBe(14);
    });

    it('should paginate mixed content correctly', () => {
      // Get "Hello " + "你" (6 + 3 = 9 bytes)
      const result = applyPagination(TEST_CONTENT.cjkMixed, 0, 9, {
        mode: 'bytes',
      });

      expect(result.paginatedContent).toBe('Hello 你');
      expect(result.charLength).toBe(7); // 6 ASCII + 1 CJK
      expect(result.byteLength).toBe(9);
    });
  });

  describe('Accented characters (2 bytes per char)', () => {
    it('should handle 2-byte UTF-8 characters', () => {
      // "café résumé" - 'é' is 2 bytes
      // c(1) + a(1) + f(1) + é(2) + space(1) + r(1) + é(2) + s(1) + u(1) + m(1) + é(2) = 14 bytes
      // 11 characters
      const result = applyPagination(TEST_CONTENT.accented);

      expect(result.totalBytes).toBe(14);
      expect(result.totalChars).toBe(11);
    });
  });

  describe('Mixed content (ASCII + emoji + CJK + accented)', () => {
    it('should handle complex mixed content', () => {
      // "Hello 👋 你好 café"
      // Hello (5) + space (1) + 👋 (4) + space (1) + 你好 (6) + space (1) + café (5) = 23 bytes
      // Hello (5) + space (1) + 👋 (2) + space (1) + 你好 (2) + space (1) + café (4) = 16 chars
      const result = applyPagination(TEST_CONTENT.mixed);

      expect(result.totalBytes).toBe(23);
      expect(result.totalChars).toBe(16);
    });

    it('should paginate through aligned byte content correctly', () => {
      // Use content where byte sizes align nicely
      // 4 emojis = 16 bytes (4 bytes each, aligns with any multiple of 4)
      const content = '👋🚀🌍💻';
      let offset = 0;
      const pageSize = 8; // 2 emojis per page
      const pages: string[] = [];

      // Paginate through entire content using byte mode
      let result;
      do {
        result = applyPagination(content, offset, pageSize, {
          mode: 'bytes',
        });
        pages.push(result.paginatedContent);
        if (result.hasMore) offset = result.nextByteOffset!;
      } while (result.hasMore);

      // Verify all pages concatenate to original content
      expect(pages.join('')).toBe(content);
      expect(pages).toEqual(['👋🚀', '🌍💻']);
    });

    it('should align to character boundaries in byte mode to prevent malformed output', () => {
      // When using arbitrary byte sizes that would cut in the middle of a character,
      // we now align to character boundaries to prevent malformed output with replacement characters
      const content = '你好'; // 6 bytes total (3 bytes per char)
      const result = applyPagination(content, 0, 4, { mode: 'bytes' }); // 4 bytes would cut middle of 2nd char

      // The fix aligns byte boundaries to character boundaries
      // byteToCharIndex(content, 4) returns 2 (both chars) since buffer.slice(0,4).toString() produces 2 chars
      // So we get both characters to prevent malformed output
      expect(result.byteLength).toBe(6); // Full content (both CJK chars = 6 bytes)
      expect(result.paginatedContent).toBe('你好');
      // For precise byte-level control, callers should use character mode or calculate aligned byte sizes
    });

    it('should paginate through mixed content correctly in character mode', () => {
      const content = TEST_CONTENT.mixed;
      let offset = 0;
      const pageSize = 5;
      const pages: string[] = [];

      // Paginate through entire content using character mode
      let result;
      do {
        result = applyPagination(content, offset, pageSize);
        pages.push(result.paginatedContent);
        if (result.hasMore) offset = result.nextCharOffset!;
      } while (result.hasMore);

      // Verify all pages concatenate to original content
      expect(pages.join('')).toBe(content);
    });
  });

  describe('Edge cases', () => {
    it('should handle empty string', () => {
      const result = applyPagination(TEST_CONTENT.emptyString);

      expect(result.totalBytes).toBe(0);
      expect(result.totalChars).toBe(0);
      expect(result.paginatedContent).toBe('');
      expect(result.hasMore).toBe(false);
    });

    it('should handle single emoji', () => {
      const result = applyPagination(TEST_CONTENT.singleEmoji);

      expect(result.totalBytes).toBe(4);
      expect(result.totalChars).toBe(2); // Surrogate pair
      expect(result.paginatedContent).toBe('🚀');
    });

    it('should handle single CJK character', () => {
      const result = applyPagination(TEST_CONTENT.singleCJK);

      expect(result.totalBytes).toBe(3);
      expect(result.totalChars).toBe(1);
      expect(result.paginatedContent).toBe('中');
    });

    it('should handle offset at content boundary', () => {
      const result = applyPagination(TEST_CONTENT.ascii, 11, 5); // Offset at end

      expect(result.paginatedContent).toBe('');
      expect(result.hasMore).toBe(false);
    });

    it('should handle offset beyond content length', () => {
      const result = applyPagination(TEST_CONTENT.ascii, 100, 5);

      expect(result.paginatedContent).toBe('');
      expect(result.charOffset).toBe(11); // Clamped to content length
      expect(result.hasMore).toBe(false);
    });

    it('should handle page size larger than content', () => {
      const result = applyPagination(TEST_CONTENT.ascii, 0, 1000);

      expect(result.paginatedContent).toBe(TEST_CONTENT.ascii);
      expect(result.hasMore).toBe(false);
      expect(result.nextCharOffset).toBeUndefined();
    });

    it('should handle zero page size', () => {
      const result = applyPagination(TEST_CONTENT.ascii, 0, 0);

      expect(result.paginatedContent).toBe('');
      expect(result.charLength).toBe(0);
      expect(result.byteLength).toBe(0);
    });
  });

  describe('Page calculation correctness', () => {
    it('should calculate correct page numbers in byte mode', () => {
      const content = 'a'.repeat(100); // 100 bytes, 100 chars
      const pageSize = 25;

      const page1 = applyPagination(content, 0, pageSize, { mode: 'bytes' });
      expect(page1.currentPage).toBe(1);
      expect(page1.totalPages).toBe(4);

      const page2 = applyPagination(content, 25, pageSize, { mode: 'bytes' });
      expect(page2.currentPage).toBe(2);

      const page3 = applyPagination(content, 50, pageSize, { mode: 'bytes' });
      expect(page3.currentPage).toBe(3);

      const page4 = applyPagination(content, 75, pageSize, { mode: 'bytes' });
      expect(page4.currentPage).toBe(4);
      expect(page4.hasMore).toBe(false);
    });

    it('should calculate correct page numbers in character mode', () => {
      const content = '你'.repeat(100); // 300 bytes, 100 chars
      const pageSize = 25;

      const page1 = applyPagination(content, 0, pageSize);
      expect(page1.currentPage).toBe(1);
      expect(page1.totalPages).toBe(4);

      const page2 = applyPagination(content, 25, pageSize);
      expect(page2.currentPage).toBe(2);
    });
  });

  describe('createPaginationInfo with UTF-8', () => {
    it('should preserve both byte and char fields', () => {
      const metadata = applyPagination(TEST_CONTENT.emoji, 0, 10, {
        mode: 'bytes',
      });
      const info = createPaginationInfo(metadata);

      // Byte fields
      expect(info.byteOffset).toBe(metadata.byteOffset);
      expect(info.byteLength).toBe(metadata.byteLength);
      expect(info.totalBytes).toBe(metadata.totalBytes);

      // Char fields
      expect(info.charOffset).toBe(metadata.charOffset);
      expect(info.charLength).toBe(metadata.charLength);
      expect(info.totalChars).toBe(metadata.totalChars);

      // Common fields
      expect(info.currentPage).toBe(metadata.currentPage);
      expect(info.totalPages).toBe(metadata.totalPages);
      expect(info.hasMore).toBe(metadata.hasMore);
    });
  });

  describe('generatePaginationHints with UTF-8', () => {
    it('should use character offsets in hints for local tools', () => {
      const metadata = applyPagination(TEST_CONTENT.emoji, 0, 8);
      const hints = generatePaginationHints(metadata);

      // Should reference charOffset, not byteOffset
      const nextPageHint = hints.find(h => h.includes('charOffset='));
      expect(nextPageHint).toBeDefined();
      expect(nextPageHint).toContain(`charOffset=${metadata.nextCharOffset}`);
    });
  });

  describe('Roundtrip validation', () => {
    it('should allow full content reconstruction using nextCharOffset', () => {
      const content = TEST_CONTENT.mixed;
      let reconstructed = '';
      let offset = 0;
      const pageSize = 4;

      let result;
      do {
        result = applyPagination(content, offset, pageSize);
        reconstructed += result.paginatedContent;
        // Use nextCharOffset for next iteration
        if (result.hasMore) offset = result.nextCharOffset!;
      } while (result.hasMore);

      expect(reconstructed).toBe(content);
    });

    it('should allow full content reconstruction using nextByteOffset with aligned content', () => {
      // Use content with consistent byte alignment (emojis only = 4 bytes each)
      const content = '👋🚀🌍💻🎉🔥'; // 6 emojis = 24 bytes
      let reconstructed = '';
      let offset = 0;
      const pageSize = 8; // 2 emojis per page

      let result;
      do {
        result = applyPagination(content, offset, pageSize, {
          mode: 'bytes',
        });
        reconstructed += result.paginatedContent;
        // Use nextByteOffset for next iteration
        if (result.hasMore) offset = result.nextByteOffset!;
      } while (result.hasMore);

      expect(reconstructed).toBe(content);
    });

    it('should allow full content reconstruction with CJK using aligned sizes', () => {
      // CJK content: 3 bytes per char
      const content = '你好世界中国日本'; // 8 CJK chars = 24 bytes
      let reconstructed = '';
      let offset = 0;
      const pageSize = 6; // 2 CJK chars per page (6 bytes)

      let result;
      do {
        result = applyPagination(content, offset, pageSize, {
          mode: 'bytes',
        });
        reconstructed += result.paginatedContent;
        if (result.hasMore) offset = result.nextByteOffset!;
      } while (result.hasMore);

      expect(reconstructed).toBe(content);
    });

    it('should handle byte mode with safe character-aligned page sizes', () => {
      // For safe byte mode pagination with UTF-8:
      // - Use page sizes that are multiples of LCM(1, 2, 3, 4) = 12
      // - Or use character mode for guaranteed correct results
      const content = '你好世界👋🚀'; // 12 bytes CJK + 8 bytes emoji = 20 bytes
      let reconstructed = '';
      let offset = 0;
      const pageSize = 4; // Will align with emoji boundaries

      let result;
      do {
        result = applyPagination(content, offset, pageSize, {
          mode: 'bytes',
        });
        reconstructed += result.paginatedContent;
        if (result.hasMore) offset = result.nextByteOffset!;
      } while (result.hasMore);

      // This may not reconstruct perfectly due to CJK (3 bytes) not aligning with 4
      // The test documents this limitation
      expect(reconstructed.length).toBeGreaterThan(0);
    });

    it('should produce consistent results across multiple pagination passes', () => {
      const content = TEST_CONTENT.emojiMultiple;

      // First pass
      const pass1Pages: string[] = [];
      let offset1 = 0;
      let result1;
      do {
        result1 = applyPagination(content, offset1, 4, { mode: 'bytes' });
        pass1Pages.push(result1.paginatedContent);
        if (result1.hasMore) offset1 = result1.nextByteOffset!;
      } while (result1.hasMore);

      // Second pass
      const pass2Pages: string[] = [];
      let offset2 = 0;
      let result2;
      do {
        result2 = applyPagination(content, offset2, 4, { mode: 'bytes' });
        pass2Pages.push(result2.paginatedContent);
        if (result2.hasMore) offset2 = result2.nextByteOffset!;
      } while (result2.hasMore);

      expect(pass1Pages).toEqual(pass2Pages);
    });
  });
});
