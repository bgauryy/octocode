import { describe, expect, it } from 'vitest';
import {
  applyContentViewMinification,
  MINIFY_CONFIG,
  minifyContentSync,
} from '@octocodeai/octocode-minifier';
import {
  buildLanguageBenchmarkCases,
  DROP_MARKER,
  KEEP_MARKER,
} from './languageBenchmarkFixtures.js';

function savingsPercent(input: string, output: string): number {
  if (input.length === 0) return 0;
  return ((input.length - output.length) / input.length) * 100;
}

describe('language quality benchmark — all configured extensions', () => {
  const cases = buildLanguageBenchmarkCases();

  it('covers every MINIFY_CONFIG file type exactly once', () => {
    const configuredExtensions = Object.keys(MINIFY_CONFIG.fileTypes).sort();
    const benchmarkExtensions = cases.map(testCase => testCase.ext).sort();

    expect(benchmarkExtensions).toEqual(configuredExtensions);
    expect(new Set(benchmarkExtensions).size).toBe(configuredExtensions.length);
  });

  it.each(cases)(
    '$ext content-view preserves useful content and strips configured comments',
    testCase => {
      const result = applyContentViewMinification(
        testCase.content,
        testCase.filePath
      );

      expect(
        result.length,
        `${testCase.ext}: content-view not longer`
      ).toBeLessThanOrEqual(testCase.content.length);
      expect(result.trim(), `${testCase.ext}: content-view not empty`).not.toBe(
        ''
      );

      for (const marker of testCase.protectedMarkers) {
        expect(result, `${testCase.ext}: preserve ${marker}`).toContain(marker);
      }

      for (const marker of testCase.droppedMarkers) {
        expect(result, `${testCase.ext}: strip ${marker}`).not.toContain(
          marker
        );
      }
    }
  );

  it.each(cases)(
    '$ext sync minification preserves useful content and strips configured comments',
    testCase => {
      const result = minifyContentSync(testCase.content, testCase.filePath);

      expect(
        result.length,
        `${testCase.ext}: sync not longer`
      ).toBeLessThanOrEqual(testCase.content.length);
      expect(result.trim(), `${testCase.ext}: sync not empty`).not.toBe('');
      expect(result, `${testCase.ext}: preserve ${KEEP_MARKER}`).toContain(
        KEEP_MARKER
      );

      for (const marker of testCase.droppedMarkers) {
        expect(result, `${testCase.ext}: strip ${marker}`).not.toContain(
          marker
        );
      }
    }
  );

  it('produces measurable savings for comment-bearing languages', () => {
    const commentCases = cases.filter(testCase =>
      testCase.droppedMarkers.includes(DROP_MARKER)
    );
    const contentViewSavings = commentCases.map(testCase =>
      savingsPercent(
        testCase.content,
        applyContentViewMinification(testCase.content, testCase.filePath)
      )
    );
    const syncSavings = commentCases.map(testCase =>
      savingsPercent(
        testCase.content,
        minifyContentSync(testCase.content, testCase.filePath)
      )
    );

    const averageContentViewSavings =
      contentViewSavings.reduce((sum, value) => sum + value, 0) /
      contentViewSavings.length;
    const averageSyncSavings =
      syncSavings.reduce((sum, value) => sum + value, 0) / syncSavings.length;

    expect(averageContentViewSavings).toBeGreaterThan(10);
    expect(averageSyncSavings).toBeGreaterThan(10);
    expect(Math.min(...contentViewSavings)).toBeGreaterThan(0);
    expect(Math.min(...syncSavings)).toBeGreaterThan(0);
  });
});
