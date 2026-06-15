import { afterEach, describe, expect, it, vi } from 'vitest';
import { join } from 'node:path';

import { viewStructure } from '../../../src/tools/local_view_structure/local_view_structure.js';
import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';

type NativeContextUtilsModule = typeof import('@octocodeai/octocode-context-utils');

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

describe('localViewStructure native filter pushdown', () => {
  const validBasePath = join(process.cwd(), 'tests');

  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('does not pre-cap bracket globs before TypeScript filtering', async () => {
    const queryFileSystem = vi.fn().mockReturnValue({
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
      filesOnly: true,
      limit: 2,
      details: true,
    });

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({
        names: undefined,
        entryType: 'f',
        limit: 10000,
      })
    );
    expect(result.entries?.map(entry => entry.path)).toEqual([
      '/repo/nested/target-a.ts',
      '/repo/nested/target-b.ts',
    ]);
  });

  it('does not pre-cap extension filters before TypeScript filtering', async () => {
    const queryFileSystem = vi.fn().mockReturnValue({
      entries: [fileEntry('alpha.txt', 'txt'), fileEntry('beta.ts', 'ts')],
      totalDiscovered: 2,
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
      filesOnly: true,
      limit: 1,
      details: true,
    });

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({
        names: undefined,
        entryType: 'f',
        limit: 10000,
      })
    );
    expect(result.entries?.map(entry => entry.path)).toEqual(['/repo/beta.ts']);
  });

  it('keeps the narrow pre-cap when filters are fully native-pushed', async () => {
    const queryFileSystem = vi.fn().mockReturnValue({
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
      filesOnly: true,
      limit: 2,
    });

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({
        names: ['target-*.ts'],
        entryType: 'f',
        limit: 4,
      })
    );
  });
});
