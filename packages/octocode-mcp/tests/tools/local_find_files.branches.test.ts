/**
 * Branch coverage tests for local_find_files/findFiles.ts
 * Targets: sortBy 'size' and 'name' branches (lines 134-139)
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { findFiles } from '../../src/tools/local_find_files/findFiles.js';
import { safeExec } from '../../src/utils/exec/safe.js';
import * as pathValidator from 'octocode-security-utils/pathValidator';

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

vi.mock('fs', () => {
  const lstat = vi.fn();
  return {
    promises: { lstat },
    default: { promises: { lstat } },
  };
});

const mockFs = vi.mocked(await import('fs')) as unknown as {
  promises: { lstat: ReturnType<typeof vi.fn> };
};

const mockSafeExec = vi.mocked(safeExec);
const mockValidate = vi.mocked(pathValidator.pathValidator.validate);

describe('findFiles sortBy branches', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockValidate.mockReturnValue({
      isValid: true,
      sanitizedPath: '/test',
    });
  });

  it('should sort by size descending when sortBy is "size"', async () => {
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '/test/big.ts\0/test/small.ts\0/test/medium.ts\0',
      stderr: '',
    });

    mockFs.promises.lstat.mockImplementation(async (p: any) => {
      const sizes: Record<string, number> = {
        '/test/big.ts': 5000,
        '/test/small.ts': 100,
        '/test/medium.ts': 2000,
      };
      return {
        isFile: () => true,
        isDirectory: () => false,
        isSymbolicLink: () => false,
        size: sizes[String(p)] ?? 0,
        mtime: new Date('2024-01-01'),
      };
    });

    const result = await findFiles({
      path: '/test',
      sortBy: 'size',
      details: true,
    });

    expect(result.status).toBeUndefined();
    const files = result.files!;
    // sortBy='size' still sorts by underlying size, but the response field
    // is `sizeFormatted` (human-readable) — raw `size` was dropped to remove
    // redundancy. Verify ordering via sizeFormatted.
    expect(files[0]!.sizeFormatted).toBe('4.9KB');
    expect(files[1]!.sizeFormatted).toBe('2.0KB');
    expect(files[2]!.sizeFormatted).toBe('100.0B');
  });

  it('should sort by name alphabetically when sortBy is "name"', async () => {
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '/test/charlie.ts\0/test/alpha.ts\0/test/bravo.ts\0',
      stderr: '',
    });

    mockFs.promises.lstat.mockImplementation(async () => {
      return {
        isFile: () => true,
        isDirectory: () => false,
        isSymbolicLink: () => false,
        size: 100,
        mtime: new Date('2024-01-01'),
      };
    });

    const result = await findFiles({
      path: '/test',
      sortBy: 'name',
      details: true,
    });

    expect(result.status).toBeUndefined();
    const files = result.files!;
    expect(files[0]!.path).toContain('alpha');
    expect(files[1]!.path).toContain('bravo');
    expect(files[2]!.path).toContain('charlie');
  });

  it('should sort by path when sortBy is "path"', async () => {
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '/test/z/file.ts\0/test/a/file.ts\0/test/m/file.ts\0',
      stderr: '',
    });

    mockFs.promises.lstat.mockImplementation(async () => {
      return {
        isFile: () => true,
        isDirectory: () => false,
        isSymbolicLink: () => false,
        size: 100,
        mtime: new Date('2024-01-01'),
      };
    });

    const result = await findFiles({
      path: '/test',
      sortBy: 'path',
      details: true,
    });

    expect(result.status).toBeUndefined();
    const files = result.files!;
    expect(files[0]!.path).toContain('/a/');
    expect(files[1]!.path).toContain('/m/');
    expect(files[2]!.path).toContain('/z/');
  });

  it('should return error when safeExec returns success: false (find command fails)', async () => {
    mockSafeExec.mockResolvedValue({
      success: false,
      code: 1,
      stdout: '',
      stderr: 'find: /nonexistent: No such file or directory',
    });

    const result = await findFiles({
      path: '/test',
    });

    expect(result.status).toBe('error');
    expect(result.error).toBeDefined();
  });

  it('should sort by modified when showLastModified and both files have modified (line 158)', async () => {
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '/test/old.ts\0/test/new.ts\0',
      stderr: '',
    });

    mockFs.promises.lstat.mockImplementation(async (p: unknown) => {
      const path = String(p);
      const mtimes: Record<string, Date> = {
        '/test/old.ts': new Date('2020-01-01'),
        '/test/new.ts': new Date('2024-06-01'),
      };
      return {
        isFile: () => true,
        isDirectory: () => false,
        isSymbolicLink: () => false,
        size: 100,
        mode: parseInt('100644', 8),
        mtime: mtimes[path] ?? new Date(),
      } as unknown as import('fs').Stats;
    });

    const result = await findFiles({
      path: '/test',
      sortBy: 'modified',
      showFileLastModified: true,
      details: true,
    });

    expect(result.status).toBeUndefined();
    const files = result.files!;
    expect(files[0]!.path).toContain('new.ts');
    expect(files[1]!.path).toContain('old.ts');
  });

  it('warns when sortBy="modified" cannot be honored without showFileLastModified', async () => {
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '/test/b.ts\0/test/a.ts\0',
      stderr: '',
    });

    mockFs.promises.lstat.mockResolvedValue({
      isFile: () => true,
      isDirectory: () => false,
      isSymbolicLink: () => false,
      size: 100,
      mode: parseInt('100644', 8),
      mtime: new Date('2024-01-01'),
    } as unknown as import('fs').Stats);

    const result = await findFiles({
      path: '/test',
      sortBy: 'modified',
      showFileLastModified: false,
    });

    expect(result.status).toBeUndefined();
    expect(result.hints).toContain(
      'sortBy="modified" ignored: showFileLastModified=false; sorted by path instead.'
    );
  });

  it('should return empty files when charOffset >= totalChars (line 262)', async () => {
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '/test/a.txt\0/test/b.txt\0',
      stderr: '',
    });

    mockFs.promises.lstat.mockResolvedValue({
      isFile: () => true,
      isDirectory: () => false,
      isSymbolicLink: () => false,
      size: 10,
      mode: parseInt('100644', 8),
      mtime: new Date(),
    } as unknown as import('fs').Stats);

    const result = await findFiles({
      path: '/test',
      charLength: 100,
      charOffset: 10000,
    });

    expect(result.status).toBeUndefined();
    expect(result.files).toEqual([]);
    expect(result.charPagination?.hasMore).toBe(false);
  });
});
