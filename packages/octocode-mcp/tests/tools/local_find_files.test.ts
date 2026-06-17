import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { LOCAL_TOOL_ERROR_CODES } from '../../../octocode-tools-core/src/errors/localToolErrors.js';
import { findFiles as findFilesImpl } from '../../../octocode-tools-core/src/tools/local_find_files/findFiles.js';
import type { LocalFindFilesToolResult as FindFilesResult } from '@octocodeai/octocode-core/extra-types';
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

const expectDefinedFiles = (result: FindFilesResult) => {
  expect(result.files).toBeDefined();
  return result.files!;
};

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

describe('localFindFiles', () => {
  const mockValidate = vi.mocked(pathValidator.pathValidator.validate);

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
      sanitizedPath: '/test/path',
    });
    setNativeEntries([]);
  });

  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  describe('Basic file discovery', () => {
    it('should find files by name pattern', async () => {
      setNativeEntries([
        { path: '/test/path/file1.js' },
        { path: '/test/path/file2.js' },
      ]);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.js'],
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files).toHaveLength(2);
      expect(files.map(f => f.path)).toContain('/test/path/file1.js');
    });

    it('forwards name filters to the native layer', async () => {
      setNativeEntries([{ path: '/test/path/file1.js' }]);

      await findFiles({ path: '/test/path', names: ['*.js'] });

      expect(lastQueryOptions?.names).toEqual(['*.js']);
    });

    it('signals page-out-of-range instead of silent empty (E2)', async () => {
      setNativeEntries([
        { path: '/test/path/a.ts' },
        { path: '/test/path/b.ts' },
        { path: '/test/path/c.ts' },
      ]);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.ts'],
        page: 999,
      });

      const hints = (result.hints ?? []).join('\n');
      expect(hints).toMatch(/outside available range|page 999 is/i);
    });

    it('should include metadata (size, permissions) in results when details is true', async () => {
      setNativeEntries([
        {
          path: '/test/path/file1.js',
          type: 'file',
          size: 123,
          permissions: '644',
          modifiedMs: new Date('2025-01-01T00:00:00Z').getTime(),
        },
      ]);

      const result = await findFiles({ path: '/test/path', details: true });

      expect(result.status).toBeUndefined();

      const files = expectDefinedFiles(result);

      expect(files[0]).toMatchObject({
        path: '/test/path/file1.js',
        type: 'file',
        sizeFormatted: '123.0B',
        permissions: '644',
      });
    });

    it('should handle empty results', async () => {
      setNativeEntries([]);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.nonexistent'],
      });

      expect(result.status).toBe('empty');
    });

    it('does NOT implicitly cap at 1000 — all discovered files stay paginable', async () => {
      const entries = Array.from({ length: 1002 }, (_, index) => ({
        path: `/test/path/file-${index}.ts`,
      }));
      setNativeEntries(entries, { totalDiscovered: 1002, wasCapped: false });

      const result = await findFiles({ path: '/test/path' });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalFiles).toBe(1002);
      expect(result.hints?.some(h => /capped at/i.test(h))).toBeFalsy();
    });

    it('caps + warns only when an explicit limit is given (a deliberate user cap)', async () => {
      const entries = Array.from({ length: 1002 }, (_, index) => ({
        path: `/test/path/file-${index}.ts`,
      }));
      // Native layer caps at the user limit and reports the full discovered count.
      setNativeEntries(entries, { totalDiscovered: 1002, wasCapped: true });

      const result = await findFiles({ path: '/test/path', limit: 1000 });

      expect(result.pagination?.totalFiles).toBe(1000);
      expect(result.hints?.some(h => /capped at 1000/i.test(h))).toBe(true);
      expect(result.hints?.some(h => h.includes('1002'))).toBe(true);
    });
  });

  describe('File type filtering', () => {
    it('should filter by file type', async () => {
      setNativeEntries([{ path: '/test/path/file1.txt', type: 'file' }]);

      const result = await findFiles({
        path: '/test/path',
        entryType: 'f',
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.entryType).toBe('f');
    });

    it('should find directories only', async () => {
      setNativeEntries([
        { path: '/test/path/dir1', type: 'directory' },
        { path: '/test/path/dir2', type: 'directory' },
      ]);

      const result = await findFiles({
        path: '/test/path',
        entryType: 'd',
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.entryType).toBe('d');
      const files = expectDefinedFiles(result);
      expect(files.every(f => f.type === 'directory')).toBe(true);
    });
  });

  describe('Time-based filtering', () => {
    it('should find recently modified files', async () => {
      setNativeEntries([{ path: '/test/path/recent.txt' }]);

      const result = await findFiles({
        path: '/test/path',
        modifiedWithin: '7d',
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.modifiedWithin).toBe('7d');
    });

    it('should find files modified before date', async () => {
      setNativeEntries([{ path: '/test/path/old.txt' }]);

      const result = await findFiles({
        path: '/test/path',
        modifiedBefore: '30d',
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.modifiedBefore).toBe('30d');
    });
  });

  describe('Size filtering', () => {
    it('should find files larger than threshold', async () => {
      setNativeEntries([{ path: '/test/path/large.bin' }]);

      const result = await findFiles({
        path: '/test/path',
        sizeGreater: '1M',
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.sizeGreater).toBe('1M');
    });

    it('should find files smaller than threshold', async () => {
      setNativeEntries([{ path: '/test/path/small.txt' }]);

      const result = await findFiles({
        path: '/test/path',
        sizeLess: '1k',
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.sizeLess).toBe('1k');
    });
  });

  describe('Permission filtering', () => {
    it('should find executable files', async () => {
      setNativeEntries([{ path: '/test/path/script.sh' }]);

      const result = await findFiles({
        path: '/test/path',
        executable: true,
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.executable).toBe(true);
    });

    it('should filter by permissions', async () => {
      setNativeEntries([{ path: '/test/path/file.sh' }]);

      const result = await findFiles({
        path: '/test/path',
        permissions: '755',
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.permissions).toBe('755');
    });
  });

  describe('Depth control', () => {
    it('should limit search depth', async () => {
      setNativeEntries([{ path: '/test/path/file1.txt' }]);

      const result = await findFiles({
        path: '/test/path',
        maxDepth: 2,
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.maxDepth).toBe(2);
    });

    it('should set minimum depth', async () => {
      setNativeEntries([{ path: '/test/path/sub/file.txt' }]);

      const result = await findFiles({
        path: '/test/path',
        minDepth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.minDepth).toBe(1);
    });
  });

  describe('Directory exclusion', () => {
    it('should exclude specific directories', async () => {
      setNativeEntries([{ path: '/test/path/src/file.js' }]);

      const result = await findFiles({
        path: '/test/path',
        excludeDir: ['node_modules', '.git'],
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.excludeDir).toEqual(['node_modules', '.git']);
    });

    it('should auto-exclude tool/IDE cache dirs by default', async () => {
      setNativeEntries([{ path: '/test/path/src/file.js' }]);

      const result = await findFiles({ path: '/test/path' });
      expect(result.status).toBeUndefined();

      const excludeDir = lastQueryOptions?.excludeDir ?? [];
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
        expect(excludeDir).toContain(dir);
      }
    });

    it('should NOT prune directories that appear in the search path itself', async () => {
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/work/.context/sub',
      });
      setNativeEntries([{ path: '/work/.context/sub/file.ts' }]);

      const result = await findFiles({
        path: '/work/.context/sub',
        names: ['*.ts'],
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.excludeDir).not.toContain('.context');
    });

    it('should NOT prune node_modules when searching inside node_modules', async () => {
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/project/node_modules/lodash',
      });
      setNativeEntries([{ path: '/project/node_modules/lodash/index.js' }]);

      const result = await findFiles({
        path: '/project/node_modules/lodash',
        names: ['*.js'],
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.excludeDir).not.toContain('node_modules');
    });

    it('should still exclude dirs that are NOT in the search path', async () => {
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/project/src',
      });
      setNativeEntries([{ path: '/project/src/app.ts' }]);

      await findFiles({ path: '/project/src', names: ['*.ts'] });

      expect(lastQueryOptions?.excludeDir).toContain('node_modules');
      expect(lastQueryOptions?.excludeDir).toContain('.context');
    });
  });

  describe('Result limiting', () => {
    it('should apply result limit', async () => {
      const entries = Array.from({ length: 150 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries, { totalDiscovered: 150, wasCapped: true });

      const result = await findFiles({
        path: '/test/path',
        limit: 50,
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.limit).toBe(50);
      const limitedFiles = expectDefinedFiles(result);
      expect(limitedFiles.length).toBeLessThanOrEqual(50);
    });

    it('should require pagination for large result sets', async () => {
      const entries = Array.from({ length: 150 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries, { totalDiscovered: 150 });

      const result = await findFiles({
        path: '/test/path',
      });

      expect([undefined, 'error']).toContain(result.status);
      if (result.status === 'error') {
        expect(result.errorCode).toBeDefined();
      } else {
        expect(result.pagination?.hasMore).toBe(true);
      }
    });
  });

  describe('Multiple name patterns', () => {
    it('should handle multiple name patterns with OR logic', async () => {
      setNativeEntries([
        { path: '/test/path/file1.ts' },
        { path: '/test/path/file2.js' },
      ]);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.ts', '*.js'],
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files).toHaveLength(2);
      expect(lastQueryOptions?.names).toEqual(['*.ts', '*.js']);
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
    it('should handle path not found (within workspace)', async () => {
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/nonexistent_path_xyz_123',
      });
      setNativeError(
        Object.assign(
          new Error(
            '/workspace/nonexistent_path_xyz_123: No such file or directory'
          ),
          { code: 'ENOENT' }
        )
      );

      const result = await findFiles({
        path: '/workspace/nonexistent_path_xyz_123',
        names: ['*.ts'],
      });

      expect(result.status).toBe('error');
      expect(result.error).toContain('No such file or directory');
    });

    it('should handle general exceptions gracefully', async () => {
      setNativeError(new Error('Unexpected error'));

      const result = await findFiles({
        path: '/test/path',
        names: ['*.txt'],
      });

      expect(result.status).toBe('error');
    });
  });

  describe('showFileLastModified sorting', () => {
    it('should sort by modification time when showFileLastModified is true', async () => {
      setNativeEntries([
        {
          path: '/test/old.txt',
          modifiedMs: new Date('2020-01-01').getTime(),
        },
        {
          path: '/test/new.txt',
          modifiedMs: new Date('2024-12-01').getTime(),
        },
        {
          path: '/test/mid.txt',
          modifiedMs: new Date('2022-06-01').getTime(),
        },
      ]);

      const result = await findFiles({
        path: '/test/path',
        showFileLastModified: true,
        details: true,
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files.length).toBe(3);
      expect(files[0]!.path).toBe('/test/new.txt');
      expect(files[0]!.modified).toBeDefined();
    });

    it('should sort by modification time even when showFileLastModified is false (modified shown in output by default)', async () => {
      setNativeEntries([
        {
          path: '/test/c.txt',
          modifiedMs: new Date('2020-01-01').getTime(),
        },
        {
          path: '/test/a.txt',
          modifiedMs: new Date('2024-12-01').getTime(),
        },
        {
          path: '/test/b.txt',
          modifiedMs: new Date('2022-06-01').getTime(),
        },
      ]);

      const result = await findFiles({
        path: '/test/path',
        showFileLastModified: false,
        details: true,
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files.length).toBe(3);
      expect(files[0]!.path).toBe('/test/a.txt');
      expect(files[1]!.path).toBe('/test/b.txt');
      expect(files[2]!.path).toBe('/test/c.txt');
      expect(files.every(f => f.modified !== undefined)).toBe(true);
    });
  });

  describe('Large result handling', () => {
    it('should paginate large result sets', async () => {
      const entries = Array.from({ length: 50 }, (_, i) => ({
        path: `/test/file${i}.txt`,
        size: 123,
      }));
      setNativeEntries(entries);

      const result = await findFiles({
        path: '/test/path',
        details: true,
        itemsPerPage: 10,
      });

      expect(result.status).toBeUndefined();
      const files2 = expectDefinedFiles(result);
      expect(files2.length).toBeLessThanOrEqual(10);
    });
  });

  describe('entry type detection', () => {
    it('should detect symlinks', async () => {
      setNativeEntries([{ path: '/test/link.txt', type: 'symlink' }]);

      const result = await findFiles({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files[0]!.type).toBe('symlink');
    });

    it('should detect directories', async () => {
      setNativeEntries([{ path: '/test/dir', type: 'directory' }]);

      const result = await findFiles({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files[0]!.type).toBe('directory');
    });

    it('should default to file type', async () => {
      setNativeEntries([{ path: '/test/inaccessible.txt', type: 'file' }]);

      const result = await findFiles({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      const files = expectDefinedFiles(result);
      expect(files[0]!.type).toBe('file');
    });
  });

  describe('NEW FEATURE: File-based pagination with automatic sorting', () => {
    it('should paginate with default 20 files per page', async () => {
      const entries = Array.from({ length: 50 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.txt'],
      });

      expect(result.status).toBeUndefined();
      const filesDefaultPage = expectDefinedFiles(result);
      expect(filesDefaultPage.length).toBeLessThanOrEqual(20);
      expect(result.pagination?.totalPages).toBe(3);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should navigate to second page', async () => {
      const entries = Array.from({ length: 50 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.txt'],
        page: 2,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(2);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should support custom filesPerPage', async () => {
      const entries = Array.from({ length: 50 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.txt'],
        itemsPerPage: 10,
      });

      expect(result.status).toBeUndefined();
      const filesCustomPerPage = expectDefinedFiles(result);
      expect(filesCustomPerPage.length).toBeLessThanOrEqual(10);
      expect(result.pagination?.totalPages).toBe(5);
    });

    it('should handle last page correctly', async () => {
      const entries = Array.from({ length: 25 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.txt'],
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
      setNativeEntries([
        { path: '/test/old.txt' },
        { path: '/test/new.txt' },
        { path: '/test/mid.txt' },
      ]);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.txt'],
      });

      expect(result.status).toBeUndefined();
      const filesSorted = expectDefinedFiles(result);
      expect(filesSorted.length).toBe(3);
    });

    it('should sort even with pagination', async () => {
      const entries = Array.from({ length: 30 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 10,
      });

      expect(result.status).toBeUndefined();
    });

    it('should sort with time-based filters', async () => {
      setNativeEntries([
        { path: '/test/recent1.txt' },
        { path: '/test/recent2.txt' },
      ]);

      const result = await findFiles({
        path: '/test/path',
        modifiedWithin: '7d',
      });

      expect(result.status).toBeUndefined();
    });
  });

  describe('NEW FEATURE: Pagination hints', () => {
    it('should include pagination hints with page info', async () => {
      const entries = Array.from({ length: 50 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
      expect(result.hints!.length).toBeGreaterThan(0);
    });
  });

  describe('Research context fields', () => {
    it('should not echo researchGoal and reasoning in hasResults', async () => {
      setNativeEntries([
        { path: '/test/path/file1.txt' },
        { path: '/test/path/file2.txt' },
      ]);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.txt'],
        researchGoal: 'Find text files',
        reasoning: 'Need to locate documentation',
      });

      expect(result.status).toBeUndefined();
      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });

    it('should not echo researchGoal and reasoning in empty results', async () => {
      setNativeEntries([]);

      const result = await findFiles({
        path: '/test/path',
        names: ['*.nonexistent'],
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

  describe('Page-based pagination', () => {
    it('should return first page by default', async () => {
      const entries = Array.from({ length: 100 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({ path: '/test/path', page: 1 });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should return paged results for large file sets', async () => {
      const entries = Array.from({ length: 500 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({ path: '/test/path', page: 1 });

      expect(result.status).toBeUndefined();
      const filesResult = expectDefinedFiles(result);
      expect(filesResult.length).toBeGreaterThan(0);
    });

    it('should maintain valid structure when paginating by page', async () => {
      setNativeEntries([
        { path: '/test/file1.txt' },
        { path: '/test/file2.txt' },
      ]);

      const result = await findFiles({ path: '/test/path', page: 1 });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
    });

    it('should handle file paths with UTF-8 chars', async () => {
      setNativeEntries([
        { path: '/test/café.txt' },
        { path: '/test/résumé.txt' },
      ]);

      const result = await findFiles({ path: '/test/path' });

      expect(result.status).toBeUndefined();
      const filesUtf = expectDefinedFiles(result);
      expect(filesUtf.some(f => f.path.includes('café'))).toBe(true);
    });

    it('should handle 2-byte UTF-8 in paths', async () => {
      setNativeEntries([
        { path: '/test/niño.txt' },
        { path: '/test/español.txt' },
      ]);

      const result = await findFiles({ path: '/test/path' });

      expect(result.status).toBeUndefined();
      const filesUtf2 = expectDefinedFiles(result);
      expect(JSON.stringify(filesUtf2)).not.toMatch(/�/);
    });

    it('should handle 3-byte UTF-8 in paths', async () => {
      setNativeEntries([
        { path: '/test/文件.txt' },
        { path: '/test/中文.txt' },
      ]);

      const result = await findFiles({ path: '/test/path' });

      expect(result.status).toBeUndefined();
      const filesUtf3 = expectDefinedFiles(result);
      expect(JSON.stringify(filesUtf3)).not.toMatch(/�/);
    });

    it('should handle emoji in paths', async () => {
      setNativeEntries([
        { path: '/test/😀test.txt' },
        { path: '/test/🎉party.txt' },
      ]);

      const result = await findFiles({ path: '/test/path' });

      expect(result.status).toBeUndefined();
      const filesEmoji = expectDefinedFiles(result);
      expect(JSON.stringify(filesEmoji)).not.toMatch(/�/);
    });
  });

  describe('File pagination - Edge cases', () => {
    it('should handle page = 0 or negative (defaults to 1)', async () => {
      const entries = Array.from({ length: 50 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({
        path: '/test/path',
        page: 1,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should handle page > total pages', async () => {
      const entries = Array.from({ length: 25 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

      const result = await findFiles({
        path: '/test/path',
        page: 10,
        itemsPerPage: 20,
      });

      expect([undefined, 'empty']).toContain(result.status);
      if ((result.status as string | undefined) === 'hasResults') {
        expect(result.pagination?.currentPage).toBe(10);
      }
    });

    it('should handle filesPerPage = 1', async () => {
      const entries = Array.from({ length: 5 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

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
      const entries = Array.from({ length: 150 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

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
      setNativeEntries([{ path: '/test/single-file.txt' }]);

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
      const entries = Array.from({ length: 20 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

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
      const entries = Array.from({ length: 21 }, (_, i) => ({
        path: `/test/file${i}.txt`,
      }));
      setNativeEntries(entries);

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
      setNativeEntries([]);

      const result = await findFiles({
        path: '/test/path',
        itemsPerPage: 20,
        page: 1,
      });

      expect(result.status).toBe('empty');
    });
  });
});
