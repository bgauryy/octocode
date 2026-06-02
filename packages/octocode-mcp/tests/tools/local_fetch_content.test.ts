/**
 * Tests for localGetFileContent tool
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { LOCAL_TOOL_ERROR_CODES } from '../../src/errors/localToolErrors.js';
import { fetchContent } from '../../src/tools/local_fetch_content/fetchContent.js';
import * as pathValidator from 'octocode-security-utils/pathValidator';
import * as fs from 'fs/promises';

// Mock fs/promises
vi.mock('fs/promises', () => ({
  open: vi.fn(),
  readFile: vi.fn(),
  stat: vi.fn(),
}));

// Mock pathValidator
vi.mock('octocode-security-utils/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn(),
  },
}));

describe('localGetFileContent', () => {
  const mockOpen = vi.mocked(fs.open);
  const mockReadFile = vi.mocked(fs.readFile);
  const mockStat = vi.mocked(fs.stat);
  const mockValidate = vi.mocked(pathValidator.pathValidator.validate);

  beforeEach(() => {
    vi.clearAllMocks();
    mockValidate.mockReturnValue({ isValid: true });
    // Default: small file size (< 100KB)
    mockStat.mockResolvedValue({ size: 1024 } as unknown as Awaited<
      ReturnType<typeof fs.stat>
    >);
    mockOpen.mockResolvedValue({
      read: vi.fn().mockResolvedValue({ bytesRead: 0 }),
      close: vi.fn().mockResolvedValue(undefined),
    } as unknown as Awaited<ReturnType<typeof fs.open>>);
  });

  describe('Full content fetch', () => {
    it('should fetch full file content', async () => {
      const testContent = 'line 1\nline 2\nline 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        fullContent: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe(testContent);
      expect(result.isPartial).toBe(false);
      expect(result.totalLines).toBe(3);
    });

    it('should apply minification by default', async () => {
      const testContent = 'function test() {\n  return true;\n}';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.js',
        fullContent: true,
      });

      expect(result.status).toBeUndefined();
      // Minification is always applied for token efficiency
    });
  });

  describe('Match string fetch', () => {
    it('should fetch lines matching pattern with context', async () => {
      const testContent = 'line 1\nline 2\nMATCH\nline 4\nline 5';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        matchStringContextLines: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('line 2');
      expect(result.content).toContain('MATCH');
      expect(result.content).toContain('line 4');
      expect(result.isPartial).toBe(true);
    });

    // Contract (src/scheme/verbosity.ts): basic/omitted = verbatim. The
    // matchString slice path must NOT minify before the verbosity finalizer;
    // minification is concise-only (applyFetchContentVerbosity).
    it('does NOT minify the matchString slice in basic verbosity', async () => {
      const testContent =
        'before\nconst x = 1; // keep this comment\nTARGET\nafter';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.js',
        matchString: 'TARGET',
        matchStringContextLines: 1,
      });

      expect(result.status).toBeUndefined();
      // Verbatim slice — the line comment survives (would be stripped if minified).
      expect(result.content).toBe(
        'const x = 1; // keep this comment\nTARGET\nafter'
      );
    });

    it('DOES minify the matchString slice under concise (finalizer owns it)', async () => {
      const testContent =
        'before\nconst x = 1; // keep this comment\nTARGET\nafter';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.js',
        matchString: 'TARGET',
        matchStringContextLines: 1,
        verbosity: 'concise',
      });

      expect(result.status).toBeUndefined();
      // Concise minifies — the line comment is stripped.
      expect(result.content).not.toContain('// keep this comment');
    });

    it('should return empty when pattern not found', async () => {
      const testContent = 'line 1\nline 2\nline 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'NOTFOUND',
      });

      expect(result.status).toBe('empty');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.NO_MATCHES);
    });

    it('signals an inverted range (startLine > endLine) instead of silent empty (E1)', async () => {
      const testContent = 'a\nb\nc\nd\ne\nf\ng\nh';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 6,
        endLine: 2,
      });

      expect(result.status).toBe('empty');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.NO_MATCHES);
      expect(
        result.hints?.some(h =>
          /startLine .*greater than endLine|startLine must be ≤ endLine/i.test(
            h
          )
        )
      ).toBe(true);
    });

    it('should show regex-specific hint when matchStringIsRegex and no matches', async () => {
      const testContent = 'line 1\nline 2\nline 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'VERY_SPECIFIC_REGEX_PATTERN',
        matchStringIsRegex: true,
      });

      expect(result.status).toBe('empty');
      expect(result.hints).toBeDefined();
      expect(result.hints?.some(h => h.includes('per-line'))).toBe(true);
    });

    it('should show case-sensitive hint when enabled and no matches', async () => {
      const testContent = 'LINE 1\nLINE 2\nLINE 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'line',
        matchStringCaseSensitive: true,
      });

      expect(result.status).toBe('empty');
      expect(result.hints).toBeDefined();
      expect(result.hints?.some(h => h.includes('caseSensitive=true'))).toBe(
        true
      );
    });

    it('should match using regex when matchStringIsRegex is true', async () => {
      const testContent = 'line 1\nexport function test() {}\nline 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'export.*function',
        matchStringIsRegex: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('export function');
    });

    // Issue verification: export.*const patterns
    describe('regex .* pattern behavior', () => {
      it('should match export.*const when content has "export const" on same line', async () => {
        const testContent = 'line 1\nexport const foo = 1;\nline 3';
        mockReadFile.mockResolvedValue(testContent);

        const result = await fetchContent({
          path: 'test.ts',
          matchString: 'export.*const',
          matchStringIsRegex: true,
        });

        expect(result.status).toBeUndefined();
        expect(result.content).toContain('export const');
      });

      it('should NOT match export.*const when file has export function only', async () => {
        // This is the scenario from the issue - file has export function but not export const
        const testContent =
          'line 1\nexport function test() {}\nexport async function foo() {}\nline 4';
        mockReadFile.mockResolvedValue(testContent);

        const result = await fetchContent({
          path: 'test.ts',
          matchString: 'export.*const',
          matchStringIsRegex: true,
        });

        // Expected: no matches because file doesn't contain "export const"
        expect(result.status).toBe('empty');
        expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.NO_MATCHES);
      });

      it('should match patterns line-by-line (not multiline)', async () => {
        // Pattern won't match across lines even if keywords exist
        const testContent = 'export\nconst foo = 1;';
        mockReadFile.mockResolvedValue(testContent);

        const result = await fetchContent({
          path: 'test.ts',
          matchString: 'export.*const',
          matchStringIsRegex: true,
        });

        // export and const are on different lines - regex matches per line
        expect(result.status).toBe('empty');
      });

      it('should work with greedy .* when content exists on same line', async () => {
        const testContent = 'export type MyType = { const: string };';
        mockReadFile.mockResolvedValue(testContent);

        const result = await fetchContent({
          path: 'test.ts',
          matchString: 'export.*const',
          matchStringIsRegex: true,
        });

        // "export" ... "const" on same line matches
        expect(result.status).toBeUndefined();
      });
    });

    it('should match case-sensitively when matchStringCaseSensitive is true', async () => {
      const testContent = 'MATCH\nmatch\nMatch';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        matchStringCaseSensitive: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('MATCH');
    });

    it('should throw error for invalid regex pattern', async () => {
      const testContent = 'line 1\nline 2\nline 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: '[invalid(regex',
        matchStringIsRegex: true,
      });

      expect(result.status).toBe('error');
    });

    it('should merge adjacent ranges and show omitted lines', async () => {
      // Create content with widely spaced matches
      const lines = [];
      for (let i = 0; i < 100; i++) {
        if (i === 10 || i === 50 || i === 90) {
          lines.push('MATCH_LINE');
        } else {
          lines.push(`line ${i}`);
        }
      }
      const testContent = lines.join('\n');
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH_LINE',
        matchStringContextLines: 2,
      });

      expect(result.status).toBeUndefined();
      // Should contain omitted lines indicator
      expect(result.content).toContain('lines omitted');
    });
  });

  describe('Conflicting extraction options', () => {
    it('should return error when fullContent and matchString are both provided', async () => {
      const testContent = 'line 1\nline 2\nMATCH\nline 4\nline 5';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        fullContent: true,
        matchString: 'MATCH',
      });

      expect(result.status).toBe('error');
      expect(result.error).toBeDefined();
      expect(String(result.error)).toContain('fullContent');
      expect(String(result.error)).toContain('matchString');
      expect(result.hints).toBeDefined();
      expect(result.hints!.length).toBeGreaterThan(0);
    });

    it('should return error when fullContent and matchString are both provided even with other options', async () => {
      const testContent = 'line 1\nline 2\nMATCH\nline 4\nline 5';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        fullContent: true,
        matchString: 'MATCH',
        matchStringContextLines: 3,
      });

      expect(result.status).toBe('error');
      expect(String(result.error)).toContain('fullContent');
      expect(String(result.error)).toContain('matchString');
    });

    it('should return error when fullContent and line range are both provided', async () => {
      const result = await fetchContent({
        path: 'test.txt',
        fullContent: true,
        startLine: 1,
        endLine: 3,
      });

      expect(result.status).toBe('error');
      expect(String(result.error)).toContain('fullContent');
      expect(String(result.error)).toContain('startLine/endLine');
    });

    it('should return error when matchString and line range are both provided', async () => {
      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        startLine: 1,
        endLine: 3,
      });

      expect(result.status).toBe('error');
      expect(String(result.error)).toContain('matchString');
      expect(String(result.error)).toContain('startLine/endLine');
    });
  });

  describe('Error handling', () => {
    it('should handle invalid paths', async () => {
      mockValidate.mockReturnValue({
        isValid: false,
        error: 'Invalid path',
      });

      const result = await fetchContent({
        path: '/invalid/path',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED
      );
    });

    it('should handle file read errors', async () => {
      mockReadFile.mockRejectedValue(new Error('File not found'));

      const result = await fetchContent({
        path: 'nonexistent.txt',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED);
    });

    it('should handle file read errors when error is not Error instance', async () => {
      mockReadFile.mockRejectedValue('String error message');

      const result = await fetchContent({
        path: 'broken.txt',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED);
    });

    it('should handle stat errors (file access failed)', async () => {
      mockStat.mockRejectedValue(new Error('Cannot access file'));

      const result = await fetchContent({
        path: 'inaccessible.txt',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED);
    });

    it('should handle stat errors with non-Error objects', async () => {
      mockStat.mockRejectedValue('String error');

      const result = await fetchContent({
        path: 'inaccessible.txt',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED);
    });

    it('should handle unexpected errors in main try-catch', async () => {
      // Force an error by mocking validate to throw
      mockValidate.mockImplementation(() => {
        throw new Error('Unexpected validation error');
      });

      const result = await fetchContent({
        path: 'test.txt',
      });

      expect(result.status).toBe('error');
    });

    it('should reject likely binary files before utf-8 decoding', async () => {
      const read = vi.fn(async (buffer: Buffer) => {
        buffer[0] = 0;
        return { bytesRead: 1, buffer };
      });
      const close = vi.fn().mockResolvedValue(undefined);
      mockOpen.mockResolvedValueOnce({
        read,
        close,
      } as unknown as Awaited<ReturnType<typeof fs.open>>);

      const result = await fetchContent({
        path: 'artifact.bin',
        fullContent: true,
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.BINARY_FILE_UNSUPPORTED
      );
      expect(mockReadFile).not.toHaveBeenCalled();
      expect(close).toHaveBeenCalled();
      expect(result.hints?.some(h => h.includes('Binary'))).toBe(true);
    });
  });

  describe('Empty content handling', () => {
    it('should handle empty files', async () => {
      mockReadFile.mockResolvedValue('');

      const result = await fetchContent({
        path: 'empty.txt',
        fullContent: true,
      });

      expect(result.status).toBe('empty');
    });

    it('should return empty when content is whitespace-only after extraction (lines 275-282)', async () => {
      mockReadFile.mockResolvedValue('   \n  \n   ');

      const result = await fetchContent({
        path: 'whitespace.txt',
        fullContent: true,
      });

      expect(result.status).toBe('empty');
      expect(result.hints).toBeDefined();
    });
  });

  describe('Large file handling', () => {
    it('should warn about large file without pagination options', async () => {
      // Mock large file (150KB)
      mockStat.mockResolvedValue({ size: 150 * 1024 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);

      const result = await fetchContent({
        path: 'large-file.txt',
        // No charLength or matchString
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_TOO_LARGE);
      expect(result.hints).toBeDefined();
      expect(result.hints!.length).toBeGreaterThan(0);
    });

    it('should auto-paginate content exceeding MAX_OUTPUT_CHARS', async () => {
      // Create content larger than MAX_OUTPUT_CHARS (10000)
      const largeContent = 'x'.repeat(15000);
      mockStat.mockResolvedValue({ size: 15000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'medium-file.txt',
        // No charLength - should auto-paginate instead of error
      });

      // Now auto-paginates instead of returning error
      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.warnings).toBeDefined();
      expect(result.warnings?.some(w => w.includes('Auto-paginated'))).toBe(
        true
      );
    });

    it('should allow large file with charLength pagination', async () => {
      mockStat.mockResolvedValue({ size: 150 * 1024 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue('test content for large file');

      const result = await fetchContent({
        path: 'large-file.txt',
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
    });

    it('should allow large file with matchString extraction', async () => {
      mockStat.mockResolvedValue({ size: 150 * 1024 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue('line 1\nMATCH\nline 3');

      const result = await fetchContent({
        path: 'large-file.txt',
        matchString: 'MATCH',
      });

      expect(result.status).toBeUndefined();
    });

    it('should allow large file with fullContent flag and charLength', async () => {
      mockStat.mockResolvedValue({ size: 150 * 1024 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue('full content of large file');

      const result = await fetchContent({
        path: 'large-file.txt',
        fullContent: true,
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
    });

    it('should not warn for files under 100KB', async () => {
      mockStat.mockResolvedValue({ size: 50 * 1024 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue('content of small file');

      const result = await fetchContent({
        path: 'small-file.txt',
        // No pagination options
      });

      expect(result.status).toBeUndefined();
    });
  });

  describe('Research context', () => {
    it('should not echo research goal and reasoning', async () => {
      const testContent = 'test content';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        fullContent: true,
        researchGoal: 'Find implementation',
        reasoning: 'Testing feature X',
      });

      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });
  });

  describe('Character-based pagination (charOffset + charLength)', () => {
    it('should fetch content with charOffset and charLength', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
        charOffset: 0,
        charLength: 5000,
      });

      expect(result.status).toBeUndefined();
      expect(result.content?.length).toBeLessThanOrEqual(5000);
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.isPartial).toBe(true);
    });

    it('should return first chunk when charOffset = 0', async () => {
      const content = 'abcdefghijklmnopqrstuvwxyz';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 0,
        charLength: 10,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('abcdefghij');
      expect(result.pagination?.charOffset).toBe(0);
    });

    it('should return second chunk with charOffset', async () => {
      const content = 'abcdefghijklmnopqrstuvwxyz';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 10,
        charLength: 10,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('klmnopqrst');
      expect(result.pagination?.charOffset).toBe(10);
    });

    it('should return last chunk correctly', async () => {
      const content = 'x'.repeat(1000);
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 900,
        charLength: 200,
      });

      expect(result.status).toBeUndefined();
      expect(result.content?.length).toBe(100); // Only 100 chars left
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should handle charOffset = 0 explicitly', async () => {
      const content = 'test content';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 0,
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.charOffset).toBe(0);
    });

    it('should handle charOffset at exact file length', async () => {
      const content = 'x'.repeat(100);
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 100,
        charLength: 50,
      });

      // charOffset >= content length → explicit error with actionable hints
      expect(result.status).toBe('error');
      expect(result.error).toContain(
        'charOffset 100 exceeds file content length'
      );
      expect(result.hints).toBeDefined();
    });

    it('should handle charOffset beyond file length', async () => {
      const content = 'short text';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 1000,
        charLength: 100,
      });

      // charOffset >> content length → explicit error with actionable hints
      expect(result.status).toBe('error');
      expect(result.error).toContain(
        'charOffset 1000 exceeds file content length'
      );
      expect(result.hints).toBeDefined();
    });

    it('should handle charLength = 1 (single char)', async () => {
      const content = 'abcdefghij';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charLength: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('a');
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should handle charLength = 10000 (max)', async () => {
      const content = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
      expect(result.content?.length).toBe(10000);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should handle charLength > remaining content', async () => {
      const content = 'short text';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('short text');
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should handle empty file with pagination params', async () => {
      mockReadFile.mockResolvedValue('');

      const result = await fetchContent({
        path: 'empty.txt',
        charOffset: 0,
        charLength: 100,
      });

      expect(result.status).toBe('empty');
    });
  });

  describe('UTF-8 multi-byte character handling', () => {
    it('should handle ASCII content pagination', async () => {
      const content = 'Hello World!\nThis is ASCII text.';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 0,
        charLength: 10,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('Hello Worl');
    });

    it('should handle 2-byte UTF-8 chars (accented letters)', async () => {
      const content = 'Café résumé piñata';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 0,
        charLength: 10,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      // Should not have replacement character
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should handle 3-byte UTF-8 chars (CJK characters)', async () => {
      const content = '你好世界 Hello 中文';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 0,
        charLength: 10,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      // Should not split UTF-8 characters
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should handle 4-byte UTF-8 chars (emoji)', async () => {
      const content = '😀 Hello 🎉 World 👍';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 0,
        charLength: 10,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      // Should not split emoji
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should handle mixed multi-byte content', async () => {
      const content = 'Hello 世界 café 😀 test';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 0,
        charLength: 15,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should not split multi-byte chars at charOffset boundary', async () => {
      // Position boundary right before a multi-byte char
      const content = 'aaaa' + 'é' + 'bbbb';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 4,
        charLength: 5,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      // Should include the 'é' without splitting
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should not split multi-byte chars at charLength boundary', async () => {
      // Create content where charLength boundary falls in middle of UTF-8 char
      const content = 'a'.repeat(95) + 'café';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 0,
        charLength: 98, // Might cut through the 'é'
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      // Should not have replacement character indicating split
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should calculate byte offsets correctly for UTF-8', async () => {
      // In UTF-8, byte offset != character offset for multi-byte chars
      const content = '中文test'; // 中文 = 6 bytes, test = 4 bytes
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        charOffset: 2, // After "中文"
        charLength: 4,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('test');
    });
  });

  describe('Integration with matchString', () => {
    it('should combine matchString with charOffset/charLength', async () => {
      const content = 'line1\nMATCH1\nline2\nMATCH2\nline3\nMATCH3\nline4';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        charOffset: 0,
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('MATCH');
    });

    it('should paginate matched sections', async () => {
      const content = 'MATCH\n' + 'x'.repeat(10000) + '\nMATCH';
      mockStat.mockResolvedValue({ size: 10020 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        matchStringContextLines: 5,
        charLength: 5000,
      });

      expect(result.status).toBeUndefined();
    });

    it('should auto-paginate (lossless cursor), not truncate, when matches are excessive without pagination', async () => {
      const manyLines = Array.from({ length: 2000 }, () => 'MATCH').join('\n');
      mockStat.mockResolvedValue({
        size: manyLines.length,
      } as unknown as Awaited<ReturnType<typeof fs.stat>>);
      mockReadFile.mockResolvedValue(manyLines);

      const result = await fetchContent({
        path: 'huge.txt',
        matchString: 'MATCH',
        // No charLength specified -> auto-paginates with a cursor (no match cap)
      });

      expect(result.status).toBeUndefined();
      expect(result.isPartial).toBe(true);
      // Lossless: a pagination cursor reaches the rest — nothing is dropped.
      expect(result.pagination).toBeDefined();
      expect(result.warnings).toBeDefined();
      expect(result.warnings?.[0]).toContain('2000');
      expect(result.warnings?.[0]).toContain('Auto-paginated');
      // The old hard "first 50 matches" cap must be gone.
      expect(result.warnings?.[0]).not.toContain('Truncated to first 50');
    });

    it('should auto-paginate when matchString result exceeds MAX_OUTPUT_CHARS without charLength (lines 206-212)', async () => {
      // Build enough extracted context to exceed the shared 8000-char budget.
      const lineContent = 'x'.repeat(500);
      const lines = Array.from({ length: 50 }, (_, i) =>
        i % 2 === 0 ? lineContent : 'MATCH'
      );
      const testContent = lines.join('\n');
      mockStat.mockResolvedValue({
        size: testContent.length,
      } as unknown as Awaited<ReturnType<typeof fs.stat>>);
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'large-matches.txt',
        matchString: 'MATCH',
        matchStringContextLines: 2,
        // No charLength - triggers auto-pagination when content > 8000
      });

      expect(result.status).toBeUndefined();
      expect(result.isPartial).toBe(true);
      expect(result.pagination).toBeDefined();
      expect(result.warnings).toBeDefined();
      expect(result.warnings?.some(w => w.includes('Auto-paginated'))).toBe(
        true
      );
    });

    it('should return line numbers for matchString extraction', async () => {
      const testContent = 'line 1\nline 2\nMATCH\nline 4\nline 5';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        matchStringContextLines: 1,
      });

      expect(result.status).toBeUndefined();
      // MATCH is on line 3, with contextLines=1, should return lines 2-4
      expect(result.startLine).toBe(2);
      expect(result.endLine).toBe(4);
      expect(result.matchRanges).toEqual([{ start: 2, end: 4 }]);
    });

    it('should return line numbers for multiple match ranges', async () => {
      // Lines: 1=line1, 2=MATCH_A, 3=line3, 4=line4, 5=line5, 6=line6, 7=MATCH_B, 8=line8
      const testContent =
        'line1\nMATCH_A\nline3\nline4\nline5\nline6\nMATCH_B\nline8';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        matchStringContextLines: 1,
      });

      expect(result.status).toBeUndefined();
      // First match at line 2 with context 1: range [1,3]
      // Second match at line 7 with context 1: range [6,8]
      expect(result.startLine).toBe(1); // First range start
      expect(result.endLine).toBe(8); // Last range end
      expect(result.matchRanges).toEqual([
        { start: 1, end: 3 },
        { start: 6, end: 8 },
      ]);
    });
  });

  describe('Pagination hints', () => {
    it('should show pagination hints when content is partial', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
        charLength: 5000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.hints).toBeDefined();
    });

    it('should show charOffset for next chunk in hints', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
        charOffset: 0,
        charLength: 5000,
      });

      expect(result.status).toBeUndefined();
      if (result.pagination?.hasMore) {
        expect(result.hints).toBeDefined();
        const hasCharOffsetHint = result.hints?.some(
          h => h.includes('charOffset') && h.includes('5000')
        );
        expect(hasCharOffsetHint).toBe(true);
      }
    });

    it('should show pagination info in hints when content is paginated', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
        charLength: 5000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalChars).toBe(20000);
      expect(result.pagination?.hasMore).toBe(true);
      if (result.hints) {
        // Lean hint format: "Page X/Y. Next: charOffset=Z"
        const hasPaginationHint = result.hints.some(
          h => h.includes('charOffset=') || h.includes('Page ')
        );
        expect(hasPaginationHint).toBe(true);
      }
    });

    it('should not show pagination hints when content fits', async () => {
      const smallContent = 'Small file content';
      mockReadFile.mockResolvedValue(smallContent);

      const result = await fetchContent({
        path: 'small.txt',
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should show helpful hints for navigating pages', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
        charLength: 5000,
        charOffset: 5000,
      });

      expect(result.status).toBeUndefined();
      if (result.pagination?.hasMore) {
        expect(result.hints).toBeDefined();
        // Should mention how to get next page
        const hasNavigationHint = result.hints?.some(
          h =>
            h.toLowerCase().includes('next') || h.includes('charOffset=10000')
        );
        expect(hasNavigationHint).toBe(true);
      }
    });
  });

  describe('Line range extraction (startLine/endLine)', () => {
    it('should extract lines by range', async () => {
      const testContent = 'line 1\nline 2\nline 3\nline 4\nline 5';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 2,
        endLine: 4,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('line 2\nline 3\nline 4');
      expect(result.isPartial).toBe(true);
      expect(result.totalLines).toBe(5);
      expect(result.startLine).toBe(2);
      expect(result.endLine).toBe(4);
    });

    it('should extract first line when startLine=1', async () => {
      const testContent = 'first\nsecond\nthird';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 1,
        endLine: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('first');
      expect(result.startLine).toBe(1);
      expect(result.endLine).toBe(1);
    });

    it('should handle endLine beyond file length', async () => {
      const testContent = 'line 1\nline 2\nline 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 2,
        endLine: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('line 2\nline 3');
      expect(result.endLine).toBe(3); // Adjusted to file end
      expect(result.warnings).toContain(
        'Requested endLine 100 adjusted to 3 (file end)'
      );
    });

    it('should return empty when startLine exceeds file length', async () => {
      const testContent = 'line 1\nline 2\nline 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 10,
        endLine: 20,
      });

      expect(result.status).toBe('empty');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.NO_MATCHES);
      expect(result.hints).toBeDefined();
      expect(result.hints?.some(h => h.includes('exceeds file length'))).toBe(
        true
      );
    });

    it('should emit a line-range continuation hint when the read stops before EOF', async () => {
      const lines = [];
      for (let i = 1; i <= 100; i++) {
        lines.push(`line ${i}`);
      }
      mockReadFile.mockResolvedValue(lines.join('\n'));

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 1,
        endLine: 40,
      });

      expect(result.status).toBeUndefined();
      expect(result.isPartial).toBe(true);
      expect(
        result.hints?.some(h =>
          h.includes(
            'More content: use startLine=41 to continue (60 lines remaining)'
          )
        )
      ).toBe(true);
    });

    it('preserves the line-range continuation hint under concise verbosity', async () => {
      const lines = [];
      for (let i = 1; i <= 100; i++) {
        lines.push(`const x${i} = ${i};`);
      }
      mockReadFile.mockResolvedValue(lines.join('\n'));

      const result = await fetchContent({
        path: 'test.ts',
        startLine: 1,
        endLine: 40,
        verbosity: 'concise',
      });

      // Concise rewrites hints to a tiny summary, but pagination is orthogonal
      // to verbosity — the continuation cursor must survive.
      expect(
        result.hints?.some(h =>
          h.includes('More content: use startLine=41 to continue')
        )
      ).toBe(true);
    });

    it('should NOT emit a continuation hint when the range reaches EOF', async () => {
      const testContent = 'line 1\nline 2\nline 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 1,
        endLine: 3,
      });

      expect(result.hints?.some(h => h.includes('More content'))).toBeFalsy();
    });

    it('should apply minification to extracted lines', async () => {
      const testContent =
        'const x = 1;\nfunction test() {\n  return true;\n}\nconst y = 2;';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.js',
        startLine: 2,
        endLine: 4,
      });

      expect(result.status).toBeUndefined();
      // Minification is always applied for token efficiency
    });

    it('should work with character pagination on extracted lines', async () => {
      const lines = [];
      for (let i = 1; i <= 100; i++) {
        lines.push(`line ${i}: ${'x'.repeat(50)}`);
      }
      const testContent = lines.join('\n');
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 10,
        endLine: 50,
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      expect(result.startLine).toBe(10);
      expect(result.endLine).toBe(50);
      expect(result.pagination).toBeDefined();
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should handle single-line file', async () => {
      const testContent = 'single line content';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 1,
        endLine: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('single line content');
      expect(result.totalLines).toBe(1);
    });

    it('should handle empty lines in range', async () => {
      const testContent = 'line 1\n\nline 3\n\nline 5';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 1,
        endLine: 5,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe(testContent);
      expect(result.totalLines).toBe(5);
    });

    it('should not echo research context in response', async () => {
      const testContent = 'line 1\nline 2\nline 3';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 1,
        endLine: 2,
        researchGoal: 'Extract header',
        reasoning: 'Checking file header',
      });

      expect(result.status).toBeUndefined();
      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });
  });

  describe('byte/character offset separation', () => {
    it('should return both byte and char offsets in pagination', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
        charLength: 5000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();

      // Should have char fields
      expect(result.pagination?.charOffset).toBeDefined();
      expect(result.pagination?.charLength).toBeDefined();
      expect(result.pagination?.totalChars).toBeDefined();
    });

    it('should handle UTF-8 content with correct byte/char separation', async () => {
      // Content with emojis: each emoji is 4 bytes, 2 JS chars
      const emojiContent = '👋'.repeat(5000) + 'x'.repeat(5000);
      // 5000 emojis * 4 bytes + 5000 'x' = 25000 bytes
      // 5000 emojis * 2 chars + 5000 'x' = 15000 chars
      mockStat.mockResolvedValue({ size: 25000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(emojiContent);

      const result = await fetchContent({
        path: 'emoji.txt',
        charLength: 5000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();

      expect(result.pagination?.totalChars).toBe(15000);

      // Local tools use character mode, so charLength should be 5000
      expect(result.pagination?.charLength).toBe(5000);
    });

    it('should use character offsets for navigation hints in local tools', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
        charLength: 5000,
      });

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();

      // Local tools should use charOffset in hints
      const hasCharOffsetHint = result.hints?.some(h =>
        h.includes('charOffset=5000')
      );
      expect(hasCharOffsetHint).toBe(true);
    });

    it('should allow sequential pagination with nextCharOffset', async () => {
      const content = '👋🚀🌍💻'.repeat(1000); // 8000 chars, 16000 bytes
      mockStat.mockResolvedValue({ size: 16000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(content);

      // First page
      const page1 = await fetchContent({
        path: 'emoji.txt',
        charLength: 2000,
        charOffset: 0,
      });

      expect(page1.status).toBeUndefined();
      expect(page1.pagination?.hasMore).toBe(true);
      expect(page1.pagination?.charOffset).toBe(0);
      expect(page1.pagination?.charLength).toBe(2000);

      // Second page using character offset from hints
      // (nextCharOffset would be 2000 for chars mode)
      const page2 = await fetchContent({
        path: 'emoji.txt',
        charLength: 2000,
        charOffset: 2000,
      });

      expect(page2.status).toBeUndefined();
      expect(page2.pagination?.charOffset).toBe(2000);
    });

    it('should correctly handle CJK content pagination', async () => {
      // CJK content: each char is 3 bytes
      const cjkContent = '你好世界'.repeat(1000); // 4000 chars, 12000 bytes
      mockStat.mockResolvedValue({ size: 12000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(cjkContent);

      const result = await fetchContent({
        path: 'cjk.txt',
        charLength: 1000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalChars).toBe(4000);
      expect(result.pagination?.charLength).toBe(1000);
    });
  });

  describe('Smart auto-pagination (no charLength specified)', () => {
    it('should auto-paginate when extracted content exceeds MAX_OUTPUT_CHARS', async () => {
      // Create content larger than MAX_OUTPUT_CHARS (10000)
      const largeContent = 'x'.repeat(15000);
      mockStat.mockResolvedValue({ size: 15000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large-file.txt',
        // NO charLength specified - should auto-paginate instead of error
      });

      // Should NOT return error, should auto-paginate
      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.pagination?.totalPages).toBeGreaterThan(1);
      expect(result.warnings).toBeDefined();
      expect(
        result.warnings?.some(w => w.toLowerCase().includes('auto-paginated'))
      ).toBe(true);
    });

    it('should auto-paginate line extraction when extracted lines exceed limit', async () => {
      // Create content with many lines that exceeds limit
      const lines = Array.from(
        { length: 500 },
        (_, i) => `line ${i}: ${'x'.repeat(50)}`
      );
      const largeContent = lines.join('\n');
      mockStat.mockResolvedValue({
        size: largeContent.length,
      } as unknown as Awaited<ReturnType<typeof fs.stat>>);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 1,
        endLine: 500,
        // NO charLength - should auto-paginate extracted lines
      });

      expect(result.status).toBeUndefined();
      expect(result.startLine).toBe(1);
      expect(result.endLine).toBe(500);
      expect(result.pagination).toBeDefined();
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should include context-aware hints for navigation', async () => {
      const largeContent = 'x'.repeat(15000);
      mockStat.mockResolvedValue({ size: 15000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
      // Should have navigation hints
      expect(
        result.hints?.some(h => h.includes('charOffset') || h.includes('Next'))
      ).toBe(true);
    });

    it('should not auto-paginate when content is under limit', async () => {
      const smallContent = 'small content under 10K';
      mockReadFile.mockResolvedValue(smallContent);

      const result = await fetchContent({
        path: 'small.txt',
      });

      expect(result.status).toBeUndefined();
      // Should NOT have pagination when content is small
      expect(result.pagination).toBeUndefined();
      expect(result.warnings).toBeUndefined();
    });

    it('should show total chars/lines in auto-paginated response', async () => {
      const lines = Array.from({ length: 200 }, (_, i) => `line ${i + 1}`);
      const content = lines.join('\n');
      mockStat.mockResolvedValue({ size: content.length } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(content);

      // Content is ~1600 chars, under 10K - but let's test with larger
      const largeLines = Array.from(
        { length: 500 },
        (_, i) => `line ${i}: ${'data'.repeat(20)}`
      );
      const largeContent = largeLines.join('\n');
      mockStat.mockResolvedValue({
        size: largeContent.length,
      } as unknown as Awaited<ReturnType<typeof fs.stat>>);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.totalLines).toBe(500);
      expect(result.pagination?.totalChars).toBeDefined();
    });
  });
});
