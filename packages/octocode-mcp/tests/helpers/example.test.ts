import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
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

const { viewStructure } =
  await import('../../../octocode-tools-core/src/tools/local_view_structure/local_view_structure.js');
const { LocalViewStructureQuerySchema } =
  await import('../../../octocode-tools-core/src/tools/local_view_structure/scheme.js');

const mockValidate = vi.mocked(pathValidator.pathValidator.validate);

/**
 * The local view-structure tool now delegates filesystem traversal/filtering to
 * the native `@octocodeai/octocode-context-utils` module via
 * `contextUtils.queryFileSystem`. These helpers let each test declare the
 * entries that the (mocked) native layer should return.
 */
interface MockEntryInput {
  name: string;
  path?: string;
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
  const path = input.path ?? `${basePath.replace(/\/$/, '')}/${input.name}`;
  const ext = input.name.includes('.')
    ? input.name.split('.').pop()
    : undefined;
  return {
    path,
    relativePath: input.name,
    name: input.name,
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

/** Make the native layer throw (e.g. ENOENT/EACCES/permission denied). */
function setNativeError(error: Error): void {
  queryFileSystemMock.mockImplementation(
    (options: FileSystemQueryOptions): FileSystemQueryResult => {
      lastQueryOptions = options;
      throw error;
    }
  );
}

function installHarness(validPath = '/workspace') {
  vi.clearAllMocks();
  lastQueryOptions = undefined;
  queryFileSystemMock = vi.fn();
  setContextUtilsNativeLoaderForTesting(
    () =>
      ({
        queryFileSystem: queryFileSystemMock,
      }) as unknown as typeof import('@octocodeai/octocode-context-utils')
  );
  mockValidate.mockReturnValue({ isValid: true, sanitizedPath: validPath });
  setNativeEntries([]);
}

describe('Example: Using Unified Test Helpers', () => {
  beforeEach(() => {
    installHarness('/workspace');
  });

  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  describe('Basic Directory Listing', () => {
    it('should list entries from the native layer', async () => {
      setNativeEntries([
        { name: 'file1.ts', type: 'file', size: 1024 },
        { name: 'file2.ts', type: 'file', size: 1024 },
        { name: 'dir1', type: 'directory' },
      ]);

      const result = await viewStructure({
        path: '/workspace',
        researchGoal: 'Test directory listing',
      });

      expect(result.status).toBeUndefined();
      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });

    it('should handle empty directories', async () => {
      setNativeEntries([]);

      const result = await viewStructure({
        path: '/workspace',
      });

      expect(result.status).toBe('empty');
    });

    it('should handle native access failures', async () => {
      setNativeError(
        Object.assign(new Error('EACCES: permission denied'), {
          code: 'EACCES',
        })
      );

      const result = await viewStructure({
        path: '/workspace',
      });

      expect(result.status).toBe('error');
    });
  });

  describe('Recursive Listing with Depth', () => {
    it('should use recursive walk for depth > 0', async () => {
      setNativeEntries([
        { name: 'src', type: 'directory', depth: 0 },
        { name: 'package.json', type: 'file', size: 100, depth: 0 },
      ]);

      const result = await viewStructure({
        path: '/workspace',
        maxDepth: 1,
      });

      expect(result.status).toBeUndefined();
      expect(lastQueryOptions?.recursive).toBe(true);
    });
  });

  describe('Path Validation', () => {
    it('should reject invalid paths', async () => {
      mockValidate.mockReturnValue({
        isValid: false,
        error: 'Path outside workspace',
      });

      const result = await viewStructure({
        path: '/etc/passwd',
      });

      expect(result.status).toBe('error');
    });

    it('should accept valid workspace paths', async () => {
      mockValidate.mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/src',
      });
      setNativeEntries([]);

      const result = await viewStructure({
        path: '/workspace/src',
      });

      expect(result.status).toBe('empty');
    });
  });

  describe('Pagination', () => {
    it('should include pagination info', async () => {
      const files = Array.from({ length: 25 }, (_, i) => ({
        name: `file${i}.ts`,
        type: 'file' as const,
        size: 1024,
      }));
      setNativeEntries(files);

      const result = await viewStructure(
        LocalViewStructureQuerySchema.parse({
          path: '/workspace',
          itemsPerPage: 10,
          page: 1,
        })
      );

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();
      expect(result.pagination?.currentPage).toBe(1);
      expect(result.pagination?.hasMore).toBe(true);
    });
  });

  describe('Research Goals Passthrough', () => {
    it('should not echo researchGoal and reasoning in results', async () => {
      setNativeEntries([]);

      const result = await viewStructure({
        path: '/workspace',
        researchGoal: 'Find configuration files',
        reasoning: 'Need to understand project setup',
      });

      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });
  });
});

describe('Example: Direct Mock Access', () => {
  beforeEach(() => {
    installHarness('/workspace');
  });

  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('should allow direct native-layer manipulation', async () => {
    setNativeEntries([
      { name: 'file1.ts', type: 'file', size: 500 },
      { name: 'file2.ts', type: 'file', size: 500 },
    ]);

    const result = await viewStructure({ path: '/workspace' });
    expect(result.status).toBeUndefined();

    expect(queryFileSystemMock).toHaveBeenCalled();
  });

  it('should track native-layer call arguments', async () => {
    setNativeEntries([]);

    await viewStructure({ path: '/workspace' });

    expect(queryFileSystemMock).toHaveBeenCalled();
    expect(lastQueryOptions?.path).toBe('/workspace');
  });
});
