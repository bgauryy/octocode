/**
 * Tests for localFindFiles pagination semantics when wasFileCapped=true.
 *
 * Scenario: user passes limit=10, but there are 183 files in the directory.
 * The filesystem layer caps entries at 10 and reports totalDiscovered=183,
 * wasCapped=true.
 *
 * hasMore reflects page navigation only (currentPage < totalPages).
 * wasFileCapped is communicated through totalFilesFound and the hint text,
 * NOT through hasMore — so agents don't get "hasMore:true" then fail on page 2.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { findFiles as findFilesImpl } from '../../../octocode-tools-core/src/tools/local_find_files/findFiles.js';
import {
  setContextUtilsNativeLoaderForTesting,
  resetContextUtilsNativeLoaderForTesting,
} from '../../../octocode-tools-core/src/utils/contextUtils.js';
import type {
  FileSystemEntry,
  FileSystemQueryOptions,
  FileSystemQueryResult,
} from '../../../octocode-tools-core/src/utils/contextUtils.js';
import * as pathValidator from 'octocode-security/pathValidator';

vi.mock('octocode-security/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn(),
  },
}));

type FindFilesInput = Parameters<typeof findFilesImpl>[0] & {
  page?: number;
  itemsPerPage?: number;
};
const findFiles = (query: FindFilesInput) => findFilesImpl(query);

let queryFileSystemMock: ReturnType<typeof vi.fn>;

function buildEntry(path: string): FileSystemEntry {
  const name = path.split('/').pop() || path;
  return {
    path,
    relativePath: name,
    name,
    entryType: 'file',
    depth: 0,
  };
}

function setNativeEntries(
  entries: string[],
  opts: { totalDiscovered: number; wasCapped: boolean }
): void {
  queryFileSystemMock.mockImplementation(
    (options: FileSystemQueryOptions): FileSystemQueryResult => {
      const limit = options.limit ?? entries.length;
      const capped = entries.slice(0, limit).map(buildEntry);
      return {
        entries: capped,
        totalDiscovered: opts.totalDiscovered,
        wasCapped: opts.wasCapped,
        skipped: 0,
        permissionDenied: 0,
        warnings: [],
      };
    }
  );
}

describe('localFindFiles.pagination — totalPages when wasFileCapped', () => {
  const mockValidate = vi.mocked(pathValidator.pathValidator.validate);

  beforeEach(() => {
    vi.clearAllMocks();
    queryFileSystemMock = vi.fn();
    setContextUtilsNativeLoaderForTesting(
      () =>
        ({
          queryFileSystem: queryFileSystemMock,
        }) as unknown as typeof import('@octocodeai/octocode-context-utils')
    );
    mockValidate.mockReturnValue({
      isValid: true,
      sanitizedPath: '/test/path',
    });
  });

  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('totalPages=1 when limit=10 returns 10 files; hasMore=false (no page 2 available)', async () => {
    // 10 entries loaded (capped at limit), 183 total discovered in filesystem
    // totalPages is based on loaded count (10), not discovered count (183)
    // hasMore=false because there is no page 2 — cap info is in totalFilesFound + hint
    const entries = Array.from(
      { length: 10 },
      (_, i) => `/test/path/file${i}.ts`
    );
    setNativeEntries(entries, { totalDiscovered: 183, wasCapped: true });

    const result = await findFiles({
      path: '/test/path',
      names: ['*.ts'],
      limit: 10,
    });

    expect(result.pagination?.totalPages).toBe(1);
    expect(result.pagination?.hasMore).toBe(false); // no page 2; use totalFilesFound + hint for cap info
    expect(result.pagination?.totalFilesFound).toBe(183);
  });

  it('hasMore=false on single page even when wasFileCapped (cap communicated via totalFilesFound)', async () => {
    const entries = Array.from(
      { length: 10 },
      (_, i) => `/test/path/file${i}.ts`
    );
    setNativeEntries(entries, { totalDiscovered: 183, wasCapped: true });

    const result = await findFiles({
      path: '/test/path',
      names: ['*.ts'],
      limit: 10,
    });

    // hasMore reflects page navigation only; wasFileCapped does NOT set hasMore
    expect(result.pagination?.hasMore).toBe(false);
    expect(result.pagination?.totalFilesFound).toBe(183);
  });

  it('totalPages uses loaded file count / filesPerPage (not discoveredFileCount)', async () => {
    // 20 entries loaded (capped at limit=20), 200 discovered, default filesPerPage=20
    // totalPages = ceil(20/20) = 1, NOT ceil(200/20) = 10
    const entries = Array.from(
      { length: 20 },
      (_, i) => `/test/path/file${i}.ts`
    );
    setNativeEntries(entries, { totalDiscovered: 200, wasCapped: true });

    const result = await findFiles({
      path: '/test/path',
      names: ['*.ts'],
      limit: 20,
    });

    expect(result.pagination?.totalPages).toBe(1);
    expect(result.pagination?.hasMore).toBe(false); // no page 2; cap info in totalFilesFound
    expect(result.pagination?.totalFilesFound).toBe(200);
  });

  it('totalPages stays 1 when not capped and single page fits all results', async () => {
    const entries = Array.from(
      { length: 5 },
      (_, i) => `/test/path/file${i}.ts`
    );
    setNativeEntries(entries, { totalDiscovered: 5, wasCapped: false });

    const result = await findFiles({
      path: '/test/path',
      names: ['*.ts'],
      itemsPerPage: 20,
    });

    expect(result.pagination?.totalPages).toBe(1);
    expect(result.pagination?.hasMore).toBe(false);
  });
});
