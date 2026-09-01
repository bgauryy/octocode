import { join } from 'node:path';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { findFiles } from '../../../src/tools/local_find_files/findFiles.js';
import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';

type NativeContextUtilsModule = typeof import('@octocodeai/octocode-engine');

describe('findFiles native pathPattern ownership', () => {
  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('passes brace alternation to one native filesystem query unchanged', async () => {
    const queryFileSystem = vi.fn().mockResolvedValue({
      entries: [],
      totalDiscovered: 0,
      wasCapped: false,
      skipped: 0,
      permissionDenied: 0,
      warnings: [],
    });
    setContextUtilsNativeLoaderForTesting(
      () => ({ queryFileSystem }) as unknown as NativeContextUtilsModule
    );

    await findFiles({
      path: join(process.cwd(), 'tests'),
      pathPattern: 'packages/{react,react-dom}/src/**',
      entryType: 'f',
    });

    expect(queryFileSystem).toHaveBeenCalledTimes(1);
    expect(queryFileSystem).toHaveBeenCalledWith(
      expect.objectContaining({
        pathPattern: 'packages/{react,react-dom}/src/**',
      })
    );
  });
});
