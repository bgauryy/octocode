import {
  extractMatchingLines as extractMatchingLinesNative,
  type ExtractMatchingLinesOptions,
} from '@octocodeai/octocode-context-utils';

export function extractMatchingLines(
  lines: string[],
  pattern: string,
  contextLines: number,
  isRegex: boolean = false,
  caseSensitive: boolean = false,
  maxMatches?: number
): {
  lines: string[];
  matchRanges: Array<{ start: number; end: number }>;
  matchCount: number;
  matchingLines: number[];
} {
  const content = lines.join('\n');

  const options: ExtractMatchingLinesOptions = {
    isRegex,
    caseSensitive,
    contextLines,
    maxMatches,
  };

  const result = extractMatchingLinesNative(content, pattern, options);

  return {
    lines: result.lines,
    matchRanges: result.matchRanges.map(r => ({ start: r.start, end: r.end })),
    matchCount: result.matchCount,
    matchingLines: result.matchingLines.map(n => n),
  };
}
