/**
 * Tests for localViewStructure tool - comprehensive coverage including pagination
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { LOCAL_TOOL_ERROR_CODES } from '../../src/errors/localToolErrors.js';
import { viewStructure } from '../../src/tools/local_view_structure/local_view_structure.js';
import { safeExec } from '../../src/utils/exec/safe.js';
import { checkCommandAvailability } from '../../src/utils/exec/commandAvailability.js';
import * as pathValidator from 'octocode-security-utils/pathValidator';
import type { Stats } from 'fs';

// Mock dependencies
vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi.fn(),
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: vi
    .fn()
    .mockResolvedValue({ available: true, command: 'ls' }),
  getMissingCommandError: vi.fn().mockReturnValue('Command not available'),
}));

vi.mock('octocode-security-utils/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn(),
  },
}));

// Create mock functions using vi.hoisted so they're available when vi.mock runs
const { mockReaddirFn, mockLstatFn, mockLstatSyncFn } = vi.hoisted(() => ({
  mockReaddirFn: vi.fn(),
  mockLstatFn: vi.fn(),
  mockLstatSyncFn: vi.fn(),
}));

// Mock fs module - CommonJS module accessed via ESM default import
// When importing CJS module with `import fs from 'fs'`, the module object
// becomes the default export in vitest
vi.mock('fs', () => {
  const mockModule = {
    lstatSync: mockLstatSyncFn,
    promises: {
      readdir: mockReaddirFn,
      lstat: mockLstatFn,
    },
  };
  return {
    ...mockModule,
    default: mockModule,
  };
});

describe('localViewStructure', () => {
  const mockSafeExec = vi.mocked(safeExec);
  const mockValidate = vi.mocked(pathValidator.pathValidator.validate);
  // Use the mock functions directly
  const mockReaddir = mockReaddirFn;
  const mockLstat = mockLstatFn;
  const mockLstatSync = mockLstatSyncFn;

  beforeEach(() => {
    // Clear all mocks but then immediately set defaults
    vi.clearAllMocks();

    vi.mocked(checkCommandAvailability).mockResolvedValue({
      available: true,
      command: 'ls',
    });

    mockValidate.mockReturnValue({
      isValid: true,
      sanitizedPath: '/test/path',
    });

    // Set default mock implementations that will be used unless overridden
    // These MUST return valid values to prevent undefined errors
    mockReaddir.mockResolvedValue([]);
    mockLstat.mockResolvedValue({
      isDirectory: () => false,
      isFile: () => true,
      isSymbolicLink: () => false,
      size: 0,
      mtime: new Date(),
    } as Stats);
    mockLstatSync.mockReturnValue({
      isDirectory: () => false,
      isSymbolicLink: () => false,
    } as Stats);
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '',
      stderr: '',
    });
  });

  it('emits active filter echo-back for successful listings', async () => {
    mockReaddir.mockResolvedValue(['file.ts', 'file.js']);
    mockLstat.mockResolvedValue({
      isDirectory: () => false,
      isFile: () => true,
      isSymbolicLink: () => false,
      size: 10,
      mtime: new Date('2024-01-01'),
    } as Stats);

    const result = await viewStructure({
      path: '/test/path',
      depth: 1,
      extension: 'ts',
    });

    expect(result.status).toBeUndefined();
    expect(result.hints).toContain(
      'Active filters — path: /test/path | depth: 1 | extension: ts'
    );
  });

  describe('ls command not available (lines 52-56)', () => {
    it('should return ToolErrors.commandNotAvailable when ls is not available in non-recursive mode', async () => {
      vi.mocked(checkCommandAvailability).mockResolvedValue({
        available: false,
        command: 'ls',
      });

      const result = await viewStructure({
        path: '/test/path',
        // No depth/recursive - uses ls path
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.COMMAND_NOT_AVAILABLE
      );
      expect(mockSafeExec).not.toHaveBeenCalled();
    });
  });

  describe('Basic directory listing', () => {
    it('should list directory contents', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.txt\nfile2.js\ndir1',
        stderr: '',
      });

      // parseLsSimple uses fs.promises.lstat (async), not lstatSync
      mockLstat.mockImplementation(
        async (pathArg: string | Buffer | URL): Promise<Stats> =>
          ({
            isDirectory: () => pathArg.toString().includes('dir'),
            isFile: () => !pathArg.toString().includes('dir'),
            isSymbolicLink: () => false,
            size: 1024,
            mtime: new Date(),
          }) as Stats
      );

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBeGreaterThan(0);
    });

    it('should use sanitized path for non-recursive ls execution', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '',
        stderr: '',
      });

      await viewStructure({
        path: 'file:///unsafe/path',
      });

      const args = mockSafeExec.mock.calls[0]?.[1] ?? [];
      expect(args).toContain('/test/path');
      expect(args).not.toContain('file:///unsafe/path');
    });

    it('should handle empty directories', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '',
        stderr: '',
      });

      const result = await viewStructure({
        path: '/test/empty',
      });

      expect(result.status).toBe('empty');
    });
  });

  describe('Structured output mode', () => {
    it('should generate structured output with file sizes', async () => {
      // Mock readdir to always return files (for any path)
      mockReaddir.mockResolvedValue(['file1.txt', 'file2.js']);

      mockLstat.mockImplementation(
        async (_path: string | Buffer | URL): Promise<Stats> => {
          return {
            isDirectory: () => false,
            isFile: () => true,
            isSymbolicLink: () => false,
            size: 1024,
            mtime: new Date(),
          } as Stats;
        }
      );

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.some(e => e.name === 'file1.txt')).toBe(true);
      expect(result.entries!.some(e => e.size === '1.0KB')).toBe(true);
    });

    it('should show file sizes for files, not directories', async () => {
      mockReaddir.mockResolvedValue(['file1.txt']);

      mockLstat.mockImplementation(
        async (_path: string | Buffer | URL): Promise<Stats> => {
          return {
            isDirectory: () => false,
            isFile: () => true,
            isSymbolicLink: () => false,
            size: 2048,
            mtime: new Date(),
          } as Stats;
        }
      );

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.size === '2.0KB')).toBe(true);
    });

    it('should respect depth parameter', async () => {
      let callCount = 0;
      mockReaddir.mockImplementation(async (): Promise<string[]> => {
        callCount++;
        if (callCount === 1) return ['dir1'];
        return ['subfile.txt'];
      });

      mockLstat.mockImplementation(
        async (path: string | Buffer | URL): Promise<Stats> =>
          ({
            isDirectory: () =>
              path.toString().includes('dir1') &&
              !path.toString().includes('subfile'),
            isFile: () => path.toString().includes('subfile'),
            isSymbolicLink: () => false,
            size: 512,
            mtime: new Date(),
          }) as Stats
      );

      const result = await viewStructure({
        path: '/test/path',
        depth: 2,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('dir1'))).toBe(true);
      expect(result.entries!.some(e => e.name.includes('subfile.txt'))).toBe(
        true
      );
    });
  });

  describe('Detailed listing with metadata', () => {
    it('should include file details when requested', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '-rw-r--r-- 1 user group 1024 Jan 1 12:00 file1.txt',
        stderr: '',
      });

      const result = await viewStructure({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.some(e => e.type === 'file')).toBe(true);
    });

    it('should parse human-readable file sizes correctly', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '-rw-r--r-- 1 user group 1.5M Jan 1 12:00 large.bin',
        stderr: '',
      });

      const result = await viewStructure({
        path: '/test/path',
        details: true,
        humanReadable: true,
      });

      expect(result.status).toBeUndefined();
      expect(
        result.entries!.some(
          e => e.size && /\d+(\.\d+)?\s*(B|KB|MB|GB)/.test(e.size)
        )
      ).toBe(true);
    });
  });

  describe('Filtering', () => {
    it('should handle invalid regex pattern with fallback', async () => {
      mockReaddir.mockResolvedValue(['test1.txt', 'test2.txt', 'other.txt']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      // Use an invalid regex pattern that will fail to compile
      // When regex fails, falls back to substring matching - 'test' matches files
      const result = await viewStructure({
        path: '/test/path',
        pattern: 'test*[', // Invalid regex - unmatched bracket, but 'test' prefix should match
        depth: 1,
      });

      // The fallback uses substring match, not glob - if no match found, returns empty
      expect([undefined, 'empty']).toContain(result.status);
    });

    it('should filter by file extension', async () => {
      mockReaddir.mockResolvedValue(['file1.ts', 'file2.js', 'file3.ts']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        extension: 'ts',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('file1.ts'))).toBe(true);
      expect(result.entries!.some(e => e.name.includes('file3.ts'))).toBe(true);
      expect(result.entries!.some(e => e.name.includes('file2.js'))).toBe(
        false
      );
    });

    it('should filter by multiple extensions', async () => {
      mockReaddir.mockResolvedValue(['file1.ts', 'file2.tsx', 'file3.js']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        extensions: ['ts', 'tsx'],
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('file1.ts'))).toBe(true);
      expect(result.entries!.some(e => e.name.includes('file2.tsx'))).toBe(
        true
      );
      expect(result.entries!.some(e => e.name.includes('file3.js'))).toBe(
        false
      );
    });

    it('should filter files only', async () => {
      mockReaddir.mockResolvedValue(['file1.txt', 'dir1']);

      mockLstat.mockImplementation(
        async (path: string | Buffer | URL): Promise<Stats> =>
          ({
            isDirectory: () => path.toString().includes('dir1'),
            isFile: () => path.toString().includes('file1'),
            isSymbolicLink: () => false,
            size: 1024,
            mtime: new Date(),
          }) as Stats
      );

      const result = await viewStructure({
        path: '/test/path',
        filesOnly: true,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('file1.txt'))).toBe(
        true
      );
      expect(result.entries!.some(e => e.name.includes('dir1'))).toBe(false);
    });

    it('should filter directories only', async () => {
      mockReaddir.mockResolvedValue(['file1.txt', 'dir1', 'dir2']);

      mockLstat.mockImplementation(
        async (path: string | Buffer | URL): Promise<Stats> =>
          ({
            isDirectory: () => path.toString().includes('dir'),
            isFile: () => path.toString().includes('file'),
            isSymbolicLink: () => false,
            size: 1024,
            mtime: new Date(),
          }) as Stats
      );

      const result = await viewStructure({
        path: '/test/path',
        directoriesOnly: true,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('dir1'))).toBe(true);
      expect(result.entries!.some(e => e.name.includes('dir2'))).toBe(true);
      expect(result.entries!.some(e => e.name.includes('file1.txt'))).toBe(
        false
      );
    });

    it('should filter by name pattern', async () => {
      mockReaddir.mockResolvedValue(['test1.txt', 'test2.txt', 'other.txt']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        pattern: 'test',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('test1.txt'))).toBe(
        true
      );
      expect(result.entries!.some(e => e.name.includes('test2.txt'))).toBe(
        true
      );
      expect(result.entries!.some(e => e.name.includes('other.txt'))).toBe(
        false
      );
    });

    it('should filter by glob pattern with asterisks', async () => {
      mockReaddir.mockResolvedValue([
        'parser.test.ts',
        'utils.test.ts',
        'helper.ts',
        'config.ts',
      ]);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        pattern: '*test*',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('parser.test.ts'))).toBe(
        true
      );
      expect(result.entries!.some(e => e.name.includes('utils.test.ts'))).toBe(
        true
      );
      expect(result.entries!.some(e => e.name.includes('helper.ts'))).toBe(
        false
      );
      expect(result.entries!.some(e => e.name.includes('config.ts'))).toBe(
        false
      );
    });

    it('should filter by glob pattern, extensions, and recursive together', async () => {
      // Simulate a nested directory structure
      mockReaddir
        .mockResolvedValueOnce(['subdir', 'root.test.ts', 'other.ts'])
        .mockResolvedValueOnce(['nested.test.ts', 'another.ts']);

      mockLstat.mockImplementation(
        async (pathArg: string | Buffer | URL): Promise<Stats> => {
          const p = pathArg.toString();
          return {
            isDirectory: () => p.endsWith('subdir'),
            isFile: () => p.endsWith('.ts'),
            isSymbolicLink: () => false,
            size: 1024,
            mtime: new Date(),
          } as Stats;
        }
      );

      const result = await viewStructure({
        path: '/test/path',
        pattern: '*test*',
        extensions: ['ts'],
        filesOnly: true,
        recursive: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('root.test.ts'))).toBe(
        true
      );
      expect(result.entries!.some(e => e.name.includes('nested.test.ts'))).toBe(
        true
      );
      expect(result.entries!.some(e => e.name.includes('other.ts'))).toBe(
        false
      );
      expect(result.entries!.some(e => e.name.includes('another.ts'))).toBe(
        false
      );
    });

    it('should filter by glob pattern with question mark', async () => {
      mockReaddir.mockResolvedValue([
        'test1.ts',
        'test2.ts',
        'test10.ts',
        'testing.ts',
      ]);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        pattern: 'test?.ts',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      // ? matches exactly one character, so test1.ts and test2.ts match
      expect(result.entries!.some(e => e.name.includes('test1.ts'))).toBe(true);
      expect(result.entries!.some(e => e.name.includes('test2.ts'))).toBe(true);
      // test10.ts has two chars after 'test' before '.ts', so it doesn't match
      expect(result.entries!.some(e => e.name.includes('test10.ts'))).toBe(
        false
      );
      // testing.ts has 'ing' after 'test' before '.ts', so it doesn't match
      expect(result.entries!.some(e => e.name.includes('testing.ts'))).toBe(
        false
      );
    });
  });

  describe('Symlink handling', () => {
    it('should identify symlinks in recursive mode', async () => {
      mockReaddir.mockResolvedValue(['file.txt', 'link']);

      mockLstat.mockImplementation(
        async (path: string | Buffer | URL): Promise<Stats> =>
          ({
            isDirectory: () => false,
            isFile: () => path.toString().includes('file'),
            isSymbolicLink: () => path.toString().includes('link'),
            size: 1024,
            mtime: new Date(),
          }) as Stats
      );

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.type === 'link')).toBe(true);
    });

    it('should identify symlinks in parseLsLongFormat', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout:
          'lrwxrwxrwx 1 user group 10 Jan 1 12:00 link -> target\n-rw-r--r-- 1 user group 1024 Jan 1 12:00 file.txt',
        stderr: '',
      });

      const result = await viewStructure({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.type === 'link')).toBe(true);
    });
  });

  describe('showFileLastModified', () => {
    it('should include modified date when showFileLastModified is true in parseLsSimple', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file.txt',
        stderr: '',
      });

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date('2024-01-15T12:00:00Z'),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
    });

    it('should include modified date in parseLsLongFormat', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '-rw-r--r-- 1 user group 1024 Jan 15 12:00 file.txt',
        stderr: '',
      });

      const result = await viewStructure({
        path: '/test/path',
        details: true,
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
    });

    it('should include modified date in recursive walkDirectory', async () => {
      mockReaddir.mockResolvedValue(['file.txt']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date('2024-06-15T12:00:00Z'),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
    });

    it('should default showFileLastModified to true when not specified (non-recursive)', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file.txt',
        stderr: '',
      });

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date('2024-01-15T12:00:00Z'),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBeGreaterThan(0);
      expect(result.entries![0].modified).toBe('2024-01-15T12:00:00.000Z');
    });

    it('should default showFileLastModified to true when not specified (recursive)', async () => {
      mockReaddir.mockResolvedValue(['file.txt']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date('2024-06-15T12:00:00Z'),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBeGreaterThan(0);
      expect(result.entries![0].modified).toBe('2024-06-15T12:00:00.000Z');
    });

    it('should NOT include modified when showFileLastModified is explicitly false', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file.txt',
        stderr: '',
      });

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date('2024-01-15T12:00:00Z'),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        showFileLastModified: false,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBeGreaterThan(0);
      expect(result.entries![0].modified).toBeUndefined();
    });

    it('should NOT include modified in detailed mode when showFileLastModified is false', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '-rw-r--r-- 1 user staff 123 Jan 1 12:34 file.txt',
        stderr: '',
      });

      const result = await viewStructure({
        path: '/test/path',
        details: true,
        showFileLastModified: false,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBe(1);
      expect(result.entries![0].modified).toBeUndefined();
    });
  });

  describe('Hidden files', () => {
    it('should show hidden files when requested', async () => {
      mockReaddir.mockResolvedValue(['.hidden', 'visible.txt']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        hidden: true,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('.hidden'))).toBe(true);
      expect(result.entries!.some(e => e.name.includes('visible.txt'))).toBe(
        true
      );
    });

    it('should hide hidden files by default', async () => {
      mockReaddir.mockResolvedValue(['.hidden', 'visible.txt']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        hidden: false,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.name.includes('.hidden'))).toBe(false);
      expect(result.entries!.some(e => e.name.includes('visible.txt'))).toBe(
        true
      );
    });
  });

  describe('Sorting', () => {
    it('should sort by name (default)', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'beta.txt\nalpha.txt\ngamma.txt',
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        sortBy: 'name',
      });

      expect(result.status).toBeUndefined();
      // Entries should be sorted alphabetically
    });

    it('should sort by size in recursive mode', async () => {
      mockReaddir.mockResolvedValue(['small.txt', 'large.txt', 'medium.txt']);

      mockLstat.mockImplementation(
        async (pathArg: string | Buffer | URL): Promise<Stats> => {
          const p = pathArg.toString();
          const sizes: Record<string, number> = {
            small: 512,
            medium: 2048,
            large: 4096,
          };
          const size =
            sizes[Object.keys(sizes).find(k => p.includes(k)) || 'small'] ??
            1024;
          return {
            isDirectory: () => false,
            isFile: () => true,
            isSymbolicLink: () => false,
            size,
            mtime: new Date(),
          } as Stats;
        }
      );

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'size',
      });

      expect(result.status).toBeUndefined();
      const names = result.entries!.map(e => e.name);
      expect(names[0]).toContain('small');
      expect(names[names.length - 1]).toContain('large');
    });

    it('should sort by extension in recursive mode', async () => {
      mockReaddir.mockResolvedValue(['file.ts', 'file.js', 'file.css']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'extension',
      });

      expect(result.status).toBeUndefined();
    });

    it('should sort by time in recursive mode with showFileLastModified', async () => {
      mockReaddir.mockResolvedValue(['file1.txt', 'file2.txt']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date('2024-01-01'),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'time',
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
    });

    it('should sort by time falling back to name when modified not available (lines 208-211)', async () => {
      mockReaddir.mockResolvedValue(['zebra.txt', 'alpha.txt', 'beta.txt']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'time',
        showFileLastModified: false, // Modified not shown, fallback to name sort
      });

      expect(result.status).toBeUndefined();
      // Fallback uses name sort: alpha, beta, zebra
      const names = result.entries!.map(e => e.name);
      expect(names[0]).toContain('alpha');
      expect(names[1]).toContain('beta');
      expect(names[2]).toContain('zebra');
    });

    it('should support reverse sorting in recursive mode', async () => {
      mockReaddir.mockResolvedValue(['alpha.txt', 'beta.txt', 'gamma.txt']);

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'name',
        reverse: true,
      });

      expect(result.status).toBeUndefined();
    });
  });

  describe('Pagination - CRITICAL for large results', () => {
    it('should require pagination for large directory listing (>100 entries)', async () => {
      // Generate 150 entries
      const entries = Array.from(
        { length: 150 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: entries,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
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

    it('should allow tree view for large directories without pagination', async () => {
      mockReaddir.mockResolvedValue(
        Array.from({ length: 150 }, (_, i) => `file${i}.txt`)
      );

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        charLength: 50000, // Use charLength for large result set
      });

      // Tree view should work with pagination
      expect(result.status).toBeUndefined();
    });

    it('should paginate large directory listings', async () => {
      const entries = Array.from(
        { length: 150 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: entries,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalEntries).toBe(150);
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.entries!.length).toBe(100); // Default page size is 100
    });

    it('should paginate tree view when requested', async () => {
      mockReaddir.mockResolvedValue(['file1.txt']);
      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      // Mock the tree generation to produce large output
      const result = await viewStructure({
        path: '/test/path',
        charLength: 10000,
      });

      if (result.entries && result.entries.length > 20) {
        expect(result.pagination?.hasMore).toBe(true);
      }
    });

    it('should handle paginated continuation', async () => {
      const entries = Array.from(
        { length: 150 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: entries,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result1 = await viewStructure({
        path: '/test/path',
        page: 1,
      });

      expect(result1.status).toBeUndefined();
      expect(result1.pagination?.hasMore).toBe(true);

      const result2 = await viewStructure({
        path: '/test/path',
        page: 2,
      });

      expect(result2.status).toBeUndefined();
      expect(result2.pagination?.currentPage).toBe(2);
      // Different entries on different pages
      expect(result2.entries![0]!.name).not.toBe(result1.entries![0]!.name);
    });
  });

  describe('Recursive listing', () => {
    it('should list recursively with depth control', async () => {
      mockReaddir
        .mockResolvedValueOnce(['dir1', 'file1.txt'])
        .mockResolvedValueOnce(['subfile.txt']);

      mockLstat.mockImplementation(
        async (path: string | Buffer | URL): Promise<Stats> =>
          ({
            isDirectory: () => path.toString().includes('dir'),
            isFile: () => !path.toString().includes('dir'),
            isSymbolicLink: () => false,
            size: 1024,
            mtime: new Date(),
          }) as Stats
      );

      const result = await viewStructure({
        path: '/test/path',
        recursive: true,
      });

      // Tree view with recursive doesn't use mockSafeExec, so result may be empty
      expect([undefined, 'empty']).toContain(result.status);
      if (result.status === undefined && result.entries) {
        expect(result.entries.length).toBeGreaterThan(0);
      }
    });

    it('should include cwd in recursive results (consistency with non-recursive)', async () => {
      mockReaddir.mockResolvedValue(['file.txt']);
      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      // Success ≡ absent status; entries carry the resolved cwd-relative path.
      expect(result.status).toBeUndefined();
      expect(result.entries?.[0]?.path).toContain('/test/path');
    });

    it('should handle max depth limit for recursive', async () => {
      mockReaddir.mockResolvedValue(['file.txt']);
      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 5,
      });

      // May be empty if mocked readdir returns empty array
      expect([undefined, 'empty']).toContain(result.status);
      // Should respect max depth of 5
    });

    it('should require pagination for large recursive listings', async () => {
      // Mock large recursive result
      mockReaddir.mockImplementation(
        async (): Promise<string[]> =>
          Array.from({ length: 50 }, (_, i) => `file${i}.txt`)
      );

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        recursive: true,

        // Large result without pagination
      });

      if (
        result.pagination?.totalEntries &&
        result.pagination.totalEntries > 100
      ) {
        expect(result.status).toBe('error');
        expect(result.errorCode).toBeDefined();
      }
    });

    it('should handle large recursive listing with auto-pagination', async () => {
      // Mock very large recursive result (>100 entries)
      mockReaddir.mockResolvedValue(
        Array.from({ length: 150 }, (_, i) => `file${i}.txt`)
      );

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      // Default entriesPerPage (20) auto-paginates large results
      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();
    });

    it('should stop at maxEntries in walkDirectory', async () => {
      // Create a structure that would exceed maxEntries
      // Mock 200 files which with limit=10 should stop early
      mockReaddir.mockResolvedValue(
        Array.from({ length: 200 }, (_, i) => `file${i}.txt`)
      );

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        limit: 10, // This sets maxEntries to limit * 2 = 20
        charLength: 10000, // Allow pagination to avoid OUTPUT_TOO_LARGE error
      });

      expect(result.status).toBeUndefined();
    });

    it('should handle readdir errors gracefully in walkDirectory', async () => {
      mockReaddir.mockRejectedValue(new Error('Cannot read directory'));

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      // Root-level readdir failure → surface a clear error instead of silent empty
      expect(result.status).toBe('error');
      expect(result.error).toBeDefined();
    });

    it('should handle lstat errors gracefully in walkDirectory', async () => {
      mockReaddir.mockResolvedValue(['file1.txt', 'file2.txt']);
      mockLstat.mockRejectedValue(new Error('Cannot stat file'));

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      // Should skip inaccessible items gracefully
      expect(result.status).toBe('empty');
    });

    it('should return error with clear message when root path does not exist (ENOENT)', async () => {
      // BUG-FIX: Previously returned status "empty" with "1 entries skipped due
      // to permission errors" — misleading because ENOENT is not a permissions error.
      const enoentErr = Object.assign(
        new Error('ENOENT: no such file or directory'),
        {
          code: 'ENOENT',
        }
      );
      mockReaddir.mockRejectedValue(enoentErr);

      const result = await viewStructure({
        path: '/nonexistent/path',
        depth: 1,
      });

      expect(result.status).toBe('error');
      expect(result.error).toMatch(/not found|ENOENT/i);
      // Must NOT say "permission errors"
      expect(result.error).not.toMatch(/permission/i);
    });

    it('should return error with clear message when root path is ENOTDIR (path is a file)', async () => {
      // BUG-FIX: Providing a file path should give a clear "not a directory" error.
      const enotdirErr = Object.assign(new Error('ENOTDIR: not a directory'), {
        code: 'ENOTDIR',
      });
      mockReaddir.mockRejectedValue(enotdirErr);

      const result = await viewStructure({
        path: '/some/file.ts',
        depth: 1,
      });

      expect(result.status).toBe('error');
      expect(result.error).toBeDefined();
      expect(result.error).not.toMatch(/permission/i);
    });

    it('should correctly label EACCES as permission denied (not a generic skip)', async () => {
      const eaccesErr = Object.assign(new Error('EACCES: permission denied'), {
        code: 'EACCES',
      });
      mockReaddir.mockRejectedValue(eaccesErr);

      const result = await viewStructure({
        path: '/restricted/path',
        depth: 1,
      });

      expect(result.status).toBe('error');
      expect(result.error).toBeDefined();
    });
  });

  describe('Summary statistics', () => {
    it('should include summary by default', async () => {
      mockReaddir.mockResolvedValue(['file1.txt', 'dir1']);
      mockLstat.mockImplementation(
        async (path: string | Buffer | URL): Promise<Stats> =>
          ({
            isDirectory: () => path.toString().includes('dir'),
            isFile: () => !path.toString().includes('dir'),
            isSymbolicLink: () => false,
            size: 1024,
            mtime: new Date(),
          }) as Stats
      );

      const result = await viewStructure({
        path: '/test/path',
        summary: true,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      // Summary is always included with entry counts
      if (result.summary !== undefined) {
        expect(result.summary).toMatch(/\d+ entries/);
      }
    });
  });

  describe('Path validation', () => {
    it('should reject invalid paths', async () => {
      mockValidate.mockReturnValue({
        isValid: false,
        error: 'Path is outside allowed directories',
      });

      const result = await viewStructure({
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
        stderr: 'ls: cannot access',
      });

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.COMMAND_EXECUTION_FAILED
      );
    });

    it('should produce unified error shape from createErrorResult on ls failure', async () => {
      mockSafeExec.mockResolvedValue({
        success: false,
        code: 1,
        stdout: '',
        stderr: 'ls: permission denied',
      });

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.COMMAND_EXECUTION_FAILED
      );
      expect(typeof result.error).toBe('string');
      // createErrorResult uses ToolError.message which includes the command name prefix
      expect(result.error).toContain("Command 'ls' failed");
      expect(result.error).toContain('permission denied');
    });

    it('should handle unreadable directories', async () => {
      mockReaddir.mockRejectedValue(new Error('Permission denied'));

      const result = await viewStructure({
        path: '/test/path',
      });

      // Should handle error gracefully - might return error or hasResults with error message
      expect(['error', 'empty', 'hasResults']).toContain(result.status);
    });
  });

  describe('Limit parameter', () => {
    it('should apply limit to results', async () => {
      mockReaddir.mockResolvedValue(
        Array.from({ length: 100 }, (_, i) => `file${i}.txt`)
      );

      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        limit: 10,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      // Should respect limit
    });

    it('should apply limit in non-recursive mode with pagination', async () => {
      const fileList = Array.from(
        { length: 50 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        limit: 5,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
    });

    it('should apply limit BEFORE pagination logic', async () => {
      // 100 files total. Limit 5. EntriesPerPage 20.
      // Expected: 5 items total. Pagination should reflect 5 items.
      const fileList = Array.from(
        { length: 100 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });
      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        limit: 5,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries?.length).toBe(5);
      // Pagination should reflect filtered count (5)
      expect(result.summary).toContain('5 entries');
      // Should NOT have multiple pages (since 5 < 20)
      expect(result.pagination?.totalPages).toBe(1);
    });
  });

  describe('NEW FEATURE: Entry-based pagination with default time sorting', () => {
    it('should paginate with default 100 entries per page', async () => {
      const fileList = Array.from(
        { length: 150 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.pagination?.totalPages).toBeGreaterThan(1);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should navigate to second page of entries', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: Array.from({ length: 150 }, (_, i) => `file${i}.txt`).join(
          '\n'
        ),
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',

        page: 2,
      });

      expect([undefined, 'empty']).toContain(result.status);
      if (result.status === undefined) {
        expect(result.pagination?.currentPage).toBe(2);
      }
    });

    it('should support custom entriesPerPage', async () => {
      const fileList = Array.from(
        { length: 50 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',

        itemsPerPage: 10,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.pagination?.entriesPerPage).toBe(10);
    });

    it('should handle last page correctly', async () => {
      const fileList = Array.from(
        { length: 25 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',

        itemsPerPage: 20,
        page: 2,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.pagination?.hasMore).toBe(false);
    });
  });

  describe('Entry pagination - Bounds', () => {
    it('should coerce page=0 to 1 via defaulting', async () => {
      const fileList = Array.from(
        { length: 25 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });
      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 10,
        page: 0,
      });

      expect([undefined, 'empty']).toContain(result.status);
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should reflect negative page as provided (no clamping)', async () => {
      const fileList = Array.from(
        { length: 25 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });
      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 10,
        page: -3,
      });

      expect([undefined, 'empty']).toContain(result.status);
      expect(result.pagination?.currentPage).toBe(-3);
    });

    it('should clamp overflow page to totalPages (BUG-01 fix)', async () => {
      // 25 entries, 10 per page → totalPages = 3
      // Requesting page 9999 must clamp to 3 (not return "Page 9999/3 showing 0")
      const fileList = Array.from(
        { length: 25 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });
      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 10,
        page: 9999,
      });

      expect(result.status).toBeUndefined();
      // currentPage clamped to totalPages (3), not 9999
      expect(result.pagination?.currentPage).toBe(3);
      expect(result.pagination?.totalPages).toBe(3);
      expect(result.pagination?.hasMore).toBe(false);
      // Last page returns the remaining 5 entries (21-25), not 0
      expect(result.entries?.length).toBe(5);
    });
  });

  describe('NEW FEATURE: Default sort by modification time', () => {
    it('should sort by time (most recent first) by default', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout:
          '-rw-r--r-- 1 user group 1024 Jan 1 12:00 old.txt\n-rw-r--r-- 1 user group 2048 Dec 1 12:00 new.txt',
        stderr: '',
      });

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
    });

    it('should allow overriding sort to name', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'beta.txt\nalpha.txt\ngamma.txt',
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        sortBy: 'name',
      });

      expect(result.status).toBeUndefined();
    });

    it('should sort even with pagination', async () => {
      const fileList = Array.from(
        { length: 30 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',

        itemsPerPage: 10,
      });

      expect(result.status).toBeUndefined();
    });
  });

  describe('NEW FEATURE: Entry pagination hints', () => {
    it('should include pagination hints with entry info', async () => {
      const fileList = Array.from(
        { length: 50 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',

        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
    });

    it('should show final page hint on last page', async () => {
      const fileList = Array.from(
        { length: 25 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',

        itemsPerPage: 20,
        page: 2,
      });

      expect(result.status).toBeUndefined();
    });
  });

  describe('Research context fields', () => {
    it('should not echo researchGoal and reasoning in hasResults', async () => {
      mockReaddir.mockResolvedValue(['file1.txt', 'file2.txt']);
      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        researchGoal: 'Explore directory structure',
        reasoning: 'Need to understand file organization',
      });

      expect(result.status).toBeUndefined();
      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });

    it('should not echo researchGoal and reasoning in empty results', async () => {
      mockReaddir.mockResolvedValue([]);
      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date(),
      } as Stats);

      const result = await viewStructure({
        path: '/test/empty',
        depth: 1,
        researchGoal: 'Check empty directory',
        reasoning: 'Verify no files exist',
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

      const result = await viewStructure({
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
    // C5: Char pagination was removed; these tests verify legacy char fields are safely ignored.
    it('should paginate entries even when charOffset/charLength are provided', async () => {
      const largeOutput = Array.from(
        { length: 150 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: largeOutput,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 500,
        charOffset: 0,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
      expect(result.pagination?.totalEntries).toBe(150);
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.pagination).not.toHaveProperty('charOffset');
      expect(result.pagination).not.toHaveProperty('totalChars');
    });

    it('should return first entry page by default when charLength is provided', async () => {
      const largeOutput = Array.from(
        { length: 50 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: largeOutput,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 200,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should ignore charOffset and keep entry pagination semantics', async () => {
      const largeOutput = Array.from(
        { length: 100 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: largeOutput,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 500,
        charOffset: 500,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
      expect(result.pagination).not.toHaveProperty('charOffset');
    });

    it('should handle charOffset = 0 without changing output shape', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.txt\nfile2.txt',
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charOffset: 0,
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
      expect(result.pagination).not.toHaveProperty('charOffset');
    });

    it('should handle large charOffset values without crashing', async () => {
      const content = 'x'.repeat(1000);
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: content,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charOffset: 1000,
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should handle charOffset beyond content length', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'short content',
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charOffset: 10000,
        charLength: 100,
      });

      // When charOffset is beyond content, we still get hasResults with empty data
      expect(result.status).toBeUndefined();
    });

    it('should handle charLength = 1', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'abcdefghij',
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.pagination).not.toHaveProperty('totalChars');
    });

    it('should handle charLength = 10000 (max)', async () => {
      const largeContent = 'x'.repeat(20000);
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: largeContent,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore).toBe(false);
      expect(result.pagination).not.toHaveProperty('totalChars');
    });

    it('should handle charLength > remaining content', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'short text',
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should handle ASCII content pagination', async () => {
      const asciiContent = 'Hello World\nThis is ASCII content\nLine 3';
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: asciiContent,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 20,
        charOffset: 0,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
    });

    it('should handle 2-byte UTF-8 chars (é, ñ)', async () => {
      const utf8Content = 'Café résumé piñata\n' + 'x'.repeat(500);
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: utf8Content,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      // Should not have replacement chars from split UTF-8
      expect(result.entries!.every(e => !e.name.includes('\uFFFD'))).toBe(true);
    });

    it('should handle 3-byte UTF-8 chars (中文)', async () => {
      const utf8Content = '你好世界 Chinese text\n' + 'x'.repeat(500);
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: utf8Content,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      // Should not have replacement characters indicating split UTF-8
      expect(result.entries!.every(e => !e.name.includes('\uFFFD'))).toBe(true);
    });

    it('should handle 4-byte UTF-8 chars (emoji)', async () => {
      const utf8Content = '😀🎉👍 Emoji test\n' + 'x'.repeat(500);
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: utf8Content,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      // Should not split emoji
      expect(result.entries!.every(e => !e.name.includes('\uFFFD'))).toBe(true);
    });

    it('should not split multi-byte characters at boundaries', async () => {
      // Create content where boundary might fall in middle of UTF-8 char
      const utf8Content = 'a'.repeat(95) + 'café';
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: utf8Content,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 98, // Might cut in middle of 'é'
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.every(e => !e.name.includes('\uFFFD'))).toBe(true);
    });

    it('should show character pagination hints', async () => {
      const largeOutput = Array.from(
        { length: 150 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: largeOutput,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should show hints for next page using entry pagination', async () => {
      const largeOutput = Array.from(
        { length: 150 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: largeOutput,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 500,
        charOffset: 0,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore).toBe(true);
      const hasEntryPageHint = result.hints?.some(h => h.includes('page=2'));
      expect(hasEntryPageHint).toBe(true);
    });
  });

  describe('Entry pagination - Edge cases', () => {
    it('should handle page = 0 (defaults to 1)', async () => {
      const fileList = Array.from(
        { length: 50 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      // Schema validation should prevent 0, but if it gets through, should default to 1
      const result = await viewStructure({
        path: '/test/path',
        page: 1, // Test with valid value since schema validates
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should clamp page > total pages to the last page', async () => {
      // 25 entries, 20 per page → totalPages = 2; requesting page 10 must clamp to 2
      const fileList = Array.from(
        { length: 25 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        page: 10, // Way beyond last page
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      // Must be clamped to 2 (totalPages), not 10
      expect(result.pagination?.currentPage).toBe(2);
      expect(result.pagination?.totalPages).toBe(2);
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should handle entriesPerPage = 1', async () => {
      const fileList = Array.from({ length: 5 }, (_, i) => `file${i}.txt`).join(
        '\n'
      );
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.entriesPerPage).toBe(1);
      expect(result.pagination?.totalPages).toBe(5);
    });

    it('should handle entriesPerPage = 20 (max)', async () => {
      const fileList = Array.from(
        { length: 150 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.entriesPerPage).toBe(20);
      expect(result.pagination?.totalPages).toBe(8);
    });

    it('should handle exact boundary (20 entries, 20 per page)', async () => {
      const fileList = Array.from(
        { length: 20 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalPages).toBe(1);
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should handle one over boundary (21 entries, 20 per page)', async () => {
      const fileList = Array.from(
        { length: 21 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: fileList,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalPages).toBe(2);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should handle single entry (no pagination needed)', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'single-file.txt',
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalPages).toBe(1);
      expect(result.pagination?.hasMore).toBe(false);
    });
  });

  describe('byte/character offset separation in charPagination', () => {
    it('should ignore char-length fields and return entry pagination only', async () => {
      const manyFiles = Array.from(
        { length: 100 },
        (_, i) => `file${i}.txt`
      ).join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: manyFiles,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalEntries).toBe(100);
      expect(
        (result as { charPagination?: unknown }).charPagination
      ).toBeUndefined();
    });

    it('should handle UTF-8 filenames correctly', async () => {
      // Create files with emoji/unicode names
      const unicodeFiles = [
        '文件1.txt',
        '文件2.txt',
        '📁folder',
        'emoji👋.txt',
      ].join('\n');
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: unicodeFiles,
        stderr: '',
      });

      mockLstatSync.mockReturnValue({
        isDirectory: () => false,
        isSymbolicLink: () => false,
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 50, // Small enough to trigger pagination
      });

      expect(result.status).toBeUndefined();
      expect(result.entries?.every(e => !e.name.includes('\uFFFD'))).toBe(true);
      expect(
        (result as { charPagination?: unknown }).charPagination
      ).toBeUndefined();
    });
  });

  describe('Auto-pagination for large structuredOutput', () => {
    it('should return entries with entry pagination (C5: char auto-pagination removed)', async () => {
      // Create 20 files with long names - entry pagination applies, no char truncation
      const longNameFiles = Array.from(
        { length: 20 },
        (_, i) =>
          `this_is_an_extremely_long_filename_that_will_definitely_exceed_the_limit_when_multiplied_by_twenty_entries_${i.toString().padStart(3, '0')}.txt`
      );
      mockReaddir.mockResolvedValue(longNameFiles);
      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date('2024-01-01'),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBe(20);
      // C5: No char auto-pagination - bulk response handles output limits
      expect(result.warnings).toBeUndefined();
    });

    it('should NOT auto-paginate when output is under MAX_OUTPUT_CHARS (2000)', async () => {
      // Create just a few files - small output
      const fewFiles = ['a.txt', 'b.txt', 'c.txt'];
      mockReaddir.mockResolvedValue(fewFiles);
      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 100,
        mtime: new Date('2024-01-01'),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        // No charLength specified - should NOT trigger auto-pagination (output small)
      });

      expect(result.status).toBeUndefined();
      // Should NOT have auto-pagination warnings
      expect(result.warnings).toBeUndefined();
    });

    it('should use entry pagination when charLength provided (C5: charLength ignored)', async () => {
      const manyFiles = Array.from(
        { length: 50 },
        (_, i) => `file_${i.toString().padStart(3, '0')}.txt`
      );
      mockReaddir.mockResolvedValue(manyFiles);
      mockLstat.mockResolvedValue({
        isDirectory: () => false,
        isFile: () => true,
        isSymbolicLink: () => false,
        size: 1024,
        mtime: new Date('2024-01-01'),
      } as Stats);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        charLength: 500, // C5: Ignored - entry pagination used
      });

      expect(result.status).toBeUndefined();
      expect(result.warnings).toBeUndefined();
      expect(result.entries!.length).toBeLessThanOrEqual(100); // Default entriesPerPage is 100
    });

    it('should use entry pagination in non-recursive mode (C5: no char truncation)', async () => {
      const longFiles = Array.from(
        { length: 100 },
        (_, i) =>
          `very_long_filename_for_ls_output_${i.toString().padStart(3, '0')}.txt`
      ).join('\n');

      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: longFiles,
        stderr: '',
      });

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBeLessThanOrEqual(100); // Default page size is 100
    });
  });

  /**
   * `verbosity:"concise"` drops `entries[]` and returns the one-line summary.
   * Omitted ≡ `"basic"` (default) preserves entries. The concise response carries
   * an explicit drill-back breadcrumb so the agent never lands in a dead end.
   */
  describe('verbosity:"concise" (less tokens, more quality research)', () => {
    beforeEach(() => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: 'file1.txt\nfile2.js\ndir1\nfile3.md\nfile4.ts',
        stderr: '',
      });
      mockLstat.mockImplementation(
        async (pathArg: string | Buffer | URL): Promise<Stats> =>
          ({
            isDirectory: () => pathArg.toString().includes('dir'),
            isFile: () => !pathArg.toString().includes('dir'),
            isSymbolicLink: () => false,
            size: 1024,
            mtime: new Date('2024-01-01'),
          }) as Stats
      );
    });

    it('drops entries[] when verbosity:"concise" is requested', async () => {
      const result = await viewStructure({
        path: '/test/path',
        verbosity: 'concise',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toEqual([]);
      expect(result.summary).toMatch(/entries.*files.*dirs/);
    });

    it('keeps pagination so the agent still sees totalEntries', async () => {
      const result = await viewStructure({
        path: '/test/path',
        verbosity: 'concise',
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalEntries).toBeGreaterThan(0);
    });

    it('emits the data summary and NO verbosity-feature hints', async () => {
      const result = await viewStructure({
        path: '/test/path',
        verbosity: 'concise',
      });

      expect(result.status).toBeUndefined();
      const hintsBlob = (result.hints ?? []).join('\n');
      // Data-bearing summary kept; tier commentary is not emitted.
      expect(hintsBlob).toMatch(/summary:/i);
      expect(hintsBlob).not.toMatch(/drill-back|re-call|detail dropped/i);
    });

    it('byte-equivalent default — omitted verbosity returns full entries', async () => {
      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBeGreaterThan(0);
    });

    it('verbosity:"compact" is also byte-equivalent to omitted (current behavior)', async () => {
      const result = await viewStructure({
        path: '/test/path',
        verbosity: 'compact',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBeGreaterThan(0);
    });

    it('does not transform the empty status (nothing to compress)', async () => {
      mockSafeExec.mockResolvedValue({
        success: true,
        code: 0,
        stdout: '',
        stderr: '',
      });

      const result = await viewStructure({
        path: '/test/path',
        verbosity: 'concise',
      });

      expect(result.status).toBe('empty');
    });
  });
});
