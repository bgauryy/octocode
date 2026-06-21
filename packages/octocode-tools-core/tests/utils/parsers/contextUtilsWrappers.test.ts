import { afterEach, describe, expect, it } from 'vitest';

import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';
import { filterPatch, trimDiffContext } from '../../../src/utils/parsers/diff.js';
import { parseRipgrepJson } from '../../../src/utils/parsers/ripgrep.js';

type NativeContextUtilsModule = typeof import('@octocodeai/octocode-engine');

function installNative(partial: Partial<NativeContextUtilsModule>): void {
  setContextUtilsNativeLoaderForTesting(() => partial as NativeContextUtilsModule);
}

describe('context-utils parser wrappers', () => {
  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('maps ripgrep native output into tools-core search result shape', () => {
    installNative({
      parseRipgrepJson: (stdout, options) => {
        expect(stdout).toBe('{"type":"summary"}');
        expect(options).toEqual({
          contextLines: 2,
          maxSnippetChars: 80,
        });
        return {
          files: [
            {
              path: 'src/a.ts',
              matchCount: 1,
              matches: [{ line: 7, column: 3, value: 'const needle = true;' }],
            },
          ],
          stats: {
            matchCount: 1,
            matchedLines: 1,
            filesMatched: 1,
            filesSearched: 4,
            bytesSearched: 123,
            searchTime: '0.002s',
          },
        };
      },
    });

    const result = parseRipgrepJson('{"type":"summary"}', {
      contextLines: 2,
      matchContentLength: 80,
    });

    expect(result.files).toEqual([
      {
        path: 'src/a.ts',
        matchCount: 1,
        matches: [{ line: 7, column: 3, value: 'const needle = true;' }],
      },
    ]);
    expect(result.stats).toEqual({
      totalOccurrences: 1,
      matchedLines: 1,
      filesMatched: 1,
      filesSearched: 4,
      bytesSearched: 123,
      searchTime: '0.002s',
    });
  });

  it('keeps selected-line no-op native-free while mapping selected-line options', () => {
    const calls: NonNullable<
      Parameters<NativeContextUtilsModule['filterPatch']>[1]
    >[] = [];
    installNative({
      filterPatch: (_patch, options) => {
        if (options) calls.push(options);
        return 'filtered';
      },
    });

    expect(filterPatch('raw patch')).toBe('raw patch');
    expect(filterPatch('raw patch', [4], [2])).toBe('filtered');

    expect(calls).toEqual([{ additions: [4], deletions: [2] }]);
  });

  it('delegates raw diff context trimming to native filterPatch', () => {
    const calls: Array<{
      patch: string;
      options: NonNullable<
        Parameters<NativeContextUtilsModule['filterPatch']>[1]
      >;
    }> = [];
    installNative({
      filterPatch: (patch, options) => {
        if (!options) throw new Error('trimDiffContext must pass options');
        calls.push({ patch, options });
        return 'native-trimmed';
      },
    });

    const lines: string[] = [];
    for (let i = 0; i < 15; i++) lines.push(` ctx${i}`);
    lines.push('+added');
    for (let i = 0; i < 19; i++) lines.push(` after${i}`);
    const patch = lines.join('\n');

    expect(trimDiffContext(patch)).toBe('native-trimmed');
    expect(calls).toEqual([
      {
        patch,
        options: { trimContext: true, contextLines: 2 },
      },
    ]);
  });

  it('keeps empty diff trimming native-free', () => {
    installNative({
      filterPatch: () => {
        throw new Error('empty trim must not load native filter');
      },
    });

    expect(trimDiffContext('')).toBe('');
  });
});
