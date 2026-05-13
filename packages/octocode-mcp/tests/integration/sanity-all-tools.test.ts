/**
 * Sanity test: exercises all tools in one end-to-end test
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { viewStructure } from '../../src/tools/local_view_structure/local_view_structure.js';
import { findFiles } from '../../src/tools/local_find_files/findFiles.js';
import { searchContentRipgrep } from '../../src/tools/local_ripgrep/searchContentRipgrep.js';
import { fetchContent } from '../../src/tools/local_fetch_content/fetchContent.js';
import { safeExec } from '../../src/utils/exec/safe.js';
import * as pathValidator from 'octocode-security-utils/pathValidator';
import type { Stats } from 'fs';

// Mocks
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
  pathValidator: { validate: vi.fn() },
}));

// Shareable fs mocks for view_structure and find_files
const { mockReaddirFn, mockLstatFn, mockLstatSyncFn } = vi.hoisted(() => ({
  mockReaddirFn: vi.fn(),
  mockLstatFn: vi.fn(),
  mockLstatSyncFn: vi.fn(),
}));

vi.mock('fs', () => ({
  default: {
    lstatSync: mockLstatSyncFn,
    promises: { readdir: mockReaddirFn, lstat: mockLstatFn },
  },
  lstatSync: mockLstatSyncFn,
  promises: { readdir: mockReaddirFn, lstat: mockLstatFn },
}));

// fs/promises mock for fetch_content
vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
  stat: vi.fn(),
}));

import * as fsp from 'fs/promises';

describe('Integration sanity: all tools', () => {
  const mockSafeExec = vi.mocked(safeExec);
  const mockValidate = vi.mocked(pathValidator.pathValidator.validate);
  const mockReaddir = mockReaddirFn;
  const mockLstat = mockLstatFn;
  const mockLstatSync = mockLstatSyncFn;
  const mockReadFile = vi.mocked(fsp.readFile);
  const mockStat = vi.mocked(fsp.stat);

  beforeEach(() => {
    vi.clearAllMocks();

    mockValidate.mockReturnValue({
      isValid: true,
      sanitizedPath: '/workspace',
    });

    mockReaddir.mockResolvedValue([]);
    mockLstat.mockResolvedValue({
      isDirectory: () => false,
      isFile: () => true,
      isSymbolicLink: () => false,
      size: 100,
      mtime: new Date('2024-06-01T00:00:00.000Z'),
    } as unknown as Stats);
    mockLstatSync.mockReturnValue({
      isDirectory: () => false,
      isSymbolicLink: () => false,
    } as unknown as Stats);

    mockReadFile.mockResolvedValue('function demo() {}\n// line2\n// line3');
    mockStat.mockResolvedValue({ size: 20000 } as unknown as Awaited<
      ReturnType<typeof fsp.stat>
    >);
  });

  it('should run all tools end-to-end with core functionality', async () => {
    // 1) localViewStructure: simple listing with pagination
    mockSafeExec.mockResolvedValueOnce({
      success: true,
      code: 0,
      stdout: 'a.txt\nb.js\ndir',
      stderr: '',
    });

    mockLstatSync.mockImplementation(
      (p: string | Buffer | URL) =>
        ({
          isDirectory: () => p.toString().endsWith('dir'),
          isSymbolicLink: () => false,
        }) as unknown as Stats
    );

    const vs = await viewStructure({
      path: '/workspace',
      entriesPerPage: 2,
      entryPageNumber: 1,
    });
    expect(vs.status).toBe('hasResults');
    expect(vs.entries).toBeDefined();
    expect(vs.pagination?.currentPage).toBe(1);
    expect(vs.pagination?.entriesPerPage).toBe(2);

    // 2) localFindFiles: NUL output, details, pagination
    mockSafeExec.mockResolvedValueOnce({
      success: true,
      code: 0,
      stdout: '/workspace/a.txt\0/workspace/b.js\0',
      stderr: '',
    });

    // ensure lstat returns size/permissions
    mockLstat.mockResolvedValueOnce({
      isDirectory: () => false,
      isFile: () => true,
      isSymbolicLink: () => false,
      size: 123,
      mode: parseInt('100644', 8),
      mtime: new Date('2024-06-01T00:00:00.000Z'),
    } as unknown as Stats);
    mockLstat.mockResolvedValueOnce({
      isDirectory: () => false,
      isFile: () => true,
      isSymbolicLink: () => false,
      size: 321,
      mode: parseInt('100755', 8),
      mtime: new Date('2024-06-02T00:00:00.000Z'),
    } as unknown as Stats);

    const ff = await findFiles({
      path: '/workspace',
      details: true,
      filesPerPage: 2,
      filePageNumber: 1,
    });
    expect(ff.status).toBe('hasResults');
    expect(ff.files?.length).toBeLessThanOrEqual(2);
    expect(ff.pagination?.currentPage).toBe(1);

    // 3) localSearchCode: NDJSON matches, per-file pagination, show modified
    const rgJson = [
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/workspace/a.txt' },
          lines: { text: 'first line match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ start: 0, end: 5, match: { text: 'first' } }],
        },
      }),
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/workspace/a.txt' },
          lines: { text: 'second line match' },
          line_number: 20,
          absolute_offset: 200,
          submatches: [{ start: 0, end: 6, match: { text: 'second' } }],
        },
      }),
    ].join('\n');

    mockSafeExec.mockResolvedValueOnce({
      success: true,
      code: 0,
      stdout: rgJson,
      stderr: '',
    });

    const rg = await searchContentRipgrep({
      pattern: 'match',
      path: '/workspace',
      showFileLastModified: true,
      matchesPerPage: 1,
    } as Parameters<typeof searchContentRipgrep>[0]);
    expect(rg.status).toBe('hasResults');
    expect(rg.files?.[0]?.matchCount).toBe(2);
    expect(rg.files?.[0]?.matches?.length).toBe(1); // paginated matches
    expect(rg.files?.[0]?.pagination?.totalPages).toBe(2);

    // 4) localGetFileContent: large file paginated and matchString
    mockReadFile.mockResolvedValueOnce('x'.repeat(20000));
    mockStat.mockResolvedValueOnce({ size: 20000 } as unknown as Awaited<
      ReturnType<typeof fsp.stat>
    >);

    const fc = await fetchContent({
      path: '/workspace/a.txt',
      matchString: 'x',
      charLength: 5000,
    });
    expect(fc.status).toBe('hasResults');
    expect(fc.pagination?.hasMore).toBe(true);

    // 5) Negative path validation for a tool (error path)
    mockValidate.mockReturnValueOnce({ isValid: false, error: 'Invalid path' });
    const vsErr = await viewStructure({ path: '/etc/passwd' });
    expect(vsErr.status).toBe('error');
  });
});
