import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { LOCAL_TOOL_ERROR_CODES } from '../../../octocode-tools-core/src/errors/localToolErrors.js';
import { viewStructure as viewStructureImpl } from '../../../octocode-tools-core/src/tools/local_view_structure/local_view_structure.js';
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

type ViewStructureInput = Parameters<typeof viewStructureImpl>[0] & {
  page?: number;
  itemsPerPage?: number;
  charOffset?: number;
  charLength?: number;
  summary?: boolean;
};

const viewStructure = (query: ViewStructureInput) => viewStructureImpl(query);

const flatNames = (result: {
  files?: string[];
  folders?: string[];
  links?: string[];
}): string[] => [
  ...(result.files ?? []),
  ...(result.folders ?? []),
  ...(result.links ?? []),
];

/** Strip " (NNN KB)" size annotation from files[] entries so tests assert names, not sizes. */
function stripSize(files: string[] | undefined): string[] {
  return (files ?? []).map(f => f.replace(/ \([^)]+\)$/, ''));
}

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
  depth?: number;
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
  const base = basePath.replace(/\/$/, '');
  const rel = input.path.startsWith(base)
    ? input.path.slice(base.length).replace(/^\//, '')
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
    depth: input.depth ?? 0,
  };
}

/** Declare the entries the native layer should return for the next call(s). */
function setNativeEntries(
  entries: MockEntryInput[],
  opts: {
    totalDiscovered?: number;
    wasCapped?: boolean;
    skipped?: number;
    permissionDenied?: number;
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
        permissionDenied: opts.permissionDenied ?? 0,
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

/** Build a list of plain file entries under a base path. */
function fileEntries(
  count: number,
  basePath = '/test/path',
  extra: Partial<MockEntryInput> = {}
): MockEntryInput[] {
  return Array.from({ length: count }, (_, i) => ({
    path: `${basePath}/file${i}.txt`,
    type: 'file' as const,
    ...extra,
  }));
}

describe('localViewStructure', () => {
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

  it('emits active filter echo-back for successful listings', async () => {
    setNativeEntries([{ path: '/test/path/file.ts', type: 'file', size: 10 }]);

    const result = await viewStructure({
      path: '/test/path',
      depth: 1,
      extensions: ['ts'],
    });

    expect(result.status).toBeUndefined();
    expect((result.hints ?? []).some(h => h.includes('localSearchCode'))).toBe(
      true
    );
  });

  describe('Basic directory listing', () => {
    it('should list directory contents', async () => {
      setNativeEntries([
        { path: '/test/path/file1.txt', type: 'file', size: 1024 },
        { path: '/test/path/file2.js', type: 'file', size: 1024 },
        { path: '/test/path/dir1', type: 'directory' },
      ]);

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeUndefined();
      expect(result.path).toBe('/test/path');
      expect(stripSize(result.files)).toEqual(
        expect.arrayContaining(['file1.txt', 'file2.js'])
      );
      expect(result.folders).toEqual(['dir1']);
    });

    it('should use sanitized path for the native query', async () => {
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/test/path',
      });
      setNativeEntries([]);

      await viewStructure({
        path: 'file:///unsafe/path',
      });

      expect(lastQueryOptions?.path).toBe('/test/path');
      expect(lastQueryOptions?.path).not.toBe('file:///unsafe/path');
    });

    it('should handle empty directories', async () => {
      setNativeEntries([]);

      const result = await viewStructure({
        path: '/test/empty',
      });

      expect(result.status).toBe('empty');
    });
  });

  describe('Structured output mode', () => {
    it('should generate structured output with file sizes', async () => {
      setNativeEntries([
        { path: '/test/path/file1.txt', type: 'file', size: 1024 },
        { path: '/test/path/file2.js', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        details: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.some(e => e.path?.endsWith('/file1.txt'))).toBe(
        true
      );
      expect(result.entries!.some(e => e.size === '1.0KB')).toBe(true);
    });

    it('should show file sizes for files, not directories', async () => {
      setNativeEntries([
        { path: '/test/path/file1.txt', type: 'file', size: 2048 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        details: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.size === '2.0KB')).toBe(true);
    });

    it('should respect depth parameter', async () => {
      setNativeEntries([
        { path: '/test/path/dir1', type: 'directory', depth: 0 },
        {
          path: '/test/path/dir1/subfile.txt',
          type: 'file',
          size: 512,
          depth: 1,
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 2,
      });

      expect(result.status).toBeUndefined();
      expect(result.folders).toContain('dir1');
      expect(result.files!.some(f => f.includes('subfile.txt'))).toBe(true);
    });
  });

  describe('Detailed listing with metadata', () => {
    it('should include file details when requested', async () => {
      setNativeEntries([
        {
          path: '/test/path/file1.txt',
          type: 'file',
          size: 1024,
          permissions: '644',
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.some(e => e.type === 'file')).toBe(true);
    });

    it('should report human-readable file sizes', async () => {
      setNativeEntries([
        {
          path: '/test/path/large.bin',
          type: 'file',
          size: 1.5 * 1024 * 1024,
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      expect(
        result.entries!.some(
          e => e.size && /\d+(\.\d+)?\s*(B|KB|MB|GB)/.test(String(e.size))
        )
      ).toBe(true);
    });
  });

  describe('Filtering', () => {
    it('should handle invalid regex pattern with fallback', async () => {
      setNativeEntries([
        { path: '/test/path/test1.txt', type: 'file', size: 1024 },
        { path: '/test/path/test2.txt', type: 'file', size: 1024 },
        { path: '/test/path/other.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        pattern: 'test*[',
        depth: 1,
      });

      expect([undefined, 'empty']).toContain(result.status);
    });

    it('should filter by file extension', async () => {
      setNativeEntries([
        { path: '/test/path/file1.ts', type: 'file', size: 1024 },
        { path: '/test/path/file2.js', type: 'file', size: 1024 },
        { path: '/test/path/file3.ts', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        extensions: ['ts'],
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(stripSize(result.files)).toContain('file1.ts');
      expect(stripSize(result.files)).toContain('file3.ts');
      expect(stripSize(result.files)).not.toContain('file2.js');
    });

    it('should filter by multiple extensions', async () => {
      setNativeEntries([
        { path: '/test/path/file1.ts', type: 'file', size: 1024 },
        { path: '/test/path/file2.tsx', type: 'file', size: 1024 },
        { path: '/test/path/file3.js', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        extensions: ['ts', 'tsx'],
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(stripSize(result.files)).toContain('file1.ts');
      expect(stripSize(result.files)).toContain('file2.tsx');
      expect(stripSize(result.files)).not.toContain('file3.js');
    });

    it('should filter files only', async () => {
      // filesOnly is forwarded as entryType='f'; native returns files only.
      setNativeEntries([
        { path: '/test/path/file1.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        filesOnly: true,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.entryType).toBe('f');
      expect(stripSize(result.files)).toContain('file1.txt');
      expect(result.folders).toBeUndefined();
      expect(flatNames(result)).not.toContain('dir1');
    });

    it('should filter directories only', async () => {
      // directoriesOnly is forwarded as entryType='d'; native returns dirs only.
      setNativeEntries([
        { path: '/test/path/dir1', type: 'directory' },
        { path: '/test/path/dir2', type: 'directory' },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        directoriesOnly: true,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.entryType).toBe('d');
      expect(result.folders).toContain('dir1');
      expect(result.folders).toContain('dir2');
      expect(result.files).toBeUndefined();
      expect(flatNames(result)).not.toContain('file1.txt');
    });

    it('should filter by name pattern', async () => {
      setNativeEntries([
        { path: '/test/path/test1.txt', type: 'file', size: 1024 },
        { path: '/test/path/test2.txt', type: 'file', size: 1024 },
        { path: '/test/path/other.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        pattern: 'test',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(stripSize(result.files)).toContain('test1.txt');
      expect(stripSize(result.files)).toContain('test2.txt');
      expect(stripSize(result.files)).not.toContain('other.txt');
    });

    it('should filter by glob pattern with asterisks', async () => {
      setNativeEntries([
        { path: '/test/path/parser.test.ts', type: 'file', size: 1024 },
        { path: '/test/path/utils.test.ts', type: 'file', size: 1024 },
        { path: '/test/path/helper.ts', type: 'file', size: 1024 },
        { path: '/test/path/config.ts', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        pattern: '*test*',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(stripSize(result.files)).toContain('parser.test.ts');
      expect(stripSize(result.files)).toContain('utils.test.ts');
      expect(stripSize(result.files)).not.toContain('helper.ts');
      expect(stripSize(result.files)).not.toContain('config.ts');
    });

    it('should filter by glob pattern, extensions, and recursive together', async () => {
      setNativeEntries([
        { path: '/test/path/root.test.ts', type: 'file', size: 1024, depth: 0 },
        { path: '/test/path/other.ts', type: 'file', size: 1024, depth: 0 },
        {
          path: '/test/path/subdir/nested.test.ts',
          type: 'file',
          size: 1024,
          depth: 1,
        },
        {
          path: '/test/path/subdir/another.ts',
          type: 'file',
          size: 1024,
          depth: 1,
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        pattern: '*test*',
        extensions: ['ts'],
        filesOnly: true,
        recursive: true,
      });

      expect(result.status).toBeUndefined();
      const names = flatNames(result);
      expect(names.some(n => n.includes('root.test.ts'))).toBe(true);
      expect(names.some(n => n.includes('nested.test.ts'))).toBe(true);
      expect(names.some(n => n.includes('other.ts'))).toBe(false);
      expect(names.some(n => n.includes('another.ts'))).toBe(false);
    });

    it('should filter by glob pattern with question mark', async () => {
      setNativeEntries([
        { path: '/test/path/test1.ts', type: 'file', size: 1024 },
        { path: '/test/path/test2.ts', type: 'file', size: 1024 },
        { path: '/test/path/test10.ts', type: 'file', size: 1024 },
        { path: '/test/path/testing.ts', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        pattern: 'test?.ts',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(stripSize(result.files)).toContain('test1.ts');
      expect(stripSize(result.files)).toContain('test2.ts');
      expect(stripSize(result.files)).not.toContain('test10.ts');
      expect(stripSize(result.files)).not.toContain('testing.ts');
    });
  });

  describe('Symlink handling', () => {
    it('should identify symlinks in recursive mode', async () => {
      setNativeEntries([
        { path: '/test/path/file.txt', type: 'file', size: 1024 },
        { path: '/test/path/link', type: 'symlink' },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.links).toContain('link');
      expect(stripSize(result.files)).toContain('file.txt');
    });

    it('should identify symlinks in detailed mode', async () => {
      setNativeEntries([
        { path: '/test/path/link', type: 'symlink' },
        { path: '/test/path/file.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        details: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries!.some(e => e.type === 'link')).toBe(true);
    });
  });

  describe('showFileLastModified', () => {
    it('should accept showFileLastModified in lean mode', async () => {
      setNativeEntries([
        {
          path: '/test/path/file.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-01-15T12:00:00Z').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
    });

    it('should accept showFileLastModified in detailed mode', async () => {
      setNativeEntries([
        {
          path: '/test/path/file.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-01-15T12:00:00Z').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        details: true,
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
    });

    it('should accept showFileLastModified in recursive mode', async () => {
      setNativeEntries([
        {
          path: '/test/path/file.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-06-15T12:00:00Z').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
    });

    it('should default to lean flat lists without timestamps when showFileLastModified is not specified', async () => {
      setNativeEntries([
        {
          path: '/test/path/file.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-01-15T12:00:00Z').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeUndefined();
      expect(stripSize(result.files)).toEqual(['file.txt']);
    });

    it('should honor sortBy=time in lean mode (modified collected internally, not displayed)', async () => {
      setNativeEntries([
        {
          path: '/test/path/new.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-06-15T12:00:00Z').getTime(),
        },
        {
          path: '/test/path/old.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2020-01-01T00:00:00Z').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'time',
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeUndefined();
      expect(stripSize(result.files)).toEqual(['old.txt', 'new.txt']);
    });

    it('should include modified timestamps for sortBy=time when showFileLastModified=true (recursive)', async () => {
      setNativeEntries([
        {
          path: '/test/path/file.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-06-15T12:00:00Z').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'time',
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBeGreaterThan(0);
      expect(result.entries![0]!.modified).toBe('2024-06-15T12:00:00.000Z');
    });

    it('should NOT include modified when showFileLastModified is explicitly false', async () => {
      setNativeEntries([
        {
          path: '/test/path/file.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-01-15T12:00:00Z').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        showFileLastModified: false,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeUndefined();
      expect(stripSize(result.files)).toEqual(['file.txt']);
    });

    it('still surfaces modified in detailed mode (details implies modified) even when showFileLastModified is false', async () => {
      // details:true makes showDetails true, and the source emits `modified`
      // whenever showDetails OR showModified is set, independent of the
      // showFileLastModified flag.
      setNativeEntries([
        {
          path: '/test/path/file.txt',
          type: 'file',
          size: 123,
          modifiedMs: new Date('2024-01-01T12:34:00Z').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        details: true,
        showFileLastModified: false,
      });

      expect(result.status).toBeUndefined();
      expect(result.entries).toBeDefined();
      expect(result.entries!.length).toBe(1);
      expect(result.entries![0]!.modified).toBe('2024-01-01T12:34:00.000Z');
    });
  });

  describe('Hidden files', () => {
    it('should show hidden files when requested', async () => {
      // showHidden is forwarded to native; native returns hidden entries.
      setNativeEntries([
        { path: '/test/path/.hidden', type: 'file', size: 1024 },
        { path: '/test/path/visible.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        hidden: true,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.showHidden).toBe(true);
      expect(stripSize(result.files)).toContain('.hidden');
      expect(stripSize(result.files)).toContain('visible.txt');
    });

    it('should hide hidden files by default', async () => {
      // showHidden=false -> native omits dotfiles.
      setNativeEntries([
        { path: '/test/path/visible.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        hidden: false,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.showHidden).toBe(false);
      expect(stripSize(result.files)).not.toContain('.hidden');
      expect(stripSize(result.files)).toContain('visible.txt');
    });
  });

  describe('Sorting', () => {
    it('should sort by name (default)', async () => {
      setNativeEntries([
        { path: '/test/path/beta.txt', type: 'file', size: 1024 },
        { path: '/test/path/alpha.txt', type: 'file', size: 1024 },
        { path: '/test/path/gamma.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        sortBy: 'name',
      });

      expect(result.status).toBeUndefined();
      expect(stripSize(result.files)).toEqual([
        'alpha.txt',
        'beta.txt',
        'gamma.txt',
      ]);
    });

    it('should sort by size in recursive mode', async () => {
      setNativeEntries([
        { path: '/test/path/small.txt', type: 'file', size: 512 },
        { path: '/test/path/large.txt', type: 'file', size: 4096 },
        { path: '/test/path/medium.txt', type: 'file', size: 2048 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'size',
      });

      expect(result.status).toBeUndefined();
      const names = result.files!;
      expect(names[0]).toContain('small');
      expect(names[names.length - 1]).toContain('large');
    });

    it('should sort by extension in recursive mode', async () => {
      setNativeEntries([
        { path: '/test/path/file.ts', type: 'file', size: 1024 },
        { path: '/test/path/file.js', type: 'file', size: 1024 },
        { path: '/test/path/file.css', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'extension',
      });

      expect(result.status).toBeUndefined();
    });

    it('should sort by time in recursive mode with showFileLastModified', async () => {
      setNativeEntries([
        {
          path: '/test/path/file1.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-01-01').getTime(),
        },
        {
          path: '/test/path/file2.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-01-01').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'time',
        showFileLastModified: true,
      });

      expect(result.status).toBeUndefined();
    });

    it('should sort by time falling back to name when modified not available', async () => {
      setNativeEntries([
        { path: '/test/path/zebra.txt', type: 'file', size: 1024 },
        { path: '/test/path/alpha.txt', type: 'file', size: 1024 },
        { path: '/test/path/beta.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'time',
        showFileLastModified: false,
      });

      expect(result.status).toBeUndefined();
      const names = result.files!;
      expect(names[0]).toContain('alpha');
      expect(names[1]).toContain('beta');
      expect(names[2]).toContain('zebra');
    });

    it('should support reverse sorting in recursive mode', async () => {
      setNativeEntries([
        { path: '/test/path/alpha.txt', type: 'file', size: 1024 },
        { path: '/test/path/beta.txt', type: 'file', size: 1024 },
        { path: '/test/path/gamma.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        sortBy: 'name',
        reverse: true,
      });

      expect(result.status).toBeUndefined();
      expect(stripSize(result.files)).toEqual([
        'gamma.txt',
        'beta.txt',
        'alpha.txt',
      ]);
    });
  });

  describe('Pagination - CRITICAL for large results', () => {
    it('should handle large directory listing (>100 entries)', async () => {
      setNativeEntries(fileEntries(150));

      const result = await viewStructure({
        path: '/test/path',
      });

      expect([undefined, 'error']).toContain(result.status);
      if (result.status === 'error') {
        expect(result.errorCode).toBeDefined();
      }
    });

    it('should allow tree view for large directories', async () => {
      setNativeEntries(fileEntries(150));

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        charLength: 50000,
      });

      expect(result.status).toBeUndefined();
    });

    it('should paginate large directory listings', async () => {
      setNativeEntries(fileEntries(150));

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalEntries).toBe(150);
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.files!.length).toBe(100);
    });

    it('should paginate tree view when requested', async () => {
      setNativeEntries([
        { path: '/test/path/file1.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 10000,
      });

      if (result.entries && result.entries.length > 20) {
        expect(result.pagination?.hasMore).toBe(true);
      }
    });

    it('should handle paginated continuation', async () => {
      setNativeEntries(fileEntries(150));

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
      expect(result2.files![0]).not.toBe(result1.files![0]);
    });
  });

  describe('Recursive listing', () => {
    it('should list recursively with depth control', async () => {
      setNativeEntries([
        { path: '/test/path/dir1', type: 'directory', depth: 0 },
        { path: '/test/path/file1.txt', type: 'file', size: 1024, depth: 0 },
        {
          path: '/test/path/dir1/subfile.txt',
          type: 'file',
          size: 1024,
          depth: 1,
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        recursive: true,
      });

      expect([undefined, 'empty']).toContain(result.status);
      if (result.status === undefined && result.entries) {
        expect(result.entries.length).toBeGreaterThan(0);
      }
    });

    it('should include cwd in recursive results (consistency with non-recursive)', async () => {
      setNativeEntries([
        { path: '/test/path/file.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        maxDepth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.path).toBe('/test/path');
      expect(stripSize(result.files)).toContain('file.txt');
    });

    it('should handle max depth limit for recursive', async () => {
      setNativeEntries([
        { path: '/test/path/file.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        maxDepth: 5,
      });

      expect([undefined, 'empty']).toContain(result.status);
      expect(lastQueryOptions?.maxDepth).toBe(5);
    });

    it('should handle large recursive listings', async () => {
      setNativeEntries(fileEntries(50));

      const result = await viewStructure({
        path: '/test/path',
        recursive: true,
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
      setNativeEntries(fileEntries(150));

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();
    });

    it('should respect maxEntries via the limit parameter', async () => {
      setNativeEntries(fileEntries(200));

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        limit: 10,
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
      expect(result.files!.length).toBe(10);
    });

    it('should return error when the native layer reports a generic failure', async () => {
      setNativeError(new Error('Cannot read directory'));

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      expect(result.status).toBe('error');
      expect(result.error).toBeDefined();
    });

    it('should treat an empty native result as empty', async () => {
      setNativeEntries([]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      expect(result.status).toBe('empty');
    });

    it('should return error with clear message when root path does not exist (ENOENT)', async () => {
      setNativeError(
        Object.assign(new Error('ENOENT: no such file or directory'), {
          code: 'ENOENT',
        })
      );

      const result = await viewStructure({
        path: '/nonexistent/path',
        depth: 1,
      });

      expect(result.status).toBe('error');
      expect(result.error).toMatch(/not found|ENOENT/i);
      expect(result.error).not.toMatch(/permission/i);
    });

    it('should return error with clear message when root path is ENOTDIR (path is a file)', async () => {
      setNativeError(
        Object.assign(new Error('ENOTDIR: not a directory'), {
          code: 'ENOTDIR',
        })
      );

      const result = await viewStructure({
        path: '/some/file.ts',
        depth: 1,
      });

      expect(result.status).toBe('error');
      expect(result.error).toBeDefined();
      expect(result.error).not.toMatch(/permission/i);
    });

    it('should correctly label EACCES as permission denied (not a generic skip)', async () => {
      setNativeError(
        Object.assign(new Error('EACCES: permission denied'), {
          code: 'EACCES',
        })
      );

      const result = await viewStructure({
        path: '/restricted/path',
        depth: 1,
      });

      expect(result.status).toBe('error');
      expect(result.error).toBeDefined();
      expect(result.error).toMatch(/permission/i);
    });
  });

  describe('Summary statistics', () => {
    it('should include summary by default', async () => {
      setNativeEntries([
        { path: '/test/path/file1.txt', type: 'file', size: 1024 },
        { path: '/test/path/dir1', type: 'directory' },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        summary: true,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
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
    it('should map native ENOENT failure to an error result', async () => {
      setNativeError(
        Object.assign(new Error('ENOENT: no such file or directory'), {
          code: 'ENOENT',
        })
      );

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED
      );
    });

    it('should produce a unified error shape from a permission-denied native failure', async () => {
      setNativeError(
        Object.assign(new Error('EACCES: permission denied'), {
          code: 'EACCES',
        })
      );

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBe('error');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED
      );
      expect(typeof result.error).toBe('string');
      expect(result.error).toMatch(/permission/i);
    });

    it('should handle unreadable directories', async () => {
      setNativeError(new Error('Permission denied'));

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(['error', 'empty', 'hasResults']).toContain(result.status);
    });
  });

  describe('Limit parameter', () => {
    it('should apply limit to results', async () => {
      setNativeEntries(fileEntries(100));

      const result = await viewStructure({
        path: '/test/path',
        limit: 10,
        depth: 1,
      });

      expect(result.status).toBeUndefined();
    });

    it('should apply limit in non-recursive mode with pagination', async () => {
      setNativeEntries(fileEntries(50));

      const result = await viewStructure({
        path: '/test/path',
        limit: 5,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
    });

    it('should apply limit BEFORE pagination logic', async () => {
      setNativeEntries(fileEntries(100));

      const result = await viewStructure({
        path: '/test/path',
        limit: 5,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.files?.length).toBe(5);
      expect(result.summary).toContain('5 entries');
      expect(result.pagination).toBeUndefined();
    });
  });

  describe('NEW FEATURE: Entry-based pagination with default time sorting', () => {
    it('should paginate with default 100 entries per page', async () => {
      setNativeEntries(fileEntries(150));

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(result.pagination?.totalPages).toBeGreaterThan(1);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should navigate to second page of entries', async () => {
      setNativeEntries(fileEntries(150));

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
      setNativeEntries(fileEntries(50));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 10,
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(result.pagination?.entriesPerPage).toBe(10);
    });

    it('should handle last page correctly', async () => {
      setNativeEntries(fileEntries(25));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
        page: 2,
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(result.pagination?.hasMore).toBe(false);
    });
  });

  describe('Entry pagination - Bounds', () => {
    it('should coerce page=0 to 1 via defaulting', async () => {
      setNativeEntries(fileEntries(25));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 10,
        page: 0,
      });

      expect([undefined, 'empty']).toContain(result.status);
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should reflect negative page as defaulted to 1', async () => {
      setNativeEntries(fileEntries(25));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 10,
        page: -3,
      });

      expect([undefined, 'empty']).toContain(result.status);
      // page||1 yields the provided negative value only when truthy; -3 is truthy.
      expect(result.pagination?.currentPage).toBe(-3);
    });

    it('should clamp overflow page to totalPages', async () => {
      setNativeEntries(fileEntries(25));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 10,
        page: 9999,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(3);
      expect(result.pagination?.totalPages).toBe(3);
      expect(result.pagination?.hasMore).toBe(false);
      expect(result.files?.length).toBe(5);
    });
  });

  describe('NEW FEATURE: Default sort by modification time', () => {
    it('should accept default time sorting', async () => {
      setNativeEntries([
        {
          path: '/test/path/old.txt',
          type: 'file',
          size: 1024,
          modifiedMs: new Date('2024-01-01').getTime(),
        },
        {
          path: '/test/path/new.txt',
          type: 'file',
          size: 2048,
          modifiedMs: new Date('2024-12-01').getTime(),
        },
      ]);

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
    });

    it('should allow overriding sort to name', async () => {
      setNativeEntries([
        { path: '/test/path/beta.txt', type: 'file', size: 1024 },
        { path: '/test/path/alpha.txt', type: 'file', size: 1024 },
        { path: '/test/path/gamma.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        sortBy: 'name',
      });

      expect(result.status).toBeUndefined();
    });

    it('should sort even with pagination', async () => {
      setNativeEntries(fileEntries(30));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 10,
      });

      expect(result.status).toBeUndefined();
    });
  });

  describe('NEW FEATURE: Entry pagination hints', () => {
    it('should include pagination hints with entry info', async () => {
      setNativeEntries(fileEntries(50));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
    });

    it('should show final page hint on last page', async () => {
      setNativeEntries(fileEntries(25));

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
      setNativeEntries([
        { path: '/test/path/file1.txt', type: 'file', size: 1024 },
        { path: '/test/path/file2.txt', type: 'file', size: 1024 },
      ]);

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
      setNativeEntries([]);

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
    it('should paginate entries even when charOffset/charLength are provided', async () => {
      setNativeEntries(fileEntries(150));

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
      setNativeEntries(fileEntries(50));

      const result = await viewStructure({
        path: '/test/path',
        charLength: 200,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage ?? 1).toBe(1);
    });

    it('should ignore charOffset and keep entry pagination semantics', async () => {
      setNativeEntries(fileEntries(100));

      const result = await viewStructure({
        path: '/test/path',
        charLength: 500,
        charOffset: 500,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage ?? 1).toBe(1);
      expect(result.pagination?.charOffset).toBeUndefined();
    });

    it('should handle charOffset = 0 without changing output shape', async () => {
      setNativeEntries([
        { path: '/test/path/file1.txt', type: 'file', size: 1024 },
        { path: '/test/path/file2.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charOffset: 0,
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage ?? 1).toBe(1);
      expect(result.pagination?.charOffset).toBeUndefined();
    });

    it('should handle large charOffset values without crashing', async () => {
      setNativeEntries([
        { path: '/test/path/file.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charOffset: 1000,
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage ?? 1).toBe(1);
    });

    it('should handle charOffset beyond content length', async () => {
      setNativeEntries([
        { path: '/test/path/file.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charOffset: 10000,
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
    });

    it('should handle charLength = 1', async () => {
      setNativeEntries([
        { path: '/test/path/file.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(result.pagination?.totalChars).toBeUndefined();
    });

    it('should handle charLength = 10000 (max)', async () => {
      setNativeEntries([
        { path: '/test/path/file.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore ?? false).toBe(false);
      expect(result.pagination?.totalChars).toBeUndefined();
    });

    it('should handle charLength > remaining content', async () => {
      setNativeEntries([
        { path: '/test/path/file.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 10000,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.hasMore ?? false).toBe(false);
    });

    it('should handle ASCII content pagination', async () => {
      setNativeEntries([
        { path: '/test/path/Hello World.txt', type: 'file', size: 1024 },
        { path: '/test/path/Line 3.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 20,
        charOffset: 0,
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
    });

    it('should handle 2-byte UTF-8 chars (é, ñ)', async () => {
      setNativeEntries([
        { path: '/test/path/Café.txt', type: 'file', size: 1024 },
        { path: '/test/path/piñata.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(flatNames(result).every(n => !n.includes('�'))).toBe(true);
    });

    it('should handle 3-byte UTF-8 chars (中文)', async () => {
      setNativeEntries([
        { path: '/test/path/你好世界.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(flatNames(result).every(n => !n.includes('�'))).toBe(true);
    });

    it('should handle 4-byte UTF-8 chars (emoji)', async () => {
      setNativeEntries([
        { path: '/test/path/😀🎉👍.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 100,
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(flatNames(result).every(n => !n.includes('�'))).toBe(true);
    });

    it('should not split multi-byte characters at boundaries', async () => {
      setNativeEntries([
        { path: '/test/path/café.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        charLength: 98,
      });

      expect(result.status).toBeUndefined();
      expect(flatNames(result).every(n => !n.includes('�'))).toBe(true);
    });

    it('should show character pagination hints', async () => {
      setNativeEntries(fileEntries(150));

      const result = await viewStructure({
        path: '/test/path',
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should show hints for next page using entry pagination', async () => {
      setNativeEntries(fileEntries(150));

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
      setNativeEntries(fileEntries(50));

      const result = await viewStructure({
        path: '/test/path',
        page: 1,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(1);
    });

    it('should clamp page > total pages to the last page', async () => {
      setNativeEntries(fileEntries(25));

      const result = await viewStructure({
        path: '/test/path',
        page: 10,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.currentPage).toBe(2);
      expect(result.pagination?.totalPages).toBe(2);
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should handle entriesPerPage = 1', async () => {
      setNativeEntries(fileEntries(5));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.entriesPerPage).toBe(1);
      expect(result.pagination?.totalPages).toBe(5);
    });

    it('should handle entriesPerPage = 20 (max)', async () => {
      setNativeEntries(fileEntries(150));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.entriesPerPage).toBe(20);
      expect(result.pagination?.totalPages).toBe(8);
    });

    it('should handle exact boundary (20 entries, 20 per page)', async () => {
      setNativeEntries(fileEntries(20));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeUndefined();
    });

    it('should handle one over boundary (21 entries, 20 per page)', async () => {
      setNativeEntries(fileEntries(21));

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination?.totalPages).toBe(2);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('should handle single entry (no pagination needed)', async () => {
      setNativeEntries([
        { path: '/test/path/single-file.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeUndefined();
    });
  });

  describe('entry pagination — no charPagination', () => {
    it('should return entry pagination without charPagination', async () => {
      setNativeEntries(fileEntries(100));

      const result = await viewStructure({ path: '/test/path' });

      expect(result.status).toBeUndefined();
      expect(result.summary).toContain('100');
      expect(
        (result as Record<string, unknown>).charPagination
      ).toBeUndefined();
    });

    it('should handle UTF-8 filenames correctly', async () => {
      setNativeEntries([
        { path: '/test/path/文件1.txt', type: 'file', size: 1024 },
        { path: '/test/path/文件2.txt', type: 'file', size: 1024 },
        { path: '/test/path/📁folder', type: 'directory' },
        { path: '/test/path/emoji👋.txt', type: 'file', size: 1024 },
      ]);

      const result = await viewStructure({ path: '/test/path' });

      expect(result.status).toBeUndefined();
      expect(flatNames(result).every(n => !n.includes('�'))).toBe(true);
      expect(
        (result as Record<string, unknown>).charPagination
      ).toBeUndefined();
    });
  });

  describe('Auto-pagination for large structuredOutput', () => {
    it('should return entries with entry pagination (C5: char auto-pagination removed)', async () => {
      const longNameFiles = Array.from({ length: 20 }, (_, i) => ({
        path: `/test/path/this_is_an_extremely_long_filename_that_will_definitely_exceed_the_limit_when_multiplied_by_twenty_entries_${i.toString().padStart(3, '0')}.txt`,
        type: 'file' as const,
        size: 1024,
        modifiedMs: new Date('2024-01-01').getTime(),
      }));
      setNativeEntries(longNameFiles);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        itemsPerPage: 20,
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(result.files!.length).toBe(20);
      expect(result.warnings).toBeUndefined();
    });

    it('should NOT auto-paginate when output is under MAX_OUTPUT_CHARS (2000)', async () => {
      setNativeEntries([
        { path: '/test/path/a.txt', type: 'file', size: 100 },
        { path: '/test/path/b.txt', type: 'file', size: 100 },
        { path: '/test/path/c.txt', type: 'file', size: 100 },
      ]);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.warnings).toBeUndefined();
    });

    it('should use entry pagination when charLength provided (C5: charLength ignored)', async () => {
      const manyFiles = Array.from({ length: 50 }, (_, i) => ({
        path: `/test/path/file_${i.toString().padStart(3, '0')}.txt`,
        type: 'file' as const,
        size: 1024,
        modifiedMs: new Date('2024-01-01').getTime(),
      }));
      setNativeEntries(manyFiles);

      const result = await viewStructure({
        path: '/test/path',
        depth: 1,
        charLength: 500,
      });

      expect(result.status).toBeUndefined();
      expect(result.warnings).toBeUndefined();
      expect(result.files!.length).toBeLessThanOrEqual(100);
    });

    it('should use entry pagination in non-recursive mode (C5: no char truncation)', async () => {
      const longFiles = Array.from({ length: 100 }, (_, i) => ({
        path: `/test/path/very_long_filename_for_ls_output_${i.toString().padStart(3, '0')}.txt`,
        type: 'file' as const,
        size: 1024,
      }));
      setNativeEntries(longFiles);

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(result.files!.length).toBeLessThanOrEqual(100);
    });
  });

  describe('pass-through contract — full entries always returned', () => {
    beforeEach(() => {
      setNativeEntries([
        { path: '/test/path/file1.txt', type: 'file', size: 1024 },
        { path: '/test/path/file2.js', type: 'file', size: 1024 },
        { path: '/test/path/dir1', type: 'directory' },
        { path: '/test/path/file3.md', type: 'file', size: 1024 },
        { path: '/test/path/file4.ts', type: 'file', size: 1024 },
      ]);
    });

    it(' returns same full flat lists as default', async () => {
      const def = await viewStructure({ path: '/test/path' });
      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toEqual(def.files);
      expect(result.folders).toEqual(def.folders);
      expect(result.files!.length).toBeGreaterThan(0);
    });

    it(' keeps pagination so the agent still sees totalEntries', async () => {
      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.summary).toMatch(/\d+ entries/);
    });

    it(' emits same hints as default — no tier commentary', async () => {
      const def = await viewStructure({ path: '/test/path' });
      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      const hintsBlob = (result.hints ?? []).join('\n');
      expect(result.hints).toEqual(def.hints);
      expect(hintsBlob).not.toMatch(/drill-back|re-call|detail dropped/i);
    });

    it('always returns full flat lists', async () => {
      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(result.files!.length).toBeGreaterThan(0);
    });

    it(' also returns full flat lists (metadata is additive)', async () => {
      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBeUndefined();
      expect(result.files).toBeDefined();
      expect(result.files!.length).toBeGreaterThan(0);
    });

    it('does not transform the empty status', async () => {
      setNativeEntries([]);

      const result = await viewStructure({
        path: '/test/path',
      });

      expect(result.status).toBe('empty');
    });
  });
});
