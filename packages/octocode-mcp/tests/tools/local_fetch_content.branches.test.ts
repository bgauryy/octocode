/**
 * Branch coverage tests for local_fetch_content
 * Targets: contentMinifier catch block, contentExtractor maxMatches edge case,
 * minifier branch when content has no comments/whitespace to strip
 */

import { describe, it, expect, vi } from 'vitest';

describe('applyMinification', () => {
  it('should return original content when minification throws', async () => {
    vi.resetModules();

    vi.doMock('../../src/utils/minifier/minifier.js', () => ({
      minifyContentSync: vi.fn(() => {
        throw new Error('Minification engine crashed');
      }),
    }));

    const { applyMinification } =
      await import('../../src/utils/minifier/applyMinification.js');

    const content = 'const x = 1;\nconst y = 2;\n';
    const result = applyMinification(content, 'test.ts');

    expect(result).toBe(content);
  });

  it('should return original when minified is not smaller', async () => {
    vi.resetModules();

    vi.doMock('../../src/utils/minifier/minifier.js', () => ({
      minifyContentSync: vi.fn((content: string) => content + '/* padded */'),
    }));

    const { applyMinification } =
      await import('../../src/utils/minifier/applyMinification.js');

    const content = 'short';
    const result = applyMinification(content, 'test.ts');

    expect(result).toBe(content);
  });

  it('should return minified content when it is smaller (branch: minified < original)', async () => {
    vi.resetModules();

    const minified = 'const x=1;const y=2;';
    vi.doMock('../../src/utils/minifier/minifier.js', () => ({
      minifyContentSync: vi.fn(() => minified),
    }));

    const { applyMinification } =
      await import('../../src/utils/minifier/applyMinification.js');

    const content = 'const x = 1;\nconst y = 2;\n';
    const result = applyMinification(content, 'test.ts');

    expect(result).toBe(minified);
  });

  it('should return original when content has no comments/whitespace to strip (same length)', async () => {
    // Content that minifier returns unchanged - exercises branch where minified.length >= content.length
    const { applyMinification } =
      await import('../../src/utils/minifier/applyMinification.js');

    const content = 'x';
    const result = applyMinification(content, 'test.txt');

    expect(result).toBe(content);
  });
});

describe('extractMatchingLines - edge cases', () => {
  it('should find matches with extractMatchingLines', async () => {
    vi.resetModules();

    const { extractMatchingLines } =
      await import('../../src/tools/local_fetch_content/contentExtractor.js');

    const lines = ['line1', 'foo', 'line3', 'foo', 'line5'];
    const result = extractMatchingLines(lines, 'foo', 1, false, false);

    expect(result.matchCount).toBe(2);
    expect(result.lines.length).toBeGreaterThan(0);
  });

  it('should return empty when no matches', async () => {
    vi.resetModules();

    const { extractMatchingLines } =
      await import('../../src/tools/local_fetch_content/contentExtractor.js');

    const lines = ['line1', 'line2', 'line3'];
    const result = extractMatchingLines(lines, 'nonexistent', 1, false, false);

    expect(result.matchCount).toBe(0);
    expect(result.lines).toEqual([]);
  });

  it('should throw for invalid regex pattern (lines 30-33)', async () => {
    vi.resetModules();
    const { extractMatchingLines } =
      await import('../../src/tools/local_fetch_content/contentExtractor.js');

    expect(() =>
      extractMatchingLines(
        ['line1', 'line2'],
        '[invalid(unclosed',
        1,
        true,
        false
      )
    ).toThrow(/Invalid regex|invalid/i);
  });

  it('should show omitted lines when ranges have gaps (lines 86-89)', async () => {
    vi.resetModules();
    const { extractMatchingLines } =
      await import('../../src/tools/local_fetch_content/contentExtractor.js');

    const lines = Array.from({ length: 100 }, (_, i) =>
      i === 5 || i === 50 ? 'MATCH' : `line${i}`
    );
    const result = extractMatchingLines(lines, 'MATCH', 1, false, false);

    expect(result.matchCount).toBe(2);
    expect(result.lines).toContain('');
    expect(result.lines.some(l => l.includes('lines omitted'))).toBe(true);
  });

  it('should return empty when matchesToProcess is empty (maxMatches yields empty slice, line 63)', async () => {
    vi.resetModules();
    const { extractMatchingLines } =
      await import('../../src/tools/local_fetch_content/contentExtractor.js');

    // maxMatches=-2 is truthy; slice(0,-2) on [2,4] yields [] (empty)
    const lines = ['line1', 'MATCH', 'line3', 'MATCH', 'line5'];
    const result = extractMatchingLines(lines, 'MATCH', 1, false, false, -2);

    expect(result.lines).toEqual([]);
    expect(result.matchRanges).toEqual([]);
    expect(result.matchCount).toBe(0);
  });
});
