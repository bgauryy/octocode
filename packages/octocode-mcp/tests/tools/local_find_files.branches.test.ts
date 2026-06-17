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

type FindFilesInput = Parameters<typeof findFilesImpl>[0] & {
  page?: number;
  itemsPerPage?: number;
};

const findFiles = (query: FindFilesInput) => findFilesImpl(query);

vi.mock('octocode-security/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn(),
  },
}));

/**
 * The local tools now delegate filesystem traversal/filtering to the native
 * `@octocodeai/octocode-context-utils` module via `contextUtils.queryFileSystem`.
 * These helpers let each test declare the entries that the (mocked) native
 * layer should return, plus optional capping/diagnostics metadata.
 */
interface MockEntryInput {
  path: string;
  type?: 'file' | 'directory' | 'symlink';
  size?: number;
  modifiedMs?: number;
  permissions?: string;
}

let queryFileSystemMock: ReturnType<typeof vi.fn>;
let lastQueryOptions: FileSystemQueryOptions | undefined;

function toEntryType(type: MockEntryInput['type']): string {
  switch (type) {
    case 'directory':
      return 'directory';
    case 'symlink':
      return 'symlink';
    default:
      return 'file';
  }
}

function buildEntry(input: MockEntryInput, basePath: string): FileSystemEntry {
  const name = input.path.split('/').pop() || input.path;
  const ext = name.includes('.') ? name.split('.').pop() : undefined;
  const rel = input.path.startsWith(basePath)
    ? input.path.slice(basePath.length).replace(/^\//, '')
    : name;
  return {
    path: input.path,
    relativePath: rel,
    name,
    entryType: toEntryType(input.type),
    ...(input.size !== undefined ? { size: input.size } : {}),
    ...(input.modifiedMs !== undefined ? { modifiedMs: input.modifiedMs } : {}),
    ...(input.permissions ? { permissions: input.permissions } : {}),
    ...(ext ? { extension: ext } : {}),
    depth: 0,
  };
}

/** Declare the entries the native layer should return for the next call(s). */
function setNativeEntries(
  entries: MockEntryInput[],
  opts: {
    totalDiscovered?: number;
    wasCapped?: boolean;
    skipped?: number;
    warnings?: string[];
  } = {}
): void {
  queryFileSystemMock.mockImplementation(
    (options: FileSystemQueryOptions): FileSystemQueryResult => {
      lastQueryOptions = options;
      const basePath = options.path;
      const limit = options.limit ?? entries.length;
      const mapped = entries.map(e => buildEntry(e, basePath));
      const capped = mapped.slice(0, limit);
      return {
        entries: capped,
        totalDiscovered: opts.totalDiscovered ?? entries.length,
        wasCapped: opts.wasCapped ?? mapped.length > limit,
        skipped: opts.skipped ?? 0,
        permissionDenied: 0,
        warnings: opts.warnings ?? [],
      };
    }
  );
}

/** Make the native layer throw (e.g. ENOENT/EACCES). */
function setNativeError(error: Error): void {
  queryFileSystemMock.mockImplementation(
    (options: FileSystemQueryOptions): FileSystemQueryResult => {
      lastQueryOptions = options;
      throw error;
    }
  );
}

const mockValidate = vi.mocked(pathValidator.pathValidator.validate);

describe('findFiles sortBy branches', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    lastQueryOptions = undefined;
    queryFileSystemMock = vi.fn();
    setContextUtilsNativeLoaderForTesting(
      () =>
        ({
          queryFileSystem: queryFileSystemMock,
        }) as unknown as typeof import('@octocodeai/octocode-context-utils')
    );
    mockValidate.mockReturnValue({
      isValid: true,
      sanitizedPath: '/test',
    });
    setNativeEntries([]);
  });

  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('should sort by size descending when sortBy is "size"', async () => {
    setNativeEntries([
      { path: '/test/big.ts', type: 'file', size: 5000 },
      { path: '/test/small.ts', type: 'file', size: 100 },
      { path: '/test/medium.ts', type: 'file', size: 2000 },
    ]);

    const result = await findFiles({
      path: '/test',
      sortBy: 'size',
      details: true,
    });

    expect(result.status).toBeUndefined();
    const files = result.files!;
    expect(files[0]!.sizeFormatted).toBe('4.9KB');
    expect(files[1]!.sizeFormatted).toBe('2.0KB');
    expect(files[2]!.sizeFormatted).toBe('100.0B');
  });

  it('should sort by name alphabetically when sortBy is "name"', async () => {
    setNativeEntries([
      { path: '/test/charlie.ts', type: 'file', size: 100 },
      { path: '/test/alpha.ts', type: 'file', size: 100 },
      { path: '/test/bravo.ts', type: 'file', size: 100 },
    ]);

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
    setNativeEntries([
      { path: '/test/z/file.ts', type: 'file', size: 100 },
      { path: '/test/a/file.ts', type: 'file', size: 100 },
      { path: '/test/m/file.ts', type: 'file', size: 100 },
    ]);

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

  it('should return error when the native layer fails (find traversal fails)', async () => {
    setNativeError(new Error('find: /nonexistent: No such file or directory'));

    const result = await findFiles({
      path: '/test',
    });

    expect(result.status).toBe('error');
    expect(result.error).toBeDefined();
  });

  it('should sort by modified when showLastModified and both files have modified (line 158)', async () => {
    setNativeEntries([
      {
        path: '/test/old.ts',
        type: 'file',
        size: 100,
        modifiedMs: new Date('2020-01-01').getTime(),
      },
      {
        path: '/test/new.ts',
        type: 'file',
        size: 100,
        modifiedMs: new Date('2024-06-01').getTime(),
      },
    ]);

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

  it('honors sortBy="modified" without showFileLastModified (no warning, modified shown in output)', async () => {
    setNativeEntries([
      {
        path: '/test/b.ts',
        type: 'file',
        size: 100,
        modifiedMs: new Date('2024-06-01').getTime(),
      },
      {
        path: '/test/a.ts',
        type: 'file',
        size: 100,
        modifiedMs: new Date('2020-01-01').getTime(),
      },
    ]);

    const result = await findFiles({
      path: '/test',
      sortBy: 'modified',
      showFileLastModified: false,
    });

    expect(result.status).toBeUndefined();
    const files = result.files!;
    expect(files[0]!.path).toBe('/test/b.ts');
    expect(files[1]!.path).toBe('/test/a.ts');
    expect(files.every(f => f.modified !== undefined)).toBe(true);
    expect(
      (result.hints ?? []).some(h => h.includes('sortBy="modified" ignored'))
    ).toBe(false);
  });

  it('should return empty files when page exceeds total pages', async () => {
    setNativeEntries([
      { path: '/test/a.txt', type: 'file', size: 10 },
      { path: '/test/b.txt', type: 'file', size: 10 },
    ]);

    const result = await findFiles({
      path: '/test',
      page: 999,
    });

    expect(result.status).toBeUndefined();
    expect(result.files?.length ?? 0).toBe(0);
  });
});
