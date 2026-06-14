import { describe, it, expect, vi } from 'vitest';

describe('extractMatchingLines - edge cases', () => {
  it('should find matches with extractMatchingLines', async () => {
    vi.resetModules();

    const { extractMatchingLines } =
      await import('../../../octocode-tools-core/src/tools/local_fetch_content/contentExtractor.js');

    const lines = ['line1', 'foo', 'line3', 'foo', 'line5'];
    const result = extractMatchingLines(lines, 'foo', 1, false, false);

    expect(result.matchCount).toBe(2);
    expect(result.lines.length).toBeGreaterThan(0);
  });

  it('should return empty when no matches', async () => {
    vi.resetModules();

    const { extractMatchingLines } =
      await import('../../../octocode-tools-core/src/tools/local_fetch_content/contentExtractor.js');

    const lines = ['line1', 'line2', 'line3'];
    const result = extractMatchingLines(lines, 'nonexistent', 1, false, false);

    expect(result.matchCount).toBe(0);
    expect(result.lines).toEqual([]);
  });

  it('should throw for invalid regex pattern (lines 30-33)', async () => {
    vi.resetModules();
    const { extractMatchingLines } =
      await import('../../../octocode-tools-core/src/tools/local_fetch_content/contentExtractor.js');

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
      await import('../../../octocode-tools-core/src/tools/local_fetch_content/contentExtractor.js');

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
      await import('../../../octocode-tools-core/src/tools/local_fetch_content/contentExtractor.js');

    const lines = ['line1', 'MATCH', 'line3', 'MATCH', 'line5'];
    const result = extractMatchingLines(lines, 'MATCH', 1, false, false, -2);

    expect(result.lines).toEqual([]);
    expect(result.matchRanges).toEqual([]);
    expect(result.matchCount).toBe(0);
  });
});
