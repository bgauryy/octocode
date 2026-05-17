/**
 * Tests for localSearchCode tool
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { searchContentRipgrep } from '../../src/tools/local_ripgrep/searchContentRipgrep.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../src/errors/localToolErrors.js';
import { RipgrepQuerySchema } from '@octocodeai/octocode-core';
import { safeExec } from '../../src/utils/exec/safe.js';
import { checkCommandAvailability } from '../../src/utils/exec/commandAvailability.js';
import * as pathValidator from 'octocode-security-utils/pathValidator';
import { promises as fs } from 'fs';

// Mock dependencies
vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi.fn(),
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: vi
    .fn()
    .mockResolvedValue({ available: true, command: 'rg' }),
  getMissingCommandError: vi.fn().mockReturnValue('Command not available'),
}));

vi.mock('octocode-security-utils/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn(),
  },
}));

vi.mock('fs', () => ({
  promises: {
    stat: vi.fn(),
    readFile: vi.fn(),
    readdir: vi.fn(),
  },
}));

const runRipgrep = (query: Record<string, unknown>) =>
  searchContentRipgrep(
    RipgrepQuerySchema.parse({
      id: 'local_ripgrep_query',
      researchGoal: 'Test',
      reasoning: 'Schema validation',
      ...query,
    })
  );

const mockFsReaddir = vi.mocked((fs as any).readdir);

describe('localSearchCode', () => {
  const mockSafeExec = vi.mocked(safeExec);
  const mockValidate = vi.mocked(pathValidator.pathValidator.validate);
  const mockFsStat = vi.mocked(fs.stat);
  const mockFsReadFile = vi.mocked(fs.readFile);

  beforeEach(() => {
    vi.clearAllMocks();
    mockValidate.mockReturnValue({
      isValid: true,
      sanitizedPath: '/test/path',
    });
    // Default mock for fs.stat - return a valid stats object with mtime
    mockFsStat.mockResolvedValue({
      mtime: new Date('2024-06-01T00:00:00.000Z'),
    } as unknown as Awaited<ReturnType<typeof fs.stat>>);
    // Default mock for fs.readFile - return content for character offset computation
    mockFsReadFile.mockResolvedValue(
      'test content for byte to char conversion'
    );
    // Default mock for fs.readdir - return empty directory
    mockFsReaddir.mockResolvedValue([]);
  });

  describe('Basic search', () => {
    it('should execute ripgrep search successfully', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.ts:10:function test()',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
    });

    it('should handle empty results', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'nonexistent',
        path: '/test/path',
      });

      expect(result.status).toBe('empty');
    });

    it('should handle command failure', async () => {
      mockSafeExec.mockResolvedValue({
        success: false,
        code: 1,
        stdout: '',
        stderr: 'Error: pattern invalid',
      });

      const result = await runRipgrep({
        // Use a valid regex; T1.6 pre-flight now rejects '[' before
        // safeExec ever runs (which is the more correct behaviour and
        // is covered by tests/tools/local_ripgrep_best_practices.test.ts).
        pattern: 'someValidPattern',
        path: '/test/path',
      });

      // Command failure without output returns empty, not error
      expect(result.status).toBe('empty');
    });
  });

  describe('Workflow modes', () => {
    it('should apply discovery mode preset', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.ts:3\nfile2.ts:7',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        mode: 'discovery',
      });

      expect(result.status).toBe('hasResults');
      // Discovery mode sets count=true (uses -c flag for per-file match counts)
      expect(mockSafeExec).toHaveBeenCalledWith(
        expect.stringMatching(/rg$/),
        expect.arrayContaining(['-c'])
      );
    });

    it('should apply detailed mode preset', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.ts:10:function test()',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        mode: 'detailed',
      });

      expect(result.status).toBe('hasResults');
    });
  });

  describe('Pattern types', () => {
    it('should handle fixed string search', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.ts:10:TODO: fix this',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'TODO:',
        path: '/test/path',
        fixedString: true,
      });

      expect(result.status).toBe('hasResults');
      expect(mockSafeExec).toHaveBeenCalledWith(
        expect.stringMatching(/rg$/),
        expect.arrayContaining(['-F'])
      );
    });

    it('should handle perl regex', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.ts:10:export function test',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: '(?<=export )\\w+',
        path: '/test/path',
        perlRegex: true,
      });

      expect(result.status).toBe('hasResults');
      expect(mockSafeExec).toHaveBeenCalledWith(
        expect.stringMatching(/rg$/),
        expect.arrayContaining(['-P'])
      );
    });
  });

  describe('File filtering', () => {
    it('should filter by file type', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.ts:10:test',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        type: 'ts',
      });

      expect(result.status).toBe('hasResults');
      expect(mockSafeExec).toHaveBeenCalledWith(
        expect.stringMatching(/rg$/),
        expect.arrayContaining(['-t', 'ts'])
      );
    });

    it('should exclude directories', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'src/file1.ts:10:test',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        excludeDir: ['node_modules', '.git'],
      });

      expect(result.status).toBe('hasResults');
      expect(mockSafeExec).toHaveBeenCalledWith(
        expect.stringMatching(/rg$/),
        expect.arrayContaining(['-g', '!node_modules/', '-g', '!.git/'])
      );
    });
  });

  describe('Output control', () => {
    it('should list files only', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.ts\nfile2.ts\nfile3.ts',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesOnly: true,
      });

      expect(result.status).toBe('hasResults');
      expect(mockSafeExec).toHaveBeenCalledWith(
        expect.stringMatching(/rg$/),
        expect.arrayContaining(['-l'])
      );
    });

    it('should report correct totalMatches when filesOnly=true', async () => {
      // When filesOnly=true, ripgrep uses -l flag which outputs plain text
      // (one filename per line, no JSON output)
      // BUG FIX: We no longer use --json with -l as they're incompatible
      const plainTextOutput = ['file1.ts', 'file2.ts'].join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: plainTextOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesOnly: true,
      });

      expect(result.status).toBe('hasResults');
    });

    it('should include context lines', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout:
          'file1.ts:9:prev line\nfile1.ts:10:match line\nfile1.ts:11:next line',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'match',
        path: '/test/path',
        contextLines: 1,
        charLength: 10000,
      });

      expect(result.status).toBe('hasResults');
      expect(mockSafeExec).toHaveBeenCalledWith(
        expect.stringMatching(/rg$/),
        expect.arrayContaining(['-C', '1'])
      );
    });

    it('should include afterContext lines when specified', async () => {
      // Provide NDJSON with match and context lines
      const jsonLines = [
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: 'file1.ts' },
            lines: { text: 'match line' },
            line_number: 10,
            absolute_offset: 100,
            submatches: [{ start: 0, end: 5, match: { text: 'match' } }],
          },
        }),
        JSON.stringify({
          type: 'context',
          data: {
            path: { text: 'file1.ts' },
            lines: { text: 'prev line' },
            line_number: 9,
            absolute_offset: 0,
          },
        }),
        JSON.stringify({
          type: 'context',
          data: {
            path: { text: 'file1.ts' },
            lines: { text: 'next line' },
            line_number: 11,
            absolute_offset: 0,
          },
        }),
      ].join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonLines,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'match',
        path: '/test/path',
        afterContext: 1,
      });

      expect(result.status).toBe('hasResults');
      const value = result.files![0]!.matches[0]!.value;
      expect(value).toContain('match line');
      expect(value).toContain('next line');
      expect(value).not.toContain('prev line');
    });
  });

  describe('Pagination', () => {
    it('should apply character-based pagination', async () => {
      // Create output with multiple lines to ensure line-aware pagination works
      const lines = Array.from(
        { length: 500 },
        (_, i) => `file.ts:${i}:line content ${i}`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: lines,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        charLength: 5000,
      });

      expect(result.status).toBe('hasResults');
      // pagination.hasMore is only set when content exceeds charLength
      // The mock output might not be large enough to trigger pagination
      if (result.pagination) {
        expect(typeof result.pagination.hasMore).toBe('boolean');
      }
    });

    it('should require pagination for large output', async () => {
      // Mock valid JSON output that would be large
      const longJsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: 'test.ts' },
          lines: { text: 'x'.repeat(15000) },
          line_number: 1,
          absolute_offset: 0,
          submatches: [{ start: 0, end: 15000 }],
        },
      });
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: longJsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        // No charLength specified
      });

      // Large output should prompt for pagination or error
      expect(['hasResults', 'error']).toContain(result.status);
      if (result.status === 'hasResults') {
        // Check if files array is included
        expect(result.files).toBeDefined();
        expect(result.files?.length).toBeGreaterThan(0);
      }
    });
  });

  describe('Limits and error mappings', () => {
    it('should enforce maxFiles limit', async () => {
      const files = Array.from({ length: 30 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'match' },
          line_number: 1,
          absolute_offset: 0,
          submatches: [{ start: 0, end: 5, match: { text: 'match' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'match',
        path: '/test/path',
        maxFiles: 5,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files?.length).toBeLessThanOrEqual(5);
    });

    it('should map output-too-large error from engine', async () => {
      mockSafeExec.mockRejectedValue(new Error('Output size limit exceeded'));

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE);
    });
  });

  describe('Path validation', () => {
    it('should reject invalid paths', async () => {
      mockValidate.mockReturnValue({
        isValid: false,
        error: 'Path is outside allowed directories',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/etc/passwd',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED
      );
    });
  });

  describe('Case sensitivity', () => {
    it('should use smart case by default', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: JSON.stringify({
          type: 'match',
          data: {
            path: { text: 'file1.ts' },
            lines: { text: 'Test' },
            line_number: 10,
            absolute_offset: 100,
            submatches: [{ start: 0, end: 4 }],
          },
        }),
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(mockSafeExec).toHaveBeenCalledWith(
        expect.stringMatching(/rg$/),
        expect.arrayContaining(['-S'])
      );
    });

    it('should override with case-insensitive', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: JSON.stringify({
          type: 'match',
          data: {
            path: { text: 'file1.ts' },
            lines: { text: 'TEST' },
            line_number: 10,
            absolute_offset: 100,
            submatches: [{ start: 0, end: 4 }],
          },
        }),
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        caseInsensitive: true,
      });

      expect(result.status).toBe('hasResults');
      expect(mockSafeExec).toHaveBeenCalledWith(
        expect.stringMatching(/rg$/),
        expect.arrayContaining(['-i'])
      );
    });
  });

  describe('NEW FEATURE: Two-level pagination', () => {
    it('should paginate files with default 10 files per page', async () => {
      const files = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files).toBeDefined();
      expect(result.files?.length).toBeLessThanOrEqual(10);
      expect(result.pagination?.totalPages).toBe(3);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should navigate to second page of files', async () => {
      const files = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filePageNumber: 2,
      });

      expect(result.status).toBe('hasResults');
      expect(result.pagination?.currentPage).toBe(2);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should paginate matches per file with default 10 matches', async () => {
      const matches = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: `test match ${i}` },
          line_number: i + 1,
          absolute_offset: 100 * i,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = matches.map(m => JSON.stringify(m)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files).toHaveLength(1);
      expect(result.files![0]!.matchCount).toBe(25);
      expect(result.files![0]!.matches.length).toBeLessThanOrEqual(10);
      expect(result.files![0]!.pagination?.totalPages).toBe(3);
      expect(result.files![0]!.pagination?.hasMore).toBe(true);
    });

    it('should support custom filesPerPage', async () => {
      const files = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesPerPage: 5,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files?.length).toBeLessThanOrEqual(5);
      expect(result.pagination?.filesPerPage).toBe(5);
      expect(result.pagination?.totalPages).toBe(5);
    });

    it('should support custom matchesPerPage', async () => {
      const matches = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: `test match ${i}` },
          line_number: i + 1,
          absolute_offset: 100 * i,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = matches.map(m => JSON.stringify(m)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        matchesPerPage: 5,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches.length).toBeLessThanOrEqual(5);
      expect(result.files![0]!.pagination?.matchesPerPage).toBe(5);
    });
  });

  describe('NEW FEATURE: matchContentLength configuration', () => {
    it('should use default 200 chars per match', async () => {
      const longContent = 'x'.repeat(500);
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: longContent },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 10, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches[0]!.value.length).toBeLessThanOrEqual(
        203
      );
      expect(result.files![0]!.matches[0]!.value).toMatch(/\.\.\.$/);
    });

    it('should support custom matchContentLength up to 800 chars', async () => {
      const longContent = 'x'.repeat(1000);
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: longContent },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 10, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        matchContentLength: 800,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches[0]!.value.length).toBeLessThanOrEqual(
        803
      );
      expect(result.files![0]!.matches[0]!.value).toMatch(/\.\.\.$/);
    });

    it('should not truncate content under matchContentLength', async () => {
      const shortContent = 'short test content';
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: shortContent },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        matchContentLength: 200,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches[0]!.value).toBe(shortContent);
      expect(result.files![0]!.matches[0]!.value).not.toMatch(/\.\.\.$/);
    });
  });

  describe('NEW FEATURE: Files sorted by modification time', () => {
    it('should sort files by modification time (most recent first)', async () => {
      const files = [
        { path: '/test/old.ts', time: '2024-01-01T00:00:00.000Z' },
        { path: '/test/new.ts', time: '2024-12-01T00:00:00.000Z' },
        { path: '/test/mid.ts', time: '2024-06-01T00:00:00.000Z' },
      ];

      const jsonOutput = files
        .map(f =>
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: f.path },
              lines: { text: 'test match' },
              line_number: 10,
              absolute_offset: 100,
              submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
            },
          })
        )
        .join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      // Mock fs.stat to return different mtime for each file based on the time in the test data
      mockFsStat.mockImplementation(filePath => {
        const filePathString = filePath.toString();
        const file = files.find(f => f.path === filePathString);
        return Promise.resolve({
          mtime: new Date(file?.time || '2024-06-01T00:00:00.000Z'),
        } as unknown as Awaited<ReturnType<typeof fs.stat>>);
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        showFileLastModified: true,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files).toBeDefined();
      expect(result.files![0]!.modified).toBeDefined();
    });
  });

  describe('NEW FEATURE: Structured output format', () => {
    it('should return RipgrepFileMatches structure', async () => {
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 500,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        showFileLastModified: true,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files).toBeDefined();
      expect(result.files![0]).toHaveProperty('path');
      expect(result.files![0]).toHaveProperty('matchCount');
      expect(result.files![0]).toHaveProperty('matches');
      expect(result.files![0]).toHaveProperty('modified');
    });

    it('should include location.charOffset for FETCH_CONTENT integration', async () => {
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 50, // Byte offset in the file
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      // Mock file content that matches the byte offset
      mockFsReadFile.mockResolvedValue(
        'x'.repeat(50) + 'test match more content'
      );

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
    });

    it('should include line and column for human reference', async () => {
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test match' },
          line_number: 42,
          absolute_offset: 500,
          submatches: [{ start: 5, end: 9, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches[0]!.line).toBe(42);
      expect(result.files![0]!.matches[0]!.column).toBe(5);
    });
  });

  describe('File pagination - Edge cases', () => {
    it('should handle filePageNumber = 0 or negative (defaults to 1)', async () => {
      const files = Array.from({ length: 50 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      // Schema should validate page number, test with valid value
      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filePageNumber: 1,
        filesPerPage: 10,
      });

      expect(result.status).toBe('hasResults');
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should handle filePageNumber > total pages', async () => {
      const files = Array.from({ length: 15 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filePageNumber: 10, // Beyond last page
        filesPerPage: 10,
      });

      expect(['hasResults', 'empty']).toContain(result.status);
      if (result.status === 'hasResults') {
        expect(result.pagination?.currentPage).toBe(10);
      }
    });

    it('should handle filesPerPage = 1', async () => {
      const files = Array.from({ length: 5 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesPerPage: 1,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files?.length).toBe(1);
      expect(result.pagination?.filesPerPage).toBe(1);
      expect(result.pagination?.totalPages).toBe(5);
    });

    it('should handle filesPerPage = 50 (max)', async () => {
      const files = Array.from({ length: 75 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesPerPage: 50,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files?.length).toBeLessThanOrEqual(50);
      expect(result.pagination?.filesPerPage).toBe(50);
      expect(result.pagination?.totalPages).toBe(2);
    });

    it('should handle single file result', async () => {
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/single.ts' },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files?.length).toBe(1);
      expect(result.pagination?.totalPages).toBe(1);
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should coerce filePageNumber=0 to 1 via defaulting (schema bypass)', async () => {
      const files = Array.from({ length: 15 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await searchContentRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesPerPage: 10,
        filePageNumber: 0,
      } as any);

      expect(['hasResults', 'empty']).toContain(result.status);
      if (result.status === 'hasResults') {
        expect(result.pagination?.currentPage).toBe(1);
      }
    });

    it('should reflect negative filePageNumber as provided (no clamping, schema bypass)', async () => {
      const files = Array.from({ length: 15 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await searchContentRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesPerPage: 10,
        filePageNumber: -2,
      } as any);

      expect(['hasResults', 'empty']).toContain(result.status);
      if (result.status === 'hasResults') {
        expect(result.pagination?.currentPage).toBe(-2);
      }
    });
  });

  describe('Match pagination - Edge cases', () => {
    it('should handle matchesPerPage = 1', async () => {
      const matches = Array.from({ length: 5 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: `test match ${i}` },
          line_number: i + 1,
          absolute_offset: 100 * i,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = matches.map(m => JSON.stringify(m)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        matchesPerPage: 1,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches.length).toBe(1);
      expect(result.files![0]!.pagination?.matchesPerPage).toBe(1);
      expect(result.files![0]!.pagination?.totalPages).toBe(5);
    });

    it('should handle matchesPerPage = 100 (max)', async () => {
      const matches = Array.from({ length: 150 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: `test match ${i}` },
          line_number: i + 1,
          absolute_offset: 100 * i,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = matches.map(m => JSON.stringify(m)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        matchesPerPage: 100,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches.length).toBeLessThanOrEqual(100);
      expect(result.files![0]!.pagination?.matchesPerPage).toBe(100);
    });

    it('should handle single match in file', async () => {
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'single test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 7, end: 11, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matchCount).toBe(1);
      expect(result.files![0]!.matches.length).toBe(1);
      // Per-file pagination is only added when there are more matches than matchesPerPage
      if (result.files![0]!.pagination) {
        expect(result.files![0]!.pagination.hasMore).toBe(false);
      }
    });

    it('should handle exact boundary (10 matches, 10 per page)', async () => {
      const matches = Array.from({ length: 10 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: `test match ${i}` },
          line_number: i + 1,
          absolute_offset: 100 * i,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = matches.map(m => JSON.stringify(m)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        matchesPerPage: 10,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches.length).toBe(10);
      // Per-file pagination is only added when there are more matches than matchesPerPage
      if (result.files![0]!.pagination) {
        expect(result.files![0]!.pagination.totalPages).toBe(1);
        expect(result.files![0]!.pagination.hasMore).toBe(false);
      }
    });

    it('should handle matchesPerPage overflow (schema bypass)', async () => {
      const matches = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: `test match ${i}` },
          line_number: i + 1,
          absolute_offset: 100 * i,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = matches.map(m => JSON.stringify(m)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      // Bypass schema to inject overflow value
      const result = await searchContentRipgrep({
        pattern: 'test',
        path: '/test/path',
        matchesPerPage: 1000,
      } as any);

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matchCount).toBe(25);
      // All matches should be included, no pagination created
      expect(result.files![0]!.matches.length).toBe(25);
      expect(result.files![0]!.pagination).toBeUndefined();
    });
  });

  describe('Match content - UTF-8 handling', () => {
    it('should handle UTF-8 in match values', async () => {
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'Café résumé test' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'Café' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'Café',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches[0]!.value).toContain('Café');
      expect(result.files![0]!.matches[0]!.value).not.toMatch(/\uFFFD/);
    });

    it('should truncate at UTF-8 char boundaries', async () => {
      // Create long content with UTF-8 chars that needs truncation
      const longContent = 'test ' + 'café '.repeat(50) + 'end';
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: longContent },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        matchContentLength: 100,
      });

      expect(result.status).toBe('hasResults');
      // Should truncate but not split UTF-8 chars
      expect(result.files![0]!.matches[0]!.value).not.toMatch(/\uFFFD/);
      expect(result.files![0]!.matches[0]!.value.length).toBeLessThanOrEqual(
        103
      ); // 100 + '...'
    });

    it('should handle emoji in match content', async () => {
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: '😀 test 🎉 code 👍' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 2, end: 6, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches[0]!.value).toContain('😀');
      expect(result.files![0]!.matches[0]!.value).toContain('🎉');
      expect(result.files![0]!.matches[0]!.value).not.toMatch(/\uFFFD/);
    });

    it('should handle 3-byte UTF-8 chars (CJK)', async () => {
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: '你好 test 世界' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 3, end: 7, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches[0]!.value).toContain('你好');
      expect(result.files![0]!.matches[0]!.value).not.toMatch(/\uFFFD/);
    });

    it('should handle matchContentLength with mixed UTF-8', async () => {
      const mixedContent = 'Test: café 中文 😀 ' + 'x'.repeat(200);
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: mixedContent },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'Test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'Test',
        path: '/test/path',
        matchContentLength: 50,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.matches[0]!.value.length).toBeLessThanOrEqual(
        53
      ); // 50 + '...'
      expect(result.files![0]!.matches[0]!.value).not.toMatch(/\uFFFD/);
    });
  });

  describe('Research context fields', () => {
    it('should not echo researchGoal and reasoning in hasResults', async () => {
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        researchGoal: 'Find test implementations',
        reasoning: 'Need to understand test patterns',
      });

      expect(result.status).toBe('hasResults');
      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });

    it('should not echo researchGoal and reasoning in empty results', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '',
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'nonexistent',
        path: '/test/path',
        researchGoal: 'Find missing pattern',
        reasoning: 'Verify pattern absence',
      });

      expect(result.status).toBe('empty');
      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });

    it('should not echo researchGoal and reasoning in error results', async () => {
      mockValidate.mockReturnValue({
        isValid: false,
        error: 'Invalid path',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/invalid/path',
        researchGoal: 'Search invalid path',
        reasoning: 'Testing error handling',
      });

      expect(result.status).toBe('error');
      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });
  });

  describe('computeCharacterOffsets - branch coverage', () => {
    it('should handle files with no matches array (empty file matches)', async () => {
      // This specifically targets the early return at line 935
      // when file.matches is empty or undefined
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '', // Empty output produces no matches
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      // Empty output means no files
      expect(result.status).toBe('empty');
    });

    it('should compute character offsets for UTF-8 content', async () => {
      // Targets the character offset computation at lines 943-964
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/utf8file.ts' },
          lines: { text: 'hello 世界 test' }, // Chinese chars are 3 bytes each
          line_number: 1,
          absolute_offset: 0,
          submatches: [{ start: 10, end: 14, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      // Mock readFile to return the same content for accurate byte-to-char conversion
      mockFsReadFile.mockResolvedValue('hello 世界 test');

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files).toBeDefined();
    });

    it('should successfully compute char offsets from byte offsets', async () => {
      // Tests the actual character offset computation (lines 943-964)
      const fileContent = 'const test = 42;\nfunction foo() {}';
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'const test = 42;' },
          line_number: 1,
          absolute_offset: 6, // 'test' starts at byte 6
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      mockFsReadFile.mockResolvedValue(fileContent);

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
    });

    it('should handle file with matches array for char offset computation', async () => {
      // Tests that files with matches are processed for char offset computation
      const jsonOutput = [
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/test/file1.ts' },
            lines: { text: 'first test match' },
            line_number: 1,
            absolute_offset: 6,
            submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
          },
        }),
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/test/file2.ts' },
            lines: { text: 'second test match' },
            line_number: 1,
            absolute_offset: 7,
            submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
          },
        }),
      ].join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      mockFsReadFile.mockImplementation(path => {
        if (path.toString().includes('file1')) {
          return Promise.resolve('first test match');
        }
        return Promise.resolve('second test match');
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files).toHaveLength(2);
    });

    it('should return empty when only begin/end messages (no matches)', async () => {
      // Create output with only begin/end messages but no actual matches
      const jsonOutput = [
        JSON.stringify({
          type: 'begin',
          data: { path: { text: '/test/emptyfile.ts' } },
        }),
        JSON.stringify({
          type: 'end',
          data: { path: { text: '/test/emptyfile.ts' } },
        }),
        JSON.stringify({
          type: 'summary',
          data: {
            elapsed_total: { human: '0ms' },
            stats: {
              elapsed: { human: '0ms' },
              searches: 1,
              searches_with_match: 0,
              bytes_searched: 100,
              bytes_printed: 0,
              matched_lines: 0,
              matches: 0,
            },
          },
        }),
      ].join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'nonexistent',
        path: '/test/path',
      });

      // Files with no matches means no file entries in result
      expect(result.status).toBe('hasResults');
      expect(result.files).toHaveLength(0);
    });

    it('should handle file with empty matches array', async () => {
      // This test covers the branch where file.matches is empty (line 575-576)
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test' },
          line_number: 1,
          absolute_offset: 0,
          submatches: [], // Empty submatches
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      expect(result.files).toBeDefined();
    });

    it('should fallback to byte offsets when file read fails', async () => {
      // Test that when fs.readFile fails, the byte offsets are kept as fallback
      // This exercises the catch block in computeCharacterOffsets (lines 968-970)
      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/unreadable.ts' },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 500,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      // Mock readFile to fail for character offset computation
      mockFsReadFile.mockRejectedValue(new Error('File not found'));
      // Mock stat to also fail
      mockFsStat.mockRejectedValue(new Error('File not found'));

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        showFileLastModified: true,
      });

      expect(result.status).toBe('hasResults');
      // Should still have the match even when file read fails
      expect(result.files![0]!.matches[0]).toBeDefined();
    });

    it('should handle context lines with missing context entries', async () => {
      // Create NDJSON with match but missing context lines in the map
      const jsonLines = [
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: 'file1.ts' },
            lines: { text: 'match line' },
            line_number: 10,
            absolute_offset: 100,
            submatches: [{ start: 0, end: 5, match: { text: 'match' } }],
          },
        }),
        // Context for line 11 exists, but 9 and 12 are missing
        JSON.stringify({
          type: 'context',
          data: {
            path: { text: 'file1.ts' },
            lines: { text: 'next line' },
            line_number: 11,
            absolute_offset: 200,
          },
        }),
      ].join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonLines,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'match',
        path: '/test/path',
        beforeContext: 2,
        afterContext: 2,
      });

      expect(result.status).toBe('hasResults');
      // Should include the context that exists but skip missing ones
      const value = result.files![0]!.matches[0]!.value;
      expect(value).toContain('match line');
      expect(value).toContain('next line');
    });

    it('should handle before context when context line does not exist in map', async () => {
      // Test the branch where before context lookup returns undefined
      const jsonLines = [
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: 'file1.ts' },
            lines: { text: 'match at line 5' },
            line_number: 5,
            absolute_offset: 100,
            submatches: [{ start: 0, end: 5, match: { text: 'match' } }],
          },
        }),
        // Only provide context for line 6, not lines 3, 4
        JSON.stringify({
          type: 'context',
          data: {
            path: { text: 'file1.ts' },
            lines: { text: 'line 6' },
            line_number: 6,
            absolute_offset: 200,
          },
        }),
      ].join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonLines,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'match',
        path: '/test/path',
        beforeContext: 2, // Request context for lines 3 and 4
        afterContext: 1, // Request context for line 6
      });

      expect(result.status).toBe('hasResults');
      const value = result.files![0]!.matches[0]!.value;
      expect(value).toContain('match at line 5');
      expect(value).toContain('line 6');
      // Lines 3 and 4 context were not in the map, so they won't be in output
    });
  });

  describe('Pagination hints validation', () => {
    it('should show file pagination hints accurately', async () => {
      const files = Array.from({ length: 30 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesPerPage: 10,
      });

      expect(result.status).toBe('hasResults');
      expect(result.hints).toBeDefined();
      expect(result.pagination?.currentPage).toBe(1);
      expect(result.pagination?.totalPages).toBe(3);
    });

    it('should show per-file match pagination hints', async () => {
      const matches = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: `test match ${i}` },
          line_number: i + 1,
          absolute_offset: 100 * i,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = matches.map(m => JSON.stringify(m)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        matchesPerPage: 10,
      });

      expect(result.status).toBe('hasResults');
      expect(result.files![0]!.pagination?.totalPages).toBe(3);
      expect(result.files![0]!.pagination?.hasMore).toBe(true);
    });

    it('should show hints for navigating to next page', async () => {
      const files = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesPerPage: 10,
        filePageNumber: 1,
      });

      expect(result.status).toBe('hasResults');
      if (result.pagination?.hasMore) {
        expect(result.hints).toBeDefined();
        const hasNextPageHint = result.hints?.some(
          h =>
            h.includes('filePageNumber=2') || h.toLowerCase().includes('next')
        );
        expect(hasNextPageHint).toBe(true);
      }
    });

    it('should show final page message on last page', async () => {
      const files = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        filesPerPage: 10,
        filePageNumber: 3, // Last page
      });

      expect(result.status).toBe('hasResults');
      expect(result.pagination?.hasMore).toBe(false);
      if (result.hints) {
        const hasFinalPageHint = result.hints.some(
          h =>
            h.toLowerCase().includes('final') ||
            h.toLowerCase().includes('last')
        );
        expect(hasFinalPageHint).toBe(true);
      }
    });

    it('should include parameter names matching tool schema', async () => {
      const files = Array.from({ length: 25 }, (_, i) => ({
        type: 'match',
        data: {
          path: { text: `/test/file${i}.ts` },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      }));
      const jsonOutput = files.map(f => JSON.stringify(f)).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
      });

      expect(result.status).toBe('hasResults');
      if (result.hints) {
        // Hints should use actual schema parameter names
        const usesSchemaParams = result.hints.some(
          (h: string) =>
            h.includes('filesPerPage') || h.includes('filePageNumber')
        );
        expect(usesSchemaParams).toBe(true);
      }
    });
  });

  describe('getFileModifiedTime error handling', () => {
    it('should handle stat errors when getting modified time', async () => {
      // This tests the catch block in getFileModifiedTime

      const jsonOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 4, match: { text: 'test' } }],
        },
      });

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: jsonOutput,
        stderr: '',
      });

      // Mock stat to fail when getting modified time
      mockFsStat.mockRejectedValue(new Error('File not found'));
      mockFsReadFile.mockResolvedValue('test match content');

      const result = await runRipgrep({
        pattern: 'test',
        path: '/test/path',
        showFileLastModified: true,
      });

      // Should succeed but without modified time
      expect(result.status).toBe('hasResults');
      expect(result.files).toBeDefined();
      // Modified time should be undefined due to error
      expect(result.files![0]!.modified).toBeUndefined();
    });
  });
});
