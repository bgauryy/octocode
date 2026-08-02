import { describe, expect, it } from 'vitest';

import { recomputeMatchPositions } from '../../src/github/codeSearch.js';
import {
  mapCodeSearchProviderResult,
  type CodeSearchGroupedMatch,
} from '../../src/tools/providerMappers/codeSearch.js';

/**
 * Regression for the audit-found silent-wrong defect: GitHub's text-match
 * `positions` index into the RAW fragment, but the fragment shown to the
 * agent is sanitized → minified → truncated. Passing the raw indices through
 * untouched made them point at the wrong text with no flag. Positions must
 * be recomputed against the transformed fragment (by locating the matched
 * text), dropped when minification removed the match, and dropped when
 * truncation cut them off.
 */
describe('recomputeMatchPositions', () => {
  it('re-anchors positions after the fragment is transformed', () => {
    const raw = '// a leading comment\n\nconst needleXyz = 1;\n';
    const matchStart = raw.indexOf('needleXyz');
    const transformed = 'const needleXyz = 1;'; // comment + blanks stripped

    const out = recomputeMatchPositions(
      raw,
      [[matchStart, matchStart + 'needleXyz'.length]],
      transformed
    );

    expect(out).toHaveLength(1);
    const [start, end] = out[0]!;
    expect(transformed.slice(start, end)).toBe('needleXyz');
  });

  it('drops a position whose matched text was stripped by the transform', () => {
    const raw = '// TODO needleXyz cleanup\nconst other = 2;\n';
    const matchStart = raw.indexOf('needleXyz');
    const transformed = 'const other = 2;'; // comment (and the match) removed

    const out = recomputeMatchPositions(
      raw,
      [[matchStart, matchStart + 'needleXyz'.length]],
      transformed
    );

    expect(out).toHaveLength(0);
  });

  it('maps repeated occurrences to successive positions, in order', () => {
    const raw = 'use(needleXyz); use(needleXyz);';
    const first = raw.indexOf('needleXyz');
    const second = raw.indexOf('needleXyz', first + 1);
    const transformed = 'use(needleXyz);use(needleXyz);';

    const out = recomputeMatchPositions(
      raw,
      [
        [first, first + 'needleXyz'.length],
        [second, second + 'needleXyz'.length],
      ],
      transformed
    );

    expect(out).toHaveLength(2);
    const [a, b] = out;
    expect(transformed.slice(a![0], a![1])).toBe('needleXyz');
    expect(transformed.slice(b![0], b![1])).toBe('needleXyz');
    expect(b![0]).toBeGreaterThan(a![0]);
  });

  it('handles an empty/degenerate raw slice without emitting a bogus position', () => {
    const out = recomputeMatchPositions('abc', [[0, 0]], 'abc');
    expect(out).toHaveLength(0);
  });
});

describe('mapCodeSearchProviderResult drops indices beyond the truncated snippet', () => {
  it('omits matchIndices that truncation cut off', () => {
    const longContext = 'x'.repeat(600) + 'needleXyz';
    const needleStart = 600;
    const data = {
      items: [
        {
          path: 'src/a.ts',
          repository: { name: 'octo/repo' },
          matches: [
            {
              context: longContext,
              positions: [
                [10, 19] as [number, number],
                [needleStart, needleStart + 9] as [number, number],
              ],
            },
          ],
        },
      ],
    };

    const result = mapCodeSearchProviderResult(
      data as never,
      { keywords: ['needleXyz'] } as never
    );

    const match = result.results[0]!.matches[0]! as CodeSearchGroupedMatch;
    // The snippet is truncated to 500 chars + '...' — the second position
    // (at 600) no longer exists in the shown value and must not be emitted.
    expect(match.value.length).toBeLessThan(600);
    const indices = match.matchIndices ?? [];
    expect(indices).toHaveLength(1);
    for (const { start, end } of indices) {
      expect(end).toBeLessThanOrEqual(match.value.length);
      expect(start).toBeGreaterThanOrEqual(0);
    }
  });
});
