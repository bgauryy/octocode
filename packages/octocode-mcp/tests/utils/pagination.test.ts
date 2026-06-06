import { describe, it, expect } from 'vitest';
import {
  applyPagination,
  serializeForPagination,
  createPaginationInfo,
} from '../../src/utils/pagination/core.js';
import {
  generatePaginationHints,
  generateGitHubPaginationHints,
  generateStructurePaginationHints,
} from '../../src/utils/pagination/hints.js';
import type { PaginationMetadata } from '../../src/utils/pagination/types.js';
import { sliceByCharRespectLines } from '../../src/utils/pagination/core.js';

describe('pagination utility', () => {
  describe('applyPagination', () => {
    it('should return full content when no charLength provided', () => {
      const content = 'Hello World';
      const result = applyPagination(content);

      expect(result.paginatedContent).toBe(content);
      expect(result.charOffset).toBe(0);
      expect(result.charLength).toBe(11);
      expect(result.totalChars).toBe(11);
      expect(result.hasMore).toBe(false);
      expect(result.currentPage).toBe(1);
      expect(result.totalPages).toBe(1);
    });

    it('should paginate content with charLength', () => {
      const content = 'Hello World, this is a test';
      const result = applyPagination(content, 0, 10);

      expect(result.paginatedContent).toBe('Hello Worl');
      expect(result.charOffset).toBe(0);
      expect(result.charLength).toBe(10);
      expect(result.totalChars).toBe(27);
      expect(result.hasMore).toBe(true);
      expect(result.nextCharOffset).toBe(10);
    });

    it('should handle charOffset', () => {
      const content = 'Hello World';
      const result = applyPagination(content, 6, 5);

      expect(result.paginatedContent).toBe('World');
      expect(result.charOffset).toBe(6);
      expect(result.charLength).toBe(5);
      expect(result.hasMore).toBe(false);
    });

    it('should calculate page numbers correctly', () => {
      const content = 'a'.repeat(100);
      const result = applyPagination(content, 50, 25);

      expect(result.currentPage).toBe(3);
      expect(result.totalPages).toBe(4);
    });

    it('should handle charOffset beyond content length', () => {
      const content = 'Short';
      const result = applyPagination(content, 100, 10);

      expect(result.paginatedContent).toBe('');
      expect(result.charOffset).toBe(5);
      expect(result.hasMore).toBe(false);
    });

    it('should use actualOffset for page calculation when provided', () => {
      const content = 'Hello World Test Content';
      const result = applyPagination(content, 5, 10, { actualOffset: 10 });

      expect(result.currentPage).toBe(2);
    });

    it('should handle UTF-8 byte offsets correctly (failing case)', () => {
      const content = 'a🚀b';
      const result = applyPagination(content, 1, 4, { mode: 'bytes' });

      expect(result.paginatedContent).toBe('🚀');
      expect(result.byteLength).toBe(4);
    });

    it('should handle bytes mode reaching end of content (hasMore=false)', () => {
      const content = 'Hello';
      const result = applyPagination(content, 3, 10, { mode: 'bytes' });

      expect(result.paginatedContent).toBe('lo');
      expect(result.hasMore).toBe(false);
      expect(result.nextCharOffset).toBeUndefined();
    });

    it('should handle bytes mode with multi-byte UTF-8 at exact boundary', () => {
      const content = '你好世界';
      const result = applyPagination(content, 0, 6, { mode: 'bytes' });

      expect(result.paginatedContent).toBe('你好');
      expect(result.byteLength).toBe(6);
      expect(result.hasMore).toBe(true);
      expect(result.nextByteOffset).toBe(6);
    });

    it('should handle bytes mode with exact fit content', () => {
      const content = 'abc';
      const result = applyPagination(content, 0, 3, { mode: 'bytes' });

      expect(result.paginatedContent).toBe('abc');
      expect(result.hasMore).toBe(false);
      expect(result.nextCharOffset).toBeUndefined();
      expect(result.totalChars).toBe(3);
    });

    it('should return estimated tokens correctly', () => {
      const content = 'a'.repeat(400);
      const result = applyPagination(content, 0, 200);

      expect(result.estimatedTokens).toBe(50);
    });

    it('should handle zero charOffset explicitly', () => {
      const content = 'Test content';
      const result = applyPagination(content, 0, 5);

      expect(result.paginatedContent).toBe('Test ');
      expect(result.charOffset).toBe(0);
    });

    it('should set nextCharOffset to undefined when at last page in character mode', () => {
      const content = 'Short';
      const result = applyPagination(content, 0, 5);

      expect(result.hasMore).toBe(false);
      expect(result.nextCharOffset).toBeUndefined();
    });

    describe('byte/character offset separation', () => {
      it('should return correct byte and char offsets for emoji content', () => {
        const content = 'Hello 👋 World';
        const result = applyPagination(content, 0, 10, { mode: 'bytes' });

        expect(result.paginatedContent).toBe('Hello 👋');
        expect(result.byteOffset).toBe(0);
        expect(result.byteLength).toBe(10);
        expect(result.totalBytes).toBe(16);
        expect(result.charOffset).toBe(0);
        expect(result.charLength).toBe(8);
        expect(result.totalChars).toBe(14);
        expect(result.nextByteOffset).toBe(10);
        expect(result.nextCharOffset).toBe(8);
      });

      it('should return correct offsets for CJK content in bytes mode', () => {
        const content = '你好世界';
        const result = applyPagination(content, 0, 6, { mode: 'bytes' });

        expect(result.paginatedContent).toBe('你好');
        expect(result.byteOffset).toBe(0);
        expect(result.byteLength).toBe(6);
        expect(result.totalBytes).toBe(12);
        expect(result.charOffset).toBe(0);
        expect(result.charLength).toBe(2);
        expect(result.totalChars).toBe(4);
      });

      it('should return correct offsets for CJK content in character mode', () => {
        const content = '你好世界';
        const result = applyPagination(content, 0, 2);

        expect(result.paginatedContent).toBe('你好');
        expect(result.charOffset).toBe(0);
        expect(result.charLength).toBe(2);
        expect(result.totalChars).toBe(4);
        expect(result.byteOffset).toBe(0);
        expect(result.byteLength).toBe(6);
        expect(result.totalBytes).toBe(12);
      });

      it('should allow using nextCharOffset with substring correctly', () => {
        const content = 'Hello 👋 World';
        const page1 = applyPagination(content, 0, 8);

        expect(page1.paginatedContent).toBe('Hello 👋');
        expect(page1.nextCharOffset).toBe(8);

        const remainingContent = content.substring(page1.nextCharOffset!);
        expect(remainingContent).toBe(' World');
      });

      it('should allow using nextByteOffset with Buffer correctly', () => {
        const content = 'Hello 👋 World';
        const page1 = applyPagination(content, 0, 10, { mode: 'bytes' });

        expect(page1.paginatedContent).toBe('Hello 👋');
        expect(page1.nextByteOffset).toBe(10);

        const buffer = Buffer.from(content, 'utf-8');
        const remainingContent = buffer
          .subarray(page1.nextByteOffset!)
          .toString('utf-8');
        expect(remainingContent).toBe(' World');
      });

      it('should return undefined for fullContent without pagination', () => {
        const content = 'Hello 👋 World';
        const result = applyPagination(content);

        expect(result.byteOffset).toBe(0);
        expect(result.byteLength).toBe(16);
        expect(result.totalBytes).toBe(16);
        expect(result.nextByteOffset).toBeUndefined();
        expect(result.charOffset).toBe(0);
        expect(result.charLength).toBe(14);
        expect(result.totalChars).toBe(14);
        expect(result.nextCharOffset).toBeUndefined();
        expect(result.hasMore).toBe(false);
      });
    });
  });

  describe('generatePaginationHints', () => {
    const withByteFields = (
      meta: Omit<
        PaginationMetadata,
        'byteOffset' | 'byteLength' | 'totalBytes'
      > & { charOffset: number; charLength: number; totalChars: number }
    ): PaginationMetadata => ({
      ...meta,
      byteOffset: meta.charOffset,
      byteLength: meta.charLength,
      totalBytes: meta.totalChars,
    });

    it('should surface a size-recovery directive above 50K tokens', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'x'.repeat(200000),
        charOffset: 0,
        charLength: 200000,
        totalChars: 200000,
        hasMore: false,
        estimatedTokens: 55000,
        currentPage: 1,
        totalPages: 1,
      });

      const hints = generatePaginationHints(metadata);

      expect(hints.some(h => h.includes('exceeds typical context'))).toBe(true);
      expect(hints.some(h => h.includes('Reduce charLength'))).toBe(true);
    });

    it('should surface a softer recovery directive between 30K and 50K tokens', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'x'.repeat(100000),
        charOffset: 0,
        charLength: 100000,
        totalChars: 100000,
        hasMore: false,
        estimatedTokens: 30001,
        currentPage: 1,
        totalPages: 1,
      });

      const hints = generatePaginationHints(metadata);

      expect(hints.some(h => h.includes('approaching context limit'))).toBe(
        true
      );
    });

    it('should NOT narrate moderate token usage (15K-30K) — agent already sees size', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'x'.repeat(50000),
        charOffset: 0,
        charLength: 50000,
        totalChars: 50000,
        hasMore: false,
        estimatedTokens: 15001,
        currentPage: 1,
        totalPages: 1,
      });

      const hints = generatePaginationHints(metadata);
      expect(hints).toEqual([]);
    });

    it('should NOT narrate "Moderate usage" / "Efficient query" — pure noise', () => {
      const moderate: PaginationMetadata = withByteFields({
        paginatedContent: 'x'.repeat(20000),
        charOffset: 0,
        charLength: 20000,
        totalChars: 20000,
        hasMore: false,
        estimatedTokens: 5001,
        currentPage: 1,
        totalPages: 1,
      });
      const efficient: PaginationMetadata = withByteFields({
        paginatedContent: 'Hello World',
        charOffset: 0,
        charLength: 11,
        totalChars: 11,
        hasMore: false,
        estimatedTokens: 3,
        currentPage: 1,
        totalPages: 1,
      });
      expect(generatePaginationHints(moderate)).toEqual([]);
      expect(generatePaginationHints(efficient)).toEqual([]);
    });

    it('should disable warnings when enableWarnings is false', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'x'.repeat(200000),
        charOffset: 0,
        charLength: 200000,
        totalChars: 200000,
        hasMore: false,
        estimatedTokens: 55000,
        currentPage: 1,
        totalPages: 1,
      });

      const hints = generatePaginationHints(metadata, {
        enableWarnings: false,
      });

      expect(hints.some(h => h.includes('exceeds typical context'))).toBe(
        false
      );
    });

    it('should include custom hints', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'test',
        charOffset: 0,
        charLength: 4,
        totalChars: 4,
        hasMore: false,
        estimatedTokens: 1,
        currentPage: 1,
        totalPages: 1,
      });

      const hints = generatePaginationHints(metadata, {
        customHints: ['Custom hint 1', 'Custom hint 2'],
      });

      expect(hints).toContain('Custom hint 1');
      expect(hints).toContain('Custom hint 2');
    });

    it('should emit a single pagination cursor when hasMore is true', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'Hello',
        charOffset: 0,
        charLength: 5,
        totalChars: 20,
        hasMore: true,
        nextCharOffset: 5,
        estimatedTokens: 2,
        currentPage: 1,
        totalPages: 4,
      });

      const hints = generatePaginationHints(metadata);

      expect(hints.some(h => h.includes('Page 1/4'))).toBe(true);
      expect(hints.some(h => h.includes('charOffset=5'))).toBe(true);
    });

    it('should emit NO hints on final page (no "Final page" tautology)', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'World',
        charOffset: 15,
        charLength: 5,
        totalChars: 20,
        hasMore: false,
        estimatedTokens: 2,
        currentPage: 4,
        totalPages: 4,
      });

      expect(generatePaginationHints(metadata)).toEqual([]);
    });

    it('should not show navigation hints when on first page with no more', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'Hello',
        charOffset: 0,
        charLength: 5,
        totalChars: 5,
        hasMore: false,
        estimatedTokens: 2,
        currentPage: 1,
        totalPages: 1,
      });

      expect(generatePaginationHints(metadata)).toEqual([]);
    });

    it('should handle metadata without estimatedTokens', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'Hello',
        charOffset: 0,
        charLength: 5,
        totalChars: 10,
        hasMore: true,
        nextCharOffset: 5,
        currentPage: 1,
        totalPages: 2,
      });

      const hints = generatePaginationHints(metadata);

      expect(hints.some(h => h.includes('charOffset=5'))).toBe(true);
      expect(hints.some(h => h.includes('tokens'))).toBe(false);
    });

    it('should handle missing nextCharOffset when hasMore is true', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'Hello',
        charOffset: 0,
        charLength: 5,
        totalChars: 10,
        hasMore: true,
        currentPage: 1,
        totalPages: 2,
      });

      const hints = generatePaginationHints(metadata);

      expect(hints.some(h => h.includes('charOffset='))).toBe(false);
    });

    it('should include toolName in hints if provided', () => {
      const metadata: PaginationMetadata = withByteFields({
        paginatedContent: 'Hello',
        charOffset: 0,
        charLength: 5,
        totalChars: 10,
        hasMore: true,
        nextCharOffset: 5,
        estimatedTokens: 2,
        currentPage: 1,
        totalPages: 2,
      });

      const hints = generatePaginationHints(metadata, {
        toolName: 'testTool',
      });

      expect(Array.isArray(hints)).toBe(true);
    });
  });

  describe('serializeForPagination', () => {
    it('should serialize data to JSON', () => {
      const data = { name: 'test', value: 123 };
      const result = serializeForPagination(data);

      expect(result).toBe('{"name":"test","value":123}');
    });

    it('should pretty print when requested', () => {
      const data = { name: 'test' };
      const result = serializeForPagination(data, true);

      expect(result).toBe('{\n  "name": "test"\n}');
    });

    it('should serialize arrays', () => {
      const data = [1, 2, 3];
      const result = serializeForPagination(data);

      expect(result).toBe('[1,2,3]');
    });
  });

  describe('sliceByCharRespectLines', () => {
    it('should handle empty text', () => {
      const result = sliceByCharRespectLines('', 0, 100);

      expect(result.sliced).toBe('');
      expect(result.actualOffset).toBe(0);
      expect(result.actualLength).toBe(0);
      expect(result.hasMore).toBe(false);
      expect(result.lineCount).toBe(0);
      expect(result.totalChars).toBe(0);
    });

    it('should handle charOffset beyond text length', () => {
      const text = 'Hello World';
      const result = sliceByCharRespectLines(text, 100, 10);

      expect(result.sliced).toBe('');
      expect(result.actualOffset).toBe(11);
      expect(result.actualLength).toBe(0);
      expect(result.hasMore).toBe(false);
      expect(result.nextOffset).toBe(11);
    });

    it('should slice from beginning respecting line boundaries', () => {
      const text = 'line1\nline2\nline3\n';
      const result = sliceByCharRespectLines(text, 0, 10);

      expect(result.sliced).toBe('line1\nline2\n');
      expect(result.actualOffset).toBe(0);
      expect(result.hasMore).toBe(true);
      expect(result.lineCount).toBe(2);
    });

    it('should adjust offset to line boundary when mid-line', () => {
      const text = 'line1\nline2\nline3\n';
      const result = sliceByCharRespectLines(text, 8, 10);

      expect(result.actualOffset).toBe(6);
      expect(result.sliced.startsWith('line2')).toBe(true);
    });

    it('should extend to complete the line at end', () => {
      const text = 'line1\nline2\nline3\n';
      const result = sliceByCharRespectLines(text, 0, 8);

      expect(result.sliced).toBe('line1\nline2\n');
      expect(result.actualLength).toBe(12);
    });

    it('should handle text without trailing newline', () => {
      const text = 'line1\nline2';
      const result = sliceByCharRespectLines(text, 0, 20);

      expect(result.sliced).toBe('line1\nline2');
      expect(result.hasMore).toBe(false);
      expect(result.lineCount).toBe(1);
    });

    it('should handle single line text', () => {
      const text = 'This is a single line without newline';
      const result = sliceByCharRespectLines(text, 0, 20);

      expect(result.sliced).toBe(text);
      expect(result.hasMore).toBe(false);
    });

    it('should return correct nextOffset', () => {
      const text = 'line1\nline2\nline3\n';
      const result = sliceByCharRespectLines(text, 0, 6);

      expect(result.sliced).toBe('line1\n');
      expect(result.nextOffset).toBe(6);
      expect(result.hasMore).toBe(true);
    });

    it('should handle minified content (single long line)', () => {
      const text = 'a'.repeat(100);
      const result = sliceByCharRespectLines(text, 0, 50);

      expect(result.sliced).toBe(text);
      expect(result.hasMore).toBe(false);
    });

    it('should correctly count lines', () => {
      const text = 'a\nb\nc\nd\n';
      const result = sliceByCharRespectLines(text, 0, 100);

      expect(result.lineCount).toBe(4);
    });

    it('should handle offset at exact line boundary', () => {
      const text = 'line1\nline2\nline3\n';
      const result = sliceByCharRespectLines(text, 6, 6);

      expect(result.actualOffset).toBe(6);
      expect(result.sliced).toBe('line2\n');
    });

    it('should handle when charLength exceeds remaining text', () => {
      const text = 'line1\nline2\n';
      const result = sliceByCharRespectLines(text, 6, 1000);

      expect(result.sliced).toBe('line2\n');
      expect(result.hasMore).toBe(false);
      expect(result.nextOffset).toBeUndefined();
    });

    it('should return totalChars correctly', () => {
      const text = 'test content\n';
      const result = sliceByCharRespectLines(text, 0, 5);

      expect(result.totalChars).toBe(13);
    });

    it('should handle text ending exactly at charLength', () => {
      const text = 'line1\n';
      const result = sliceByCharRespectLines(text, 0, 6);

      expect(result.sliced).toBe('line1\n');
      expect(result.hasMore).toBe(false);
      expect(result.actualLength).toBe(6);
    });
  });

  describe('createPaginationInfo', () => {
    const withByteFieldsInfo = (
      meta: Omit<
        PaginationMetadata,
        'byteOffset' | 'byteLength' | 'totalBytes'
      > & { charOffset: number; charLength: number; totalChars: number }
    ): PaginationMetadata => ({
      ...meta,
      byteOffset: meta.charOffset,
      byteLength: meta.charLength,
      totalBytes: meta.totalChars,
    });

    it('should extract pagination info from metadata', () => {
      const metadata: PaginationMetadata = withByteFieldsInfo({
        paginatedContent: 'Hello World',
        charOffset: 10,
        charLength: 11,
        totalChars: 100,
        hasMore: true,
        nextCharOffset: 21,
        estimatedTokens: 3,
        currentPage: 2,
        totalPages: 10,
      });

      const info = createPaginationInfo(metadata);

      expect(info.currentPage).toBe(2);
      expect(info.totalPages).toBe(10);
      expect(info.charOffset).toBe(10);
      expect(info.charLength).toBe(11);
      expect(info.totalChars).toBe(100);
      expect(info.hasMore).toBe(true);
    });

    it('should work for non-paginated content', () => {
      const metadata: PaginationMetadata = withByteFieldsInfo({
        paginatedContent: 'Full content',
        charOffset: 0,
        charLength: 12,
        totalChars: 12,
        hasMore: false,
        estimatedTokens: 3,
        currentPage: 1,
        totalPages: 1,
      });

      const info = createPaginationInfo(metadata);

      expect(info.currentPage).toBe(1);
      expect(info.totalPages).toBe(1);
      expect(info.hasMore).toBe(false);
    });
  });

  describe('generateGitHubPaginationHints', () => {
    it('emits NO hints on final page (no "Complete content retrieved" tautology)', () => {
      const pagination = {
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        charOffset: 0,
        charLength: 100,
        totalChars: 100,
      };
      const query = {
        owner: 'test-owner',
        repo: 'test-repo',
        path: 'src/index.ts',
        branch: 'main',
      };

      expect(generateGitHubPaginationHints(pagination, query)).toEqual([]);
    });

    it('emits a single cursor line when hasMore is true', () => {
      const pagination = {
        currentPage: 1,
        totalPages: 3,
        hasMore: true,
        byteOffset: 0,
        byteLength: 20000,
        totalBytes: 60000,
      };
      const query = {
        owner: 'test-owner',
        repo: 'test-repo',
        path: 'src/index.ts',
        branch: 'main',
      };

      const hints = generateGitHubPaginationHints(pagination, query);

      expect(hints).toHaveLength(1);
      expect(hints[0]).toContain('Page 1/3');
      expect(hints[0]).toContain('charOffset=20000');
      expect(hints[0]).not.toContain('owner=');
      expect(hints[0]).not.toContain('TO GET NEXT PAGE');
    });

    it('emits no hint on the final page even with branch provided', () => {
      const pagination = {
        currentPage: 2,
        totalPages: 3,
        hasMore: false,
        charOffset: 40000,
        charLength: 20000,
        totalChars: 60000,
      };
      const query = {
        owner: 'test-owner',
        repo: 'test-repo',
        path: 'src/index.ts',
      };

      expect(generateGitHubPaginationHints(pagination, query)).toEqual([]);
    });
  });

  describe('generateStructurePaginationHints', () => {
    it('emits NO hint when there is only one page (no narration of counts)', () => {
      const pagination = {
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        entriesPerPage: 50,
        totalEntries: 35,
      };
      const context = {
        owner: 'test-owner',
        repo: 'test-repo',
        branch: 'main',
        pageFiles: 30,
        pageFolders: 5,
        allFiles: 30,
        allFolders: 5,
      };

      expect(generateStructurePaginationHints(pagination, context)).toEqual([]);
    });

    it('emits a single cursor line when hasMore is true', () => {
      const pagination = {
        currentPage: 1,
        totalPages: 3,
        hasMore: true,
        entriesPerPage: 20,
        totalEntries: 55,
      };
      const context = {
        owner: 'test-owner',
        repo: 'test-repo',
        branch: 'main',
        path: 'src',
        depth: 2,
        pageFiles: 15,
        pageFolders: 5,
        allFiles: 40,
        allFolders: 15,
      };

      const hints = generateStructurePaginationHints(pagination, context);

      expect(hints).toHaveLength(1);
      expect(hints[0]).toContain('Page 1/3');
      expect(hints[0]).toContain('entryPageNumber=2');
      expect(hints[0]).not.toContain('owner=');
      expect(hints[0]).not.toContain('TO GET NEXT PAGE');
    });

    it('emits NO hint on final page regardless of total entries', () => {
      const pagination = {
        currentPage: 3,
        totalPages: 3,
        hasMore: false,
        entriesPerPage: 20,
        totalEntries: 55,
      };
      const context = {
        owner: 'o',
        repo: 'r',
        branch: 'main',
        pageFiles: 15,
        pageFolders: 0,
        allFiles: 55,
        allFolders: 0,
      };

      expect(generateStructurePaginationHints(pagination, context)).toEqual([]);
    });
  });
});
