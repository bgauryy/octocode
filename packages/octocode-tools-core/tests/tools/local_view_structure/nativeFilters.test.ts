import { afterEach, describe, expect, it, vi } from 'vitest';
import { join } from 'node:path';

import { viewStructure } from '../../../src/tools/local_view_structure/local_view_structure.js';
import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';

type NativeContextUtilsModule = typeof import('@octocodeai/octocode-engine');

function installQueryFileSystem(
  queryFileSystem: ReturnType<typeof vi.fn>
): void {
  setContextUtilsNativeLoaderForTesting(
    () =>
      ({
        queryFileSystem,
      }) as unknown as NativeContextUtilsModule
  );
}

function fileEntry(relativePath: string, extension = '') {
  return {
    path: `/repo/${relativePath}`,
    relativePath,
    name: relativePath.split('/').pop()!,
    entryType: 'file' as const,
    depth: relativePath.includes('/') ? 2 : 1,
    size: 10,
    extension,
    permissions: '644',
  };
}

describe('local.tree native filter pushdown', () => {
  const validBasePath = join(process.cwd(), 'tests');

  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('does not pre-cap bracket globs before TypeScript filtering', async () => {
    const queryFileSystem = vi.fn().mockResolvedValue({
      entries: [
        ...Array.from({ length: 20 }, (_, index) =>
          fileEntry(`filler-${index}.txt`, 'txt')
        ),
        fileEntry('nested/target-a.ts', 'ts'),
        fileEntry('nested/target-b.ts', 'ts'),
      ],
      totalDiscovered: 22,
      wasCapped: false,
      skipped: 0,
      permissionDenied: 0,
      warnings: [],
    });
    installQueryFileSystem(queryFileSystem);

    const result = await viewStructure({
      path: validBasePath,
      recursive: true,
      depth: 3,
      pattern: 'target-[ab].ts',
      entryType: 'f',
      limit: 2,
      detail: 'full',
    });

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({
        names: undefined,
        entryType: 'f',
        excludeDir: expect.arrayContaining(['node_modules', 'dist', 'target']),
        limit: 10000,
      })
    );
    expect(result.entries?.map(entry => entry.path)).toEqual([
      '/repo/nested/target-a.ts',
      '/repo/nested/target-b.ts',
    ]);
  });

  it('pushes extension filters into the native query without limiting before sort', async () => {
    const queryFileSystem = vi.fn().mockResolvedValue({
      entries: [fileEntry('beta.ts', 'ts')],
      totalDiscovered: 1,
      wasCapped: false,
      skipped: 0,
      permissionDenied: 0,
      warnings: [],
    });
    installQueryFileSystem(queryFileSystem);

    const result = await viewStructure({
      path: validBasePath,
      recursive: true,
      depth: 2,
      extensions: ['ts'],
      entryType: 'f',
      limit: 1,
      detail: 'full',
    });

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({
        extensions: ['ts'],
        names: undefined,
        entryType: 'f',
        excludeDir: expect.arrayContaining(['node_modules', 'dist', 'target']),
        limit: 10000,
      })
    );
    expect(result.entries?.map(entry => entry.path)).toEqual(['/repo/beta.ts']);
  });

  it('keeps the narrow pre-cap when filters are fully native-pushed', async () => {
    const queryFileSystem = vi.fn().mockResolvedValue({
      entries: [fileEntry('target-a.ts', 'ts'), fileEntry('target-b.ts', 'ts')],
      totalDiscovered: 2,
      wasCapped: false,
      skipped: 0,
      permissionDenied: 0,
      warnings: [],
    });
    installQueryFileSystem(queryFileSystem);

    await viewStructure({
      path: validBasePath,
      recursive: true,
      depth: 2,
      pattern: 'target-*.ts',
      entryType: 'f',
      limit: 2,
    });

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({
        names: ['target-*.ts'],
        entryType: 'f',
        excludeDir: expect.arrayContaining(['node_modules', 'dist', 'target']),
        limit: 10000,
      })
    );
  });

  it('allows callers to opt out of default generated/vendor directory excludes', async () => {
    const queryFileSystem = vi.fn().mockResolvedValue({
      entries: [fileEntry('node_modules/pkg/index.js', 'js')],
      totalDiscovered: 1,
      wasCapped: false,
      skipped: 0,
      permissionDenied: 0,
      warnings: [],
    });
    installQueryFileSystem(queryFileSystem);

    await viewStructure({
      path: validBasePath,
      recursive: true,
      excludeDir: [],
      detail: 'full',
    } as Parameters<typeof viewStructure>[0]);

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({
        excludeDir: [],
      })
    );
  });

  it('warns when a capped sorted result can only sort a partial walk', async () => {
    const queryFileSystem = vi.fn().mockResolvedValue({
      entries: [fileEntry('small.ts', 'ts')],
      totalDiscovered: 10001,
      wasCapped: true,
      skipped: 0,
      permissionDenied: 0,
      warnings: [],
    });
    installQueryFileSystem(queryFileSystem);

    const result = await viewStructure({
      path: validBasePath,
      recursive: true,
      sortBy: 'size',
      reverse: true,
      detail: 'full',
    });

    expect(result.warnings?.join('\n')).toContain(
      'sortBy:"size" only orders that partial set'
    );
  });
});
