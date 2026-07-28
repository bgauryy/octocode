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

function emptyNativeResult() {
  return {
    entries: [],
    totalDiscovered: 0,
    wasCapped: false,
    skipped: 0,
    permissionDenied: 0,
    warnings: [],
  };
}

describe('localViewStructure effective maxDepth (documented defaults)', () => {
  const validBasePath = join(process.cwd(), 'tests');

  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('omitting maxDepth without recursive lists immediate children only (depth 1, non-recursive)', async () => {
    const queryFileSystem = vi.fn().mockReturnValue(emptyNativeResult());
    installQueryFileSystem(queryFileSystem);

    await viewStructure({ path: validBasePath });

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({ recursive: false, maxDepth: 1 })
    );
  });

  it('omitting maxDepth with recursive:true defaults to depth 5', async () => {
    const queryFileSystem = vi.fn().mockReturnValue(emptyNativeResult());
    installQueryFileSystem(queryFileSystem);

    await viewStructure({ path: validBasePath, recursive: true });

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({ recursive: true, maxDepth: 5 })
    );
  });

  it('maxDepth on its own enables recursion to that depth without recursive:true', async () => {
    const queryFileSystem = vi.fn().mockReturnValue(emptyNativeResult());
    installQueryFileSystem(queryFileSystem);

    await viewStructure({ path: validBasePath, maxDepth: 3 });

    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({ recursive: true, maxDepth: 3 })
    );
  });
});
