import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  ContextUtilsLoadError,
  contextUtils,
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../src/utils/contextUtils.js';

describe('contextUtils native boundary', () => {
  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('does not load the native package while importing high-level modules', async () => {
    setContextUtilsNativeLoaderForTesting(() => {
      throw new Error('native unavailable');
    });

    await expect(import('../../src/responses.js')).resolves.toBeDefined();
    await expect(
      import('../../src/github/fileContentProcess.js')
    ).resolves.toBeDefined();
    await expect(import('../../src/github/codeSearch.js')).resolves.toBeDefined();

    expect(() => contextUtils.jsonToYamlString({ ok: true })).toThrow(
      ContextUtilsLoadError
    );
  });

  it('throws a clear boundary error without fallback when native loading fails', () => {
    const nativeError = new Error('dlopen failed');
    setContextUtilsNativeLoaderForTesting(() => {
      throw nativeError;
    });

    try {
      contextUtils.applyContentViewMinification('const x = 1;', 'x.ts');
      throw new Error('expected contextUtils to throw');
    } catch (error) {
      expect(error).toBeInstanceOf(ContextUtilsLoadError);
      expect((error as ContextUtilsLoadError).cause).toBe(nativeError);
    }
  });

  it('delegates filesystem queries to the native package', () => {
    const queryFileSystem = vi.fn().mockReturnValue({
      entries: [],
      totalDiscovered: 0,
      wasCapped: false,
      skipped: 0,
      permissionDenied: 0,
      warnings: [],
    });
    setContextUtilsNativeLoaderForTesting(
      () =>
        ({
          queryFileSystem,
        }) as unknown as typeof import('@octocodeai/octocode-context-utils')
    );

    const result = contextUtils.queryFileSystem({
      path: '/repo',
      names: ['*.ts'],
      entryType: 'f',
    });

    expect(queryFileSystem).toHaveBeenCalledWith({
      path: '/repo',
      names: ['*.ts'],
      entryType: 'f',
    });
    expect(result.totalDiscovered).toBe(0);
  });
});
