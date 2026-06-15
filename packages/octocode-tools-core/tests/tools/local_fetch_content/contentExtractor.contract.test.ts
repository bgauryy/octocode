import { afterEach, describe, expect, it } from 'vitest';

import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';
import { extractMatchingLines } from '../../../src/tools/local_fetch_content/contentExtractor.js';

type NativeContextUtilsModule = typeof import('@octocodeai/octocode-context-utils');

function installNative(partial: Partial<NativeContextUtilsModule>): void {
  setContextUtilsNativeLoaderForTesting(() => partial as NativeContextUtilsModule);
}

describe('extractMatchingLines wrapper contract', () => {
  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('joins caller-provided lines and maps native match metadata unchanged', () => {
    installNative({
      extractMatchingLines: (content, pattern, options) => {
        expect(content).toBe('alpha\nneedle\nomega');
        expect(pattern).toBe('needle');
        expect(options).toEqual({
          isRegex: true,
          caseSensitive: true,
          contextLines: 3,
          maxMatches: 1,
        });
        return {
          lines: ['needle'],
          matchingLines: [2],
          matchCount: 4,
          matchRanges: [{ start: 2, end: 2 }],
        };
      },
    });

    expect(
      extractMatchingLines(
        ['alpha', 'needle', 'omega'],
        'needle',
        3,
        true,
        true,
        1
      )
    ).toEqual({
      lines: ['needle'],
      matchingLines: [2],
      matchCount: 4,
      matchRanges: [{ start: 2, end: 2 }],
    });
  });
});
