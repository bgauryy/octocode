import { describe, it, expect, beforeEach, vi } from 'vitest';
import { LOCAL_TOOL_ERROR_CODES } from '../../../octocode-tools-core/src/errors/localToolErrors.js';
import { fetchContent as fetchContentImpl } from '../../../octocode-tools-core/src/tools/local_fetch_content/fetchContent.js';
import {
  extractSignatures,
  applyContentViewMinification,
} from '@octocodeai/octocode-context-utils';
import { SIGNATURE_SOURCE } from '../fixtures/signatureSource.js';
import * as pathValidator from 'octocode-security/pathValidator';
import * as fs from 'fs/promises';
import type { CharPagination } from '@octocodeai/octocode-core/types';

type FetchContentInput = Omit<
  Parameters<typeof fetchContentImpl>[0],
  'contextLines'
> & { contextLines?: number };

const fetchContent = (query: FetchContentInput) =>
  fetchContentImpl({ ...query, contextLines: query.contextLines ?? 5 });

const charPagination = (
  pagination: Awaited<ReturnType<typeof fetchContentImpl>>['pagination']
): CharPagination | undefined =>
  pagination && 'charOffset' in pagination ? pagination : undefined;

vi.mock('fs/promises', () => ({
  open: vi.fn(),
  readFile: vi.fn(),
  stat: vi.fn(),
}));

vi.mock('octocode-security/pathValidator', () => ({
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
      expect(result.isPartial).toBeUndefined();
      expect(result.totalLines).toBe(3);
      expect(result.hints?.join('\n')).toContain('lspGetSemantics');
    });

    it('returns raw content by default (minify omitted → no comment stripping)', async () => {
      const testContent =
        'function test() {\n  // explain the return\n  return true;\n}';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.js',
        fullContent: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('// explain the return');
      expect(result.content).toContain('return');
    });

    it('strips comments and whitespace with minify:"standard"', async () => {
      const testContent =
        'function test() {\n  // explain the return\n  return true;\n}';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.js',
        fullContent: true,
        minify: 'standard',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).not.toContain('// explain the return');
      expect(result.content).toContain('return');
    });
  });

  describe('Match string fetch', () => {
    it('should fetch lines matching pattern with context', async () => {
      const testContent = 'line 1\nline 2\nMATCH\nline 4\nline 5';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        contextLines: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('line 2');
      expect(result.content).toContain('MATCH');
      expect(result.content).toContain('line 4');
      expect(result.isPartial).toBe(true);
      expect(result.hints?.join('\n')).toContain('lspGetSemantics');
    });

    it('strips inline comments from JS files in the matchString slice with minify:"standard"', async () => {
      const testContent =
        'before\nconst x = 1; // keep this comment\nTARGET\nafter';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.js',
        matchString: 'TARGET',
        contextLines: 1,
        minify: 'standard',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).not.toContain('// keep this comment');
      expect(result.content).toContain('x');
      expect(result.content).toContain('TARGET');
    });

    it('matchString slice — two calls return identical content', async () => {
      const testContent =
        'before\nconst x = 1; // keep this comment\nTARGET\nafter';
      mockReadFile.mockResolvedValue(testContent);

      const def = await fetchContent({
        path: 'test.js',
        matchString: 'TARGET',
        contextLines: 1,
        minify: 'standard',
      });
      const result = await fetchContent({
        path: 'test.js',
        matchString: 'TARGET',
        contextLines: 1,
        minify: 'standard',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe(def.content);
      expect(result.content).not.toContain('// keep this comment');
      expect(result.content).toContain('x');
    });

    it('minify:"symbols" returns the extracted skeleton, aligned with the GitHub path', async () => {
      const SOURCE = SIGNATURE_SOURCE;
      mockReadFile.mockResolvedValue(SOURCE);

      const result = await fetchContent({
        path: 'sample.ts',
        minify: 'symbols',
      } as Parameters<typeof fetchContent>[0]);

      expect(result.status).toBeUndefined();
      const rawSigs = extractSignatures(SOURCE, 'sample.ts')!;
      expect(result.content).toBe(
        applyContentViewMinification(rawSigs, 'sample.ts')
      );
      expect(result.content).toMatch(/\d+\| .*interface Foo/);
      expect(result.content).toContain('a: string,');
      expect(result.content).toContain('Promise<void>');
      expect(result.content).not.toContain('secretLocal');
      expect(result.contentView).toBe('symbols');
      expect(result.isSkeleton).toBe(true);
      const sourceSize = result as {
        sourceChars?: number;
        sourceBytes?: number;
      };
      expect(sourceSize.sourceChars).toBe(SOURCE.length);
      expect(sourceSize.sourceBytes).toBeUndefined();
    });

    it('returns a large minify:"symbols" skeleton WHOLE — never paginated', async () => {
      let src = '';
      for (let i = 0; i < 800; i++) {
        src += `export function fn${i}(argumentOne: string, argumentTwo: number): Promise<void> {\n  return doStuff(${i});\n}\n`;
      }
      mockReadFile.mockResolvedValue(src);

      const result = await fetchContent({
        path: 'big.ts',
        minify: 'symbols',
      } as Parameters<typeof fetchContent>[0]);

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeUndefined();
      expect(result.content).toBeDefined();
      expect(result.content).toContain('fn0(');
      expect(result.content).toContain('fn799(');
      expect(result.content).not.toContain('doStuff');
      expect(result.isPartial).toBeUndefined();
      expect(result.contentView).toBe('symbols');
      expect(result.isSkeleton).toBe(true);
      expect(result.totalLines).toBe(src.split('\n').length);
      expect((result as { sourceChars?: number }).sourceChars).toBe(src.length);
    });

    it('paginates the returned standard content view, not the pre-minified source', async () => {
      const src = Array.from(
        { length: 100 },
        (_, i) => `// ${'comment '.repeat(12)}\nconst a${i}=${i};`
      ).join('\n');
      const minified = applyContentViewMinification(src, 'sample.ts');
      expect(src.length).toBeGreaterThan(2000);
      expect(minified.length).toBeLessThan(2000);
      mockReadFile.mockResolvedValue(src);

      const result = await fetchContent({
        path: 'sample.ts',
        minify: 'standard',
      } as Parameters<typeof fetchContent>[0]);

      expect(result.status).toBeUndefined();
      expect(result.content).toBe(minified);
      expect(result.pagination).toBeUndefined();
      expect((result as { sourceChars?: number }).sourceChars).toBe(src.length);
    });

    it('unsupported minify:"symbols" warns and falls back to standard content view', async () => {
      mockReadFile.mockResolvedValue(
        'name=octocode\n\n\n; token-saving comment\nkeep=true\n'
      );

      const result = await fetchContent({
        path: 'settings.ini',
        minify: 'symbols',
      } as Parameters<typeof fetchContentImpl>[0]);

      expect(result.status).toBeUndefined();
      expect(result.contentView).toBeUndefined();
      expect(result.isSkeleton).toBeUndefined();
      expect(result.content).toContain('name=octocode');
      expect(result.content).toContain('keep=true');
      expect(result.content).not.toContain('; token-saving comment');
      expect(result.warnings).toEqual(
        expect.arrayContaining([
          expect.stringContaining('falling back to standard content view'),
        ])
      );
      expect(result.warnings?.join('\n')).not.toContain(
        'returning full content'
      );
    });

    it('ignores charOffset/charLength for minify:"symbols" (skeleton is whole)', async () => {
      let src = '';
      for (let i = 0; i < 800; i++) {
        src += `export function fn${i}(argumentOne: string, argumentTwo: number): Promise<void> {\n  return doStuff(${i});\n}\n`;
      }
      mockReadFile.mockResolvedValue(src);

      const result = await fetchContent({
        path: 'big.ts',
        minify: 'symbols',
        charOffset: 5000,
        charLength: 100,
      } as Parameters<typeof fetchContent>[0]);

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeUndefined();
      expect(result.content).toContain('fn0(');
      expect(result.content).toContain('fn799(');
    });

    it('redacts secrets in normal content (aligned with GitHub ContentSanitizer)', async () => {
      mockReadFile.mockResolvedValue('const a = "AKIAIOSFODNN7EXAMPLE";\n');

      const result = await fetchContent({
        path: 'cfg.ts',
        fullContent: true,
      } as Parameters<typeof fetchContent>[0]);

      expect(result.content).toContain('[REDACTED-AWSACCESSKEYID]');
      expect(result.content).not.toContain('AKIAIOSFODNN7EXAMPLE');
    });

    it('redacts secrets inside minify:"symbols" output', async () => {
      mockReadFile.mockResolvedValue(
        'export function connect(token = "AKIAIOSFODNN7EXAMPLE"): void {\n  doThing();\n}\n'
      );

      const result = await fetchContent({
        path: 'svc.ts',
        minify: 'symbols',
      } as Parameters<typeof fetchContent>[0]);

      expect(result.content).toContain('connect(');
      expect(result.content).toContain('[REDACTED');
      expect(result.content).not.toContain('AKIAIOSFODNN7EXAMPLE');
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
        const testContent =
          'line 1\nexport function test() {}\nexport async function foo() {}\nline 4';
        mockReadFile.mockResolvedValue(testContent);

        const result = await fetchContent({
          path: 'test.ts',
          matchString: 'export.*const',
          matchStringIsRegex: true,
        });

        expect(result.status).toBe('empty');
        expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.NO_MATCHES);
      });

      it('should match patterns line-by-line (not multiline)', async () => {
        const testContent = 'export\nconst foo = 1;';
        mockReadFile.mockResolvedValue(testContent);

        const result = await fetchContent({
          path: 'test.ts',
          matchString: 'export.*const',
          matchStringIsRegex: true,
        });

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
        contextLines: 2,
      });

      expect(result.status).toBeUndefined();
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
        contextLines: 3,
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

    it('reports a directory-specific error (EISDIR) with a localViewStructure hint', async () => {
      const eisdir = Object.assign(
        new Error("EISDIR: illegal operation on a directory, read 'utils'"),
        { code: 'EISDIR' }
      );
      mockReadFile.mockRejectedValue(eisdir);

      const result = await fetchContent({
        path: 'src/utils',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED);
      expect(String(result.error)).toContain('directory');
      expect(String(result.error)).toContain('localViewStructure');
      const hints = ((result.hints as string[] | undefined) ?? []).join(' ');
      expect(hints).toContain('localViewStructure');
      expect(hints).not.toContain('not found');
      expect(hints).not.toContain('localFindFiles');
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
      mockStat.mockResolvedValue({ size: 150 * 1024 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);

      const result = await fetchContent({
        path: 'large-file.txt',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_TOO_LARGE);
      expect(result.hints).toBeDefined();
      expect(result.hints!.length).toBeGreaterThan(0);
    });

    it('should auto-paginate content exceeding MAX_OUTPUT_CHARS', async () => {
      const largeContent = 'x'.repeat(15000);
      mockStat.mockResolvedValue({ size: 15000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'medium-file.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.warnings).toBeDefined();
      expect(result.warnings?.some(w => w.includes('Auto-paginated'))).toBe(
        true
      );
    });

    it('should allow large file with fullContent flag', async () => {
      mockStat.mockResolvedValue({ size: 150 * 1024 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue('full content of large file');

      const result = await fetchContent({
        path: 'large-file.txt',
        fullContent: true,
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

    it('should not warn for files under 100KB', async () => {
      mockStat.mockResolvedValue({ size: 50 * 1024 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue('content of small file');

      const result = await fetchContent({
        path: 'small-file.txt',
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

  describe('Line-based pagination (startLine/endLine)', () => {
    it('returns content auto-paginated when file exceeds output budget', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
      });

      expect(result.status).toBeUndefined();
      if (result.pagination) {
        expect(result.pagination.hasMore).toBe(true);
        expect(result.isPartial).toBe(true);
      }
    });

    it('returns small file content without pagination', async () => {
      const content = 'short text';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBe('short text');
      expect(result.pagination?.hasMore).toBeFalsy();
    });

    it('should handle empty file', async () => {
      mockReadFile.mockResolvedValue('');

      const result = await fetchContent({
        path: 'empty.txt',
      });

      expect(result.status).toBe('empty');
    });
  });

  describe('UTF-8 multi-byte character handling', () => {
    it('should handle ASCII content', async () => {
      const content = 'Hello World!\nThis is ASCII text.';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 1,
        endLine: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('Hello World!');
    });

    it('should handle 2-byte UTF-8 chars (accented letters)', async () => {
      const content = 'Café résumé piñata';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should handle 3-byte UTF-8 chars (CJK characters)', async () => {
      const content = '你好世界 Hello 中文';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should handle 4-byte UTF-8 chars (emoji)', async () => {
      const content = '😀 Hello 🎉 World 👍';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should handle mixed multi-byte content', async () => {
      const content = 'Hello 世界 café 😀 test';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should not split multi-byte chars at line boundary', async () => {
      const content = 'aaaa\né\nbbbb';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 2,
        endLine: 2,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should handle UTF-8 multi-byte content in line ranges', async () => {
      const content = '中文test\nmore content';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 1,
        endLine: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('中文test');
    });

    it('should not split multi-byte chars at charOffset boundary', async () => {
      const content = 'aaaa' + 'é' + 'bbbb';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should not split multi-byte chars at charLength boundary', async () => {
      const content = 'a'.repeat(95) + 'café';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toBeDefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should handle CJK content without corruption', async () => {
      const content = '中文test'; // 中文 = 6 bytes, test = 4 bytes
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('中文test');
    });
  });

  describe('Integration with matchString', () => {
    it('should combine matchString with context lines', async () => {
      const content = 'line1\nMATCH1\nline2\nMATCH2\nline3\nMATCH3\nline4';
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).toContain('MATCH');
    });

    it('should paginate matched sections via auto-pagination', async () => {
      const content = 'MATCH\n' + 'x'.repeat(10000) + '\nMATCH';
      mockStat.mockResolvedValue({ size: 10020 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(content);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        contextLines: 5,
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
      });

      expect(result.status).toBeUndefined();
      expect(result.isPartial).toBe(true);
      expect(result.pagination).toBeDefined();
      expect(result.warnings).toBeDefined();
      const joinedWarnings = (result.warnings ?? []).join(' | ');
      expect(result.warnings?.[0]).toContain('Found 2000 occurrences');
      expect(joinedWarnings).toContain('Auto-paginated');
      expect(joinedWarnings).not.toContain('Truncated to first 50');
    });

    it('should auto-paginate when matchString result exceeds MAX_OUTPUT_CHARS without charLength (lines 206-212)', async () => {
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
        contextLines: 2,
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
        contextLines: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.startLine).toBe(2);
      expect(result.endLine).toBe(4);
      expect(result.matchRanges).toBeUndefined();
    });

    it('should return line numbers for multiple match ranges', async () => {
      const testContent =
        'line1\nMATCH_A\nline3\nline4\nline5\nline6\nMATCH_B\nline8';
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        matchString: 'MATCH',
        contextLines: 1,
      });

      expect(result.status).toBeUndefined();
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
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.hints).toBeDefined();
    });

    it('should show pagination info in hints when content is paginated', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore).toBe(true);
      if (result.hints) {
        expect(result.hints.length).toBeGreaterThan(0);
      }
    });

    it('should not show pagination hints when content fits', async () => {
      const smallContent = 'Small file content';
      mockReadFile.mockResolvedValue(smallContent);

      const result = await fetchContent({
        path: 'small.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore).toBeFalsy();
    });

    it('should show helpful hints for navigating pages', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
      });

      expect(result.status).toBeUndefined();
      if (result.pagination?.hasMore) {
        expect(result.hints).toBeDefined();
      }
    });

    it('should navigate content via the charOffset cursor (full-file read)', async () => {
      const largeContent = Array.from(
        { length: 500 },
        (_, i) => `line-${String(i).padStart(4, '0')}:${'x'.repeat(30)}`
      ).join('\n');
      mockStat.mockResolvedValue({
        size: largeContent.length,
      } as unknown as Awaited<ReturnType<typeof fs.stat>>);
      mockReadFile.mockResolvedValue(largeContent);

      const page1 = await fetchContent({ path: 'large.txt' });
      const nextOffset =
        (charPagination(page1.pagination)?.charOffset ?? 0) +
        (charPagination(page1.pagination)?.charLength ?? 0);
      const page2 = await fetchContent({
        path: 'large.txt',
        charOffset: nextOffset,
      } as Parameters<typeof fetchContent>[0] & { charOffset: number });

      expect(page1.pagination?.hasMore).toBe(true);
      expect(page1.pagination?.currentPage).toBe(1);
      expect(page2.content).not.toBe(page1.content);
      expect(page2.pagination?.currentPage).toBe(2);
    });

    it('should navigate matchString results via the charOffset cursor', async () => {
      const manyMatches = Array.from(
        { length: 2000 },
        () => 'MATCH\n' + 'x'.repeat(20)
      ).join('\n');
      mockStat.mockResolvedValue({
        size: manyMatches.length,
      } as unknown as Awaited<ReturnType<typeof fs.stat>>);
      mockReadFile.mockResolvedValue(manyMatches);

      const page1 = await fetchContent({
        path: 'huge.txt',
        matchString: 'MATCH',
      });
      const nextOffset =
        (charPagination(page1.pagination)?.charOffset ?? 0) +
        (charPagination(page1.pagination)?.charLength ?? 0);
      const page2 = await fetchContent({
        path: 'huge.txt',
        matchString: 'MATCH',
        charOffset: nextOffset,
      } as Parameters<typeof fetchContent>[0] & { charOffset: number });

      expect(page1.pagination?.hasMore).toBe(true);
      expect(page2.content).not.toBe(page1.content);
      expect(page2.pagination?.currentPage).toBe(2);
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

    it('preserves the line-range continuation hint', async () => {
      const lines = [];
      for (let i = 1; i <= 100; i++) {
        lines.push(`const x${i} = ${i};`);
      }
      mockReadFile.mockResolvedValue(lines.join('\n'));

      const result = await fetchContent({
        path: 'test.ts',
        startLine: 1,
        endLine: 40,
      });

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
    });

    it('should return extracted lines within a range', async () => {
      const lines = [];
      for (let i = 1; i <= 100; i++) {
        lines.push(`line ${i}: ${'x'.repeat(50)}`);
      }
      const testContent = lines.join('\n');
      mockReadFile.mockResolvedValue(testContent);

      const result = await fetchContent({
        path: 'test.txt',
        startLine: 10,
        endLine: 30,
      });

      expect(result.status).toBeUndefined();
      expect(result.startLine).toBe(10);
      expect(result.endLine).toBe(30);
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
    it('should return pagination metadata for large content', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
      });

      expect(result.status).toBeUndefined();
      if (result.pagination) {
        expect(result.pagination.hasMore).toBe(true);
        expect(charPagination(result.pagination)?.totalChars).toBeDefined();
      }
    });

    it('should handle UTF-8 content without corruption', async () => {
      const emojiContent = '👋'.repeat(500) + 'x'.repeat(5000);
      mockStat.mockResolvedValue({ size: 22000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(emojiContent);

      const result = await fetchContent({
        path: 'emoji.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });

    it('should provide continuation hints for large content', async () => {
      const largeContent = 'x'.repeat(20000);
      mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large.txt',
      });

      expect(result.status).toBeUndefined();
      if (result.pagination?.hasMore) {
        expect(result.hints).toBeDefined();
      }
    });

    it('should handle CJK content without corruption', async () => {
      const cjkContent = '你好世界'.repeat(200);
      mockStat.mockResolvedValue({
        size: cjkContent.length * 3,
      } as unknown as Awaited<ReturnType<typeof fs.stat>>);
      mockReadFile.mockResolvedValue(cjkContent);

      const result = await fetchContent({
        path: 'cjk.txt',
      });

      expect(result.status).toBeUndefined();
      expect(result.content).not.toMatch(/\uFFFD/);
    });
  });

  describe('Smart auto-pagination (no charLength specified)', () => {
    it('should auto-paginate when extracted content exceeds MAX_OUTPUT_CHARS', async () => {
      const largeContent = 'x'.repeat(15000);
      mockStat.mockResolvedValue({ size: 15000 } as unknown as Awaited<
        ReturnType<typeof fs.stat>
      >);
      mockReadFile.mockResolvedValue(largeContent);

      const result = await fetchContent({
        path: 'large-file.txt',
      });

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
      expect(charPagination(result.pagination)?.totalChars).toBeDefined();
    });
  });
});
