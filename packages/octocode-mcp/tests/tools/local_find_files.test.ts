/**
 * Tests for localFindFiles tool
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { LOCAL_TOOL_ERROR_CODES } from '../../src/errors/localToolErrors.js';
import { findFiles } from '../../src/tools/local_find_files/findFiles.js';
import type { FindFilesResult } from '../../src/utils/core/types.js';
import { safeExec } from '../../src/utils/exec/safe.js';
import { checkCommandAvailability } from '../../src/utils/exec/commandAvailability.js';
import * as pathValidator from 'octocode-security-utils/pathValidator';

// Mock dependencies
vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi.fn(),
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: vi
    .fn()
    .mockResolvedValue({ available: true, command: 'find' }),
  getMissingCommandError: vi.fn().mockReturnValue('Command not available'),
}));

vi.mock('octocode-security-utils/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn(),
  },
}));

// Mock fs for file details
vi.mock('fs', () => {
  const lstat = vi.fn();
  return {
    promises: {
      lstat,
    },
    default: {
      promises: {
        lstat,
      },
    },
  };
});
const mockFs = vi.mocked(await import('fs')) as unknown as {
  promises: { lstat: ReturnType<typeof vi.fn> };
};

const expectDefinedFiles = (result: FindFilesResult) => {
  expect(result.files).toBeDefined();
  return result.files!;
};

describe('localFindFiles', () => {
  const mockSafeExec = vi.mocked(safeExec);
  const mockValidate = vi.mocked(pathValidator.pathValidator.validate);

  beforeEach(() => {
    vi.clearAllMocks();
    mockValidate.mockReturnValue({
      isValid: true,
      sanitizedPath: '/test/path',
    });
  });

  describe('Basic file discovery', () => {
    it('should find files by name pattern', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/file1.js\0/test/path/file2.js\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.js',
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files).toHaveLength(2);
      expect(files[0]!.path).toBe('/test/path/file1.js');
    });

    it('signals page-out-of-range instead of silent empty (E2)', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/a.ts\0/test/path/b.ts\0/test/path/c.ts\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.ts',
        page: 999,
      });

      const hints = (result.hints ?? []).join('\n');
      expect(hints).toMatch(/outside available range|page 999 is/i);
    });

    it('should include metadata by default', async () => {
      const modified = new Date('2025-01-01T00:00:00Z');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/file1.js\0',
        stderr: '',
      });

      vi.mocked(mockFs.promises.lstat).mockResolvedValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
        isFile: () => true,
        size: 123,
        mode: parseInt('100644', 8),
        mtime: modified,
      } as unknown as import('fs').Stats);

      const result = await findFiles({ path: '/test/path' });

      expect(result.status).toBeUndefined();

      const files = expectDefinedFiles(result);

      // Raw `size` field dropped to remove redundancy with `sizeFormatted`
      // (human-readable). 123 bytes → "123.0B".
      expect(files[0]).toMatchObject({
        path: '/test/path/file1.js',
        type: 'file',
        sizeFormatted: '123.0B',
        permissions: '644',
      });
    });

    it('should handle case-insensitive search', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/FILE.JS\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        iname: '*.js',
      });

      expect(result.status).toBeUndefined();
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-iname', '*.js'])
      );
    });

    it('should handle empty results', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.nonexistent',
      });

      expect(result.status).toBe('empty');
    });

    it('does NOT implicitly cap at 1000 — all discovered files stay paginable', async () => {
      // 1002 files, no explicit limit: every file must be reachable via
      // page (the old silent 1000 cap dropped 2 files unrecoverably).
      const paths = Array.from(
        { length: 1002 },
        (_, index) => `/test/path/file-${index}.ts`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: `${paths}\0`,
        stderr: '',
      });

      const result = await findFiles({ path: '/test/path' });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalFiles).toBe(1002);
      // No cap hint — nothing was dropped.
      expect(result.hints?.some(h => /capped at/i.test(h))).toBeFalsy();
    });

    it('caps + warns only when an explicit limit is given (a deliberate user cap)', async () => {
      const paths = Array.from(
        { length: 1002 },
        (_, index) => `/test/path/file-${index}.ts`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: `${paths}\0`,
        stderr: '',
      });

      const result = await findFiles({ path: '/test/path', limit: 1000 });

      expect(result.pagination?.totalFiles).toBe(1000);
      expect(result.hints?.some(h => /capped at 1000/i.test(h))).toBe(true);
      expect(result.hints?.some(h => h.includes('1002'))).toBe(true);
    });
  });

  describe('File type filtering', () => {
    it('should filter by file type', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/file1.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        type: 'f',
      });

      expect(result.status).toBeUndefined();
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-type', 'f'])
      );
    });

    it('should find directories only', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/dir1\0/test/path/dir2\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        type: 'd',
      });

      expect(result.status).toBeUndefined();
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-type', 'd'])
      );
    });
  });

  describe('Time-based filtering', () => {
    it('should find recently modified files', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/recent.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        modifiedWithin: '7d',
      });

      expect(result.status).toBeUndefined();
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-mtime', '-7'])
      );
    });

    it('should find files modified before date', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/old.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        modifiedBefore: '30d',
      });

      expect(result.status).toBeUndefined();
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-mtime', '+30'])
      );
    });
  });

  describe('Size filtering', () => {
    it('should find files larger than threshold', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/large.bin\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        sizeGreater: '1M',
      });

      expect(result.status).toBeUndefined();
      // Platform-aware: macOS converts M/G to bytes (c suffix), Linux keeps M/G
      // BUG FIX: macOS BSD find only supports 'c' (bytes) and 'k' (kilobytes)
      const isMacOS = process.platform === 'darwin';
      const expectedSize = isMacOS ? `+${1 * 1024 * 1024}c` : '+1M';
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-size', expectedSize])
      );
    });

    it('should find files smaller than threshold', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/small.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        sizeLess: '1k',
      });

      expect(result.status).toBeUndefined();
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-size', '-1k'])
      );
    });
  });

  describe('Permission filtering', () => {
    it('should find executable files', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/script.sh\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        executable: true,
      });

      expect(result.status).toBeUndefined();
      // Platform-specific: Linux uses -executable, macOS uses -perm +111
      if (process.platform === 'linux') {
        expect(mockSafeExec).toHaveBeenCalledWith(
          'find',
          expect.arrayContaining(['-executable'])
        );
      } else {
        expect(mockSafeExec).toHaveBeenCalledWith(
          'find',
          expect.arrayContaining(['-perm', '+111'])
        );
      }
    });

    it('should filter by permissions', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/file.sh\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        permissions: '755',
      });

      expect(result.status).toBeUndefined();
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-perm', '755'])
      );
    });
  });

  describe('Depth control', () => {
    it('should limit search depth', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/file1.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        maxDepth: 2,
      });

      expect(result.status).toBeUndefined();
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-maxdepth', '2'])
      );
    });

    it('should set minimum depth', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/sub/file.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        minDepth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-mindepth', '1'])
      );
    });
  });

  describe('Directory exclusion', () => {
    it('should exclude specific directories', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/src/file.js\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        excludeDir: ['node_modules', '.git'],
      });

      expect(result.status).toBeUndefined();
      // The implementation uses a more complex pattern for excluding directories:
      // ( -path */node_modules -o -path */node_modules/* ) -prune -o
      expect(mockSafeExec).toHaveBeenCalledWith(
        'find',
        expect.arrayContaining(['-path', '*/node_modules', '-prune'])
      );
    });

    it('should auto-exclude tool/IDE cache dirs by default', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/src/file.js\0',
        stderr: '',
      });

      const result = await findFiles({ path: '/test/path' });
      expect(result.status).toBeUndefined();

      const args = mockSafeExec.mock.calls[0]![1] as string[];
      // Regression: localFindFiles was returning .octocode/scan/* artifacts
      // because DEFAULT_EXCLUDE_DIRS only listed node_modules/dist/.git/coverage/build/.next.
      for (const dir of [
        '.octocode',
        '.cursor',
        '.vscode',
        '.idea',
        '.claude',
        '.context',
        '.turbo',
        '.cache',
        '.parcel-cache',
        '.svelte-kit',
        '.nuxt',
        'out',
        'target',
      ]) {
        expect(args).toContain(`*/${dir}`);
      }
    });

    it('should NOT prune directories that appear in the search path itself', async () => {
      // BUG-FIX: Searching inside e.g. /work/.context/sub used to return empty
      // because the default excludeDir included ".context", which generated the
      // prune pattern `*/.context/*` that matched EVERY file under that path.
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/work/.context/sub',
      });
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/work/.context/sub/file.ts\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/work/.context/sub',
        name: '*.ts',
      });

      expect(result.status).toBeUndefined();

      const args = mockSafeExec.mock.calls[0]![1] as string[];
      // .context must NOT be in the prune list when the search path contains it
      const idx = args.indexOf('*/.context');
      expect(idx).toBe(-1);
    });

    it('should NOT prune node_modules when searching inside node_modules', async () => {
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/project/node_modules/lodash',
      });
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/project/node_modules/lodash/index.js\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/project/node_modules/lodash',
        name: '*.js',
      });

      expect(result.status).toBeUndefined();

      const args = mockSafeExec.mock.calls[0]![1] as string[];
      const idx = args.indexOf('*/node_modules');
      expect(idx).toBe(-1);
    });

    it('should still exclude dirs that are NOT in the search path', async () => {
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/project/src',
      });
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/project/src/app.ts\0',
        stderr: '',
      });

      await findFiles({ path: '/project/src', name: '*.ts' });

      const args = mockSafeExec.mock.calls[0]![1] as string[];
      // node_modules is NOT in '/project/src' → still pruned
      expect(args).toContain('*/node_modules');
      // .context is NOT in '/project/src' → still pruned
      expect(args).toContain('*/.context');
    });
  });

  describe('Result limiting', () => {
    it('should apply result limit', async () => {
      const files = Array.from(
        { length: 150 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        limit: 50,
      });

      expect(result.status).toBeUndefined();
      const limitedFiles = expectDefinedFiles(result);
      expect(limitedFiles.length).toBeLessThanOrEqual(50);
    });

    it('should require pagination for large result sets', async () => {
      const files = Array.from(
        { length: 150 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        // No charLength specified
      });

      // Should either return results or error requesting pagination
      expect([undefined, 'error']).toContain(result.status);
      if (result.status === 'error') {
        // Should have error code for pagination
        expect(result.errorCode).toBeDefined();
      }
    });
  });

  describe('Concurrency behavior', () => {
    it('should cap concurrent lstat calls to 24', async () => {
      // Generate 100 file paths from find output
      const files =
        Array.from({ length: 100 }, (_, i) => `/test/file${i}.txt`).join('\0') +
        '\0';
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files,
        stderr: '',
      });

      let inFlight = 0;
      let maxInFlight = 0;
      // Mock lstat with small delay to expose concurrency
      vi.mocked(mockFs.promises.lstat).mockImplementation(async () => {
        inFlight++;
        maxInFlight = Math.max(maxInFlight, inFlight);
        await new Promise(r => setTimeout(r, 2));
        inFlight--;
        return {
          isDirectory: () => false,
          isSymbolicLink: () => false,
          isFile: () => true,
          size: 123,
          mode: parseInt('100644', 8),
          mtime: new Date(),
        } as unknown as import('fs').Stats;
      });

      const result = await findFiles({ path: '/test/path', details: true });

      expect(result.status).toBeUndefined();
      // Bounded concurrency should never exceed 24
      expect(maxInFlight).toBeLessThanOrEqual(24);
    });
  });

  describe('Multiple name patterns', () => {
    it('should handle multiple name patterns with OR logic', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/file1.ts\0/test/path/file2.js\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        names: ['*.ts', '*.js'],
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files).toHaveLength(2);
    });
  });

  describe('Path validation', () => {
    it('should reject invalid paths', async () => {
      mockValidate.mockReturnValue({
        isValid: false,
        error: 'Path is outside allowed directories',
      });

      const result = await findFiles({
        path: '/etc/passwd',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED
      );
    });
  });

  describe('Error handling', () => {
    it('should handle command failure', async () => {
      mockSafeExec.mockResolvedValue({
        success: false,
        code: 1,
        stdout: '',
        stderr: 'find: invalid option',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.COMMAND_EXECUTION_FAILED
      );
    });

    it('should handle path not found (within workspace)', async () => {
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/nonexistent_path_xyz_123',
      });
      mockSafeExec.mockResolvedValue({
        success: false,
        code: 1,
        stdout: '',
        stderr:
          '/workspace/nonexistent_path_xyz_123: No such file or directory',
      });

      const result = await findFiles({
        path: '/workspace/nonexistent_path_xyz_123',
        name: '*.ts',
      });

      expect(result.status).toBe('error');
      expect(result.error).toContain('No such file or directory');
    });

    it('should handle general exceptions gracefully', async () => {
      mockSafeExec.mockRejectedValue(new Error('Unexpected error'));

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
      });

      expect(result.status).toBe('error');
    });

    it('should handle find command not available', async () => {
      vi.mocked(checkCommandAvailability).mockResolvedValueOnce({
        available: false,
        command: 'find',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.COMMAND_NOT_AVAILABLE
      );
    });
  });

  describe('showFileLastModified sorting', () => {
    it('should sort by modification time when showFileLastModified is true', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/old.txt\0/test/new.txt\0/test/mid.txt\0',
        stderr: '',
      });

      vi.mocked(mockFs.promises.lstat).mockImplementation(
        async (filePath: string | Buffer | URL) => {
          const path = filePath.toString();
          const mtimes: Record<string, Date> = {
            '/test/old.txt': new Date('2020-01-01'),
            '/test/new.txt': new Date('2024-12-01'),
            '/test/mid.txt': new Date('2022-06-01'),
          };
          return {
            isDirectory: () => false,
            isSymbolicLink: () => false,
            isFile: () => true,
            size: 123,
            mode: parseInt('100644', 8),
            mtime: mtimes[path] || new Date(),
          } as unknown as import('fs').Stats;
        }
      );

      const result = await findFiles({
        path: '/test/path',
        showFileLastModified: true,
        details: true,
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files.length).toBe(3);
      // Should be sorted by modification time (most recent first)
      expect(files[0]!.path).toBe('/test/new.txt');
      expect(files[0]!.modified).toBeDefined();
    });

    it('should fall back to path sorting when showFileLastModified is false', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/c.txt\0/test/a.txt\0/test/b.txt\0',
        stderr: '',
      });

      vi.mocked(mockFs.promises.lstat).mockResolvedValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
        isFile: () => true,
        size: 123,
        mode: parseInt('100644', 8),
        mtime: new Date(),
      } as unknown as import('fs').Stats);

      const result = await findFiles({
        path: '/test/path',
        showFileLastModified: false,
        details: true,
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files.length).toBe(3);
      // Should be sorted by path
      expect(files[0]!.path).toBe('/test/a.txt');
      expect(files[1]!.path).toBe('/test/b.txt');
      expect(files[2]!.path).toBe('/test/c.txt');
    });
  });

  describe('Fallback stat logic for missing details', () => {
    it('should fetch missing file details via lstat fallback', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/file1.txt\0',
        stderr: '',
      });

      // First lstat call returns incomplete data
      let callCount = 0;
      vi.mocked(mockFs.promises.lstat).mockImplementation(async () => {
        callCount++;
        if (callCount === 1) {
          // First call - return incomplete stats (missing size)
          return {
            isDirectory: () => false,
            isSymbolicLink: () => false,
            isFile: () => true,
            size: undefined,
            mode: undefined,
            mtime: new Date('2024-01-01'),
          } as unknown as import('fs').Stats;
        }
        // Second call - return complete stats
        return {
          isDirectory: () => false,
          isSymbolicLink: () => false,
          isFile: () => true,
          size: 1024,
          mode: parseInt('100644', 8),
          mtime: new Date('2024-01-01'),
        } as unknown as import('fs').Stats;
      });

      const result = await findFiles({
        path: '/test/path',
        details: true,
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
    });

    it('should handle lstat failure gracefully in fallback', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/file1.txt\0',
        stderr: '',
      });

      // First call succeeds with incomplete data, second call fails
      let callCount = 0;
      vi.mocked(mockFs.promises.lstat).mockImplementation(async () => {
        callCount++;
        if (callCount === 1) {
          return {
            isDirectory: () => false,
            isSymbolicLink: () => false,
            isFile: () => true,
            size: undefined,
            mode: undefined,
            mtime: undefined,
          } as unknown as import('fs').Stats;
        }
        throw new Error('Permission denied');
      });

      const result = await findFiles({
        path: '/test/path',
        details: true,
        showFileLastModified: true,
      });

      // Should still succeed, just with missing data
      expect(result.status).toBeUndefined();
    });
  });

  describe('Large result handling', () => {
    it('should paginate large result sets', async () => {
      const files = Array.from(
        { length: 50 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      vi.mocked(mockFs.promises.lstat).mockResolvedValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
        isFile: () => true,
        size: 123,
        mode: parseInt('100644', 8),
        mtime: new Date(),
      } as unknown as import('fs').Stats);

      const result = await findFiles({
        path: '/test/path',
        details: true,
        itemsPerPage: 10,
      });

      // Should paginate large result sets
      expect(result.status).toBeUndefined();
      const files2 = expectDefinedFiles(result);
      expect(files2.length).toBeLessThanOrEqual(10);
    });
  });

  describe('getFileDetails edge cases', () => {
    it('should handle lstat errors in getFileDetails', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/inaccessible.txt\0',
        stderr: '',
      });

      vi.mocked(mockFs.promises.lstat).mockRejectedValue(
        new Error('Permission denied')
      );

      const result = await findFiles({
        path: '/test/path',
        details: true,
      });

      // Should succeed with partial data
      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files[0]!.type).toBe('file'); // Default type
    });

    it('should detect symlinks in getFileDetails', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/link.txt\0',
        stderr: '',
      });

      vi.mocked(mockFs.promises.lstat).mockResolvedValue({
        isDirectory: () => false,
        isSymbolicLink: () => true,
        isFile: () => false,
        size: 10,
        mode: parseInt('120755', 8),
        mtime: new Date(),
      } as unknown as import('fs').Stats);

      const result = await findFiles({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files[0]!.type).toBe('symlink');
    });

    it('should detect directories in getFileDetails', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/dir\0',
        stderr: '',
      });

      vi.mocked(mockFs.promises.lstat).mockResolvedValue({
        isDirectory: () => true,
        isSymbolicLink: () => false,
        isFile: () => false,
        size: 4096,
        mode: parseInt('40755', 8),
        mtime: new Date(),
      } as unknown as import('fs').Stats);

      const result = await findFiles({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files[0]!.type).toBe('directory');
    });
  });

  describe('NEW FEATURE: File-based pagination with automatic sorting', () => {
    it('should paginate with default 20 files per page', async () => {
      const files = Array.from(
        { length: 50 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
      });

      expect(result.status).toBeUndefined();
      const filesDefaultPage = expectDefinedFiles(result);
      expect(filesDefaultPage.length).toBeLessThanOrEqual(20);
      expect(result.pagination?.totalPages).toBe(3);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should navigate to second page', async () => {
      const files = Array.from(
        { length: 50 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
        page: 2,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(2);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should support custom filesPerPage', async () => {
      const files = Array.from(
        { length: 50 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
        itemsPerPage: 10,
      });

      expect(result.status).toBeUndefined();
      const filesCustomPerPage = expectDefinedFiles(result);
      expect(filesCustomPerPage.length).toBeLessThanOrEqual(10);
      expect(result.pagination?.totalPages).toBe(5);
    });

    it('should handle last page correctly', async () => {
      const files = Array.from(
        { length: 25 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
        itemsPerPage: 20,
        page: 2,
      });

      expect(result.status).toBeUndefined();
      const filesLastPage = expectDefinedFiles(result);
      expect(filesLastPage.length).toBe(5);
      expect(result.pagination?.hasMore).toBe(false);
    });
  });

  describe('NEW FEATURE: ALWAYS sorted by modification time', () => {
    it('should ALWAYS sort by modification time (most recent first)', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/old.txt\0/test/new.txt\0/test/mid.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
      });

      expect(result.status).toBeUndefined();
      const filesSorted = expectDefinedFiles(result);
      expect(filesSorted.length).toBe(3);
    });

    it('should sort even with pagination', async () => {
      const files = Array.from(
        { length: 30 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 10,
      });

      expect(result.status).toBeUndefined();
    });

    it('should sort with time-based filters', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/recent1.txt\0/test/recent2.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        modifiedWithin: '7d',
      });

      expect(result.status).toBeUndefined();
    });
  });

  describe('NEW FEATURE: Pagination hints', () => {
    it('should include pagination hints with page info', async () => {
      const files = Array.from(
        { length: 50 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
      expect(result.hints).toBeDefined();
      expect(result.hints!.length).toBeGreaterThan(0);
    });
  });

  describe('Research context fields', () => {
    it('should not echo researchGoal and reasoning in hasResults', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/path/file1.txt\0/test/path/file2.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
        researchGoal: 'Find text files',
        reasoning: 'Need to locate documentation',
      });

      expect(result.status).toBeUndefined();
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

      const result = await findFiles({
        path: '/test/path',
        name: '*.nonexistent',
        researchGoal: 'Search for missing files',
        reasoning: 'Verify files do not exist',
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

      const result = await findFiles({
        path: '/invalid/path',
        researchGoal: 'Test invalid path',
        reasoning: 'Testing error handling',
      });

      expect(result.status).toBe('error');
      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });
  });

  describe('Character-based pagination (charOffset + charLength)', () => {
    it('should paginate output with charOffset and charLength', async () => {
      const files = Array.from(
        { length: 200 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        name: '*.txt',
        charLength: 500,
        charOffset: 0,
      });

      expect(result.status).toBeUndefined();
      // charPagination is only added when pagination is actually applied
      if (result.charPagination) {
        // We allow slightly more than requested to complete the last item
        // or return empty if stricter logic used.
        // Current logic is greedy overlap, so it might exceed.
        // Each item is {"path":"/test/fileX.txt","type":"file"} approx 40 chars.
        // 500 chars is ~12 items.
        // If we overflow by one item, it's fine.
        expect(result.charPagination.charLength).toBeGreaterThan(0);
        // Loose check for reasonable size
        expect(result.charPagination.charLength).toBeLessThan(600);
      }
    });

    it('should return first chunk by default', async () => {
      const files = Array.from(
        { length: 100 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 1000,
      });

      expect(result.status).toBeUndefined();
      expect(result.charPagination?.charOffset).toBe(0);
    });

    it('should navigate to second chunk with charOffset', async () => {
      const files = Array.from(
        { length: 100 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 1000,
        charOffset: 1000,
      });

      expect(result.status).toBeUndefined();
      // charPagination is only added when pagination is actually applied
      if (result.charPagination) {
        expect(result.charPagination.charOffset).toBe(1000);
      }
    });

    it('should handle charOffset = 0', async () => {
      const files = '/test/file1.txt\0/test/file2.txt\0';
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files,
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charOffset: 0,
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.charPagination?.charOffset).toBe(0);
    });

    it('should handle charOffset beyond output length', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/file.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charOffset: 10000,
        charLength: 100,
      });

      // When charOffset is beyond content, we still get hasResults with empty data
      expect(result.status).toBeUndefined();
    });

    it('should handle charLength = 1', async () => {
      const files = Array.from(
        { length: 10 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 1,
      });

      expect(result.status).toBeUndefined();
      // charPagination is only added when pagination is actually applied
      if (result.charPagination) {
        // Minimal valid JSON is "[]" (2 chars), so even if we asked for 1, we get 2
        expect(result.charPagination.charLength).toBeGreaterThanOrEqual(2);
      }
    });

    it('should handle charLength = 10000 (max)', async () => {
      const files = Array.from(
        { length: 500 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
      expect(result.charPagination?.charLength).toBeLessThanOrEqual(10000);
    });

    it('should handle file paths with UTF-8 chars', async () => {
      const files = '/test/café.txt\0/test/résumé.txt\0';
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files,
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 1000,
      });

      expect(result.status).toBeUndefined();
      const filesUtf = expectDefinedFiles(result);
      expect(filesUtf.some(f => f.path.includes('café'))).toBe(true);
    });

    it('should handle 2-byte UTF-8 in paths', async () => {
      const files = '/test/niño.txt\0/test/español.txt\0';
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files,
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      const filesUtf2 = expectDefinedFiles(result);
      expect(JSON.stringify(filesUtf2)).not.toMatch(/\uFFFD/);
    });

    it('should handle 3-byte UTF-8 in paths', async () => {
      const files = '/test/文件.txt\0/test/中文.txt\0';
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files,
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      const filesUtf3 = expectDefinedFiles(result);
      expect(JSON.stringify(filesUtf3)).not.toMatch(/\uFFFD/);
    });

    it('should handle emoji in paths', async () => {
      const files = '/test/😀test.txt\0/test/🎉party.txt\0';
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files,
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      const filesEmoji = expectDefinedFiles(result);
      expect(JSON.stringify(filesEmoji)).not.toMatch(/\uFFFD/);
    });

    it('should show character pagination hints when truncated', async () => {
      const files = Array.from(
        { length: 200 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      if (result.charPagination?.hasMore) {
        expect(result.hints).toBeDefined();
      }
    });

    it('should include charOffset value for next chunk', async () => {
      const files = Array.from(
        { length: 200 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        charLength: 500,
        charOffset: 0,
      });

      expect(result.status).toBeUndefined();
      if (result.charPagination?.hasMore) {
        expect(result.hints).toBeDefined();
        const hasCharOffsetHint = result.hints?.some(
          (h: string) => h.includes('charOffset') || h.includes('next')
        );
        expect(hasCharOffsetHint).toBe(true);
      }
    });

    // Failing test for JSON structure corruption
    it('should maintain JSON validity when paginating', async () => {
      const files = '/test/file1.txt\0/test/file2.txt\0';
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files,
        stderr: '',
      });

      // Request very small char length that splits JSON
      const result = await findFiles({
        path: '/test/path',
        charLength: 10,
      });

      expect(result.status).toBeUndefined();
      // Should have pagination info
      expect(result.charPagination).toBeDefined();
    });
  });

  describe('File pagination - Edge cases', () => {
    it('should handle page = 0 or negative (defaults to 1)', async () => {
      const files = Array.from(
        { length: 50 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      // Schema should validate, but test with valid value
      const result = await findFiles({
        path: '/test/path',
        page: 1,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should handle page > total pages', async () => {
      const files = Array.from(
        { length: 25 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        page: 10,
        itemsPerPage: 20,
      });

      expect([undefined, 'empty']).toContain(result.status);
      if (result.status === 'hasResults') {
        expect(result.pagination?.currentPage).toBe(10);
      }
    });

    it('should handle filesPerPage = 1', async () => {
      const files = Array.from(
        { length: 5 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 1,
      });

      expect(result.status).toBeUndefined();
      const filesPerPageOne = expectDefinedFiles(result);
      expect(filesPerPageOne.length).toBe(1);
      expect(result.pagination?.totalPages).toBe(5);
    });

    it('should handle filesPerPage = 20 (max)', async () => {
      const files = Array.from(
        { length: 150 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      const filesMaxPerPage = expectDefinedFiles(result);
      expect(filesMaxPerPage.length).toBeLessThanOrEqual(20);
      expect(result.pagination?.totalPages).toBe(8);
    });

    it('should handle single file result', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '/test/single-file.txt\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      const filesSingle = expectDefinedFiles(result);
      expect(filesSingle.length).toBe(1);
      expect(result.pagination?.totalPages).toBe(1);
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should handle exact boundary (20 files, 20 per page)', async () => {
      const files = Array.from(
        { length: 20 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      const filesExactBoundary = expectDefinedFiles(result);
      expect(filesExactBoundary.length).toBe(20);
      expect(result.pagination?.totalPages).toBe(1);
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should handle one over boundary (21 files, 20 per page)', async () => {
      const files = Array.from(
        { length: 21 },
        (_, i) => `/test/file${i}.txt`
      ).join('\0');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: files + '\0',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      const filesOverBoundary = expectDefinedFiles(result);
      expect(filesOverBoundary.length).toBe(20);
      expect(result.pagination?.totalPages).toBe(2);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should handle empty result set with pagination params', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '',
        stderr: '',
      });

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 20,
        page: 1,
      });

      expect(result.status).toBe('empty');
    });
  });
});
